// SPDX-License-Identifier: AGPL-3.0-only
//! Topic-matching: identifies whether an incoming memory is an update to an
//! existing topic, enabling in-place upsert instead of creating duplicates.

use crate::db::MemoryDB;
use crate::error::OriginError;
use crate::tuning::TopicMatchConfig;

/// Result of a topic-match check.
#[derive(Debug)]
pub struct TopicMatchResult {
    /// The matched memory's source_id, if a topic match was found.
    pub matched_source_id: Option<String>,
    /// The matched memory's content at the time of matching (for changelog delta).
    pub old_content: Option<String>,
    /// The matched memory's embedding at the time of matching (for change classification).
    pub old_embedding: Option<Vec<f32>>,
    /// Signals that contributed to the match decision (for logging / debugging).
    pub signals: MatchSignals,
}

/// Signals that contributed to the match decision.
#[derive(Debug, Default)]
pub struct MatchSignals {
    pub entity_match: bool,
    pub fts_title_hit: bool,
    pub embedding_similarity: Option<f64>,
    pub candidate_count: usize,
}

/// A lightweight candidate memory for topic matching.
#[derive(Debug, Clone)]
pub struct TopicMatchCandidate {
    pub source_id: String,
    pub title: String,
    pub content: String,
    pub entity_id: Option<String>,
    pub embedding: Vec<f32>,
    pub domain: Option<String>,
    pub memory_type: Option<String>,
}

/// Check if an incoming memory matches an existing topic for in-place upsert.
///
/// Runs pre-batcher in `handle_store_memory`. Returns the matched memory's
/// source_id if a topic match is found, `None` otherwise.
///
/// Matching uses tiered thresholds based on domain+type overlap:
/// - Both match: 0.70 (high confidence context)
/// - One matches: 0.80 (partial context)
/// - Neither: 0.90 (semantic-only, very conservative)
///
/// Priority: entity overlap (with similarity ≥ `threshold_exact`)
/// > title+embedding > embedding-only.
///
/// Note: entity overlap *alone* is not enough. Multiple atomic captures
/// during a single `/handoff` are all anchored to the same entity but carry
/// distinct content; a pure entity-id match would coalesce them. Entity
/// match still wins ties when paired with sufficient embedding similarity.
pub async fn find_topic_match(
    db: &MemoryDB,
    title: &str,
    memory_type: Option<&str>,
    domain: Option<&str>,
    entity_id: Option<&str>,
    content_embedding: &[f32],
    config: &TopicMatchConfig,
) -> Result<TopicMatchResult, OriginError> {
    let no_match = TopicMatchResult {
        matched_source_id: None,
        old_content: None,
        old_embedding: None,
        signals: MatchSignals::default(),
    };

    // Fetch candidates: prefers same domain+type but includes all recent memories.
    let candidates = db
        .topic_match_candidates(domain, memory_type, config.max_candidates)
        .await?;

    if candidates.is_empty() {
        return Ok(no_match);
    }

    let mut signals = MatchSignals {
        candidate_count: candidates.len(),
        ..Default::default()
    };

    // Step 1: Entity ID overlap — strong signal, but content must also be
    // similar enough to avoid coalescing distinct ideas about the same
    // entity. Earlier versions returned on entity-id match alone, which
    // silently dropped data when multiple atomic captures during one
    // `/handoff` were all anchored to the same entity.
    if let Some(eid) = entity_id {
        if let Some((matched, sim)) =
            best_entity_match(&candidates, eid, content_embedding, config.threshold_exact)
        {
            signals.entity_match = true;
            signals.embedding_similarity = Some(sim);
            log::info!(
                "[topic_match] entity match (sim={:.3} ≥ threshold_exact={:.2}): entity={eid} → source_id={}",
                sim,
                config.threshold_exact,
                matched.source_id
            );
            return Ok(TopicMatchResult {
                matched_source_id: Some(matched.source_id.clone()),
                old_content: Some(matched.content.clone()),
                old_embedding: Some(matched.embedding.clone()),
                signals,
            });
        }
    }

    // Step 2: FTS5 title query against memories_fts.
    let candidate_ids: Vec<&str> = candidates.iter().map(|c| c.source_id.as_str()).collect();
    let fts_hits: std::collections::HashSet<String> = db
        .topic_match_title_fts(title, &candidate_ids)
        .await?
        .into_iter()
        .collect();

    // Step 3: Rank candidates by embedding similarity, apply tiered thresholds.
    let mut ranked: Vec<(&TopicMatchCandidate, f64)> = candidates
        .iter()
        .map(|c| {
            let sim = cosine_similarity(content_embedding, &c.embedding);
            (c, sim)
        })
        .collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    for (candidate, similarity) in &ranked {
        let title_hit = fts_hits.contains(&candidate.source_id);

        // Compute tiered threshold based on domain+type overlap
        let domain_match = domain.is_some() && candidate.domain.as_deref() == domain;
        let type_match = memory_type.is_some() && candidate.memory_type.as_deref() == memory_type;

        let threshold = match (domain_match, type_match) {
            (true, true) => config.threshold_exact, // 0.70
            (true, false) | (false, true) => config.threshold_partial, // 0.80
            (false, false) => config.threshold_none, // 0.90
        };

        if *similarity >= threshold {
            signals.fts_title_hit = title_hit;
            signals.embedding_similarity = Some(*similarity);
            let tier = match (domain_match, type_match) {
                (true, true) => "exact",
                (true, false) | (false, true) => "partial",
                (false, false) => "semantic-only",
            };
            log::info!(
                "[topic_match] {} match (tier={}, threshold={:.2}): sim={:.3} title_fts={} source_id={}",
                if title_hit { "title+embedding" } else { "embedding" },
                tier,
                threshold,
                similarity,
                title_hit,
                candidate.source_id,
            );
            return Ok(TopicMatchResult {
                matched_source_id: Some(candidate.source_id.clone()),
                old_content: Some(candidate.content.clone()),
                old_embedding: Some(candidate.embedding.clone()),
                signals,
            });
        }
    }

    Ok(no_match)
}

/// Among `candidates` with `entity_id == eid`, return the one with the
/// highest embedding-cosine similarity to `content_embedding` — but only
/// if that similarity is at least `threshold`. Pure helper, no DB access,
/// no I/O.
///
/// Returns `None` when no candidate shares the entity id, or when the
/// best entity-matched candidate's similarity falls below `threshold`.
fn best_entity_match<'a>(
    candidates: &'a [TopicMatchCandidate],
    eid: &str,
    content_embedding: &[f32],
    threshold: f64,
) -> Option<(&'a TopicMatchCandidate, f64)> {
    candidates
        .iter()
        .filter(|c| c.entity_id.as_deref() == Some(eid))
        .map(|c| (c, cosine_similarity(content_embedding, &c.embedding)))
        .filter(|(_, sim)| *sim >= threshold)
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
}

/// Compute cosine similarity between two f32 embedding vectors.
/// Returns 0.0 for empty or mismatched-length vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f64;
    let mut norm_a = 0.0f64;
    let mut norm_b = 0.0f64;
    for (x, y) in a.iter().zip(b.iter()) {
        let xf = *x as f64;
        let yf = *y as f64;
        dot += xf * yf;
        norm_a += xf * xf;
        norm_b += yf * yf;
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_similarity_identical() {
        let v = vec![1.0f32, 0.0, 0.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_orthogonal() {
        let a = vec![1.0f32, 0.0, 0.0];
        let b = vec![0.0f32, 1.0, 0.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_empty() {
        assert_eq!(cosine_similarity(&[], &[]), 0.0);
    }

    #[test]
    fn cosine_similarity_mismatched() {
        let a = vec![1.0f32, 0.0];
        let b = vec![0.0f32, 1.0, 0.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    // --- best_entity_match ---

    fn candidate(
        source_id: &str,
        entity_id: Option<&str>,
        embedding: Vec<f32>,
    ) -> TopicMatchCandidate {
        TopicMatchCandidate {
            source_id: source_id.into(),
            title: "t".into(),
            content: "c".into(),
            entity_id: entity_id.map(|s| s.into()),
            embedding,
            domain: None,
            memory_type: None,
        }
    }

    #[test]
    fn entity_match_returns_none_when_no_entity_overlap() {
        let candidates = vec![
            candidate("a", Some("Alice"), vec![1.0, 0.0, 0.0]),
            candidate("b", Some("Bob"), vec![1.0, 0.0, 0.0]),
        ];
        let probe = vec![1.0f32, 0.0, 0.0];
        assert!(best_entity_match(&candidates, "Carol", &probe, 0.70).is_none());
    }

    #[test]
    fn entity_match_filters_by_similarity_threshold() {
        // Two captures anchored to the same entity but with orthogonal
        // embeddings — distinct ideas about the same topic. Must NOT match.
        let candidates = vec![candidate("existing", Some("Origin"), vec![1.0, 0.0, 0.0])];
        let probe = vec![0.0f32, 1.0, 0.0]; // orthogonal => sim = 0.0
        assert!(
            best_entity_match(&candidates, "Origin", &probe, 0.70).is_none(),
            "entity match alone must NOT coalesce when content differs (sim < threshold)"
        );
    }

    #[test]
    fn entity_match_returns_high_similarity_candidate() {
        let candidates = vec![candidate("existing", Some("Origin"), vec![1.0, 0.0, 0.0])];
        let probe = vec![1.0f32, 0.0, 0.0]; // identical => sim = 1.0
        let result = best_entity_match(&candidates, "Origin", &probe, 0.70);
        assert!(result.is_some());
        let (matched, sim) = result.unwrap();
        assert_eq!(matched.source_id, "existing");
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn entity_match_picks_best_among_multiple_entity_candidates() {
        // Two existing memories share entity "Origin"; one is a close match,
        // the other is moderately related. Helper must pick the close match,
        // not just the first.
        let candidates = vec![
            candidate("low_sim", Some("Origin"), vec![1.0, 0.0, 0.0]),
            candidate("high_sim", Some("Origin"), vec![0.95, 0.31, 0.0]),
        ];
        let probe = vec![0.95f32, 0.31, 0.0];
        let (matched, _) = best_entity_match(&candidates, "Origin", &probe, 0.70).unwrap();
        assert_eq!(matched.source_id, "high_sim");
    }

    #[test]
    fn entity_match_ignores_candidates_without_entity_id() {
        let candidates = vec![
            candidate("no_entity", None, vec![1.0, 0.0, 0.0]),
            candidate(
                "with_entity",
                Some("Origin"),
                vec![0.5, 0.5, std::f32::consts::FRAC_1_SQRT_2],
            ),
        ];
        let probe = vec![0.5f32, 0.5, std::f32::consts::FRAC_1_SQRT_2];
        let (matched, _) = best_entity_match(&candidates, "Origin", &probe, 0.70).unwrap();
        assert_eq!(matched.source_id, "with_entity");
    }

    /// Regression for 2026-05-11: four atomic /handoff captures all
    /// anchored to entity="Origin" but carrying distinct decisions
    /// previously coalesced into a single in-place upsert (returning the
    /// same source_id `mem_60c6f1b75dd1` for all four). With the
    /// threshold guard, each pair-wise check against the existing memory
    /// must return None when content similarity is low.
    #[test]
    fn distinct_captures_same_entity_do_not_coalesce() {
        let existing = candidate("existing", Some("Origin"), vec![1.0, 0.0, 0.0, 0.0]);
        let candidates = vec![existing];
        // Four distinct embeddings, each orthogonal-ish to the existing one
        let captures = [
            vec![0.0f32, 1.0, 0.0, 0.0],
            vec![0.0f32, 0.0, 1.0, 0.0],
            vec![0.0f32, 0.0, 0.0, 1.0],
            vec![0.5f32, 0.5, 0.5, 0.5], // mixed but still orthogonal to e1
        ];
        for probe in captures.iter() {
            let result = best_entity_match(&candidates, "Origin", probe, 0.70);
            assert!(
                result.is_none(),
                "distinct content sharing entity must not coalesce; sim={:.3}",
                cosine_similarity(probe, &candidates[0].embedding)
            );
        }
    }
}
