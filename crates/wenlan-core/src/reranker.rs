// SPDX-License-Identifier: Apache-2.0
//! Cross-encoder reranker for retrieval candidates.
//!
//! Replaces LLM-as-judge reranking with a purpose-built cross-encoder model
//! via fastembed. Faster (milliseconds vs seconds), cost-free at runtime,
//! and typically higher quality on retrieval metrics. SuperLocalMemory's V3.3
//! ablation showed cross-encoder rerank is the single largest contributor
//! across their math layers + channels (-30.7pp when removed).
//!
//! Three impls:
//! - [`CrossEncoderReranker`] — fastembed `TextRerank` (default).
//! - [`NoopReranker`] — passthrough, preserves input order. Tests + opt-out.
//! - LLM reranker stays in [`crate::rerank`] as `LlmEngine::rerank_results`
//!   for A/B comparison on the eval harness. Refactor into trait in a follow-up.
//!
//! Trait is sync because fastembed's `TextRerank::rerank` is sync CPU work.
//! Async callers should wrap calls in `tokio::task::spawn_blocking` to avoid
//! blocking the runtime.

use std::sync::Arc;
use std::sync::Mutex;

use crate::error::WenlanError;

/// A reranker takes a query and ordered candidates `(id, content)` and returns
/// `(id, score)` pairs sorted by score descending. Score range is
/// reranker-specific; callers should treat scores as ordinal only.
///
/// Implementations MUST degrade gracefully on internal failure: return
/// `Ok(vec![])` after logging. Callers fall back to the original pre-rerank
/// ordering. Never propagate transient model errors as `Err`.
pub trait Reranker: Send + Sync {
    fn rerank(
        &self,
        query: &str,
        candidates: &[(String, String)],
    ) -> Result<Vec<(String, f32)>, WenlanError>;

    fn model_id(&self) -> &str;
}

/// Passthrough reranker. Returns candidates unchanged with score = 0.0.
pub struct NoopReranker;

impl Reranker for NoopReranker {
    fn rerank(
        &self,
        _query: &str,
        candidates: &[(String, String)],
    ) -> Result<Vec<(String, f32)>, WenlanError> {
        Ok(candidates.iter().map(|(id, _)| (id.clone(), 0.0)).collect())
    }

    fn model_id(&self) -> &str {
        "noop"
    }
}

/// Cross-encoder reranker via fastembed's `TextRerank`.
///
/// Model downloads on first construction (size varies by model and export —
/// e.g. BGE-reranker-base ~1.1GB, BGE-v2-m3 ~2.27GB as full fp32 ONNX). Pass
/// a `cache_dir` aligned with the embedder cache for
/// reuse. `TextRerank::rerank` takes `&mut self`, so we wrap in `Mutex` and
/// share one instance per process behind `Arc`.
pub struct CrossEncoderReranker {
    inner: Mutex<fastembed::TextRerank>,
    model_id: String,
}

impl CrossEncoderReranker {
    pub fn try_new(
        model: fastembed::RerankerModel,
        cache_dir: Option<std::path::PathBuf>,
    ) -> Result<Self, WenlanError> {
        let model_id = format!("{:?}", model);
        let mut options = fastembed::RerankInitOptions::new(model);
        if let Some(dir) = cache_dir {
            options = options.with_cache_dir(dir);
        }
        let inner = fastembed::TextRerank::try_new(options)
            .map_err(|e| WenlanError::Llm(format!("CrossEncoderReranker init: {e}")))?;
        Ok(Self {
            inner: Mutex::new(inner),
            model_id,
        })
    }

    /// Bring-your-own reranker: load an ONNX cross-encoder + tokenizer from a
    /// local directory, bypassing fastembed's hf-hub downloader (which cannot
    /// fetch Xet-backed model files — e.g. jinaai/jina-reranker-v1-turbo-en).
    /// The directory must hold `model.onnx`, `tokenizer.json`, `config.json`,
    /// `special_tokens_map.json`, `tokenizer_config.json`. `model_id` is stamped
    /// verbatim (the user-defined path has no model enum) so eval baselines stay
    /// honest about which weights produced them.
    ///
    /// Probes once at init: a cross-encoder whose ONNX output is not named
    /// `logits` yields EMPTY scores, which would silently degrade to a
    /// no-op passthrough. We rerank a trivial probe and refuse to load on an
    /// empty result, so a wiring slip fails LOUD here rather than as a
    /// mysteriously-flat A/B.
    pub fn try_new_user_defined(
        onnx_dir: &std::path::Path,
        model_id: impl Into<String>,
    ) -> Result<Self, WenlanError> {
        let read = |name: &str| -> Result<Vec<u8>, WenlanError> {
            std::fs::read(onnx_dir.join(name))
                .map_err(|e| WenlanError::Llm(format!("BYO reranker: read {name}: {e}")))
        };
        let onnx_path = onnx_dir.join("model.onnx");
        if !onnx_path.exists() {
            return Err(WenlanError::Llm(format!(
                "BYO reranker: model.onnx missing in {}",
                onnx_dir.display()
            )));
        }
        let tokenizer_files = fastembed::TokenizerFiles {
            tokenizer_file: read("tokenizer.json")?,
            config_file: read("config.json")?,
            special_tokens_map_file: read("special_tokens_map.json")?,
            tokenizer_config_file: read("tokenizer_config.json")?,
        };
        let model = fastembed::UserDefinedRerankingModel::new(
            fastembed::OnnxSource::File(onnx_path),
            tokenizer_files,
        );
        let inner = fastembed::TextRerank::try_new_from_user_defined(
            model,
            fastembed::RerankInitOptionsUserDefined::default(),
        )
        .map_err(|e| WenlanError::Llm(format!("BYO reranker init: {e}")))?;
        let reranker = Self {
            inner: Mutex::new(inner),
            model_id: model_id.into(),
        };
        let probe = reranker.rerank(
            "probe query",
            &[
                ("hit".to_string(), "probe query exact match".to_string()),
                ("miss".to_string(), "an unrelated sentence".to_string()),
            ],
        )?;
        if probe.is_empty() {
            return Err(WenlanError::Llm(
                "BYO reranker produced no scores on a probe (likely an ONNX output-name \
                 mismatch — fastembed reads the 'logits' output); refusing to load"
                    .to_string(),
            ));
        }
        Ok(reranker)
    }
}

impl Reranker for CrossEncoderReranker {
    fn rerank(
        &self,
        query: &str,
        candidates: &[(String, String)],
    ) -> Result<Vec<(String, f32)>, WenlanError> {
        if candidates.is_empty() {
            return Ok(Vec::new());
        }
        let docs: Vec<&str> = candidates.iter().map(|(_, c)| c.as_str()).collect();
        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(_) => {
                log::warn!("[reranker] mutex poisoned; returning empty");
                return Ok(Vec::new());
            }
        };
        let results = match guard.rerank(query, &docs, false, None) {
            Ok(r) => r,
            Err(e) => {
                log::warn!("[reranker] cross-encoder failed: {e}; returning empty");
                return Ok(Vec::new());
            }
        };
        drop(guard);
        let mut out: Vec<(String, f32)> = results
            .into_iter()
            .filter_map(|r| candidates.get(r.index).map(|(id, _)| (id.clone(), r.score)))
            .collect();
        out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(out)
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }
}

/// Resolve the cross-encoder model from `WENLAN_RERANKER_MODEL`. Default (unset
/// or unrecognized) is `BGERerankerBase` since 2026-06-11: on the LME paired
/// sweep (PR #260, scaffold N=1) it kept 94% of BGE-v2-m3's NDCG lift
/// (+0.1058 vs +0.1130 agg, both BH-sig) at 29% of its marginal P50 cost
/// (dP50 +318ms vs +1094ms over the ~190ms base; on-arm P50 510ms vs
/// 1279ms on CPU) and downloads 1.1GB vs 2.27GB.
///
/// `bge` / `bge-v2-m3` still select `BGERerankerV2M3` — the quality ceiling
/// (largest multilingual model; SuperLocalMemory's V3.3 ablation cited it as
/// the single largest contributor to their retrieval stack) for users who
/// accept the latency. `turbo` selects `JINARerankerV1TurboEn` (~37M params,
/// English-only): near-free on CPU (+20ms P50 in the same sweep), for eval
/// A/B sweeps and latency-critical deployments. The chosen model_id is
/// stamped into the reranker so eval baselines stay honest about which
/// reranker produced them.
fn reranker_model_from_env() -> fastembed::RerankerModel {
    use fastembed::RerankerModel::{BGERerankerBase, BGERerankerV2M3, JINARerankerV1TurboEn};
    let raw = std::env::var("WENLAN_RERANKER_MODEL").unwrap_or_default();
    match raw.trim().to_ascii_lowercase().as_str() {
        "turbo" | "jina-turbo" | "jina" => JINARerankerV1TurboEn,
        "" | "bge-base" => BGERerankerBase,
        "bge" | "bge-v2-m3" => BGERerankerV2M3,
        other => {
            log::warn!("[reranker] unknown WENLAN_RERANKER_MODEL={other:?}; using BGERerankerBase");
            BGERerankerBase
        }
    }
}

/// Reranker activation mode, selected by `WENLAN_RERANKER_MODE`.
///
/// Governs WHICH retrieval paths get a cross-encoder and which model:
/// - `Off` (default): no CE anywhere — byte-identical to the pre-mode daemon.
/// - `Lite`: the light turbo CE (`JINARerankerV1TurboEn`) on the quick
///   (`/api/search`) and context (`/api/chat-context`) paths AND on an explicit
///   `rerank=true` deep search; the heavy bge-base model is not loaded.
/// - `Full`: turbo on quick/context, plus the heavy `BGERerankerBase` (pool-10)
///   on the explicit `rerank=true` deep path.
///
/// Distinct from the legacy `WENLAN_RERANKER_ENABLED=1` switch, which — with
/// `WENLAN_RERANKER_MODE` unset — maps to deep-only CE using the
/// [`reranker_model_from_env`] model (exactly the pre-mode behavior). When both
/// are set, the explicit mode wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RerankerMode {
    /// No cross-encoder on any path.
    #[default]
    Off,
    /// Turbo CE on quick + context + explicit deep rerank; no heavy model.
    Lite,
    /// Turbo on quick + context; heavy bge-base on explicit deep rerank.
    Full,
}

/// Parse [`RerankerMode`] from `WENLAN_RERANKER_MODE`. Unset / empty / `off` /
/// unrecognized → `Off` (fail-safe: an unknown value never silently enables CE).
pub fn reranker_mode_from_env() -> RerankerMode {
    match std::env::var("WENLAN_RERANKER_MODE")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "lite" => RerankerMode::Lite,
        "full" => RerankerMode::Full,
        "" | "off" => RerankerMode::Off,
        other => {
            log::warn!("[reranker] unknown WENLAN_RERANKER_MODE={other:?}; using Off");
            RerankerMode::Off
        }
    }
}

/// Construct the cross-encoder reranker and return it as a trait object.
///
/// Model is selected by [`reranker_model_from_env`] — defaults to
/// `BGERerankerBase`. Pass `cache_dir` aligned with the embedder cache so the
/// weights download once and stay reusable across processes.
///
/// Not called in any default-running test — first construction downloads the
/// model weights. Callers must opt in (daemon startup, `--ignored` tests,
/// manual eval runs).
pub fn init_cross_encoder_reranker(
    cache_dir: Option<std::path::PathBuf>,
) -> Result<Arc<dyn Reranker>, WenlanError> {
    // BYO opt-in: load a local ONNX cross-encoder from WENLAN_RERANKER_ONNX_DIR,
    // bypassing the hf-hub downloader for Xet-backed models it can't fetch. Unset
    // (the production default) keeps the enum download path below.
    if let Ok(dir) = std::env::var("WENLAN_RERANKER_ONNX_DIR") {
        let dir = std::path::PathBuf::from(dir);
        let model_id = std::env::var("WENLAN_RERANKER_MODEL_ID").unwrap_or_else(|_| {
            dir.file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "user-defined".to_string())
        });
        let inner = CrossEncoderReranker::try_new_user_defined(&dir, model_id)
            .map_err(|e| WenlanError::Llm(format!("init_cross_encoder_reranker (BYO): {e}")))?;
        log::info!("[reranker] loaded BYO model_id={}", inner.model_id());
        return Ok(Arc::new(inner) as Arc<dyn Reranker>);
    }
    let model = reranker_model_from_env();
    log::info!("[reranker] loading enum model {model:?} (download path)");
    let inner = CrossEncoderReranker::try_new(model, cache_dir)
        .map_err(|e| WenlanError::Llm(format!("init_cross_encoder_reranker: {e}")))?;
    Ok(Arc::new(inner) as Arc<dyn Reranker>)
}

/// Load a SPECIFIC cross-encoder model, bypassing `WENLAN_RERANKER_MODEL`. Used by
/// the per-path `WENLAN_RERANKER_MODE` wiring where each path pins its own model
/// (turbo on the light paths, bge-base on the deep path). First construction
/// downloads the weights; callers run it off the async runtime via `spawn_blocking`.
pub fn init_cross_encoder_reranker_model(
    model: fastembed::RerankerModel,
    cache_dir: Option<std::path::PathBuf>,
) -> Result<Arc<dyn Reranker>, WenlanError> {
    let inner = CrossEncoderReranker::try_new(model, cache_dir)
        .map_err(|e| WenlanError::Llm(format!("init_cross_encoder_reranker_model: {e}")))?;
    Ok(Arc::new(inner) as Arc<dyn Reranker>)
}

/// Load the reranker for a resolved [`RerankerPick`]: a pinned model for
/// `Turbo`/`BgeBase`, or the legacy env+BYO selection for `Configured`. Lets the
/// daemon wire per-path rerankers without naming `fastembed` types directly.
pub fn init_cross_encoder_reranker_pick(
    pick: RerankerPick,
    cache_dir: Option<std::path::PathBuf>,
) -> Result<Arc<dyn Reranker>, WenlanError> {
    match pick.fastembed_model() {
        Some(model) => init_cross_encoder_reranker_model(model, cache_dir),
        None => init_cross_encoder_reranker(cache_dir),
    }
}

/// Which cross-encoder model a retrieval path should use under a given
/// [`RerankerMode`]. `Configured` defers to [`reranker_model_from_env`] (legacy
/// `WENLAN_RERANKER_ENABLED=1` back-compat: honors `WENLAN_RERANKER_MODEL` + BYO).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RerankerPick {
    /// JINA turbo (small, ~146MB) — the light/quick model.
    Turbo,
    /// bge-reranker-base (~1.1GB) — the heavy deep model.
    BgeBase,
    /// The legacy `WENLAN_RERANKER_MODEL` selection (back-compat path).
    Configured,
}

impl RerankerPick {
    /// Map a non-`Configured` pick to its concrete fastembed model. `Configured`
    /// returns `None` — that path uses [`init_cross_encoder_reranker`] (env + BYO).
    pub fn fastembed_model(self) -> Option<fastembed::RerankerModel> {
        match self {
            RerankerPick::Turbo => Some(fastembed::RerankerModel::JINARerankerV1TurboEn),
            RerankerPick::BgeBase => Some(fastembed::RerankerModel::BGERerankerBase),
            RerankerPick::Configured => None,
        }
    }
}

/// Per-path reranker selection resolved from `(mode, legacy WENLAN_RERANKER_ENABLED)`.
/// `light` = quick (`/api/search`) + context (`/api/chat-context`);
/// `deep`  = `/api/memory/search` with `rerank=true`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RerankerPlan {
    pub light: Option<RerankerPick>,
    pub deep: Option<RerankerPick>,
}

/// Resolve which model each path uses. An explicit `WENLAN_RERANKER_MODE` wins;
/// when mode is `Off`, the legacy `WENLAN_RERANKER_ENABLED=1` switch maps to
/// deep-only `Configured` — exactly the pre-mode behavior.
pub fn resolve_reranker_plan(mode: RerankerMode, legacy_enabled: bool) -> RerankerPlan {
    match mode {
        RerankerMode::Off => {
            if legacy_enabled {
                RerankerPlan {
                    light: None,
                    deep: Some(RerankerPick::Configured),
                }
            } else {
                RerankerPlan::default()
            }
        }
        // lite: turbo everywhere CE applies — quick + context + explicit deep rerank.
        RerankerMode::Lite => RerankerPlan {
            light: Some(RerankerPick::Turbo),
            deep: Some(RerankerPick::Turbo),
        },
        // full: turbo on the light paths, heavy bge-base on the explicit deep path.
        RerankerMode::Full => RerankerPlan {
            light: Some(RerankerPick::Turbo),
            deep: Some(RerankerPick::BgeBase),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_preserves_order_and_emits_zero_score() {
        let candidates = vec![
            ("a".to_string(), "first".to_string()),
            ("b".to_string(), "second".to_string()),
            ("c".to_string(), "third".to_string()),
        ];
        let reranker = NoopReranker;
        let out = reranker.rerank("query", &candidates).unwrap();
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].0, "a");
        assert_eq!(out[1].0, "b");
        assert_eq!(out[2].0, "c");
        assert!(out.iter().all(|(_, s)| *s == 0.0));
    }

    #[test]
    fn noop_handles_empty() {
        let reranker = NoopReranker;
        let out = reranker.rerank("query", &[]).unwrap();
        assert_eq!(out.len(), 0);
    }

    #[test]
    fn noop_reports_model_id() {
        let reranker = NoopReranker;
        assert_eq!(reranker.model_id(), "noop");
    }

    #[test]
    fn trait_object_safe() {
        let _r: Box<dyn Reranker> = Box::new(NoopReranker);
    }

    #[test]
    fn reranker_model_env_default_is_bge_base() {
        temp_env::with_var("WENLAN_RERANKER_MODEL", None::<&str>, || {
            assert!(matches!(
                reranker_model_from_env(),
                fastembed::RerankerModel::BGERerankerBase
            ));
        });
    }

    #[test]
    fn reranker_model_env_turbo_selects_jina() {
        for v in ["turbo", "jina-turbo", "JINA", "Turbo"] {
            temp_env::with_var("WENLAN_RERANKER_MODEL", Some(v), || {
                assert!(
                    matches!(
                        reranker_model_from_env(),
                        fastembed::RerankerModel::JINARerankerV1TurboEn
                    ),
                    "value {v:?} should select the turbo model"
                );
            });
        }
    }

    #[test]
    fn reranker_model_env_explicit_aliases() {
        use fastembed::RerankerModel::{BGERerankerBase, BGERerankerV2M3};
        for (v, want_v2m3) in [
            ("bge", true),
            ("bge-v2-m3", true),
            ("BGE-V2-M3", true),
            ("bge-base", false),
        ] {
            temp_env::with_var("WENLAN_RERANKER_MODEL", Some(v), || {
                let got = reranker_model_from_env();
                if want_v2m3 {
                    assert!(
                        matches!(got, BGERerankerV2M3),
                        "value {v:?} should select BGERerankerV2M3"
                    );
                } else {
                    assert!(
                        matches!(got, BGERerankerBase),
                        "value {v:?} should select BGERerankerBase"
                    );
                }
            });
        }
    }

    #[test]
    fn reranker_model_env_unknown_falls_back_to_default() {
        temp_env::with_var("WENLAN_RERANKER_MODEL", Some("nonsense"), || {
            assert!(matches!(
                reranker_model_from_env(),
                fastembed::RerankerModel::BGERerankerBase
            ));
        });
    }

    #[test]
    fn reranker_mode_env_default_is_off() {
        temp_env::with_var("WENLAN_RERANKER_MODE", None::<&str>, || {
            assert_eq!(reranker_mode_from_env(), RerankerMode::Off);
        });
    }

    #[test]
    fn reranker_mode_env_parses_lite_and_full_case_insensitive() {
        for v in ["lite", "LITE", " Lite "] {
            temp_env::with_var("WENLAN_RERANKER_MODE", Some(v), || {
                assert_eq!(reranker_mode_from_env(), RerankerMode::Lite, "value {v:?}");
            });
        }
        for v in ["full", "FULL", " Full "] {
            temp_env::with_var("WENLAN_RERANKER_MODE", Some(v), || {
                assert_eq!(reranker_mode_from_env(), RerankerMode::Full, "value {v:?}");
            });
        }
    }

    #[test]
    fn reranker_mode_env_explicit_off_and_unknown_are_off() {
        for v in ["off", "OFF", "nonsense", ""] {
            temp_env::with_var("WENLAN_RERANKER_MODE", Some(v), || {
                assert_eq!(reranker_mode_from_env(), RerankerMode::Off, "value {v:?}");
            });
        }
    }

    #[test]
    fn resolve_plan_off_without_legacy_is_empty() {
        assert_eq!(
            resolve_reranker_plan(RerankerMode::Off, false),
            RerankerPlan::default()
        );
    }

    #[test]
    fn resolve_plan_off_with_legacy_is_deep_configured() {
        // back-compat: ENABLED=1 + mode unset == deep-only configured model (today).
        let p = resolve_reranker_plan(RerankerMode::Off, true);
        assert_eq!(p.light, None);
        assert_eq!(p.deep, Some(RerankerPick::Configured));
    }

    #[test]
    fn resolve_plan_lite_is_turbo_light_and_deep() {
        let p = resolve_reranker_plan(RerankerMode::Lite, false);
        assert_eq!(p.light, Some(RerankerPick::Turbo));
        assert_eq!(p.deep, Some(RerankerPick::Turbo));
    }

    #[test]
    fn resolve_plan_full_is_turbo_light_bgebase_deep() {
        let p = resolve_reranker_plan(RerankerMode::Full, false);
        assert_eq!(p.light, Some(RerankerPick::Turbo));
        assert_eq!(p.deep, Some(RerankerPick::BgeBase));
    }

    #[test]
    fn resolve_plan_explicit_mode_wins_over_legacy() {
        // mode set AND legacy enabled -> mode wins (not back-compat deep-only).
        let p = resolve_reranker_plan(RerankerMode::Full, true);
        assert_eq!(p.light, Some(RerankerPick::Turbo));
        assert_eq!(p.deep, Some(RerankerPick::BgeBase));
    }

    #[test]
    fn reranker_pick_fastembed_mapping() {
        assert!(matches!(
            RerankerPick::Turbo.fastembed_model(),
            Some(fastembed::RerankerModel::JINARerankerV1TurboEn)
        ));
        assert!(matches!(
            RerankerPick::BgeBase.fastembed_model(),
            Some(fastembed::RerankerModel::BGERerankerBase)
        ));
        assert!(RerankerPick::Configured.fastembed_model().is_none());
    }

    /// Smoke test for the real cross-encoder model. `#[ignore]` because the
    /// first construction downloads ~600MB of weights. Run manually with
    /// `cargo test -p wenlan-core --lib reranker::tests -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn cross_encoder_real_model_smoke() {
        let reranker = init_cross_encoder_reranker(None)
            .expect("cross-encoder init should succeed when weights are reachable");
        let candidates = vec![
            ("a".to_string(), "Rust is a systems language".to_string()),
            ("b".to_string(), "Pasta recipes".to_string()),
            ("c".to_string(), "Cargo manages Rust deps".to_string()),
        ];
        let out = reranker
            .rerank("rust programming", &candidates)
            .expect("rerank should not error on a healthy model");
        assert!(!out.is_empty(), "expected non-empty rerank output");
        let top_id = out[0].0.as_str();
        assert!(
            top_id == "a" || top_id == "c",
            "expected Rust-related doc on top, got {top_id}"
        );
    }
}
