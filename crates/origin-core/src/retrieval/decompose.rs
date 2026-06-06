// SPDX-License-Identifier: Apache-2.0
//! Query decomposition: split a compositional query into independent factual
//! subqueries so each clause's evidence is retrieved separately, then RRF-merged.
//!
//! Distinct from query *expansion* (`search_memory_expanded`), which rewrites the
//! whole query into paraphrases. Decomposition splits "which projects did Alice
//! work on after joining Acme" into independent clauses whose facts live in
//! different memories — the multi-hop case a single embedding starves.

use crate::llm_provider::{LlmProvider, LlmRequest};
use std::sync::Arc;

const DECOMP_SYSTEM_PROMPT: &str = "Split this question into the minimal set of INDEPENDENT factual subqueries needed to answer it. If the question is atomic, return a single-element array. Output ONLY a JSON array of strings.";

/// Parse the LLM output (expected: a JSON array of strings) into a subquery list,
/// always prefixed with the original query so the worst case degrades to a plain
/// single-query search. Pure + synchronous so the parse contract is unit-tested
/// in isolation.
///
/// - empty / malformed / no-array output -> just `[original]`
/// - caps the returned list at `cap` total (including the original)
/// - drops subqueries that exactly match the original (case-insensitive) or are blank
pub(crate) fn parse_subqueries(output: &str, original: &str, cap: usize) -> Vec<String> {
    let mut out = vec![original.to_string()];
    let room = cap.saturating_sub(1);
    if room == 0 {
        return out;
    }
    let (Some(si), Some(ei)) = (output.find('['), output.rfind(']')) else {
        log::warn!("[decompose] no JSON array in output");
        return out;
    };
    if ei <= si {
        return out;
    }
    match serde_json::from_str::<Vec<String>>(&output[si..=ei]) {
        Ok(subs) if !subs.is_empty() => {
            for s in subs {
                // out always holds the original first, so out.len() - 1 == subs added.
                if out.len() > room {
                    break;
                }
                let t = s.trim();
                if !t.is_empty() && !t.eq_ignore_ascii_case(original) {
                    out.push(t.to_string());
                }
            }
        }
        Ok(_) => log::warn!("[decompose] empty subquery array"),
        Err(e) => log::warn!("[decompose] JSON parse failed: {e}"),
    }
    out
}

/// Decompose `query` into independent subqueries via the LLM. Returns just the
/// original query when no LLM is available or the call errors/times out
/// (graceful degradation — same contract as `search_memory_expanded`).
pub(crate) async fn decompose_query(
    llm: Option<&Arc<dyn LlmProvider>>,
    query: &str,
    cap: usize,
) -> Vec<String> {
    let Some(llm) = llm else {
        return vec![query.to_string()];
    };
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        llm.generate(LlmRequest {
            system_prompt: Some(DECOMP_SYSTEM_PROMPT.into()),
            user_prompt: query.to_string(),
            max_tokens: 256,
            temperature: 0.2,
            label: Some("query_decompose".into()),
            timeout_secs: None,
        }),
    )
    .await;
    match result {
        Ok(Ok(output)) => parse_subqueries(&output, query, cap),
        Ok(Err(e)) => {
            log::warn!("[decompose] LLM failed: {e}");
            vec![query.to_string()]
        }
        // Timeout arm: identical degradation to the error arm. Not unit-tested
        // (simulating a >10s elapse in a unit test is impractical); mirrors
        // search_memory_expanded which leaves the same arm untested.
        Err(_) => {
            log::warn!("[decompose] timed out");
            vec![query.to_string()]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm_provider::{LlmBackend, LlmError};
    use async_trait::async_trait;

    struct MockLlm {
        response: Result<String, ()>,
    }

    #[async_trait]
    impl LlmProvider for MockLlm {
        async fn generate(&self, _req: LlmRequest) -> Result<String, LlmError> {
            self.response
                .clone()
                .map_err(|_| LlmError::InferenceFailed("mock".into()))
        }
        fn is_available(&self) -> bool {
            true
        }
        fn name(&self) -> &str {
            "mock"
        }
        fn backend(&self) -> LlmBackend {
            LlmBackend::OnDevice
        }
    }

    fn arc(resp: Result<&str, ()>) -> Arc<dyn LlmProvider> {
        Arc::new(MockLlm {
            response: resp.map(|s| s.to_string()),
        })
    }

    #[test]
    fn parse_subqueries_clean() {
        let v = parse_subqueries(r#"["a","b"]"#, "orig", 4);
        assert_eq!(v, vec!["orig", "a", "b"]);
    }

    #[test]
    fn parse_subqueries_with_noise() {
        let v = parse_subqueries(r#"sure: ["a"] done"#, "orig", 4);
        assert_eq!(v, vec!["orig", "a"]);
    }

    #[test]
    fn parse_subqueries_empty_array() {
        // silent-zero guard: empty array must degrade to original only.
        let v = parse_subqueries("[]", "orig", 4);
        assert_eq!(v, vec!["orig"]);
    }

    #[test]
    fn parse_subqueries_malformed() {
        // PR #147 silent-zero class: malformed JSON must not panic, degrade to original.
        let v = parse_subqueries("[not valid json", "orig", 4);
        assert_eq!(v, vec!["orig"]);
    }

    #[test]
    fn parse_subqueries_no_array() {
        let v = parse_subqueries("no brackets here", "orig", 4);
        assert_eq!(v, vec!["orig"]);
    }

    #[test]
    fn parse_subqueries_caps_total() {
        // cap=2 -> original + at most 1 subquery.
        let v = parse_subqueries(r#"["a","b","c"]"#, "orig", 2);
        assert_eq!(v, vec!["orig", "a"]);
    }

    #[test]
    fn parse_subqueries_dedups_original() {
        let v = parse_subqueries(r#"["orig","b"]"#, "orig", 4);
        assert_eq!(v, vec!["orig", "b"]);
    }

    #[test]
    fn parse_subqueries_skips_blank() {
        let v = parse_subqueries(r#"["  ","b"]"#, "orig", 4);
        assert_eq!(v, vec!["orig", "b"]);
    }

    #[tokio::test]
    async fn decompose_query_none_llm_returns_original() {
        let v = decompose_query(None, "q", 4).await;
        assert_eq!(v, vec!["q"]);
    }

    #[tokio::test]
    async fn decompose_query_happy_path() {
        let llm = arc(Ok(r#"["clause one","clause two"]"#));
        let v = decompose_query(Some(&llm), "orig", 4).await;
        assert_eq!(v, vec!["orig", "clause one", "clause two"]);
    }

    #[tokio::test]
    async fn decompose_query_llm_error_returns_original() {
        let llm = arc(Err(()));
        let v = decompose_query(Some(&llm), "orig", 4).await;
        assert_eq!(v, vec!["orig"]);
    }
}
