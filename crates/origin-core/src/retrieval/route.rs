// SPDX-License-Identifier: Apache-2.0
// T7 — LLM read-time strategy router.
// Default-OFF: when ORIGIN_LLM_ROUTE is unset/falsey, `classify_strategy` never
// calls the LLM and `db::search_memory_routed` delegates straight to
// `search_memory_cross_rerank`, so behaviour is byte-identical to pre-T7.
//
// This module is a thin classifier: it maps a query to one of four existing
// retrieval strategies. The dispatch itself lives in `db::search_memory_routed`,
// which owns the already-built search methods each strategy delegates to.

use crate::llm_provider::{LlmProvider, LlmRequest};
use std::sync::Arc;

/// System prompt for the LLM strategy classifier. Module-level const (not the
/// PromptRegistry) to mirror the closest siblings `search_memory_expanded` and
/// `retrieval::decompose`, which both inline their classifier prompt rather than
/// thread a `PromptRegistry` through the read path. Override is out of scope for
/// the residual dispatcher; the keyword fallback covers the offline default.
const ROUTE_STRATEGY_SYSTEM_PROMPT: &str = "Classify this memory-search query into exactly one retrieval strategy. \
Strategies:\n\
- \"TemporalScoped\": the query is about WHEN something happened or asks for recent/historical changes (e.g. \"what changed last week\", \"timeline of the project\").\n\
- \"GraphCompletion\": the query is about RELATIONSHIPS between people/projects/entities (e.g. \"who works with Alice\", \"how does X relate to Y\").\n\
- \"Expanded\": the query is broad or vocabulary-sensitive and benefits from paraphrasing (e.g. open-ended \"tell me about the database design tradeoffs\").\n\
- \"PlainRag\": a direct factual lookup; the default for anything else.\n\
Output ONLY a JSON object: {\"strategy\":\"<one-of-the-four>\"}";

/// Which existing retrieval method a query should be dispatched to.
///
/// Every arm maps to a method that already exists in `db.rs`; the router does
/// not introduce a new retrieval algorithm. `PlainRag` is the safe default that
/// every degradation path collapses to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RetrievalStrategy {
    /// Plain hybrid search (`search_memory_cross_rerank`). Safe default.
    PlainRag,
    /// Temporal-window-scoped search (`search_memory_temporal`, T4).
    TemporalScoped,
    /// Graph-augmented completion. Until T3 gates graph independently this is
    /// the same pipeline as `PlainRag` (graph augmentation already runs inside
    /// `search_memory`), so no speculative branch — it maps to the plain path.
    GraphCompletion,
    /// LLM query-expansion search (`search_memory_expanded`).
    Expanded,
}

/// True iff `ORIGIN_LLM_ROUTE` is set to a truthy value (`1`, `true`, or `yes`,
/// case-insensitive). The router is OPT-IN: unset or a falsey value
/// (`0`/`false`/`no`/"") leaves it disabled, so `search_memory_routed` is
/// byte-identical to `search_memory_cross_rerank`. Truthy-only parse copied
/// verbatim from [`crate::db::page_channel_enabled`] so production and the eval
/// harness can never disagree on the flag (which would make baseline filenames
/// lie — see AGENTS.md Eval Citation Discipline).
pub(crate) fn route_enabled() -> bool {
    std::env::var("ORIGIN_LLM_ROUTE")
        .ok()
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

/// Parse the LLM classifier output into a strategy. Pure + synchronous so the
/// parse contract is unit-tested in isolation (mirrors
/// `retrieval::decompose::parse_subqueries`).
///
/// Silent-zero / PR #147 guard: any malformed JSON, missing object, missing
/// `strategy` key, or unknown token returns `None` so the caller degrades to
/// `PlainRag`. Only the four known variant tokens map to a strategy.
pub(crate) fn parse_strategy(output: &str) -> Option<RetrievalStrategy> {
    // Object (not array): find the outermost { ... } and parse it.
    let (si, ei) = (output.find('{')?, output.rfind('}')?);
    if ei <= si {
        return None;
    }
    let value: serde_json::Value = match serde_json::from_str(&output[si..=ei]) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("[route] strategy JSON parse failed: {e}");
            return None;
        }
    };
    let token = value.get("strategy").and_then(|s| s.as_str())?;
    match token {
        "PlainRag" => Some(RetrievalStrategy::PlainRag),
        "TemporalScoped" => Some(RetrievalStrategy::TemporalScoped),
        "GraphCompletion" => Some(RetrievalStrategy::GraphCompletion),
        "Expanded" => Some(RetrievalStrategy::Expanded),
        other => {
            log::warn!("[route] unknown strategy token '{other}', degrading to PlainRag");
            None
        }
    }
}

/// Zero-LLM keyword classification. Delegates to the shared `router::classify`
/// keyword constants (via `query_intent::classify_intent` + the shared
/// `RELATIONAL_KEYWORDS` slice) so the temporal/relational vocabulary is not
/// duplicated and cannot drift.
///
/// - temporal cue -> `TemporalScoped`
/// - relational cue -> `GraphCompletion`
/// - otherwise -> `PlainRag`
pub(crate) fn keyword_fallback(query: &str) -> RetrievalStrategy {
    use crate::retrieval::query_intent::{classify_intent, QueryIntent};
    match classify_intent(query) {
        QueryIntent::Temporal => RetrievalStrategy::TemporalScoped,
        // `classify_intent` treats relational queries as `General` (not a
        // distinct arm), so disambiguate relational here using the shared
        // RELATIONAL_KEYWORDS constant — still no duplicated keyword list.
        QueryIntent::General => {
            let lower = query.to_lowercase();
            if crate::router::classify::RELATIONAL_KEYWORDS
                .iter()
                .any(|kw| lower.contains(kw))
            {
                RetrievalStrategy::GraphCompletion
            } else {
                RetrievalStrategy::PlainRag
            }
        }
        QueryIntent::Factual => RetrievalStrategy::PlainRag,
    }
}

/// Classify a query into a [`RetrievalStrategy`].
///
/// - When the router is disabled (`!route_enabled()`) or no LLM is available,
///   uses the deterministic [`keyword_fallback`] — no LLM call, offline-safe.
/// - Otherwise issues a single timeout-bound LLM call (structure copied from
///   `search_memory_expanded`): any timeout, LLM error, malformed/unknown
///   output degrades to `PlainRag`. Never errors or panics.
pub(crate) async fn classify_strategy(
    query: &str,
    llm: Option<Arc<dyn LlmProvider>>,
) -> RetrievalStrategy {
    let Some(llm) = llm.filter(|_| route_enabled()) else {
        return keyword_fallback(query);
    };

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        llm.generate(LlmRequest {
            system_prompt: Some(ROUTE_STRATEGY_SYSTEM_PROMPT.into()),
            user_prompt: query.to_string(),
            max_tokens: 32,
            temperature: 0.0,
            label: Some("route_strategy".into()),
            timeout_secs: None,
        }),
    )
    .await;

    let strategy = match result {
        Ok(Ok(output)) => parse_strategy(&output).unwrap_or(RetrievalStrategy::PlainRag),
        Ok(Err(e)) => {
            log::warn!("[route] classifier LLM failed: {e}; using PlainRag");
            RetrievalStrategy::PlainRag
        }
        Err(_) => {
            log::warn!("[route] classifier timed out; using PlainRag");
            RetrievalStrategy::PlainRag
        }
    };
    log::debug!("[route] strategy={strategy:?}");
    strategy
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm_provider::{LlmBackend, LlmError};
    use async_trait::async_trait;

    /// Mock provider returning a fixed string (or error). Distinct from the
    /// crate `MockProvider` because the timeout test needs a *sleeping* arm.
    struct MockLlm {
        response: Result<String, ()>,
        sleep: std::time::Duration,
    }

    #[async_trait]
    impl LlmProvider for MockLlm {
        async fn generate(&self, _req: LlmRequest) -> Result<String, LlmError> {
            if !self.sleep.is_zero() {
                tokio::time::sleep(self.sleep).await;
            }
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
            sleep: std::time::Duration::ZERO,
        })
    }

    fn sleeping_arc(secs: u64) -> Arc<dyn LlmProvider> {
        Arc::new(MockLlm {
            response: Ok("{\"strategy\":\"Expanded\"}".to_string()),
            sleep: std::time::Duration::from_secs(secs),
        })
    }

    // --- keyword fallback (offline, no LLM) ---------------------------------

    #[tokio::test]
    async fn classify_strategy_no_llm_falls_back_to_keyword() {
        // Offline daemon: no LLM -> deterministic keyword routing.
        assert_eq!(
            classify_strategy("what changed last week", None).await,
            RetrievalStrategy::TemporalScoped
        );
        assert_eq!(
            classify_strategy("database password", None).await,
            RetrievalStrategy::PlainRag
        );
    }

    #[test]
    fn keyword_fallback_matches_classify_query() {
        // Delegates to the shared router::classify constants — relational query
        // routes to GraphCompletion (guards against keyword-list drift).
        assert_eq!(
            keyword_fallback("what is the relationship between Alice and Bob"),
            RetrievalStrategy::GraphCompletion
        );
        assert_eq!(
            keyword_fallback("what changed recently"),
            RetrievalStrategy::TemporalScoped
        );
        assert_eq!(
            keyword_fallback("database password"),
            RetrievalStrategy::PlainRag
        );
    }

    // --- degradation paths (all collapse to PlainRag) -----------------------

    #[tokio::test(start_paused = true)]
    async fn classify_strategy_timeout_returns_plainrag() {
        // Provider sleeps past the 10s timeout. With a paused clock the timeout
        // fires in virtual time; assert PlainRag and no panic.
        temp_env::async_with_vars([("ORIGIN_LLM_ROUTE", Some("1"))], async {
            let llm = sleeping_arc(30);
            assert_eq!(
                classify_strategy("anything", Some(llm)).await,
                RetrievalStrategy::PlainRag
            );
        })
        .await;
    }

    #[tokio::test]
    async fn classify_strategy_unparseable_returns_plainrag() {
        temp_env::async_with_vars([("ORIGIN_LLM_ROUTE", Some("1"))], async {
            let llm = arc(Ok("banana"));
            assert_eq!(
                classify_strategy("anything", Some(llm)).await,
                RetrievalStrategy::PlainRag
            );
        })
        .await;
    }

    #[tokio::test]
    async fn classify_strategy_unknown_variant_returns_plainrag() {
        temp_env::async_with_vars([("ORIGIN_LLM_ROUTE", Some("1"))], async {
            let llm = arc(Ok(r#"{"strategy":"CYPHER"}"#));
            assert_eq!(
                classify_strategy("anything", Some(llm)).await,
                RetrievalStrategy::PlainRag
            );
        })
        .await;
    }

    #[tokio::test]
    async fn classify_strategy_llm_error_returns_plainrag() {
        temp_env::async_with_vars([("ORIGIN_LLM_ROUTE", Some("1"))], async {
            let llm = arc(Err(()));
            assert_eq!(
                classify_strategy("anything", Some(llm)).await,
                RetrievalStrategy::PlainRag
            );
        })
        .await;
    }

    // --- valid variants parse ----------------------------------------------

    #[tokio::test]
    async fn classify_strategy_valid_variants_parse() {
        let cases = [
            (
                "{\"strategy\":\"TemporalScoped\"}",
                RetrievalStrategy::TemporalScoped,
            ),
            ("{\"strategy\":\"Expanded\"}", RetrievalStrategy::Expanded),
            (
                "{\"strategy\":\"GraphCompletion\"}",
                RetrievalStrategy::GraphCompletion,
            ),
            ("{\"strategy\":\"PlainRag\"}", RetrievalStrategy::PlainRag),
        ];
        for (out, expected) in cases {
            temp_env::async_with_vars([("ORIGIN_LLM_ROUTE", Some("1"))], async {
                let llm = arc(Ok(out));
                assert_eq!(
                    classify_strategy("anything", Some(llm)).await,
                    expected,
                    "output {out} should map to {expected:?}"
                );
            })
            .await;
        }
    }

    #[tokio::test]
    async fn classify_strategy_parses_with_surrounding_noise() {
        // Real on-device output often wraps the JSON in prose.
        temp_env::async_with_vars([("ORIGIN_LLM_ROUTE", Some("1"))], async {
            let llm = arc(Ok(r#"Sure! {"strategy":"Expanded"} done."#));
            assert_eq!(
                classify_strategy("anything", Some(llm)).await,
                RetrievalStrategy::Expanded
            );
        })
        .await;
    }

    // --- dark-by-default ----------------------------------------------------

    #[tokio::test]
    async fn route_disabled_env_returns_plainrag() {
        // Flag unset/falsey -> route_enabled() false -> keyword path even when
        // an LLM is supplied. A relational query the LLM would route to
        // GraphCompletion still goes through keyword classification.
        for val in [None::<&str>, Some("0"), Some("false"), Some("")] {
            temp_env::async_with_vars([("ORIGIN_LLM_ROUTE", val)], async {
                assert!(!route_enabled());
                // LLM returns Expanded, but flag-off ignores it and uses keyword.
                let llm = arc(Ok(r#"{"strategy":"Expanded"}"#));
                assert_eq!(
                    classify_strategy("database password", Some(llm)).await,
                    RetrievalStrategy::PlainRag,
                    "flag={val:?} must ignore the LLM and use keyword fallback"
                );
            })
            .await;
        }
    }

    #[tokio::test]
    async fn route_enabled_accepts_truthy_synonyms() {
        for val in ["1", "true", "YES", "True"] {
            temp_env::async_with_vars([("ORIGIN_LLM_ROUTE", Some(val))], async {
                assert!(route_enabled(), "value {val} should enable routing");
            })
            .await;
        }
    }

    // --- pure parse contract ------------------------------------------------

    #[test]
    fn parse_strategy_empty_object_is_none() {
        assert_eq!(parse_strategy("{}"), None);
    }

    #[test]
    fn parse_strategy_malformed_is_none() {
        // PR #147 silent-zero class: malformed JSON must not panic.
        assert_eq!(parse_strategy("{not valid"), None);
        assert_eq!(parse_strategy("no braces here"), None);
    }

    #[test]
    fn parse_strategy_unknown_token_is_none() {
        assert_eq!(parse_strategy(r#"{"strategy":"CYPHER"}"#), None);
    }

    #[test]
    fn parse_strategy_known_tokens() {
        assert_eq!(
            parse_strategy(r#"{"strategy":"PlainRag"}"#),
            Some(RetrievalStrategy::PlainRag)
        );
        assert_eq!(
            parse_strategy(r#"{"strategy":"TemporalScoped"}"#),
            Some(RetrievalStrategy::TemporalScoped)
        );
    }
}
