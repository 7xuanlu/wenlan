// SPDX-License-Identifier: Apache-2.0
//! T18 — Hierarchical global-context prelude (ship-dark, opt-in).
//!
//! Builds a two-level rollup over the memory corpus: per-bucket summary nodes
//! (keyed on `entities.community_id`, content-derived) plus one root node that
//! summarizes the buckets. At read time the root is always returned and the
//! top vector-matched buckets are prepended to the retrieval context as a
//! `## Corpus Overview` prelude (see `db::search_summary_nodes`).
//!
//! The whole build path is gated behind [`crate::db::global_prelude_enabled`]
//! so a disabled flag means zero build cost and a byte-identical read path.
//!
//! The LLM-generated body is not trusted blindly: before a summary node is
//! written, the candidate body is gated behind a lexical content-token overlap
//! floor (default 0.5, matching the page-distillation faithfulness bench) against
//! the union of its source members. A body that falls below the floor degrades to
//! the deterministic template rather than shipping unverified prose into the
//! always-prepended `## Corpus Overview` prelude. The check is lexical, not
//! semantic (a paraphrased-but-faithful body can fall to the template), mirroring
//! the `eval::page_faithfulness` scope limit.
//!
//! This module holds the pure (DB-free) build helpers so they unit-test without
//! a libSQL connection: the deterministic template fallback, the min-members
//! gate, the content-overlap floor, and the root-provenance union. The DB-touching
//! orchestration (`build_summary_nodes`) lives below and is exercised via the
//! integration tests in `db.rs`.

use crate::db::MemoryDB;
use crate::llm_provider::{LlmProvider, LlmRequest};
use std::collections::BTreeSet;

/// A member memory loaded for a summary bucket.
#[derive(Debug, Clone)]
pub struct SummaryMember {
    pub source_id: String,
    pub title: String,
    pub content: String,
}

/// Minimum members a bucket must have before it earns a summary node. Buckets
/// below this are skipped (a one- or two-memory "community" carries no rollup
/// signal the leaf memories don't already provide). Override with
/// `WENLAN_PRELUDE_MIN_MEMBERS`.
pub const MIN_BUCKET_MEMBERS: usize = 3;

/// Default number of vector-matched buckets returned at read time (the root is
/// always returned on top of this). Override with `WENLAN_PRELUDE_BUCKET_K`.
pub const DEFAULT_BUCKET_K: usize = 3;

/// Read [`MIN_BUCKET_MEMBERS`], honoring `WENLAN_PRELUDE_MIN_MEMBERS`.
pub fn min_bucket_members() -> usize {
    std::env::var("WENLAN_PRELUDE_MIN_MEMBERS")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(MIN_BUCKET_MEMBERS)
}

/// Read [`DEFAULT_BUCKET_K`], honoring `WENLAN_PRELUDE_BUCKET_K`.
pub fn bucket_k() -> usize {
    std::env::var("WENLAN_PRELUDE_BUCKET_K")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(DEFAULT_BUCKET_K)
}

/// True iff a bucket of `member_count` members earns a summary node.
pub fn bucket_qualifies(member_count: usize) -> bool {
    member_count >= min_bucket_members()
}

/// Deterministic, LLM-free bucket summary: a title + body assembled from member
/// titles/contents. Mirrors `narrative::assemble_narrative_template`'s fallback
/// role — used whenever no LLM is available or the LLM call fails/returns
/// garbage, so the build path never emits an empty node or a silent zero
/// (the PR #147 regression class).
///
/// Returns `(title, body)`. Body is non-empty for non-empty input.
pub fn assemble_summary_template(members: &[SummaryMember]) -> (String, String) {
    if members.is_empty() {
        return (String::new(), String::new());
    }

    // Title: first member's title (or a leading content fragment), capped.
    let head = members
        .iter()
        .map(|m| {
            if !m.title.trim().is_empty() {
                m.title.trim()
            } else {
                m.content.trim()
            }
        })
        .find(|s| !s.is_empty())
        .unwrap_or("Untitled");
    // UTF-8-safe truncation (chars().take, never byte-index — repo footgun).
    let title: String = head.chars().take(80).collect();

    // Body: a bulleted digest of member titles/content fragments.
    let mut lines: Vec<String> = Vec::with_capacity(members.len());
    for m in members {
        let text = if !m.title.trim().is_empty() {
            m.title.trim()
        } else {
            m.content.trim()
        };
        if text.is_empty() {
            continue;
        }
        let frag: String = text.chars().take(160).collect();
        lines.push(format!("- {frag}"));
    }
    let body = if lines.is_empty() {
        // All members blank-titled and blank-content: degrade to the title so
        // the node is still non-empty for non-empty input.
        title.clone()
    } else {
        lines.join("\n")
    };
    (title, body)
}

/// Deterministic root summary over the per-bucket titles. Same template shape
/// as the bucket fallback; used when no LLM is available for the root rollup.
pub fn assemble_root_template(bucket_titles: &[String]) -> (String, String) {
    if bucket_titles.is_empty() {
        return (String::new(), String::new());
    }
    let title = "Corpus Overview".to_string();
    let body = bucket_titles
        .iter()
        .filter(|t| !t.trim().is_empty())
        .map(|t| format!("- {}", t.trim()))
        .collect::<Vec<_>>()
        .join("\n");
    let body = if body.is_empty() { title.clone() } else { body };
    (title, body)
}

/// Union of source-memory ids across a set of buckets, deduplicated and sorted
/// (sorted for deterministic insert order / stable tests). The root node's
/// provenance is exactly this union.
pub fn union_sources<'a>(bucket_sources: impl IntoIterator<Item = &'a [String]>) -> Vec<String> {
    let mut set: BTreeSet<String> = BTreeSet::new();
    for srcs in bucket_sources {
        for s in srcs {
            set.insert(s.clone());
        }
    }
    set.into_iter().collect()
}

/// Sanitize an LLM-produced summary body. Returns `Some(cleaned)` only when the
/// output is plausibly a real summary; `None` on empty/garbage so the caller
/// falls back to the deterministic template (never a silent empty node).
pub fn sanitize_llm_body(raw: &str) -> Option<String> {
    let cleaned = crate::llm_provider::strip_think_tags(raw);
    let cleaned = cleaned.trim();
    // Length sanity (mirrors narrative.rs): too short to be a summary -> reject.
    if cleaned.chars().count() < 8 {
        return None;
    }
    Some(cleaned.to_string())
}

/// Minimum fraction of a candidate body's content-tokens that must appear in the
/// union of its source members before the LLM body is trusted over the template.
/// Mirrors the 50% content-token floor documented for the parallel
/// page-distillation faithfulness bench (see AGENTS.md "Page-distillation
/// faithfulness bench"). Lexical, not semantic: a paraphrased-but-faithful body
/// can fall below the floor (it degrades to the template, never to an empty node).
pub const BODY_OVERLAP_FLOOR: f64 = 0.5;

/// Stopwords stripped before content-token overlap scoring. Mirrors the list in
/// `eval::page_faithfulness` so the prelude floor and the page bench share one
/// notion of "content token".
const STOPWORDS: &[&str] = &[
    "with", "from", "that", "this", "these", "those", "have", "been", "will", "would", "could",
    "should", "their", "there", "where", "when", "what", "which", "while", "about", "after",
    "before", "between", "into", "over", "under", "very", "more", "most", "some", "such", "than",
    "then", "they", "them", "your", "yours",
];

/// Content-bearing tokens: lowercased alphanumeric words of length >= 4, minus
/// stopwords. Lifted from `eval::page_faithfulness::content_tokens` so the
/// production prelude path does not depend on the eval (benchmark) module.
fn content_tokens(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .map(|t| t.to_ascii_lowercase())
        .filter(|t| t.len() >= 4 && !STOPWORDS.contains(&t.as_str()))
        .collect()
}

/// Fraction of `body`'s content-tokens that also appear in `source` (the union
/// of member texts). A body with no content-tokens (pure punctuation / all
/// stopwords) is vacuously grounded (returns 1.0) so it never spuriously fails
/// the floor — `sanitize_llm_body` already rejects too-short bodies upstream.
fn body_overlap_fraction(body: &str, source: &str) -> f64 {
    let toks = content_tokens(body);
    if toks.is_empty() {
        return 1.0;
    }
    let source_toks: BTreeSet<String> = content_tokens(source).into_iter().collect();
    let hits = toks.iter().filter(|t| source_toks.contains(*t)).count();
    hits as f64 / toks.len() as f64
}

/// True iff `body` is lexically grounded in `source` at or above
/// [`BODY_OVERLAP_FLOOR`]. Used to gate an LLM-produced summary body against the
/// union of its member source texts before trusting it over the template.
pub fn body_meets_overlap_floor(body: &str, source: &str) -> bool {
    body_overlap_fraction(body, source) >= BODY_OVERLAP_FLOOR
}

/// Orchestrates the T18 build: group eligible memories by `community_id`,
/// summarize qualifying buckets, then summarize the buckets into one root.
/// Writes `summary_nodes` + `summary_node_sources`. Returns the number of nodes
/// written (buckets + root). No-op (returns 0) when the corpus has no
/// qualifying bucket.
///
/// Gate the call site behind [`crate::db::global_prelude_enabled`] so a disabled
/// flag means zero build cost.
pub(crate) async fn build_summary_nodes(
    db: &MemoryDB,
    llm: Option<&dyn LlmProvider>,
) -> Result<usize, crate::error::WenlanError> {
    let _activity =
        db.begin_derived_artifact_write(crate::derived_artifact_state::DerivedArtifact::Summary);
    let buckets = db.load_summary_buckets().await?;
    if buckets.is_empty() {
        return Ok(0);
    }

    let generated_at = chrono::Utc::now().timestamp();

    // Clear prior nodes for a clean rebuild (delete+reinsert; ON DELETE CASCADE
    // wipes summary_node_sources). Cheap relative to the corpus scan.
    db.clear_summary_nodes().await?;

    let mut bucket_titles: Vec<String> = Vec::new();
    let mut bucket_source_sets: Vec<Vec<String>> = Vec::new();
    let mut node_count = 0usize;

    for (community_id, members) in buckets {
        if !bucket_qualifies(members.len()) {
            continue;
        }
        let (title, body) = summarize_bucket(llm, &members).await;
        if body.trim().is_empty() {
            continue;
        }
        let sources: Vec<String> = {
            let mut set: BTreeSet<String> = BTreeSet::new();
            for m in &members {
                set.insert(m.source_id.clone());
            }
            set.into_iter().collect()
        };
        let node_id = format!("sum_b_{community_id}");
        let embedding = db.embed_for_summary(&body)?;
        db.insert_summary_node(
            &node_id,
            0,
            Some(&community_id.to_string()),
            &title,
            &body,
            &embedding,
            sources.len() as i64,
            generated_at,
            &sources,
        )
        .await?;
        bucket_titles.push(title);
        bucket_source_sets.push(sources);
        node_count += 1;
    }

    if node_count == 0 {
        return Ok(0);
    }

    // Root rollup over the bucket titles.
    let root_sources = union_sources(bucket_source_sets.iter().map(|v| v.as_slice()));
    let (root_title, root_body) = summarize_root(llm, &bucket_titles).await;
    if !root_body.trim().is_empty() {
        let embedding = db.embed_for_summary(&root_body)?;
        db.insert_summary_node(
            "sum_root",
            1,
            None,
            &root_title,
            &root_body,
            &embedding,
            root_sources.len() as i64,
            generated_at,
            &root_sources,
        )
        .await?;
        node_count += 1;
    }

    Ok(node_count)
}

/// Summarize one bucket, preferring the LLM and degrading to the deterministic
/// template on absent/failed/garbage LLM output. Never returns an empty body
/// for non-empty input.
async fn summarize_bucket(
    llm: Option<&dyn LlmProvider>,
    members: &[SummaryMember],
) -> (String, String) {
    let (tmpl_title, tmpl_body) = assemble_summary_template(members);
    let llm = match llm {
        Some(l) if l.is_available() => l,
        _ => return (tmpl_title, tmpl_body),
    };
    let prompt = members
        .iter()
        .map(|m| {
            let t = if !m.title.trim().is_empty() {
                m.title.trim()
            } else {
                m.content.trim()
            };
            format!("- {t}")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let req = LlmRequest {
        system_prompt: Some(
            "Summarize these related memories into a single short paragraph (2-3 sentences) \
             describing the shared theme. Output only the summary prose, no preamble."
                .into(),
        ),
        user_prompt: prompt,
        max_tokens: 200,
        temperature: 0.3,
        label: Some("summary_bucket".into()),
        timeout_secs: None,
    };
    // Union of member source texts (titles + content) the body must be grounded in.
    let member_source: String = members
        .iter()
        .map(|m| format!("{} {}", m.title, m.content))
        .collect::<Vec<_>>()
        .join("\n");
    // Wrap in a timeout so a hung provider degrades to the template rather than
    // stalling the refinery (no silent-zero: failure -> deterministic body).
    match tokio::time::timeout(std::time::Duration::from_secs(15), llm.generate(req)).await {
        Ok(Ok(raw)) => match sanitize_llm_body(&raw) {
            // Gate the LLM body behind a content-overlap floor against the member
            // source texts; reject hallucinated prose and fall back to the template.
            Some(body) if body_meets_overlap_floor(&body, &member_source) => (tmpl_title, body),
            Some(_) => {
                log::debug!(
                    "[summary] bucket LLM body below overlap floor ({}); using template",
                    BODY_OVERLAP_FLOOR
                );
                (tmpl_title, tmpl_body)
            }
            None => (tmpl_title, tmpl_body),
        },
        Ok(Err(e)) => {
            log::warn!("[summary] bucket LLM failed: {e}; using template");
            (tmpl_title, tmpl_body)
        }
        Err(_) => {
            log::warn!("[summary] bucket LLM timed out; using template");
            (tmpl_title, tmpl_body)
        }
    }
}

/// Summarize the bucket titles into one root paragraph. Same degrade contract
/// as [`summarize_bucket`].
async fn summarize_root(
    llm: Option<&dyn LlmProvider>,
    bucket_titles: &[String],
) -> (String, String) {
    let (tmpl_title, tmpl_body) = assemble_root_template(bucket_titles);
    let llm = match llm {
        Some(l) if l.is_available() => l,
        _ => return (tmpl_title, tmpl_body),
    };
    let prompt = bucket_titles
        .iter()
        .map(|t| format!("- {t}"))
        .collect::<Vec<_>>()
        .join("\n");
    let req = LlmRequest {
        system_prompt: Some(
            "These are the main themes in a personal knowledge base. Write a 1-2 sentence \
             overview of what this corpus is about. Output only the overview prose."
                .into(),
        ),
        user_prompt: prompt,
        max_tokens: 150,
        temperature: 0.3,
        label: Some("summary_root".into()),
        timeout_secs: None,
    };
    // Union of bucket titles the root body must be grounded in (the root is
    // prompted only on these titles, so they are its source members).
    let root_source: String = bucket_titles.join("\n");
    match tokio::time::timeout(std::time::Duration::from_secs(15), llm.generate(req)).await {
        Ok(Ok(raw)) => match sanitize_llm_body(&raw) {
            // Gate the LLM body behind a content-overlap floor against the bucket
            // titles; reject hallucinated prose and fall back to the template.
            Some(body) if body_meets_overlap_floor(&body, &root_source) => (tmpl_title, body),
            Some(_) => {
                log::debug!(
                    "[summary] root LLM body below overlap floor ({}); using template",
                    BODY_OVERLAP_FLOOR
                );
                (tmpl_title, tmpl_body)
            }
            None => (tmpl_title, tmpl_body),
        },
        Ok(Err(e)) => {
            log::warn!("[summary] root LLM failed: {e}; using template");
            (tmpl_title, tmpl_body)
        }
        Err(_) => {
            log::warn!("[summary] root LLM timed out; using template");
            (tmpl_title, tmpl_body)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(id: &str, title: &str, content: &str) -> SummaryMember {
        SummaryMember {
            source_id: id.to_string(),
            title: title.to_string(),
            content: content.to_string(),
        }
    }

    /// Template fallback with `llm=None` must produce a deterministic, non-empty
    /// body from member titles (mirrors assemble_narrative_template fallback).
    #[test]
    fn test_bucket_summary_template_fallback_when_no_llm() {
        let members = vec![
            member("m1", "Rust ownership", "borrow checker enforces moves"),
            member("m2", "Rust lifetimes", "elision rules"),
        ];
        let (title, body) = assemble_summary_template(&members);
        assert!(
            !title.is_empty(),
            "title must be non-empty for non-empty input"
        );
        assert!(
            !body.is_empty(),
            "body must be non-empty for non-empty input"
        );
        assert!(body.contains("Rust ownership"));
        assert!(body.contains("Rust lifetimes"));
        // Empty input -> empty (graceful).
        let (et, eb) = assemble_summary_template(&[]);
        assert!(et.is_empty() && eb.is_empty());
    }

    /// A bucket with < MIN_BUCKET_MEMBERS produces no node (gate helper).
    #[test]
    fn test_summary_build_skips_buckets_below_min_members() {
        assert!(!bucket_qualifies(0));
        assert!(!bucket_qualifies(MIN_BUCKET_MEMBERS - 1));
        assert!(bucket_qualifies(MIN_BUCKET_MEMBERS));
        assert!(bucket_qualifies(MIN_BUCKET_MEMBERS + 5));
    }

    /// Root node source set == union of its bucket nodes' source sets.
    #[test]
    fn test_root_summary_provenance_is_union_of_buckets() {
        let b1 = vec!["m1".to_string(), "m2".to_string()];
        let b2 = vec!["m2".to_string(), "m3".to_string()];
        let union = union_sources([b1.as_slice(), b2.as_slice()]);
        assert_eq!(union, vec!["m1", "m2", "m3"]); // sorted + deduped
    }

    /// Garbage / empty LLM output is rejected so the caller uses the template
    /// (no silent-zero / empty node — PR #147 class).
    #[test]
    fn test_sanitize_llm_body_rejects_garbage() {
        assert_eq!(sanitize_llm_body(""), None);
        assert_eq!(sanitize_llm_body("   "), None);
        assert_eq!(sanitize_llm_body("ok"), None); // too short
        assert_eq!(
            sanitize_llm_body("<think>noise</think>  "),
            None,
            "think-only output must reject"
        );
        assert_eq!(
            sanitize_llm_body("This corpus is about Rust systems programming."),
            Some("This corpus is about Rust systems programming.".to_string())
        );
    }

    /// Min-members + bucket-k env overrides default when unset.
    #[test]
    fn test_env_overrides_default_and_parse() {
        assert_eq!(min_bucket_members(), MIN_BUCKET_MEMBERS);
        assert_eq!(bucket_k(), DEFAULT_BUCKET_K);
    }

    /// Root template is non-empty for non-empty bucket titles, empty otherwise.
    #[test]
    fn test_root_template_shape() {
        let (t, b) = assemble_root_template(&["Rust".into(), "Cooking".into()]);
        assert_eq!(t, "Corpus Overview");
        assert!(b.contains("Rust") && b.contains("Cooking"));
        let (et, eb) = assemble_root_template(&[]);
        assert!(et.is_empty() && eb.is_empty());
    }

    /// A candidate LLM body whose content-tokens are absent from the member
    /// source texts falls below the overlap floor, so the gated path keeps the
    /// deterministic template body (hallucination guard).
    #[test]
    fn summary_body_below_overlap_floor_falls_back_to_template() {
        let members = vec![
            member("m1", "Rust ownership", "borrow checker enforces moves"),
            member("m2", "Rust lifetimes", "elision rules"),
        ];
        let (_tmpl_title, tmpl_body) = assemble_summary_template(&members);
        let member_source: String = members
            .iter()
            .map(|m| format!("{} {}", m.title, m.content))
            .collect::<Vec<_>>()
            .join("\n");
        // Hallucinated body: real prose, passes sanitize, but none of its content
        // tokens appear in the members.
        let candidate = "Photosynthesis converts sunlight into glucose inside chloroplasts.";
        assert!(
            sanitize_llm_body(candidate).is_some(),
            "candidate must survive sanitize so the overlap floor is what rejects it"
        );
        assert!(
            !body_meets_overlap_floor(candidate, &member_source),
            "candidate with no member content-tokens must fail the floor"
        );
        let chosen = if body_meets_overlap_floor(candidate, &member_source) {
            candidate.to_string()
        } else {
            tmpl_body.clone()
        };
        assert_eq!(chosen, tmpl_body);
    }

    /// A candidate LLM body whose content-tokens are mostly drawn from the member
    /// source texts clears the overlap floor, so the gated path keeps the LLM body.
    #[test]
    fn summary_body_above_overlap_floor_is_kept() {
        let members = vec![
            member("m1", "Rust ownership", "borrow checker enforces moves"),
            member("m2", "Rust lifetimes", "elision rules govern references"),
        ];
        let (_tmpl_title, tmpl_body) = assemble_summary_template(&members);
        let member_source: String = members
            .iter()
            .map(|m| format!("{} {}", m.title, m.content))
            .collect::<Vec<_>>()
            .join("\n");
        // Faithful body: every content token is drawn from the members.
        let candidate =
            "Rust ownership and lifetimes: the borrow checker enforces moves and elision rules.";
        assert!(sanitize_llm_body(candidate).is_some());
        assert!(
            body_meets_overlap_floor(candidate, &member_source),
            "candidate built from member tokens must clear the floor"
        );
        let chosen = if body_meets_overlap_floor(candidate, &member_source) {
            candidate.to_string()
        } else {
            tmpl_body.clone()
        };
        assert_eq!(chosen, candidate);
        assert_ne!(chosen, tmpl_body);
    }
}
