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

/// Token-estimate divisor. The `LlmProvider::generate` trait returns
/// `Result<String, LlmError>` with no token counts, so we fall back to the
/// industry-standard `chars / 4` rule of thumb (English BPE-like tokenizers
/// average ~4 chars per token). This is good enough for gating purposes
/// (deciding whether to enable decompose under a cost budget) but should
/// not be quoted as an exact figure.
const CHARS_PER_TOKEN_ESTIMATE: usize = 4;

/// Estimated token usage from the single decomposition LLM call.
///
/// `input_tokens` and `output_tokens` are heuristics derived from
/// `chars / CHARS_PER_TOKEN_ESTIMATE` because `LlmProvider::generate` does
/// not surface per-call token usage today. Wire-types exposure (so deployments
/// can gate the decompose path under a cost budget) is a follow-up commit —
/// for now the estimate is emitted at `log::info!` from inside
/// `decompose_query` under the `[decompose]` tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecomposeOutput {
    pub sub_queries: Vec<String>,
    pub input_tokens: usize,
    pub output_tokens: usize,
}

/// Estimate token count from a UTF-8 string via `chars / 4`. Counts characters
/// (not bytes) so multi-byte glyphs don't inflate the estimate. Used by
/// `decompose_query` for the cost-telemetry log line.
#[inline]
pub fn estimate_tokens(text: &str) -> usize {
    text.chars().count() / CHARS_PER_TOKEN_ESTIMATE
}

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
///
/// Cost telemetry: emits one `log::info!` line under the `[decompose]` tag
/// containing the estimated input + output token counts (heuristic
/// `chars / 4`, see [`estimate_tokens`]). Failure paths log
/// `output=0` plus a parenthetical reason. Surfacing tokens through wire
/// types (so deployments can gate the path under a cost budget) is a
/// follow-up; the unused [`DecomposeOutput`] struct reserves the shape.
pub async fn decompose_query(
    query: &str,
    llm: &Arc<dyn LlmProvider>,
) -> Result<Vec<String>, OriginError> {
    let fallback = || vec![query.to_string()];

    // Estimate the input cost up front so we still log a number on any
    // failure path. Input ≈ system prompt + user prompt.
    let input_tokens = estimate_tokens(DECOMPOSE_SYSTEM_PROMPT) + estimate_tokens(query);

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
            log::info!(
                "[decompose] estimated tokens: input={input_tokens} output=0 (provider error)"
            );
            return Ok(fallback());
        }
        Err(_) => {
            log::warn!(
                "[decompose] LLM call exceeded {DECOMPOSE_TIMEOUT_SECS}s timeout, falling back to single query"
            );
            log::info!("[decompose] estimated tokens: input={input_tokens} output=0 (timeout)");
            return Ok(fallback());
        }
    };

    let output_tokens = estimate_tokens(&output);
    log::info!("[decompose] estimated tokens: input={input_tokens} output={output_tokens}");

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

    #[test]
    fn test_estimate_tokens_basic() {
        // chars / 4 — empty stays 0, exact multiples land where expected.
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcdefgh"), 2);
        // Multi-byte glyphs count as one char each, not by byte length.
        assert_eq!(estimate_tokens("日本語日"), 1);
    }

    #[tokio::test]
    async fn test_decompose_output_estimates_tokens() {
        // The DecomposeOutput struct is a forward-looking shape; verify its
        // estimator inputs produce strictly positive token counts for the
        // standard valid-JSON path so a future wire-types caller can rely on
        // non-zero counts whenever the LLM call actually happens.
        let response = r#"["What is X?", "What is Y?"]"#;
        let llm = mock(response);
        let query = "Did X change my opinion about Y?";

        // decompose_query itself only logs; the public estimator + the struct
        // are what callers wire up. Mirror what the function computes.
        let input_tokens = estimate_tokens(DECOMPOSE_SYSTEM_PROMPT) + estimate_tokens(query);
        let output_tokens = estimate_tokens(response);

        let sub_queries = decompose_query(query, &llm).await.unwrap();
        let stats = DecomposeOutput {
            sub_queries,
            input_tokens,
            output_tokens,
        };

        assert!(
            stats.input_tokens > 0,
            "input estimate must be > 0 (system prompt alone is non-empty)"
        );
        assert!(
            stats.output_tokens > 0,
            "output estimate must be > 0 for a valid JSON response"
        );
        assert_eq!(stats.sub_queries.len(), 2);
    }

    #[tokio::test]
    async fn test_decompose_logs_token_estimate() {
        // Smoke test: the logging path (info! macro) must not panic on any
        // success or failure variant. We don't capture stderr here; the goal
        // is to make sure the format-string args still compile + evaluate.
        let llm = mock(r#"["one", "two"]"#);
        let _ = decompose_query("happy path", &llm).await.unwrap();

        let llm = mock("garbage with no brackets");
        let _ = decompose_query("malformed path", &llm).await.unwrap();
    }
}
