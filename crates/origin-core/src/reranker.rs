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

use crate::error::OriginError;

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
    ) -> Result<Vec<(String, f32)>, OriginError>;

    fn model_id(&self) -> &str;
}

/// Passthrough reranker. Returns candidates unchanged with score = 0.0.
pub struct NoopReranker;

impl Reranker for NoopReranker {
    fn rerank(
        &self,
        _query: &str,
        candidates: &[(String, String)],
    ) -> Result<Vec<(String, f32)>, OriginError> {
        Ok(candidates.iter().map(|(id, _)| (id.clone(), 0.0)).collect())
    }

    fn model_id(&self) -> &str {
        "noop"
    }
}

/// Cross-encoder reranker via fastembed's `TextRerank`.
///
/// Model downloads on first construction. `BGERerankerV2M3` is ~568M params /
/// ~600MB on disk. Pass a `cache_dir` aligned with the embedder cache for
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
    ) -> Result<Self, OriginError> {
        let model_id = format!("{:?}", model);
        let mut options = fastembed::RerankInitOptions::new(model);
        if let Some(dir) = cache_dir {
            options = options.with_cache_dir(dir);
        }
        let inner = fastembed::TextRerank::try_new(options)
            .map_err(|e| OriginError::Llm(format!("CrossEncoderReranker init: {e}")))?;
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
    ) -> Result<Self, OriginError> {
        let read = |name: &str| -> Result<Vec<u8>, OriginError> {
            std::fs::read(onnx_dir.join(name))
                .map_err(|e| OriginError::Llm(format!("BYO reranker: read {name}: {e}")))
        };
        let onnx_path = onnx_dir.join("model.onnx");
        if !onnx_path.exists() {
            return Err(OriginError::Llm(format!(
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
        .map_err(|e| OriginError::Llm(format!("BYO reranker init: {e}")))?;
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
            return Err(OriginError::Llm(
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
    ) -> Result<Vec<(String, f32)>, OriginError> {
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

/// Resolve the cross-encoder model from `ORIGIN_RERANKER_MODEL`. Default (unset
/// or unrecognized) is `BGERerankerV2M3` — the larger multilingual model that
/// SuperLocalMemory's V3.3 ablation cited as the single largest contributor to
/// their retrieval stack — so production behaviour is unchanged.
///
/// `turbo` selects `JINARerankerV1TurboEn` (~37M params vs BGE-v2-m3's ~568M,
/// English-only): far faster on CPU, for eval A/B sweeps and CPU-only Linux +
/// Windows deployments where the 568M model would dominate latency. `bge-base`
/// selects the mid-size `BGERerankerBase`. The chosen model_id is stamped into
/// the reranker so eval baselines stay honest about which reranker produced them.
fn reranker_model_from_env() -> fastembed::RerankerModel {
    use fastembed::RerankerModel::{BGERerankerBase, BGERerankerV2M3, JINARerankerV1TurboEn};
    let raw = std::env::var("ORIGIN_RERANKER_MODEL").unwrap_or_default();
    match raw.trim().to_ascii_lowercase().as_str() {
        "turbo" | "jina-turbo" | "jina" => JINARerankerV1TurboEn,
        "bge-base" => BGERerankerBase,
        "" | "bge" | "bge-v2-m3" => BGERerankerV2M3,
        other => {
            log::warn!("[reranker] unknown ORIGIN_RERANKER_MODEL={other:?}; using BGERerankerV2M3");
            BGERerankerV2M3
        }
    }
}

/// Construct the cross-encoder reranker and return it as a trait object.
///
/// Model is selected by [`reranker_model_from_env`] — defaults to
/// `BGERerankerV2M3`. Pass `cache_dir` aligned with the embedder cache so the
/// weights download once and stay reusable across processes.
///
/// Not called in any default-running test — first construction downloads the
/// model weights. Callers must opt in (daemon startup, `--ignored` tests,
/// manual eval runs).
pub fn init_cross_encoder_reranker(
    cache_dir: Option<std::path::PathBuf>,
) -> Result<Arc<dyn Reranker>, OriginError> {
    // BYO opt-in: load a local ONNX cross-encoder from ORIGIN_RERANKER_ONNX_DIR,
    // bypassing the hf-hub downloader for Xet-backed models it can't fetch. Unset
    // (the production default) keeps the enum download path below.
    if let Ok(dir) = std::env::var("ORIGIN_RERANKER_ONNX_DIR") {
        let dir = std::path::PathBuf::from(dir);
        let model_id = std::env::var("ORIGIN_RERANKER_MODEL_ID").unwrap_or_else(|_| {
            dir.file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "user-defined".to_string())
        });
        let inner = CrossEncoderReranker::try_new_user_defined(&dir, model_id)
            .map_err(|e| OriginError::Llm(format!("init_cross_encoder_reranker (BYO): {e}")))?;
        log::info!("[reranker] loaded BYO model_id={}", inner.model_id());
        return Ok(Arc::new(inner) as Arc<dyn Reranker>);
    }
    let model = reranker_model_from_env();
    log::info!("[reranker] loading enum model {model:?} (download path)");
    let inner = CrossEncoderReranker::try_new(model, cache_dir)
        .map_err(|e| OriginError::Llm(format!("init_cross_encoder_reranker: {e}")))?;
    Ok(Arc::new(inner) as Arc<dyn Reranker>)
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
    fn reranker_model_env_default_is_bge_v2m3() {
        temp_env::with_var("ORIGIN_RERANKER_MODEL", None::<&str>, || {
            assert!(matches!(
                reranker_model_from_env(),
                fastembed::RerankerModel::BGERerankerV2M3
            ));
        });
    }

    #[test]
    fn reranker_model_env_turbo_selects_jina() {
        for v in ["turbo", "jina-turbo", "JINA", "Turbo"] {
            temp_env::with_var("ORIGIN_RERANKER_MODEL", Some(v), || {
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
    fn reranker_model_env_unknown_falls_back_to_bge() {
        temp_env::with_var("ORIGIN_RERANKER_MODEL", Some("nonsense"), || {
            assert!(matches!(
                reranker_model_from_env(),
                fastembed::RerankerModel::BGERerankerV2M3
            ));
        });
    }

    /// Smoke test for the real cross-encoder model. `#[ignore]` because the
    /// first construction downloads ~600MB of weights. Run manually with
    /// `cargo test -p origin-core --lib reranker::tests -- --ignored`.
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
