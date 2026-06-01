// SPDX-License-Identifier: Apache-2.0
//! Chain-of-Thought iterative retrieval helpers (T5).
//!
//! Pure, DB-free helpers for the `db::search_memory_iterative` retrieve-reason-
//! retrieve loop: draft an answer from the current context, validate it against
//! that context, and — only when the validator judges the answer incomplete —
//! emit a single follow-up question that targets the missing fact. The follow-up
//! re-retrieves, the new evidence is RRF-merged into the pool, and the loop
//! re-answers (bounded by `max_iter`).
//!
//! Distinct from `search_memory_expanded` (paraphrase one clause up-front, ONE
//! call, no draft answer) and `search_memory_decomposed` (split clauses up-front,
//! no validation gate). The validation gate IS the difference: re-retrieve only
//! when the LLM says the current evidence is insufficient.
//!
//! Every JSON parse degrades to the SAFE default (`Validation::Complete`, i.e.
//! stop looping) on malformed / empty / missing-key output — the PR #147
//! silent-zero class is guarded by the unit tests below.

use crate::llm_provider::LlmRequest;
use origin_types::SearchResult;
use std::collections::HashMap;

/// Outcome of validating a draft answer against the retrieved context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Validation {
    /// The draft is supported by the context (or the validator output was
    /// unusable). Stop the loop.
    Complete,
    /// The draft is missing a fact; `String` is a single follow-up question
    /// to re-retrieve for. Never empty (empty -> `Complete`).
    Followup(String),
}

/// Parse the validator LLM output into a [`Validation`].
///
/// Expected shape: `{"complete": bool, "followup": Option<String>}`. Defensive
/// against every malformed-output class:
/// - non-JSON / no `{...}` -> `Complete` (safe default: stop looping)
/// - `{"complete": true}` -> `Complete`
/// - `{"complete": false, "followup": "q"}` -> `Followup("q")`
/// - `{"complete": false}` (missing followup) -> `Complete`
/// - `{"complete": false, "followup": ""}` (blank) -> `Complete`
/// - surrounding prose (`Sure! {...}`) -> still parses via `extract_json`
///
/// `<think>...</think>` blocks are stripped first (Qwen3 safety net).
pub(crate) fn parse_validation(raw: &str) -> Validation {
    let stripped = crate::engine::strip_think_tags(raw);
    let Some(json) = crate::engine::extract_json(&stripped) else {
        log::warn!("[cot] no JSON object in validation output; treating as complete");
        return Validation::Complete;
    };

    #[derive(serde::Deserialize)]
    struct Raw {
        #[serde(default)]
        complete: bool,
        #[serde(default)]
        followup: Option<String>,
    }

    match serde_json::from_str::<Raw>(json) {
        Ok(parsed) => {
            if parsed.complete {
                return Validation::Complete;
            }
            match parsed.followup {
                Some(q) if !q.trim().is_empty() => Validation::Followup(q.trim().to_string()),
                // complete:false but no usable follow-up -> nothing to re-retrieve.
                _ => Validation::Complete,
            }
        }
        Err(e) => {
            log::warn!("[cot] validation JSON parse failed: {e}; treating as complete");
            Validation::Complete
        }
    }
}

/// Render the top-`top_k` results into a numbered context block for prompts.
/// UTF-8 safe truncation via `chars().take(N)` (never byte-indexes).
pub(crate) fn format_context(results: &[SearchResult], top_k: usize) -> String {
    const PER_SNIPPET_CHARS: usize = 500;
    let mut out = String::new();
    for (i, r) in results.iter().take(top_k).enumerate() {
        let snippet: String = r.content.chars().take(PER_SNIPPET_CHARS).collect();
        out.push_str(&format!("{}. {}\n", i + 1, snippet));
    }
    out
}

/// Build the draft-answer prompt: answer the query using ONLY the retrieved
/// context, in 1-3 sentences. Mirrors the answer-quality system prompt.
pub(crate) fn build_draft_prompt(query: &str, context: &str) -> LlmRequest {
    LlmRequest {
        system_prompt: Some(
            "Answer the question using ONLY the numbered context below. \
             Be concise: 1-3 sentences. If the context does not contain the \
             answer, say what is missing."
                .into(),
        ),
        user_prompt: format!("Context:\n{context}\nQuestion: {query}\nAnswer:"),
        max_tokens: 200,
        temperature: 0.1,
        label: Some("cot_draft".into()),
        timeout_secs: None,
    }
}

/// Build the validation prompt: critique the draft against the context and
/// return JSON-only `{"complete": true}` or
/// `{"complete": false, "followup": "<one question>"}`.
pub(crate) fn build_validation_prompt(query: &str, draft: &str, context: &str) -> LlmRequest {
    LlmRequest {
        system_prompt: Some(
            "You validate whether a draft answer is fully supported by the context. \
             If the draft answers the question and every claim is grounded in the \
             context, output exactly {\"complete\": true}. If a fact is missing, \
             output {\"complete\": false, \"followup\": \"<one short question that \
             would retrieve the missing fact>\"}. Output ONLY the JSON object."
                .into(),
        ),
        user_prompt: format!(
            "Question: {query}\nDraft answer: {draft}\nContext:\n{context}\nJSON:"
        ),
        max_tokens: 256,
        temperature: 0.2,
        label: Some("cot_validate".into()),
        timeout_secs: None,
    }
}

/// RRF-merge several ranked result lists into one deduped, score-desc pool.
///
/// Verbatim Reciprocal Rank Fusion block lifted from
/// `search_memory_cross_rerank` (`1/(60+rank)` accumulation, dedup by `id`,
/// seed the first list's `score` then add RRF mass). The first list in `lists`
/// is the round-0 pool and SEEDS the score map (preserving its multiplied
/// score), so a high-rank round-0 hit stays above a low-rank round-1-only hit
/// (round-0 floor invariant). Later lists contribute only `1/(60+rank)`.
pub(crate) fn merge_pools(lists: Vec<Vec<SearchResult>>) -> Vec<SearchResult> {
    let mut score_map: HashMap<String, f32> = HashMap::new();
    let mut result_map: HashMap<String, SearchResult> = HashMap::new();

    let mut first = true;
    for ranked in lists {
        if first {
            // Seed from the round-0 pool: keep its multiplied score, add RRF mass.
            for (rank, r) in ranked.into_iter().enumerate() {
                let rrf_score = 1.0 / (60.0 + rank as f32);
                *score_map.entry(r.id.clone()).or_default() += r.score + rrf_score;
                result_map.entry(r.id.clone()).or_insert(r);
            }
            first = false;
        } else {
            for (rank, r) in ranked.into_iter().enumerate() {
                let rrf_score = 1.0 / (60.0 + rank as f32);
                *score_map.entry(r.id.clone()).or_default() += rrf_score;
                result_map.entry(r.id.clone()).or_insert(r);
            }
        }
    }

    let mut merged: Vec<SearchResult> = result_map
        .into_values()
        .map(|mut r| {
            r.score = *score_map.get(&r.id).unwrap_or(&0.0);
            r
        })
        .collect();
    merged.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.source_id.cmp(&b.source_id))
    });
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sr(id: &str, content: &str, score: f32) -> SearchResult {
        SearchResult {
            id: id.to_string(),
            content: content.to_string(),
            source: "memory".to_string(),
            source_id: id.to_string(),
            title: String::new(),
            url: None,
            chunk_index: 0,
            last_modified: 0,
            score,
            chunk_type: None,
            language: None,
            semantic_unit: None,
            memory_type: None,
            space: None,
            source_agent: None,
            confidence: None,
            confirmed: None,
            stability: None,
            supersedes: None,
            summary: None,
            entity_id: None,
            entity_name: None,
            quality: None,
            importance: None,
            is_archived: false,
            is_recap: false,
            structured_fields: None,
            retrieval_cue: None,
            source_text: None,
            raw_score: 0.0,
            version: 0,
            pending_revision: false,
            merged_from: None,
            last_delta_summary: None,
        }
    }

    // ── parse_validation (tests 1-6) ──────────────────────────────────────────

    #[test]
    fn parse_validation_complete() {
        assert_eq!(
            parse_validation(r#"{"complete": true}"#),
            Validation::Complete
        );
    }

    #[test]
    fn parse_validation_followup() {
        assert_eq!(
            parse_validation(r#"{"complete": false, "followup": "where did Alice move?"}"#),
            Validation::Followup("where did Alice move?".to_string())
        );
    }

    #[test]
    fn parse_validation_followup_with_surrounding_prose() {
        // Guards PR #147 silent-zero class: prose around the JSON must not break parsing.
        assert_eq!(
            parse_validation(r#"Sure! {"complete": false, "followup": "x"}"#),
            Validation::Followup("x".to_string())
        );
    }

    #[test]
    fn parse_validation_malformed_returns_complete() {
        // Safe default: stop looping. Must NOT panic.
        assert_eq!(parse_validation("not json"), Validation::Complete);
    }

    #[test]
    fn parse_validation_empty_followup_treated_complete() {
        // No empty re-retrieve.
        assert_eq!(
            parse_validation(r#"{"complete":false,"followup":""}"#),
            Validation::Complete
        );
    }

    #[test]
    fn parse_validation_complete_false_missing_followup() {
        // Defensive: complete:false with no followup key -> Complete.
        assert_eq!(
            parse_validation(r#"{"complete":false}"#),
            Validation::Complete
        );
    }

    #[test]
    fn parse_validation_strips_think_tags() {
        assert_eq!(
            parse_validation(r#"<think>hmm</think>{"complete": true}"#),
            Validation::Complete
        );
    }

    // ── prompt builders (tests 7-8) ───────────────────────────────────────────

    #[test]
    fn build_draft_prompt_includes_context_and_query() {
        let ctx = format_context(&[sr("m1", "Alice lives in Paris", 1.0)], 5);
        let req = build_draft_prompt("Where does Alice live?", &ctx);
        assert!(req.user_prompt.contains("Where does Alice live?"));
        assert!(req.user_prompt.contains("Alice lives in Paris"));
        let sys = req.system_prompt.unwrap().to_lowercase();
        assert!(
            sys.contains("only"),
            "system prompt must instruct answer-using-only-context"
        );
    }

    #[test]
    fn build_validation_prompt_includes_draft_and_query() {
        let ctx = format_context(&[sr("m1", "ctx fact", 1.0)], 5);
        let req = build_validation_prompt("orig query", "the draft answer", &ctx);
        assert!(req.user_prompt.contains("orig query"));
        assert!(req.user_prompt.contains("the draft answer"));
    }

    // ── merge_pools (tests 9-10) ──────────────────────────────────────────────

    #[test]
    fn merge_pools_rrf_dedups_by_id() {
        // Two lists share id "shared"; it must appear once with summed mass.
        let list_a = vec![sr("shared", "c", 0.0), sr("a_only", "c", 0.0)];
        let list_b = vec![sr("shared", "c", 0.0), sr("b_only", "c", 0.0)];
        let merged = merge_pools(vec![list_a, list_b]);
        let shared_count = merged.iter().filter(|r| r.id == "shared").count();
        assert_eq!(shared_count, 1, "shared id must be deduped to one row");
        // shared: round0 rank0 (1/60) + round1 rank0 (1/60) = 2/60.
        let shared = merged.iter().find(|r| r.id == "shared").unwrap();
        assert!(
            (shared.score - (2.0 / 60.0)).abs() < 1e-6,
            "shared score should be summed 1/(60+rank): got {}",
            shared.score
        );
        // Ordering is score-desc: shared (highest mass) first.
        assert_eq!(merged[0].id, "shared");
    }

    #[test]
    fn merge_pools_preserves_round0_floor() {
        // High-rank round-0 hit (rank 0, score seeded high) must stay above a
        // low-rank round-1-only hit.
        let round0 = vec![sr("r0_top", "c", 1.0), sr("r0_low", "c", 0.5)];
        // round-1 introduces a brand-new id only at a low rank.
        let mut round1 = Vec::new();
        for i in 0..5 {
            round1.push(sr(&format!("r1_filler_{i}"), "c", 0.0));
        }
        round1.push(sr("r1_new", "c", 0.0)); // rank 5 -> 1/65
        let merged = merge_pools(vec![round0, round1]);
        let pos_top = merged.iter().position(|r| r.id == "r0_top").unwrap();
        let pos_new = merged.iter().position(|r| r.id == "r1_new").unwrap();
        assert!(
            pos_top < pos_new,
            "round-0 top hit must outrank a low-rank round-1-only hit"
        );
    }

    #[test]
    fn format_context_utf8_safe_truncation() {
        // Multi-byte content must not panic on truncation.
        let big = "é".repeat(1000);
        let out = format_context(&[sr("m1", &big, 1.0)], 1);
        assert!(out.starts_with("1. "));
    }
}
