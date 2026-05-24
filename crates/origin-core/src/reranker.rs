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
}
