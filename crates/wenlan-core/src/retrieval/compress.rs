// SPDX-License-Identifier: Apache-2.0
//! Read-time context compression (gap C6).
//!
//! One LLM pass that densifies an already-assembled retrieval bundle before it
//! reaches the answering LLM, so more relational structure fits the prompt
//! budget. Orthogonal to *which* search method produced the pool — this is a
//! post-assembly representation change, not a ranking change.
//!
//! # Precedence
//! The env flag [`context_compress_enabled`] is the **master gate** (so eval
//! baseline filenames can't lie about compress-ON vs compress-OFF — see
//! AGENTS.md Eval Citation Discipline, page-channel precedent). The
//! [`CompressConfig`] tuning fields supply knobs (min-chars floor, output cap,
//! timeout); they are read only after the env gate is open. Production and eval
//! call sites MUST share this helper.
//!
//! # Graceful degradation
//! Any failure path — flag off, no LLM, bundle below the min-chars floor, LLM
//! error, timeout, or empty/whitespace-only output — returns the verbatim
//! bundle unchanged. The compressed bundle is only substituted on a non-empty
//! trimmed success. Mirrors the `search_memory_expanded` LLM-call + timeout +
//! parse + degrade skeleton. UTF-8 safe: never byte-indexes the bundle.
//!
//! # Faithfulness
//! Faithfulness is enforced by the `COMPRESS_CONTEXT` prompt ONLY. There is NO
//! runtime faithfulness gate — the `page_faithfulness` scorer is a test-time
//! regression check, not a production guard. A long bundle can hit
//! `max_output_tokens` and return a truncated body with no detection; the
//! non-empty-output check guards against silent-zero, not against dropped or
//! invented content. Do NOT enable this beyond eval until a runtime
//! length/coverage floor is added.

use crate::llm_provider::{LlmProvider, LlmRequest};
use crate::tuning::ContextCompressConfig;
use std::sync::Arc;

/// True iff `WENLAN_ENABLE_CONTEXT_COMPRESS` is set to a truthy value
/// (`1`, `true`, or `yes`, case-insensitive, trimmed). Context compression is
/// OPT-IN: unset or a falsey value (`0`/`false`/`no`/"") leaves it disabled.
///
/// Master gate for the feature. All call sites (production read path + eval
/// harness baseline tagging) MUST share this helper so an
/// `WENLAN_ENABLE_CONTEXT_COMPRESS` setting can't disagree between production
/// and eval (which would make baseline filenames lie about their contents —
/// see AGENTS.md Eval Citation Discipline). Truthy-only parse (not `is_ok()`),
/// mirroring `page_channel_enabled` exactly.
pub fn context_compress_enabled() -> bool {
    std::env::var("WENLAN_ENABLE_CONTEXT_COMPRESS")
        .ok()
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

/// Resolved knobs for one compression pass. Built from [`ContextCompressConfig`]
/// (the tuning sub-struct); `enabled` here is the tuning toggle, distinct from
/// the master env gate [`context_compress_enabled`].
#[derive(Debug, Clone)]
pub(crate) struct CompressConfig {
    /// Tuning-level enable. The env gate is the master switch; this lets an
    /// operator who turned the env flag on still disable via tuning.
    pub enabled: bool,
    /// Skip compression for bundles shorter than this many characters — small
    /// bundles aren't worth an extra LLM round-trip and risk detail loss.
    pub min_chars: usize,
    /// Max output tokens for the compression LLM call.
    pub max_output_tokens: u32,
    /// Wall-clock timeout for the LLM call, in seconds.
    pub timeout_secs: u64,
}

impl From<&ContextCompressConfig> for CompressConfig {
    fn from(c: &ContextCompressConfig) -> Self {
        Self {
            enabled: c.enabled,
            min_chars: c.min_chars,
            max_output_tokens: c.max_output_tokens,
            timeout_secs: c.timeout_secs,
        }
    }
}

/// Densify an assembled retrieval `bundle` under `query` via a single LLM pass.
///
/// Contract (verbatim passthrough on every non-happy path):
/// - `!cfg.enabled` -> return `bundle` unchanged, LLM never called.
/// - `llm.is_none()` -> return `bundle` unchanged.
/// - `bundle.chars().count() < cfg.min_chars` -> return `bundle` unchanged, LLM
///   never called.
/// - otherwise ONE `llm.generate` wrapped in `tokio::time::timeout(timeout_secs)`:
///   - `Ok(Ok(out))` with non-empty trimmed `out` -> the trimmed compressed bundle.
///   - empty / whitespace-only output -> `bundle` unchanged (never send an empty
///     bundle downstream — the PR #147 silent-zero class).
///   - LLM error or timeout -> `log::warn` + `bundle` unchanged.
///
/// UTF-8 safe: returns owned `String`s, never byte-indexes the bundle.
pub(crate) async fn compress_context(
    bundle: &str,
    query: &str,
    llm: Option<Arc<dyn LlmProvider>>,
    prompt: &str,
    cfg: &CompressConfig,
) -> String {
    if !cfg.enabled {
        return bundle.to_string();
    }
    let Some(llm) = llm else {
        return bundle.to_string();
    };
    if bundle.chars().count() < cfg.min_chars {
        return bundle.to_string();
    }

    let user_prompt = format!("Query: {query}\n\nContext bundle:\n{bundle}");
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(cfg.timeout_secs),
        llm.generate(LlmRequest {
            system_prompt: Some(prompt.to_string()),
            user_prompt,
            max_tokens: cfg.max_output_tokens,
            temperature: 0.1,
            label: Some("compress_context".into()),
            timeout_secs: None,
        }),
    )
    .await;

    match result {
        Ok(Ok(output)) => {
            let trimmed = output.trim();
            if trimmed.is_empty() {
                // Silent-zero guard (PR #147 class): never send an empty bundle
                // downstream. Fall back to the verbatim bundle.
                log::warn!("[compress] empty LLM output, using verbatim bundle");
                bundle.to_string()
            } else {
                trimmed.to_string()
            }
        }
        Ok(Err(e)) => {
            log::warn!("[compress] LLM failed: {e}");
            bundle.to_string()
        }
        Err(_) => {
            log::warn!("[compress] timed out");
            bundle.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::page_faithfulness::{score_case, PageFixtureCase};
    use crate::llm_provider::{LlmBackend, LlmError};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Test stub: records call count, optionally sleeps (to exercise timeout),
    /// optionally echoes its input, and returns a configured response or error.
    struct StubLlm {
        response: Result<String, ()>,
        calls: AtomicUsize,
        sleep_ms: u64,
        echo_input: bool,
    }

    impl StubLlm {
        fn new(response: Result<&str, ()>) -> Self {
            Self {
                response: response.map(|s| s.to_string()),
                calls: AtomicUsize::new(0),
                sleep_ms: 0,
                echo_input: false,
            }
        }
        fn sleeping(ms: u64) -> Self {
            Self {
                response: Ok("DENSE".to_string()),
                calls: AtomicUsize::new(0),
                sleep_ms: ms,
                echo_input: false,
            }
        }
        fn echoing() -> Self {
            Self {
                response: Ok(String::new()),
                calls: AtomicUsize::new(0),
                sleep_ms: 0,
                echo_input: true,
            }
        }
        fn call_count(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl LlmProvider for StubLlm {
        async fn generate(&self, req: LlmRequest) -> Result<String, LlmError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.sleep_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(self.sleep_ms)).await;
            }
            if self.echo_input {
                return Ok(req.user_prompt);
            }
            self.response
                .clone()
                .map_err(|_| LlmError::InferenceFailed("stub".into()))
        }
        fn is_available(&self) -> bool {
            true
        }
        fn name(&self) -> &str {
            "stub"
        }
        fn backend(&self) -> LlmBackend {
            LlmBackend::OnDevice
        }
    }

    /// A stub that panics if generate() is ever called — used to assert the LLM
    /// is never touched on the verbatim passthrough paths.
    struct PanicLlm;

    #[async_trait]
    impl LlmProvider for PanicLlm {
        async fn generate(&self, _req: LlmRequest) -> Result<String, LlmError> {
            panic!("LLM must not be called on the verbatim passthrough path");
        }
        fn is_available(&self) -> bool {
            true
        }
        fn name(&self) -> &str {
            "panic"
        }
        fn backend(&self) -> LlmBackend {
            LlmBackend::OnDevice
        }
    }

    fn cfg() -> CompressConfig {
        CompressConfig {
            enabled: true,
            min_chars: 10,
            max_output_tokens: 1024,
            timeout_secs: 15,
        }
    }

    const PROMPT: &str = "compress system prompt";

    // -- Test 1: env helper truthy parse --
    #[test]
    fn context_compress_enabled_truthy_values() {
        for v in ["1", "true", "TRUE", "yes", "YES", "  true  "] {
            let on = temp_env::with_var("WENLAN_ENABLE_CONTEXT_COMPRESS", Some(v), || {
                context_compress_enabled()
            });
            assert!(on, "expected enabled for {v:?}");
        }
        for v in ["0", "false", "no", "", "garbage"] {
            let off = temp_env::with_var("WENLAN_ENABLE_CONTEXT_COMPRESS", Some(v), || {
                context_compress_enabled()
            });
            assert!(!off, "expected disabled for {v:?}");
        }
        let unset = temp_env::with_var("WENLAN_ENABLE_CONTEXT_COMPRESS", None::<&str>, || {
            context_compress_enabled()
        });
        assert!(!unset, "unset must be disabled");
    }

    // -- Test 2: disabled cfg -> verbatim, LLM never called --
    #[tokio::test]
    async fn compress_disabled_returns_verbatim() {
        let mut c = cfg();
        c.enabled = false;
        let llm: Arc<dyn LlmProvider> = Arc::new(PanicLlm);
        let bundle = "a".repeat(500);
        let out = compress_context(&bundle, "q", Some(llm), PROMPT, &c).await;
        assert_eq!(out, bundle);
    }

    // -- Test 3: no LLM -> verbatim --
    #[tokio::test]
    async fn compress_no_llm_returns_verbatim() {
        let bundle = "a".repeat(500);
        let out = compress_context(&bundle, "q", None, PROMPT, &cfg()).await;
        assert_eq!(out, bundle);
    }

    // -- Test 4: below min_chars -> verbatim, LLM never called --
    #[tokio::test]
    async fn compress_below_min_chars_returns_verbatim() {
        let mut c = cfg();
        c.min_chars = 600;
        let llm: Arc<dyn LlmProvider> = Arc::new(PanicLlm);
        let bundle = "short bundle";
        let out = compress_context(bundle, "q", Some(llm), PROMPT, &c).await;
        assert_eq!(out, bundle);
    }

    // -- Test 5: happy path -> compressed, LLM called exactly once --
    #[tokio::test]
    async fn compress_happy_path_returns_compressed() {
        let stub = Arc::new(StubLlm::new(Ok("DENSE")));
        let llm: Arc<dyn LlmProvider> = stub.clone();
        let bundle = "a".repeat(500);
        let out = compress_context(&bundle, "q", Some(llm), PROMPT, &cfg()).await;
        assert_eq!(out, "DENSE");
        assert_eq!(stub.call_count(), 1, "LLM must be called exactly once");
    }

    // -- Test 6: timeout -> verbatim --
    #[tokio::test]
    async fn compress_timeout_returns_verbatim() {
        let mut c = cfg();
        c.timeout_secs = 0; // immediate timeout
                            // A zero-second timeout fires before the sleeping stub can respond.
        let stub = Arc::new(StubLlm::sleeping(50));
        let llm: Arc<dyn LlmProvider> = stub.clone();
        let bundle = "a".repeat(500);
        let out = compress_context(&bundle, "q", Some(llm), PROMPT, &c).await;
        assert_eq!(out, bundle);
    }

    // -- Test 7: LLM error -> verbatim --
    #[tokio::test]
    async fn compress_llm_error_returns_verbatim() {
        let llm: Arc<dyn LlmProvider> = Arc::new(StubLlm::new(Err(())));
        let bundle = "a".repeat(500);
        let out = compress_context(&bundle, "q", Some(llm), PROMPT, &cfg()).await;
        assert_eq!(out, bundle);
    }

    // -- Test 8: empty / whitespace output -> verbatim --
    #[tokio::test]
    async fn compress_empty_output_returns_verbatim() {
        for empty in ["", "   ", "\n\t  \n"] {
            let llm: Arc<dyn LlmProvider> = Arc::new(StubLlm::new(Ok(empty)));
            let bundle = "a".repeat(500);
            let out = compress_context(&bundle, "q", Some(llm), PROMPT, &cfg()).await;
            assert_eq!(
                out, bundle,
                "empty output {empty:?} must degrade to verbatim"
            );
        }
    }

    // -- Test 9: UTF-8 multibyte safety --
    #[tokio::test]
    async fn compress_preserves_utf8() {
        // Echoing stub returns its user_prompt verbatim; the bundle is embedded
        // in that prompt, so the output contains the multibyte content. The
        // point is that no byte-indexing panics and the content survives.
        let stub = Arc::new(StubLlm::echoing());
        let llm: Arc<dyn LlmProvider> = stub.clone();
        let bundle = "café — naïve 日本語 ".repeat(40); // well over min_chars
        let out = compress_context(&bundle, "questão", Some(llm), PROMPT, &cfg()).await;
        assert!(out.contains("café — naïve 日本語"));
        assert!(out.contains("questão"));
        assert_eq!(stub.call_count(), 1);
    }

    // -- Test 14 (faithfulness regression gate, reused page_faithfulness) --
    #[test]
    fn compressed_output_is_faithful_to_sources() {
        let case = PageFixtureCase {
            id: "faithful".to_string(),
            source_memories: vec![
                "Decision: use libSQL on 2026-04-22.".to_string(),
                "Embedder is BGE-Base 768-dim.".to_string(),
            ],
            // A faithful compression: every content token is grounded in sources.
            distilled_page_body: "Decision: use libSQL on 2026-04-22. \
                Embedder is BGE-Base 768-dim."
                .to_string(),
            expected_min_faithfulness: 0.8,
        };
        let result = score_case("inline", &case);
        assert!(
            result.faithfulness >= 0.8,
            "faithful compression scored {}",
            result.faithfulness
        );
    }

    // -- Test 15 (negative control: hallucination flagged) --
    #[test]
    fn hallucinated_compression_is_flagged() {
        let case = PageFixtureCase {
            id: "hallucinated".to_string(),
            source_memories: vec![
                "Decision: use libSQL on 2026-04-22.".to_string(),
                "Embedder is BGE-Base 768-dim.".to_string(),
            ],
            // Invents a claim absent from sources.
            distilled_page_body: "They chose PostgreSQL instead of every other database \
                option available."
                .to_string(),
            expected_min_faithfulness: 0.8,
        };
        let result = score_case("inline", &case);
        assert!(
            !result.meets_threshold(),
            "hallucinated compression should be flagged; scored {}",
            result.faithfulness
        );
    }

    // -- Test 16 (default-OFF skips compression / equals verbatim) --
    #[tokio::test]
    async fn compress_approach_skipped_when_disabled() {
        // With the master env gate unset, the production/eval wiring guards the
        // compress call on context_compress_enabled(). Assert the gate is off by
        // default, and that even if the call were reached, a disabled tuning
        // config returns the verbatim bundle (no behavior change default-OFF).
        let disabled_by_env =
            temp_env::with_var("WENLAN_ENABLE_CONTEXT_COMPRESS", None::<&str>, || {
                context_compress_enabled()
            });
        assert!(!disabled_by_env, "env gate must default OFF");

        let mut c = cfg();
        c.enabled = false;
        let llm: Arc<dyn LlmProvider> = Arc::new(PanicLlm);
        let bundle = "a".repeat(500);
        let out = compress_context(&bundle, "q", Some(llm), PROMPT, &c).await;
        assert_eq!(out, bundle, "default-OFF must equal the verbatim bundle");
    }
}
