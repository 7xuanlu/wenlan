// SPDX-License-Identifier: Apache-2.0
//! Multi-hop query decomposition.
//!
//! Addresses Origin's LoCoMo multi-hop category (currently 37%) by rewriting
//! compound user queries into 2-4 standalone sub-queries that each search
//! independently. The caller then fans the sub-queries out and merges results.
//!
//! A single LLM call handles both classification (compound vs. not) and
//! decomposition. If the query is single-hop / open-ended, the LLM returns a
//! one-element array containing the original query verbatim, and the caller
//! can detect "not decomposed" via `len() <= 1`.
//!
//! Failure-mode policy: decomposition is a quality lift, not a correctness
//! gate. Every failure path (timeout, provider error, missing JSON brackets,
//! malformed JSON, empty array) logs a `warn!` and returns
//! `Ok(vec![query.to_string()])` so the caller silently falls back to a
//! plain single-query search.
//!
//! Scaffold only — P0 #3 Phase A. Search integration, surface exposure, and
//! cost gating land in subsequent commits.
//! See `docs/superpowers/p0-plan-retrieval-fixes-2026-05-24.md`.
//! Salvaged from `feature/query-decomposition` (greenfield rewrite).

use std::sync::Arc;
use std::time::Duration;

use tokio::time::timeout;

use crate::error::OriginError;
use crate::llm_provider::{LlmProvider, LlmRequest};

const DECOMPOSE_TIMEOUT_SECS: u64 = 10;
const MAX_SUB_QUERIES: usize = 4;

const DECOMPOSE_SYSTEM_PROMPT: &str =
    "You are a query decomposition assistant for a personal memory search system.
The user is searching their own memory database, not the public web.

Decide first whether the question is genuinely compound (requires combining
facts that would need separate searches). Examples of compound: 'Did X
change my opinion about Y?', 'What was my approach before vs after Z?',
'How does A compare to B in my notes?'.

Examples of NOT compound (do NOT decompose): 'What did I do today?',
'Tell me about my week', 'What is my favorite restaurant?',
'Summarize my recent thoughts on X'. These are single-hop or open-ended
and should NOT be split.

If the question is compound, output a JSON array of 2-4 standalone
sub-questions, each independently searchable, no pronouns referring to others.

If the question is not compound, output a JSON array containing exactly one
element: the original question verbatim.

Output ONLY the JSON array, no prose, no markdown.";

/// Decompose `query` into 1..=`MAX_SUB_QUERIES` standalone sub-queries.
///
/// Calls the configured `LlmProvider` once with `DECOMPOSE_SYSTEM_PROMPT`.
/// Every failure path (timeout, provider error, bracket-locating failure,
/// `serde_json` parse failure, empty result) logs a `warn!` and returns
/// `Ok(vec![query.to_string()])`. The caller can treat `len() <= 1` as
/// "not decomposed" and run a plain single-query search.
pub async fn decompose_query(
    query: &str,
    llm: &Arc<dyn LlmProvider>,
) -> Result<Vec<String>, OriginError> {
    let fallback = || vec![query.to_string()];

    let request = LlmRequest {
        system_prompt: Some(DECOMPOSE_SYSTEM_PROMPT.to_string()),
        user_prompt: query.to_string(),
        max_tokens: 256,
        temperature: 0.0,
        label: Some("decompose_query".to_string()),
        timeout_secs: Some(DECOMPOSE_TIMEOUT_SECS),
    };

    let gen_future = llm.generate(request);
    let output = match timeout(Duration::from_secs(DECOMPOSE_TIMEOUT_SECS), gen_future).await {
        Ok(Ok(output)) => output,
        Ok(Err(err)) => {
            log::warn!("[decompose] LLM provider error, falling back to single query: {err}");
            return Ok(fallback());
        }
        Err(_) => {
            log::warn!(
                "[decompose] LLM call exceeded {DECOMPOSE_TIMEOUT_SECS}s timeout, falling back to single query"
            );
            return Ok(fallback());
        }
    };

    let start = match output.find('[') {
        Some(i) => i,
        None => {
            log::warn!("[decompose] LLM output missing '[', falling back to single query");
            return Ok(fallback());
        }
    };
    let end = match output.rfind(']') {
        Some(i) if i > start => i,
        _ => {
            log::warn!("[decompose] LLM output missing ']', falling back to single query");
            return Ok(fallback());
        }
    };
    let json_str = &output[start..=end];

    let parsed: Vec<String> = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(err) => {
            log::warn!("[decompose] JSON parse failed ({err}), falling back to single query");
            return Ok(fallback());
        }
    };

    if parsed.is_empty() {
        log::warn!("[decompose] LLM returned empty array, falling back to single query");
        return Ok(fallback());
    }

    let mut result = parsed;
    result.truncate(MAX_SUB_QUERIES);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm_provider::MockProvider;

    fn mock(response: &str) -> Arc<dyn LlmProvider> {
        Arc::new(MockProvider::new(response))
    }

    #[tokio::test]
    async fn test_valid_json_returns_subqueries() {
        let llm = mock(r#"["What is X?", "What is Y?"]"#);
        let result = decompose_query("Did X change my opinion about Y?", &llm)
            .await
            .unwrap();
        assert_eq!(
            result,
            vec!["What is X?".to_string(), "What is Y?".to_string()]
        );
    }

    #[tokio::test]
    async fn test_malformed_json_falls_back_to_single() {
        let llm = mock("not even close to JSON, no brackets here");
        let original = "Some compound query?";
        let result = decompose_query(original, &llm).await.unwrap();
        assert_eq!(result, vec![original.to_string()]);
    }

    #[tokio::test]
    async fn test_single_element_passthrough() {
        let original = "What did I do today?";
        let llm = mock(&format!(r#"["{original}"]"#));
        let result = decompose_query(original, &llm).await.unwrap();
        assert_eq!(result, vec![original.to_string()]);
    }
}
