// SPDX-License-Identifier: Apache-2.0
//! Pseudo-relevance feedback (PRF): answer-as-next-query iterative retrieval.
//!
//! PRF generates a short *draft answer* from the current top-K retrieved
//! snippets, then feeds that draft back as the next retrieval query and
//! RRF-merges the new pool into the accumulated candidate set. The original
//! literal query is always RRF round 0, which weights the literal query as one
//! equal RRF stream; a strong off-topic draft answer can still displace
//! mid-rank literal hits — this is recall expansion, not a protection guarantee.
//!
//! Distinct from query *expansion* (`search_memory_expanded`): expansion
//! paraphrases the literal query up front with no draft answer and no loop.
//! PRF reads the *retrieved content*, drafts an answer, and iterates until the
//! candidate set converges (no new ids) or the round budget is exhausted.
//!
//! All helpers here are pure + synchronous (no DB, no async, no axum/tauri) so
//! the parse/keying/truncation contracts are unit-tested in isolation. The
//! async orchestration lives in `db::search_memory_prf`.

use crate::llm_provider::LlmRequest;
use origin_types::SearchResult;
use std::collections::HashSet;

/// Hard ceiling on PRF rounds. Matches Cognee's `max_iter` ceiling and bounds
/// worst-case cost (each round = 1 LLM generate + 1 hybrid search).
const PRF_ROUNDS_MAX: usize = 4;

const PRF_SYSTEM_PROMPT: &str = "Using ONLY the context below, write a 1-2 sentence direct answer to the question. Do not invent facts. If insufficient, give best partial guess using context terms.";

/// Resolve the PRF round budget from `ORIGIN_PRF_ROUNDS`.
///
/// Default `0` (unset, empty, or unparseable) — when `0` the caller skips the
/// feedback loop entirely and `search_memory_prf` is id-order-identical to a
/// plain `search_memory` when rounds==0 (scores are re-derived as
/// `1/(60+rank)`; raw_score is not preserved). Parsed values are clamped to
/// `<= PRF_ROUNDS_MAX` so an
/// operator typo cannot blow up latency/cost. Parse failure is the default,
/// never a panic (mirrors `db::page_channel_limit`'s parse-with-default).
pub(crate) fn prf_rounds() -> usize {
    std::env::var("ORIGIN_PRF_ROUNDS")
        .ok()
        .and_then(|s| s.trim().parse::<usize>().ok())
        .unwrap_or(0)
        .min(PRF_ROUNDS_MAX)
}

/// Collect the candidate-set keys from a result list, keyed on `r.id` (the
/// chunk id, matching the RRF dedup key used by `search_memory_*`). Keying on
/// `id` (not `source_id`) means two chunks of the same document are distinct
/// candidates, consistent with how the RRF merge dedups.
pub(crate) fn candidate_set(results: &[SearchResult]) -> HashSet<String> {
    results.iter().map(|r| r.id.clone()).collect()
}

/// Convergence test: the loop has converged once the newest feedback round
/// surfaced no ids the accumulated set didn't already contain — i.e. `cur` is a
/// subset of `prev`. Boolean subset (not Jaccard) for v1 minimal correctness.
/// Both-empty returns `true` (empty set is a subset of empty set), so a feedback
/// round that returns nothing stops the loop instead of spinning.
pub(crate) fn converged(prev: &HashSet<String>, cur: &HashSet<String>) -> bool {
    cur.is_subset(prev)
}

/// Build feedback snippets from the current best result list: take the first
/// `top_k` non-empty results and truncate each `content` to `max_chars`
/// characters. Truncation is UTF-8-safe (`chars().take`, never byte-slice per
/// AGENTS.md) so a multi-byte char at the boundary cannot panic. Empty-content
/// results are skipped (they contribute no feedback signal).
pub(crate) fn feedback_snippets(
    results: &[SearchResult],
    top_k: usize,
    max_chars: usize,
) -> Vec<String> {
    results
        .iter()
        .filter(|r| !r.content.is_empty())
        .take(top_k)
        .map(|r| r.content.chars().take(max_chars).collect())
        .collect()
}

/// Build the draft-answer LLM request from the original question + feedback
/// snippets. The draft is FREE TEXT (no JSON parse), which removes the
/// silent-zero parse-failure class (PR #147) entirely — a degenerate draft is a
/// blank/whitespace string the caller treats as "no feedback" and degrades.
///
/// `max_tokens = 128` (a 1-2 sentence answer) and `temperature = 0.2` (low, to
/// keep the draft anchored to the context rather than inventing).
pub(crate) fn draft_answer_request(original: &str, snippets: &[String]) -> LlmRequest {
    let context = snippets
        .iter()
        .map(|s| format!("- {s}"))
        .collect::<Vec<_>>()
        .join("\n");
    LlmRequest {
        system_prompt: Some(PRF_SYSTEM_PROMPT.to_string()),
        user_prompt: format!("Question: {original}\n\nContext:\n{context}"),
        max_tokens: 128,
        temperature: 0.2,
        label: None,
        timeout_secs: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sr(id: &str, source_id: &str, content: &str) -> SearchResult {
        SearchResult {
            id: id.to_string(),
            content: content.to_string(),
            source: "memory".to_string(),
            source_id: source_id.to_string(),
            title: String::new(),
            url: None,
            chunk_index: 0,
            last_modified: 0,
            score: 0.0,
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
            importance: None,
            entity_name: None,
            quality: None,
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

    #[test]
    fn prf_rounds_defaults_to_zero_when_unset() {
        temp_env::with_var("ORIGIN_PRF_ROUNDS", None::<&str>, || {
            assert_eq!(prf_rounds(), 0);
        });
    }

    #[test]
    fn prf_rounds_parses_value() {
        temp_env::with_var("ORIGIN_PRF_ROUNDS", Some("2"), || {
            assert_eq!(prf_rounds(), 2);
        });
        // parse failure = default 0, never a panic
        temp_env::with_var("ORIGIN_PRF_ROUNDS", Some("garbage"), || {
            assert_eq!(prf_rounds(), 0);
        });
    }

    #[test]
    fn prf_rounds_clamps_to_max() {
        temp_env::with_var("ORIGIN_PRF_ROUNDS", Some("99"), || {
            assert_eq!(prf_rounds(), 4);
        });
    }

    #[test]
    fn candidate_set_keys_on_id() {
        // Two results with the SAME source_id but DISTINCT ids must both appear:
        // proves keying on id, not source_id (matches the RRF dedup key).
        let results = vec![sr("a", "doc1", "alpha"), sr("b", "doc1", "beta")];
        let set = candidate_set(&results);
        let expected: HashSet<String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
        assert_eq!(set, expected);
    }

    #[test]
    fn converged_true_when_no_new_ids() {
        let s = |xs: &[&str]| -> HashSet<String> { xs.iter().map(|x| x.to_string()).collect() };
        // cur subset of prev -> converged
        assert!(converged(&s(&["a", "b", "c"]), &s(&["a", "b"])));
        // cur has a new id (d) -> not converged
        assert!(!converged(&s(&["a", "b"]), &s(&["a", "b", "d"])));
        // both empty -> converged (empty subset of empty)
        assert!(converged(&s(&[]), &s(&[])));
    }

    #[test]
    fn feedback_snippets_truncates_utf8_safe() {
        // Multi-byte chars (each 'é' is 2 bytes) at a truncation boundary that
        // lands mid-byte-sequence must NOT panic and must return valid UTF-8.
        let content = "éééééééééé"; // 10 chars, 20 bytes
        let results = vec![sr("a", "d", content)];
        let snippets = feedback_snippets(&results, 5, 3);
        assert_eq!(snippets.len(), 1);
        // char-len <= max_chars (NOT byte-len)
        assert_eq!(snippets[0].chars().count(), 3);
        assert_eq!(snippets[0], "ééé");
    }

    #[test]
    fn feedback_snippets_respects_top_k_and_skips_empty() {
        let results = vec![
            sr("a", "d", "alpha"),
            sr("b", "d", ""), // empty -> skipped
            sr("c", "d", "gamma"),
            sr("d", "d", "delta"),
            sr("e", "d", "epsilon"),
        ];
        let snippets = feedback_snippets(&results, 3, 200);
        // at most 3 non-empty snippets, none empty
        assert!(snippets.len() <= 3);
        assert!(snippets.iter().all(|s| !s.is_empty()));
        // first three NON-EMPTY are alpha, gamma, delta
        assert_eq!(snippets, vec!["alpha", "gamma", "delta"]);
    }

    #[test]
    fn draft_answer_request_contains_question_and_context() {
        let snippets = vec!["alpha fact".to_string(), "beta fact".to_string()];
        let req = draft_answer_request("what is the thing?", &snippets);
        assert!(req.user_prompt.contains("what is the thing?"));
        assert!(req.user_prompt.contains("alpha fact"));
        assert!(req.user_prompt.contains("beta fact"));
        assert_eq!(req.max_tokens, 128);
        assert_eq!(req.temperature, 0.2);
        assert!(req.system_prompt.is_some());
    }
}
