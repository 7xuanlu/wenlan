// SPDX-License-Identifier: Apache-2.0
//! Generic on-device LLM inference engine.
//!
//! Wraps `llama-cpp-2` with Metal GPU offload and provides a reusable
//! inference loop. Domain-specific operations (classification, KG extraction,
//! reranking, memory merging) live in sibling modules (`classify`, `extract`,
//! `rerank`, `merge`) and call through to [`LlmEngine::run_inference`] or
//! compose their own tokenize/decode loops against [`LlmEngine::model`] /
//! [`LlmEngine::backend`].

use crate::error::OriginError;
use crate::llm_provider::format_chatml_prompt;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::llama_backend::LlamaBackend;
use std::sync::{Arc, OnceLock};

/// Process-wide llama.cpp backend. Initialized lazily on first use and shared
/// across every `LlmEngine` instance in the process. `LlamaBackend::init()` is
/// a one-shot — calling it twice returns `BackendAlreadyInitialized`, which is
/// why we must go through this `OnceLock`.
static LLAMA_BACKEND: OnceLock<Arc<LlamaBackend>> = OnceLock::new();

fn shared_backend() -> Result<Arc<LlamaBackend>, OriginError> {
    // Fast path: already initialized.
    if let Some(b) = LLAMA_BACKEND.get() {
        return Ok(b.clone());
    }
    // Slow path: init and store. If another thread beats us here, our new
    // backend is dropped and we use theirs.
    match LlamaBackend::init() {
        Ok(backend) => {
            let arc = Arc::new(backend);
            let _ = LLAMA_BACKEND.set(arc.clone());
            // Another thread may have raced us; prefer the stored value.
            Ok(LLAMA_BACKEND.get().cloned().unwrap_or(arc))
        }
        Err(e) => {
            // If the backend was initialized elsewhere (e.g. in a test, in a
            // previous engine that didn't use this helper), we can't recover
            // its handle — fail loudly so the caller knows.
            Err(OriginError::Llm(format!("backend init: {e}")))
        }
    }
}
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel as LlamaCppModel};
use llama_cpp_2::sampling::LlamaSampler;

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Maximum input chars sent to the LLM (truncated at word boundary).
pub const MAX_INPUT_CHARS: usize = 20_000;
/// Maximum tokens the LLM can generate per request.
pub const MAX_OUTPUT_TOKENS: i32 = 2048;
/// Timeout for a single LLM inference call. 120s accommodates larger context
/// windows (16K+) where prompt prefill and generation take longer, especially
/// on first call after boot (Metal JIT shader compilation).
pub const INFERENCE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);
/// Context window size for the LLM.
pub const CTX_SIZE: u32 = 8192;

#[allow(dead_code)]
#[derive(Debug, serde::Deserialize)]
pub struct FormattedResult {
    pub formatted_text: String,
    pub summary: String,
    pub space: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub stream_name: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, serde::Deserialize)]
pub struct SessionSynthesisResult {
    pub summary: String,
    pub tags: Vec<String>,
}

/// Generic on-device LLM engine backed by a loaded GGUF model.
///
/// Construction is blocking — call [`LlmEngine::download_model`] and
/// [`LlmEngine::new`] from a dedicated initialization thread or
/// `spawn_blocking` context. Once constructed, inference methods are called
/// from a single worker thread; the engine is neither `Send` nor `Sync` by
/// default (the `unsafe impl` below is sound only because the app guarantees
/// single-threaded access after construction).
pub struct LlmEngine {
    /// Shared process-wide llama.cpp backend. Every `LlmEngine` holds an
    /// `Arc` to the same global backend (see [`shared_backend`]). This is
    /// required because `LlamaBackend::init()` can only be called once per
    /// process — attempting to construct a second fresh backend fails with
    /// `BackendAlreadyInitialized`.
    pub(crate) backend: Arc<LlamaBackend>,
    pub(crate) model: LlamaCppModel,
    pub(crate) prompts: crate::prompts::PromptRegistry,
}

// SAFETY: LlamaCppModel and LlamaBackend are created once on the init thread
// and then used exclusively on the worker thread. Any Arc<LlmEngine> is
// shared only so the app can hold a reference; inference is always called
// from the single worker thread.
unsafe impl Send for LlmEngine {}
unsafe impl Sync for LlmEngine {}

#[allow(dead_code)] // Full inference API -- only run_inference used currently via OnDeviceProvider
impl LlmEngine {
    /// Download a GGUF model via hf-hub by repo and filename.
    /// Uses the sync API (blocking). Cached in ~/.cache/huggingface/hub/.
    pub fn download_model_by_spec(repo_id: &str, filename: &str) -> Result<PathBuf, OriginError> {
        log::info!(
            "[llm_engine] downloading model {}/{} (cached if already present)...",
            repo_id,
            filename
        );

        let api = hf_hub::api::sync::Api::new()
            .map_err(|e| OriginError::Llm(format!("hf-hub init: {e}")))?;

        let repo = api.model(repo_id.to_string());

        let path = repo
            .get(filename)
            .map_err(|e| OriginError::Llm(format!("model download: {e}")))?;

        log::info!("[llm_engine] model path: {}", path.display());
        Ok(path)
    }

    /// Download the default on-device model (backward compat).
    pub fn download_model() -> Result<PathBuf, OriginError> {
        let model = crate::on_device_models::get_default_model();
        Self::download_model_by_spec(model.repo_id, model.filename)
    }

    /// Load the GGUF model with full Metal GPU offload.
    pub fn new(
        model_path: &Path,
        prompts: crate::prompts::PromptRegistry,
    ) -> Result<Self, OriginError> {
        log::info!("[llm_engine] acquiring shared backend...");

        let backend = shared_backend()?;

        let model_params = LlamaModelParams::default().with_n_gpu_layers(99);

        log::info!("[llm_engine] loading model...");
        let model = LlamaCppModel::load_from_file(&backend, model_path, &model_params)
            .map_err(|e| OriginError::Llm(format!("model load: {e}")))?;

        log::info!("[llm_engine] model loaded successfully");
        Ok(Self {
            backend,
            model,
            prompts,
        })
    }

    /// Probe whether Metal context creation works. Returns true if a minimal
    /// context can be allocated. Used by the auto-degrade pattern: if this fails,
    /// the caller can set GGML_METAL_BF16_DISABLE=1 and recreate the engine.
    pub fn probe_metal_context(&self) -> bool {
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(Some(NonZeroU32::new(512).unwrap()))
            .with_n_batch(512);
        self.model.new_context(&self.backend, ctx_params).is_ok()
    }

    /// Access the loaded llama-cpp model. Used by domain modules that need to
    /// run their own tokenize/decode loops (e.g. classification with a smaller
    /// per-call context window).
    pub(crate) fn model(&self) -> &LlamaCppModel {
        &self.model
    }

    /// Access the llama-cpp backend (needed when creating per-call contexts).
    pub(crate) fn backend(&self) -> &LlamaBackend {
        &self.backend
    }

    // Helper for methods that need to pass `&LlamaBackend` to llama_cpp_2 APIs.
    // Arc<LlamaBackend> derefs to LlamaBackend so `&*self.backend` gives the
    // right type.

    /// Access the shared prompt registry.
    pub(crate) fn prompts(&self) -> &crate::prompts::PromptRegistry {
        &self.prompts
    }

    /// Format raw OCR text using the LLM with unconstrained generation + JSON extraction.
    /// Returns None if inference fails, times out, or output is not valid JSON.
    #[allow(dead_code)]
    pub fn format_ocr_text(
        &self,
        raw_text: &str,
        app_name: &str,
        window_title: Option<&str>,
        spaces: &[String],
    ) -> Option<FormattedResult> {
        let start = Instant::now();

        // Input is already sanitized+structured from the capture pipeline
        // Truncate input at word boundary
        let truncated = truncate_at_word_boundary(raw_text, MAX_INPUT_CHARS);

        let window_title = window_title.unwrap_or("Unknown");

        // Build ChatML prompt for Qwen3 (with thinking disabled)
        let spaces_str = spaces.join(", ");
        let system_prompt = self
            .prompts
            .format_ocr_text
            .replace("{spaces}", &spaces_str);
        let prompt = format!(
            "<|im_start|>system\n\
             {system_prompt}\n\
             <|im_end|>\n\
             <|im_start|>user\n\
             App: {app_name}\n\
             Window: {window_title}\n\n\
             {truncated}\n\
             <|im_end|>\n\
             <|im_start|>assistant\n",
        );

        // Tokenize
        let tokens = match self.model.str_to_token(&prompt, AddBos::Always) {
            Ok(t) => t,
            Err(e) => {
                log::warn!("[llm_engine] tokenization failed: {e}");
                return None;
            }
        };

        log::info!("[llm_engine] prompt tokens={}", tokens.len());

        // Truncate prompt tokens so there's room for output within context window
        let max_prompt_tokens = CTX_SIZE as usize - MAX_OUTPUT_TOKENS as usize;
        let tokens = if tokens.len() > max_prompt_tokens {
            log::warn!(
                "[llm_engine] prompt tokens ({}) exceed budget ({}), truncating",
                tokens.len(),
                max_prompt_tokens
            );
            tokens[..max_prompt_tokens].to_vec()
        } else {
            tokens
        };

        // Create per-call context -- n_batch must be >= prompt token count
        let n_batch = tokens.len().max(512) as u32;
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(Some(NonZeroU32::new(CTX_SIZE).unwrap()))
            .with_n_batch(n_batch);

        let mut ctx = match self.model.new_context(&self.backend, ctx_params) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("[llm_engine] context creation failed: {e}");
                return None;
            }
        };

        // Fill batch with prompt tokens
        let mut batch = LlamaBatch::new(tokens.len(), 1);
        for (i, token) in tokens.iter().enumerate() {
            if let Err(e) = batch.add(*token, i as i32, &[0], i == tokens.len() - 1) {
                log::warn!("[llm_engine] batch add failed: {e}");
                return None;
            }
        }

        // Decode prompt
        if let Err(e) = ctx.decode(&mut batch) {
            log::warn!("[llm_engine] prompt decode failed: {e}");
            return None;
        }

        // Build sampler chain -- unconstrained generation (no grammar)
        let mut sampler =
            LlamaSampler::chain_simple([LlamaSampler::temp(0.3), LlamaSampler::dist(42)]);

        // Generate tokens
        let mut decoder = encoding_rs::UTF_8.new_decoder();
        let mut output = String::new();
        let mut n_cur = batch.n_tokens();

        while n_cur < MAX_OUTPUT_TOKENS {
            // Check timeout
            if start.elapsed() > INFERENCE_TIMEOUT {
                log::warn!("[llm_engine] inference timeout after {:?}", start.elapsed());
                break;
            }

            let token = sampler.sample(&ctx, batch.n_tokens() - 1);
            sampler.accept(token);

            if self.model.is_eog_token(token) {
                break;
            }

            match self.model.token_to_piece(token, &mut decoder, true, None) {
                Ok(piece) => output.push_str(&piece),
                Err(e) => {
                    log::warn!("[llm_engine] token decode failed: {e}");
                    break;
                }
            }

            batch.clear();
            if let Err(e) = batch.add(token, n_cur, &[0], true) {
                log::warn!("[llm_engine] batch add failed: {e}");
                break;
            }

            if let Err(e) = ctx.decode(&mut batch) {
                log::warn!("[llm_engine] decode failed: {e}");
                break;
            }

            n_cur += 1;
        }

        log::info!(
            "[llm_engine] generated {} chars in {:?}",
            output.len(),
            start.elapsed()
        );

        // Strip any residual <think> tags (safety net), then extract JSON
        let cleaned = strip_think_tags(&output);
        let json_str = extract_json(&cleaned).unwrap_or(&cleaned);

        // Parse JSON output
        match serde_json::from_str::<FormattedResult>(json_str) {
            Ok(result) => {
                if result.formatted_text.is_empty() {
                    log::debug!("[llm_engine] empty formatted_text, skipping");
                    return None;
                }
                Some(result)
            }
            Err(e) => {
                log::warn!(
                    "[llm_engine] JSON parse failed: {e}, output: {}",
                    &output[..output.floor_char_boundary(200)]
                );
                None
            }
        }
    }

    /// Generic inference helper: tokenize prompt, run generation, return raw output string.
    /// Used by refinement prompts (dedup merge, extract patterns, detect contradiction).
    pub fn run_inference(
        &self,
        prompt: &str,
        max_output_tokens: i32,
        temperature: f32,
        ctx_size: u32,
        label: Option<&str>,
    ) -> Option<String> {
        let start = Instant::now();

        let tokens = match self.model.str_to_token(prompt, AddBos::Always) {
            Ok(t) => t,
            Err(e) => {
                log::warn!("[llm_engine] inference tokenization failed: {e}");
                return None;
            }
        };

        let max_prompt_tokens = (ctx_size as usize).saturating_sub(max_output_tokens as usize);
        if max_prompt_tokens == 0 {
            log::warn!(
                "[llm_engine] max_output_tokens={} >= ctx_size={}, refusing to run inference",
                max_output_tokens,
                ctx_size
            );
            return None;
        }
        let tokens = if tokens.len() > max_prompt_tokens {
            tokens[..max_prompt_tokens].to_vec()
        } else {
            tokens
        };

        let n_batch = tokens.len().max(512) as u32;
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(Some(NonZeroU32::new(ctx_size).unwrap()))
            .with_n_batch(n_batch);

        let mut ctx = match self.model.new_context(&self.backend, ctx_params) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("[llm_engine] inference context failed: {e}");
                return None;
            }
        };

        let mut batch = LlamaBatch::new(tokens.len(), 1);
        for (i, token) in tokens.iter().enumerate() {
            if batch
                .add(*token, i as i32, &[0], i == tokens.len() - 1)
                .is_err()
            {
                return None;
            }
        }

        if ctx.decode(&mut batch).is_err() {
            return None;
        }

        let mut sampler = LlamaSampler::chain_simple([
            LlamaSampler::penalties(256, 1.2, 0.0, 0.0),
            LlamaSampler::temp(temperature),
            LlamaSampler::dist(42),
        ]);

        let mut decoder = encoding_rs::UTF_8.new_decoder();
        let mut output = String::new();
        let mut n_cur = batch.n_tokens();
        let max_pos = n_cur + max_output_tokens;

        while n_cur < max_pos {
            if start.elapsed() > std::time::Duration::from_secs(30) {
                log::warn!("[llm_engine] inference timeout");
                break;
            }

            let token = sampler.sample(&ctx, batch.n_tokens() - 1);
            sampler.accept(token);

            if self.model.is_eog_token(token) {
                break;
            }

            match self.model.token_to_piece(token, &mut decoder, true, None) {
                Ok(piece) => output.push_str(&piece),
                Err(_) => break,
            }

            batch.clear();
            if batch.add(token, n_cur, &[0], true).is_err() {
                break;
            }
            if ctx.decode(&mut batch).is_err() {
                break;
            }
            n_cur += 1;
        }

        let cleaned = strip_think_tags(&output);
        let trimmed = cleaned.trim().to_string();

        let label_suffix = label.map(|l| format!(" [{}]", l)).unwrap_or_default();
        log::info!(
            "[llm_engine] inference{}: {} chars in {:?}",
            label_suffix,
            trimmed.len(),
            start.elapsed()
        );

        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }

    /// Build a long-lived context that the caller owns. Designed for the
    /// `OnDeviceProvider` worker thread: one allocation, then `clear_kv_cache()`
    /// between requests instead of `new_context()` every call.
    ///
    /// `n_batch` is set to `ctx_size` so any prompt up to the context window
    /// fits in a single `decode()` call. `n_ubatch` defaults to the same value.
    ///
    /// Returns `None` if Metal context creation fails (caller should log and
    /// fall back to per-call context creation, which is still valid).
    pub fn build_persistent_context(&self, ctx_size: u32) -> Option<LlamaContext<'_>> {
        self.build_persistent_context_with_seq_max(ctx_size, 1)
    }

    /// Build a long-lived context with `n_seq_max` parallel sequence slots.
    /// Used by the continuous-batching worker (Option B / S2): one
    /// `LlamaContext` decodes up to `n_seq_max` independent sequences in
    /// parallel via llama.cpp's continuous-batching scheduler.
    ///
    /// At `n_seq_max == 1` this is byte-equivalent to `build_persistent_context`
    /// (the underlying llama.cpp default for `n_seq_max` is 1). The KV cache
    /// budget per sequence is `ctx_size / n_seq_max` — callers must enforce
    /// per-seq prompt+output bounds accordingly.
    pub fn build_persistent_context_with_seq_max(
        &self,
        ctx_size: u32,
        n_seq_max: u32,
    ) -> Option<LlamaContext<'_>> {
        let params = LlamaContextParams::default()
            .with_n_ctx(Some(NonZeroU32::new(ctx_size)?))
            .with_n_batch(ctx_size)
            .with_n_seq_max(n_seq_max.max(1));
        match self.model.new_context(&self.backend, params) {
            Ok(c) => Some(c),
            Err(e) => {
                log::warn!(
                    "[llm_engine] persistent context creation failed (ctx_size={ctx_size}, \
                     n_seq_max={n_seq_max}): {e}"
                );
                None
            }
        }
    }

    /// Run inference reusing a caller-owned `LlamaContext`. The KV cache is
    /// cleared at the start of each call, so callers must not assume any
    /// session state persists between invocations. Saves the per-call cost of
    /// `new_context()` (KV allocation + Metal pipeline setup), which is the
    /// dominant overhead for short inference calls (~5-20s on M2 Pro for
    /// quantized models at 8K context).
    ///
    /// `timeout_secs` and `strip_think` mirror the choices in `run_inference`
    /// (30s + strip) vs `run_inference_raw` (configurable + raw). The worker
    /// thread routes by `LlmRequest::timeout_secs` so we cover both paths.
    #[allow(clippy::too_many_arguments)]
    pub fn run_inference_persistent(
        &self,
        ctx: &mut LlamaContext<'_>,
        prompt: &str,
        max_output_tokens: i32,
        temperature: f32,
        timeout_secs: u64,
        strip_think: bool,
        label: Option<&str>,
    ) -> Option<String> {
        let start = Instant::now();

        // Reset KV cache from previous request. Cheap (no allocation) compared
        // to creating a new context.
        ctx.clear_kv_cache();

        let tokens = match self.model.str_to_token(prompt, AddBos::Always) {
            Ok(t) => t,
            Err(e) => {
                log::warn!("[llm_engine] persistent tokenization failed: {e}");
                return None;
            }
        };

        let ctx_size = ctx.n_ctx();
        let max_prompt_tokens = (ctx_size as usize).saturating_sub(max_output_tokens as usize);
        if max_prompt_tokens == 0 {
            log::warn!(
                "[llm_engine] max_output_tokens={} >= ctx_size={}, refusing to run inference",
                max_output_tokens,
                ctx_size
            );
            return None;
        }
        let tokens = if tokens.len() > max_prompt_tokens {
            tokens[..max_prompt_tokens].to_vec()
        } else {
            tokens
        };

        let mut batch = LlamaBatch::new(tokens.len(), 1);
        for (i, token) in tokens.iter().enumerate() {
            if batch
                .add(*token, i as i32, &[0], i == tokens.len() - 1)
                .is_err()
            {
                return None;
            }
        }

        if ctx.decode(&mut batch).is_err() {
            return None;
        }

        let mut sampler = LlamaSampler::chain_simple([
            LlamaSampler::penalties(256, 1.2, 0.0, 0.0),
            LlamaSampler::temp(temperature),
            LlamaSampler::dist(42),
        ]);

        let mut decoder = encoding_rs::UTF_8.new_decoder();
        let mut output = String::new();
        let mut n_cur = batch.n_tokens();
        let max_pos = n_cur + max_output_tokens;
        let mut failed = false;

        while n_cur < max_pos {
            if start.elapsed() > std::time::Duration::from_secs(timeout_secs) {
                log::warn!(
                    "[llm_engine] persistent inference timeout at {}s",
                    timeout_secs
                );
                break;
            }

            let token = sampler.sample(ctx, batch.n_tokens() - 1);
            sampler.accept(token);

            if self.model.is_eog_token(token) {
                break;
            }

            match self.model.token_to_piece(token, &mut decoder, true, None) {
                Ok(piece) => output.push_str(&piece),
                Err(e) => {
                    log::warn!("[llm_engine] persistent token decode failed: {e}");
                    failed = true;
                    break;
                }
            }

            batch.clear();
            if batch.add(token, n_cur, &[0], true).is_err() {
                failed = true;
                break;
            }
            if ctx.decode(&mut batch).is_err() {
                failed = true;
                break;
            }
            n_cur += 1;
        }

        let label_suffix = label.map(|l| format!(" [{}]", l)).unwrap_or_default();
        if failed {
            log::warn!(
                "[llm_engine] persistent inference{} failed after {} partial chars in {:?}",
                label_suffix,
                output.len(),
                start.elapsed()
            );
            return None;
        }

        if strip_think {
            let cleaned = strip_think_tags(&output);
            let trimmed = cleaned.trim().to_string();
            log::info!(
                "[llm_engine] persistent inference{}: {} chars in {:?}",
                label_suffix,
                trimmed.len(),
                start.elapsed()
            );
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        } else {
            log::info!(
                "[llm_engine] persistent raw inference{}: {} chars in {:?}",
                label_suffix,
                output.len(),
                start.elapsed()
            );
            if output.is_empty() {
                None
            } else {
                Some(output)
            }
        }
    }

    /// Count tokens for the formatted ChatML prompt used by the provider path.
    pub fn count_prompt_tokens(&self, system_prompt: Option<&str>, prompt: &str) -> usize {
        let formatted = format_chatml_prompt(system_prompt, prompt);
        self.model
            .str_to_token(&formatted, AddBos::Always)
            .map(|t| t.len())
            .unwrap_or(usize::MAX)
    }

    /// Run continuous-batch inference over multiple prompts in a single context.
    ///
    /// This is the Option B (S2) entry point: one `LlamaContext` (built with
    /// `n_seq_max >= prompts.len()`) decodes all input prompts in parallel via
    /// llama.cpp's continuous-batching scheduler. Each prompt occupies a
    /// distinct `seq_id` slot; their sampled tokens are demultiplexed back to
    /// per-prompt output strings.
    ///
    /// Inputs:
    /// - `ctx`: a context built with `build_persistent_context_with_seq_max`.
    /// - `prompts`: list of (prompt, max_output_tokens, temperature, timeout_secs,
    ///   strip_think, label) tuples — one per sequence.
    /// - `seq_capacity`: the configured `n_seq_max` for the context.
    ///
    /// Returns a vector aligned with `prompts`: each element is `Some(output)`
    /// if that sequence finished successfully, `None` if it timed out or was
    /// refused for budget. The KV cache is fully cleared on
    /// entry so previous decodes do not leak into this batch.
    ///
    /// Per-seq budget math: each prompt's token count must fit
    /// `(ctx_size / seq_capacity) - max_output_tokens`. This intentionally uses
    /// configured capacity, not current batch size, so refusal behavior does
    /// not depend on queue timing.
    #[allow(clippy::type_complexity)]
    pub fn run_inference_continuous_batch(
        &self,
        ctx: &mut LlamaContext<'_>,
        prompts: &[(String, i32, f32, u64, bool, Option<String>)],
        seq_capacity: usize,
    ) -> Vec<Option<String>> {
        let batch_start = Instant::now();
        let n_seqs = prompts.len();
        if n_seqs == 0 {
            return Vec::new();
        }

        // Reset KV cache from any previous batch.
        ctx.clear_kv_cache();

        let ctx_size = ctx.n_ctx();
        // `m` = configured parallelism = number of physical KV slots the
        // context was allocated with (n_seq_max). The KV cache is partitioned
        // into `m` slots, each capped at ctx/m (per-slot cap, not pooled —
        // proven by GPU probe a4db3a65), so the per-seq budget divides by `m`
        // regardless of how many requests the queue holds. When the queue is
        // longer than `m`, slot backfill reuses these same `m` slots.
        let m = seq_capacity.max(1);
        let max_per_seq = (ctx_size as usize) / m;
        log::debug!(
            "[llm_engine] continuous batch: {n_seqs} reqs, m={m}, ctx_size={ctx_size}, \
             per-seq budget={max_per_seq}"
        );

        // Tokenize each prompt and refuse prompts that exceed the per-seq cap.
        let mut tokenized: Vec<Vec<llama_cpp_2::token::LlamaToken>> = Vec::with_capacity(n_seqs);
        let mut max_output_per_seq: Vec<i32> = Vec::with_capacity(n_seqs);
        let mut temperatures: Vec<f32> = Vec::with_capacity(n_seqs);
        let mut timeouts: Vec<u64> = Vec::with_capacity(n_seqs);
        let mut strip_think_flags: Vec<bool> = Vec::with_capacity(n_seqs);
        let mut labels: Vec<Option<String>> = Vec::with_capacity(n_seqs);
        let mut failed: Vec<bool> = vec![false; n_seqs];

        for (seq_id, (prompt, max_out, temp, timeout_secs, strip_think, label)) in
            prompts.iter().enumerate()
        {
            let max_out_usize = (*max_out).max(0) as usize;
            let max_prompt_tokens = max_per_seq.saturating_sub(max_out_usize);
            if max_prompt_tokens == 0 {
                log::warn!(
                    "[llm_engine] continuous: max_output_tokens={} >= per-seq budget={}, \
                     refusing seq",
                    max_out,
                    max_per_seq
                );
                tokenized.push(Vec::new());
                max_output_per_seq.push(*max_out);
                temperatures.push(*temp);
                timeouts.push(*timeout_secs);
                strip_think_flags.push(*strip_think);
                labels.push(label.clone());
                failed[seq_id] = true;
                continue;
            }
            let tokens = match self.model.str_to_token(prompt, AddBos::Always) {
                Ok(t) => t,
                Err(e) => {
                    log::warn!("[llm_engine] continuous tokenize failed: {e}");
                    failed[seq_id] = true;
                    Vec::new()
                }
            };
            let tokens = if tokens.len() > max_prompt_tokens {
                log::warn!(
                    "[llm_engine] continuous: prompt tokens ({}) exceed per-seq budget ({}), \
                     refusing seq",
                    tokens.len(),
                    max_prompt_tokens
                );
                failed[seq_id] = true;
                Vec::new()
            } else {
                tokens
            };
            tokenized.push(tokens);
            max_output_per_seq.push(*max_out);
            temperatures.push(*temp);
            timeouts.push(*timeout_secs);
            strip_think_flags.push(*strip_think);
            labels.push(label.clone());
        }

        // Total prefill tokens across all valid sequences (each valid prompt is
        // prefilled exactly once, even when backfilled into a reused slot).
        let total_prefill: usize = tokenized.iter().map(|t| t.len()).sum();

        // Live request queue: indices with a non-empty tokenization. Failed /
        // over-budget requests keep `failed[req]=true` and an empty output and
        // are never scheduled onto a slot.
        let valid_reqs: Vec<usize> = (0..n_seqs).filter(|&r| !tokenized[r].is_empty()).collect();
        let n_valid = valid_reqs.len();
        if n_valid == 0 {
            // All sequences were rejected (empty prompts or budget exhaustion).
            return vec![None; n_seqs];
        }

        // Physical sequence slots driven this call. Capped at `m` (the context's
        // n_seq_max); when the queue is longer than `m`, slot backfill reuses
        // these same slots so decode width stays at `n_slots` until the queue
        // drains, instead of raggedly draining as short outputs finish first.
        let n_slots = m.min(n_valid);

        // One decode iteration may prefill freed slots (each up to max_per_seq
        // tokens) plus continue the rest (one token each); that total is bounded
        // by the context window. Size the batch for the worst case once.
        let batch_capacity = (ctx_size as usize).max(n_slots);
        let mut batch = LlamaBatch::new(batch_capacity, n_slots as i32);

        // Per-REQUEST output state (indexed by original request index).
        let mut outputs: Vec<String> = vec![String::new(); n_seqs];
        let mut tokens_generated: Vec<i32> = vec![0; n_seqs];

        // Slot-backfill scheduler keeps every physical slot full from the
        // `valid_reqs` queue. When `n_valid <= n_slots` the queue is empty after
        // the initial fill and no backfill ever happens — that path is
        // byte-identical to the pre-backfill single-batch behavior.
        let mut sched = BackfillScheduler::new(n_valid, n_slots);

        // Per-slot llama state, rebuilt on backfill. `None` = free slot. `pos`
        // is the scheduler's request index = a position into `valid_reqs`.
        struct SlotState {
            pos: usize,
            sampler: LlamaSampler,
            decoder: encoding_rs::Decoder,
            n_past: i32,
            logits_idx: i32,
            tokens_gen: i32,
            start_time: Instant,
            needs_prefill: bool,
            pending_token: Option<llama_cpp_2::token::LlamaToken>,
        }
        // Sampler seed = 42 + original request index, so the no-overflow path
        // reproduces the previous per-seq seeds (seq_id == request index) exactly.
        fn make_slot_state(pos: usize, valid_reqs: &[usize], temperatures: &[f32]) -> SlotState {
            let req = valid_reqs[pos];
            SlotState {
                pos,
                sampler: LlamaSampler::chain_simple([
                    LlamaSampler::penalties(256, 1.2, 0.0, 0.0),
                    LlamaSampler::temp(temperatures[req]),
                    LlamaSampler::dist(42 + req as u32),
                ]),
                decoder: encoding_rs::UTF_8.new_decoder(),
                n_past: 0,
                logits_idx: -1,
                tokens_gen: 0,
                start_time: Instant::now(),
                needs_prefill: true,
                pending_token: None,
            }
        }
        let mut slots: Vec<Option<SlotState>> = (0..n_slots)
            .map(|slot| {
                sched
                    .request_in_slot(slot)
                    .map(|pos| make_slot_state(pos, &valid_reqs, &temperatures))
            })
            .collect();

        // [validation instrumentation] prefill-vs-decode wall-time split (gated
        // by ORIGIN_BATCH_LOG; no behavior change). prep = tokenize + build
        // (CPU); prefill = the first (initial-fill) decode; decode = the rest of
        // the generation loop, including backfill prefills.
        let prep_ms = batch_start.elapsed().as_millis();
        let gen_start = Instant::now();
        let mut prefill_ms: u128 = 0;
        let mut first_decode = true;

        // Unified continuous-batch loop. (A) Sample one token for every slot
        // with fresh logits, retiring + backfilling finished ones. (B) Build one
        // batch holding the continuation token for each surviving slot plus the
        // full prompt for each freshly-backfilled (or initial) slot, then a
        // single decode. Repeats until the queue drains and all slots free.
        loop {
            // (A) Sample, advance, retire/backfill. Each slot's borrow ends
            // before the (sched + slots) backfill mutation via `should_retire`.
            // `slot` is the llama seq_id (passed to sched.retire), not just an
            // index, so the range loop is the clearest form here.
            #[allow(clippy::needless_range_loop)]
            for slot in 0..n_slots {
                let needs_prefill = match slots[slot].as_ref() {
                    Some(st) => st.needs_prefill,
                    None => continue, // free slot
                };
                if needs_prefill {
                    continue; // awaiting its first (prefill) decode — no logits yet
                }
                let st = slots[slot].as_mut().unwrap();
                let req = valid_reqs[st.pos];

                let should_retire = if st.start_time.elapsed().as_secs() > timeouts[req] {
                    log::warn!(
                        "[llm_engine] continuous seq {req} timeout at {}s",
                        timeouts[req]
                    );
                    failed[req] = true;
                    true
                } else if st.logits_idx < 0 {
                    log::warn!("[llm_engine] continuous seq {req} has no logits row");
                    failed[req] = true;
                    true
                } else {
                    let token = st.sampler.sample(ctx, st.logits_idx);
                    st.sampler.accept(token);
                    if self.model.is_eog_token(token) {
                        true
                    } else {
                        match self
                            .model
                            .token_to_piece(token, &mut st.decoder, true, None)
                        {
                            Ok(piece) => {
                                outputs[req].push_str(&piece);
                                st.tokens_gen += 1;
                                tokens_generated[req] = st.tokens_gen;
                                if st.tokens_gen >= max_output_per_seq[req] {
                                    true
                                } else {
                                    st.pending_token = Some(token);
                                    false
                                }
                            }
                            Err(_) => {
                                failed[req] = true;
                                true
                            }
                        }
                    }
                };

                if should_retire {
                    match sched.retire(slot) {
                        Some(pos) => {
                            slots[slot] = Some(make_slot_state(pos, &valid_reqs, &temperatures))
                        }
                        None => slots[slot] = None,
                    }
                }
            }

            // (B) Build the next batch (prefill freshly-filled slots, continue
            // the rest) and decode once.
            batch.clear();
            let mut batch_pos: i32 = 0;
            let mut staged = false;
            // `slot` is the llama seq_id (passed to batch.add and sched.retire),
            // not just an index, so the range loop is the clearest form here.
            #[allow(clippy::needless_range_loop)]
            for slot in 0..n_slots {
                let Some(st) = slots[slot].as_mut() else {
                    continue;
                };
                let req = valid_reqs[st.pos];
                if st.needs_prefill {
                    // Wipe any prior occupant's KV for this physical slot before
                    // prefilling the new request. The slot's seq_id is reused on
                    // backfill, so stale KV from the finished request — at the
                    // same positions the new prompt writes — would corrupt this
                    // request's attention. (Initial-fill slots are already clean
                    // from the entry `clear_kv_cache()`; clearing again is a
                    // harmless no-op.)
                    let _ = ctx.clear_kv_cache_seq(Some(slot as u32), None, None);
                    let tokens = &tokenized[req];
                    let mut prefill_ok = true;
                    // Tokens actually written to the batch buffer so far. Advance
                    // `batch_pos` by this even on a mid-prefill failure: the
                    // partial tokens still occupy batch rows, so a later slot's
                    // `logits_idx` must account for them. (The bound
                    // `batch_capacity = ctx_size` makes failure unreachable, but
                    // this keeps `batch_pos` honest if that ever changes.)
                    let mut added: i32 = 0;
                    for (i, token) in tokens.iter().enumerate() {
                        let is_last = i == tokens.len() - 1;
                        if let Err(e) = batch.add(*token, i as i32, &[slot as i32], is_last) {
                            log::warn!("[llm_engine] continuous prefill batch.add failed: {e}");
                            failed[req] = true;
                            prefill_ok = false;
                            break;
                        }
                        if is_last {
                            st.logits_idx = batch_pos + i as i32;
                        }
                        added += 1;
                    }
                    batch_pos += added;
                    if !prefill_ok {
                        match sched.retire(slot) {
                            Some(pos) => {
                                slots[slot] = Some(make_slot_state(pos, &valid_reqs, &temperatures))
                            }
                            None => slots[slot] = None,
                        }
                        continue;
                    }
                    st.n_past = tokens.len() as i32;
                    st.needs_prefill = false;
                    staged = true;
                } else if let Some(token) = st.pending_token.take() {
                    if let Err(e) = batch.add(token, st.n_past, &[slot as i32], true) {
                        log::warn!(
                            "[llm_engine] continuous decode batch.add failed (seq {req}): {e}"
                        );
                        failed[req] = true;
                        match sched.retire(slot) {
                            Some(pos) => {
                                slots[slot] = Some(make_slot_state(pos, &valid_reqs, &temperatures))
                            }
                            None => slots[slot] = None,
                        }
                        continue;
                    }
                    st.logits_idx = batch_pos;
                    batch_pos += 1;
                    st.n_past += 1;
                    staged = true;
                }
            }

            if slots.iter().all(|s| s.is_none()) {
                // Queue drained and every slot finished.
                break;
            }
            if !staged {
                // Nothing decodable this pass (e.g. a slot was just backfilled
                // after a prefill error and awaits its prefill next pass). Don't
                // decode an empty batch; loop again rather than dropping the
                // still-queued requests.
                continue;
            }

            if let Err(e) = ctx.decode(&mut batch) {
                log::warn!("[llm_engine] continuous decode failed: {e}");
                // All slots still holding a request are unrecoverable.
                for cell in slots.iter_mut() {
                    if let Some(st) = cell.as_ref() {
                        failed[valid_reqs[st.pos]] = true;
                    }
                    *cell = None;
                }
                break;
            }
            if first_decode {
                prefill_ms = gen_start.elapsed().as_millis();
                first_decode = false;
            }
        }

        // [validation instrumentation] prefill-vs-decode split per batch.
        if std::env::var("ORIGIN_BATCH_LOG").is_ok() {
            let decode_ms = gen_start.elapsed().as_millis();
            let out_tok: i32 = tokens_generated.iter().sum();
            eprintln!(
                "[batch_timing] n_seqs={n_seqs} prefill_tok={total_prefill} out_tok={out_tok} \
                 prep_ms={prep_ms} prefill_ms={prefill_ms} decode_ms={decode_ms} total_ms={}",
                batch_start.elapsed().as_millis()
            );
        }

        // Free each physical slot's KV cache so the next batch reuses slots
        // cleanly. (clear_kv_cache() at the next entry would do this anyway, but
        // explicit per-slot removal keeps the invariant tight if the same ctx is
        // later used at a different M.) Only `n_slots` seq ids were ever used.
        for slot in 0..n_slots {
            let _ = ctx.clear_kv_cache_seq(Some(slot as u32), None, None);
        }

        // Apply strip_think + trimming per seq, mirroring run_inference_persistent.
        Self::finalize_continuous_batch_outputs(
            outputs,
            &failed,
            &strip_think_flags,
            &labels,
            batch_start,
        )
    }

    fn finalize_continuous_batch_outputs(
        outputs: Vec<String>,
        failed: &[bool],
        strip_think_flags: &[bool],
        labels: &[Option<String>],
        batch_start: Instant,
    ) -> Vec<Option<String>> {
        outputs
            .into_iter()
            .enumerate()
            .map(|(seq_id, raw)| {
                let label_suffix = labels[seq_id]
                    .as_deref()
                    .map(|l| format!(" [{}]", l))
                    .unwrap_or_default();

                if failed.get(seq_id).copied().unwrap_or(false) {
                    log::warn!(
                        "[llm_engine] continuous inference{} failed (seq={}, partial_chars={}, batch={:?})",
                        label_suffix,
                        seq_id,
                        raw.len(),
                        batch_start.elapsed()
                    );
                    return None;
                }

                if strip_think_flags[seq_id] {
                    let cleaned = strip_think_tags(&raw);
                    let trimmed = cleaned.trim().to_string();
                    log::info!(
                        "[llm_engine] continuous inference{}: {} chars (seq={}, batch={:?})",
                        label_suffix,
                        trimmed.len(),
                        seq_id,
                        batch_start.elapsed()
                    );
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed)
                    }
                } else {
                    log::info!(
                        "[llm_engine] continuous raw inference{}: {} chars (seq={}, batch={:?})",
                        label_suffix,
                        raw.len(),
                        seq_id,
                        batch_start.elapsed()
                    );
                    if raw.is_empty() {
                        None
                    } else {
                        Some(raw)
                    }
                }
            })
            .collect()
    }

    /// Benchmark-focused inference: configurable timeout, larger context, and
    /// returns raw output (caller decides whether to strip think tags).
    pub fn run_inference_raw(
        &self,
        prompt: &str,
        max_output_tokens: i32,
        temperature: f32,
        timeout_secs: u64,
        ctx_size: u32,
    ) -> Option<String> {
        let start = Instant::now();

        let tokens = match self.model.str_to_token(prompt, AddBos::Always) {
            Ok(t) => t,
            Err(e) => {
                log::warn!("[llm_engine] raw tokenization failed: {e}");
                return None;
            }
        };

        let max_prompt_tokens = (ctx_size as usize).saturating_sub(max_output_tokens as usize);
        if max_prompt_tokens == 0 {
            log::warn!(
                "[llm_engine] max_output_tokens={} >= ctx_size={}, refusing to run inference",
                max_output_tokens,
                ctx_size
            );
            return None;
        }
        let tokens = if tokens.len() > max_prompt_tokens {
            tokens[..max_prompt_tokens].to_vec()
        } else {
            tokens
        };

        let n_batch = tokens.len().max(512) as u32;
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(Some(NonZeroU32::new(ctx_size).unwrap()))
            .with_n_batch(n_batch);

        let mut ctx = match self.model.new_context(&self.backend, ctx_params) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("[llm_engine] raw context failed: {e}");
                return None;
            }
        };

        let mut batch = LlamaBatch::new(tokens.len(), 1);
        for (i, token) in tokens.iter().enumerate() {
            if batch
                .add(*token, i as i32, &[0], i == tokens.len() - 1)
                .is_err()
            {
                return None;
            }
        }

        if ctx.decode(&mut batch).is_err() {
            return None;
        }

        let mut sampler = LlamaSampler::chain_simple([
            LlamaSampler::penalties(256, 1.2, 0.0, 0.0),
            LlamaSampler::temp(temperature),
            LlamaSampler::dist(42),
        ]);

        let mut decoder = encoding_rs::UTF_8.new_decoder();
        let mut output = String::new();
        let mut n_cur = batch.n_tokens();
        let max_pos = n_cur + max_output_tokens;

        while n_cur < max_pos {
            if start.elapsed() > std::time::Duration::from_secs(timeout_secs) {
                log::warn!("[llm_engine] raw inference timeout at {}s", timeout_secs);
                break;
            }

            let token = sampler.sample(&ctx, batch.n_tokens() - 1);
            sampler.accept(token);

            if self.model.is_eog_token(token) {
                break;
            }

            match self.model.token_to_piece(token, &mut decoder, true, None) {
                Ok(piece) => output.push_str(&piece),
                Err(_) => break,
            }

            batch.clear();
            if batch.add(token, n_cur, &[0], true).is_err() {
                break;
            }
            if ctx.decode(&mut batch).is_err() {
                break;
            }
            n_cur += 1;
        }

        log::info!(
            "[llm_engine] raw inference: {} chars in {:?}",
            output.len(),
            start.elapsed()
        );

        if output.is_empty() {
            None
        } else {
            Some(output)
        }
    }

    /// Synthesize a session summary from an activity log.
    /// Returns None if inference fails.
    #[allow(dead_code)]
    pub fn format_session(&self, raw_text: &str, app_name: &str) -> Option<SessionSynthesisResult> {
        let start = Instant::now();

        let truncated = truncate_at_word_boundary(raw_text, MAX_INPUT_CHARS);

        // Build ChatML prompt for session synthesis
        let user_prompt = self
            .prompts
            .summarize_activity_user
            .replace("{apps}", app_name)
            .replace("{log}", truncated);
        let prompt = format!(
            "<|im_start|>system\n\
             {sys}\n\
             <|im_end|>\n\
             <|im_start|>user\n\
             {user_prompt}\n\
             <|im_end|>\n\
             <|im_start|>assistant\n{{\"summary\": \"",
            sys = self.prompts.summarize_activity_system,
        );

        // Tokenize
        let tokens = match self.model.str_to_token(&prompt, AddBos::Always) {
            Ok(t) => t,
            Err(e) => {
                log::warn!("[llm_engine] session tokenization failed: {e}");
                return None;
            }
        };

        log::info!("[llm_engine] session prompt tokens={}", tokens.len());

        // Truncate if needed
        let max_prompt_tokens = CTX_SIZE as usize - MAX_OUTPUT_TOKENS as usize;
        let tokens = if tokens.len() > max_prompt_tokens {
            tokens[..max_prompt_tokens].to_vec()
        } else {
            tokens
        };

        // Create context
        let n_batch = tokens.len().max(512) as u32;
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(Some(NonZeroU32::new(CTX_SIZE).unwrap()))
            .with_n_batch(n_batch);

        let mut ctx = match self.model.new_context(&self.backend, ctx_params) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("[llm_engine] session context creation failed: {e}");
                return None;
            }
        };

        // Fill and decode prompt
        let mut batch = LlamaBatch::new(tokens.len(), 1);
        for (i, token) in tokens.iter().enumerate() {
            if let Err(e) = batch.add(*token, i as i32, &[0], i == tokens.len() - 1) {
                log::warn!("[llm_engine] session batch add failed: {e}");
                return None;
            }
        }

        if let Err(e) = ctx.decode(&mut batch) {
            log::warn!("[llm_engine] session prompt decode failed: {e}");
            return None;
        }

        // Generate
        let mut sampler =
            LlamaSampler::chain_simple([LlamaSampler::temp(0.3), LlamaSampler::dist(42)]);

        let mut decoder = encoding_rs::UTF_8.new_decoder();
        let mut output = String::new();
        let mut n_cur = batch.n_tokens();

        while n_cur < MAX_OUTPUT_TOKENS {
            if start.elapsed() > INFERENCE_TIMEOUT {
                log::warn!("[llm_engine] session inference timeout");
                break;
            }

            let token = sampler.sample(&ctx, batch.n_tokens() - 1);
            sampler.accept(token);

            if self.model.is_eog_token(token) {
                break;
            }

            match self.model.token_to_piece(token, &mut decoder, true, None) {
                Ok(piece) => output.push_str(&piece),
                Err(e) => {
                    log::warn!("[llm_engine] session token decode failed: {e}");
                    break;
                }
            }

            batch.clear();
            if batch.add(token, n_cur, &[0], true).is_err() {
                break;
            }

            if ctx.decode(&mut batch).is_err() {
                break;
            }

            n_cur += 1;
        }

        log::info!(
            "[llm_engine] session generated {} chars in {:?}",
            output.len(),
            start.elapsed()
        );

        // Strip any residual <think> tags, then parse JSON
        // Prompt starts assistant with {"summary": " so prepend it
        let cleaned = strip_think_tags(&output);
        let full_output = format!("{{\"summary\": \"{}", cleaned);
        let json_str = extract_json(&full_output).unwrap_or(&full_output);
        match serde_json::from_str::<SessionSynthesisResult>(json_str) {
            Ok(result) => {
                log::info!("[llm_engine] session synthesis: \"{}\"", result.summary);
                Some(result)
            }
            Err(e) => {
                log::warn!(
                    "[llm_engine] session JSON parse failed: {e}, output: {}",
                    &output[..output.floor_char_boundary(200)]
                );
                None
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Shared parsing and text helpers
// ---------------------------------------------------------------------------

/// Strip any `<think>...</think>` blocks from LLM output (safety net for Qwen3).
pub fn strip_think_tags(text: &str) -> String {
    let mut result = text.to_string();
    while let Some(start) = result.find("<think>") {
        if let Some(end_offset) = result[start..].find("</think>") {
            let end = start + end_offset + "</think>".len();
            result = format!("{}{}", &result[..start], &result[end..]);
        } else {
            // Unclosed <think> -- strip from <think> to end
            result.truncate(start);
            break;
        }
    }
    result
}

/// Try to extract a JSON object from text that may contain markdown fences or preamble.
/// Finds the first `{` and last `}` and returns the substring.
pub fn extract_json(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end > start {
        Some(&text[start..=end])
    } else {
        None
    }
}

/// Extract a JSON array from text that may have surrounding prose.
/// Finds the first `[` and last `]` and returns the substring.
/// Falls back to wrapping a single JSON object `{...}` in array brackets,
/// since small on-device models (e.g., Qwen3-4B) often return a single
/// object instead of an array when given a single input item.
pub fn extract_json_array(text: &str) -> Option<String> {
    // Strip markdown code fences (Qwen3.5-9B wraps output in ```json...```).
    // Find first JSON-relevant char (`[` or `{`) and last `]` or `}` to
    // narrow the window. The streaming Deserializer (Strategy 2) cannot
    // skip leading backticks on its own.
    let trimmed = {
        let json_start = text.find(['[', '{']);
        match json_start {
            Some(start) => &text[start..],
            None => return None,
        }
    };

    // Strategy 1: try array extraction `[...]` if present and parses cleanly.
    if let (Some(start), Some(end)) = (trimmed.find('['), trimmed.rfind(']')) {
        if end > start {
            let candidate = trimmed[start..=end].to_string();
            if serde_json::from_str::<Vec<serde_json::Value>>(&candidate).is_ok() {
                return Some(candidate);
            }
        }
    }
    // Strategy 2: walk brace depth to collect each complete top-level `{...}`.
    // Handles:
    //   (a) NDJSON `{...}{...}` (no enclosing array)
    //   (b) Truncated array `[{...},{...},{..` where strategy 1 fails because
    //       the closing `]` is missing
    //   (c) Comma-separated objects `{...},{...}` (which Deserializer streaming
    //       chokes on)
    // Tracks string state so braces inside JSON string literals don't confuse
    // the depth count.
    if trimmed.contains('{') {
        let slices = collect_top_level_objects(trimmed);
        let collected: Vec<serde_json::Value> = slices
            .iter()
            .filter_map(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .filter(|v| v.is_object())
            .collect();
        if !collected.is_empty() {
            if let Ok(s) = serde_json::to_string(&collected) {
                return Some(s);
            }
        }
    }
    // Strategy 3: last resort — wrap a single best-effort `{...}` slice in array brackets.
    // Validate the result before returning so callers never receive unparseable JSON.
    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        if end > start {
            let candidate = format!("[{}]", &trimmed[start..=end]);
            if serde_json::from_str::<Vec<serde_json::Value>>(&candidate).is_ok() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Walk `text` and return each complete top-level `{...}` slice in order.
/// Tracks string state (with `\\"` escape handling) so braces inside JSON
/// string literals are not counted toward depth. Truncated trailing objects
/// are skipped.
fn collect_top_level_objects(text: &str) -> Vec<&str> {
    let mut results = Vec::new();
    let mut depth = 0usize;
    let mut start: Option<usize> = None;
    let mut in_string = false;
    let mut escaped = false;
    for (i, c) in text.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            '}' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    if let Some(s) = start {
                        results.push(&text[s..=i]);
                        start = None;
                    }
                }
            }
            _ => {}
        }
    }
    results
}

/// Truncate text at a word boundary, not exceeding `max_chars` bytes.
/// Uses `floor_char_boundary` to avoid panicking on multi-byte UTF-8.
pub(crate) fn truncate_at_word_boundary(text: &str, max_chars: usize) -> &str {
    if text.len() <= max_chars {
        return text;
    }
    let safe_end = text.floor_char_boundary(max_chars);
    match text[..safe_end].rfind(' ') {
        Some(pos) => &text[..pos],
        None => &text[..safe_end],
    }
}

/// Slot→request assignment policy for continuous-batch slot backfill.
///
/// The continuous-batch decoder owns `n_slots` physical sequence slots (M).
/// When a request queue is longer than the slots, this scheduler keeps every
/// slot full: as soon as a slot's request finishes, the next queued request is
/// assigned to that slot (backfill), so decode width stays near M instead of
/// raggedly draining M→1 as short outputs finish first.
///
/// Pure bookkeeping — no GPU/llama state — so it is exhaustively unit-testable.
/// When `n_requests <= n_slots` the queue is empty after the initial fill and
/// `retire` never backfills, making the no-overflow path byte-identical to the
/// pre-backfill behavior (each request keeps its own slot for its whole life).
struct BackfillScheduler {
    /// Logical request index currently occupying each slot (`None` = free).
    slot_request: Vec<Option<usize>>,
    /// Next queued request index not yet assigned to any slot.
    next_request: usize,
    /// Total logical requests to service.
    n_requests: usize,
}

impl BackfillScheduler {
    /// Fill slots 0..min(n_slots, n_requests) with the first requests; the rest
    /// of the queue waits to backfill freed slots.
    fn new(n_requests: usize, n_slots: usize) -> Self {
        let initial = n_slots.min(n_requests);
        let slot_request = (0..n_slots)
            .map(|slot| if slot < initial { Some(slot) } else { None })
            .collect();
        Self {
            slot_request,
            next_request: initial,
            n_requests,
        }
    }

    /// Request currently assigned to `slot`, if any.
    fn request_in_slot(&self, slot: usize) -> Option<usize> {
        self.slot_request.get(slot).copied().flatten()
    }

    /// Number of slots currently holding a request.
    #[cfg(test)]
    fn active_count(&self) -> usize {
        self.slot_request.iter().filter(|r| r.is_some()).count()
    }

    /// Retire the request in `slot` and backfill from the queue if any remain.
    /// Returns the newly-assigned request index (caller must prefill it), or
    /// `None` if the queue is empty (slot becomes free).
    fn retire(&mut self, slot: usize) -> Option<usize> {
        let cell = self.slot_request.get_mut(slot)?;
        if self.next_request < self.n_requests {
            let assigned = self.next_request;
            self.next_request += 1;
            *cell = Some(assigned);
            Some(assigned)
        } else {
            *cell = None;
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backfill_initial_fill_caps_at_slots() {
        let s = BackfillScheduler::new(20, 8);
        for slot in 0..8 {
            assert_eq!(s.request_in_slot(slot), Some(slot));
        }
        assert_eq!(s.active_count(), 8);
    }

    #[test]
    fn backfill_fewer_requests_than_slots_leaves_tail_free() {
        let s = BackfillScheduler::new(3, 8);
        assert_eq!(s.request_in_slot(0), Some(0));
        assert_eq!(s.request_in_slot(2), Some(2));
        assert_eq!(s.request_in_slot(3), None);
        assert_eq!(s.active_count(), 3);
    }

    #[test]
    fn backfill_retire_assigns_next_queued_request() {
        let mut s = BackfillScheduler::new(20, 8);
        // Slot 0 finishes its request 0 -> next queued (8) backfills it.
        assert_eq!(s.retire(0), Some(8));
        assert_eq!(s.request_in_slot(0), Some(8));
        assert_eq!(s.retire(1), Some(9));
        assert_eq!(s.request_in_slot(1), Some(9));
        assert_eq!(s.active_count(), 8); // still full
    }

    #[test]
    fn backfill_no_overflow_never_backfills() {
        // n_requests <= n_slots: queue empty after initial fill (byte-identical
        // to pre-backfill behavior — each request owns its slot for its life).
        let mut s = BackfillScheduler::new(5, 8);
        assert_eq!(s.retire(0), None);
        assert_eq!(s.request_in_slot(0), None);
        assert_eq!(s.active_count(), 4);
    }

    #[test]
    fn backfill_queue_drains_then_slots_free() {
        let mut s = BackfillScheduler::new(10, 8);
        // 2 overflow requests (8, 9) remain after the initial fill of 0..8.
        assert_eq!(s.retire(0), Some(8));
        assert_eq!(s.retire(1), Some(9));
        // Queue now empty: further retires free the slot.
        assert_eq!(s.retire(2), None);
        assert_eq!(s.request_in_slot(2), None);
    }

    #[test]
    fn backfill_services_every_request_exactly_once() {
        // Retire slots in a deliberately uneven order until all slots free;
        // every request 0..n must be serviced exactly once.
        let n_requests = 37;
        let n_slots = 8;
        let mut s = BackfillScheduler::new(n_requests, n_slots);
        let mut serviced: Vec<usize> = (0..n_slots.min(n_requests))
            .filter_map(|slot| s.request_in_slot(slot))
            .collect();
        // Round-robin retire with a skew so slots free at different rates.
        loop {
            let mut any_active = false;
            for slot in 0..n_slots {
                if s.request_in_slot(slot).is_some() {
                    any_active = true;
                    if let Some(req) = s.retire(slot) {
                        serviced.push(req);
                    }
                }
            }
            if !any_active {
                break;
            }
        }
        serviced.sort_unstable();
        assert_eq!(serviced, (0..n_requests).collect::<Vec<_>>());
    }

    #[test]
    fn backfill_zero_requests_is_inert() {
        let mut s = BackfillScheduler::new(0, 8);
        assert_eq!(s.active_count(), 0);
        assert_eq!(s.request_in_slot(0), None);
        assert_eq!(s.retire(0), None);
    }

    #[test]
    fn test_truncate_at_word_boundary_ascii() {
        let text = "hello world foo bar";
        // max=12 -> text[..12]="hello world " -> rfind(' ') at 11 -> "hello world"
        assert_eq!(truncate_at_word_boundary(text, 12), "hello world");
        // max=11 -> text[..11]="hello world" -> rfind(' ') at 5 -> "hello"
        assert_eq!(truncate_at_word_boundary(text, 11), "hello");
        assert_eq!(truncate_at_word_boundary(text, 100), text);
    }

    #[test]
    fn test_truncate_at_word_boundary_multibyte() {
        // Curly quotes: \u{201c} and \u{201d} are 3 bytes each
        let text = "he said \u{201c}hello\u{201d} world";
        // Truncating at byte 10 (inside \u{201c}) should not panic
        let result = truncate_at_word_boundary(text, 10);
        assert!(result.len() <= 10);
        assert!(result.is_char_boundary(result.len()));

        // CJK: each char is 3 bytes, no spaces -> returns at safe boundary
        let cjk = "\u{4e16}\u{754c}\u{4f60}\u{597d}";
        let result = truncate_at_word_boundary(cjk, 5);
        assert!(result.len() <= 5);
        assert!(result.is_char_boundary(result.len()));

        // Em dash (\u{2014}, 3 bytes) in text
        let text = "foo \u{2014} bar baz";
        let result = truncate_at_word_boundary(text, 6);
        assert_eq!(result, "foo");
    }

    #[test]
    fn test_truncate_at_word_boundary_exact() {
        let text = "abc def";
        assert_eq!(truncate_at_word_boundary(text, 7), text);
        assert_eq!(truncate_at_word_boundary(text, 3), "abc");
    }

    #[test]
    fn test_strip_think_tags() {
        assert_eq!(
            strip_think_tags("<think>\nsome reasoning\n</think>\n{\"key\": \"val\"}"),
            "\n{\"key\": \"val\"}"
        );
        assert_eq!(
            strip_think_tags("<think>\n\n</think>\n\n{\"a\": 1}"),
            "\n\n{\"a\": 1}"
        );
        assert_eq!(strip_think_tags("{\"key\": \"val\"}"), "{\"key\": \"val\"}");
        assert_eq!(strip_think_tags("prefix<think>dangling"), "prefix");
        assert_eq!(
            strip_think_tags("<think>a</think>mid<think>b</think>end"),
            "midend"
        );
    }

    #[test]
    fn test_finalize_continuous_batch_outputs_marks_failed_partial_none() {
        let outputs = vec![
            "partial json".to_string(),
            "<think>x</think> ok ".to_string(),
        ];
        let failed = vec![true, false];
        let strip = vec![true, true];
        let labels = vec![Some("failed".to_string()), Some("ok".to_string())];

        let result = LlmEngine::finalize_continuous_batch_outputs(
            outputs,
            &failed,
            &strip,
            &labels,
            Instant::now(),
        );

        assert_eq!(result[0], None);
        assert_eq!(result[1].as_deref(), Some("ok"));
    }

    #[test]
    fn test_extract_json() {
        assert_eq!(
            extract_json(r#"Sure! {"key": "val"}"#),
            Some(r#"{"key": "val"}"#)
        );
        assert_eq!(extract_json("no json here"), None);
        assert_eq!(extract_json("{"), None);
    }

    #[test]
    fn test_extract_json_array_with_surrounding_text() {
        let text = r#"Here are the results: [{"i":1}] Hope that helps!"#;
        assert_eq!(extract_json_array(text), Some(r#"[{"i":1}]"#.to_string()));
    }

    #[test]
    fn test_extract_json_array_fallback_wraps_single_object() {
        let text = r#"{"i":1,"type":"fact"}"#;
        assert_eq!(
            extract_json_array(text),
            Some(r#"[{"i":1,"type":"fact"}]"#.to_string())
        );
    }

    /// Regression: single KG object with inner arrays must be wrapped, not
    /// misextracted by matching the inner `[`/`]` as the top-level array.
    #[test]
    fn test_extract_json_array_single_object_with_inner_arrays() {
        let text = r#"{"i": 0, "entities": [{"name": "caroline", "type": "person"}], "observations": [{"entity": "caroline", "content": "joined the group"}]}"#;
        let result = extract_json_array(text).unwrap();
        // Must be a valid JSON array
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.len(), 1);
        // Inner entities must survive
        let entities = parsed[0]["entities"].as_array().unwrap();
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0]["name"], "caroline");
    }

    /// Array response (batch extraction) still works.
    #[test]
    fn test_extract_json_array_real_array_with_inner_arrays() {
        let text = r#"[{"i": 0, "entities": [{"name": "a"}]}, {"i": 1, "entities": []}]"#;
        let result = extract_json_array(text).unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.len(), 2);
    }

    /// NDJSON-style: model returns multiple top-level objects without enclosing
    /// `[]`. This is what Qwen3-4B emits at batch>1. Must collect all into array.
    #[test]
    fn test_extract_json_array_ndjson_multiple_objects() {
        let text = r#"{"i": 0, "entities": [{"name": "a"}]}
{"i": 1, "entities": [{"name": "b"}]}
{"i": 2, "entities": []}"#;
        let result = extract_json_array(text).unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0]["i"], 0);
        assert_eq!(parsed[1]["entities"][0]["name"], "b");
    }

    /// NDJSON without separator: `{...}{...}` directly back-to-back.
    #[test]
    fn test_extract_json_array_ndjson_no_separator() {
        let text = r#"{"i":0,"entities":[]}{"i":1,"entities":[]}"#;
        let result = extract_json_array(text).unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.len(), 2);
    }

    /// Markdown code-fenced output (Qwen3.5-9B). Must strip leading fence.
    #[test]
    fn test_extract_json_array_markdown_fence() {
        let text = "```json\n[{\"i\": 0, \"entities\": [{\"name\": \"a\"}]}]\n```";
        let result = extract_json_array(text).unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0]["entities"][0]["name"], "a");
    }

    /// Markdown fence + NDJSON inside.
    #[test]
    fn test_extract_json_array_fence_with_ndjson() {
        let text = "```json\n{\"i\": 0, \"entities\": []}\n{\"i\": 1, \"entities\": []}\n```";
        let result = extract_json_array(text).unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.len(), 2);
    }

    /// Truncated array — model wrote `[{..},{..},{..` and got cut off.
    /// Common at high batch sizes where output exceeds time budget.
    /// Strategy 2 must skip the leading `[` and stream-collect partial objects.
    #[test]
    fn test_extract_json_array_truncated_no_close() {
        let text = r#"[{"i":0,"entities":[{"name":"a"}]},{"i":1,"entities":[{"name":"b"}]},{"i":2,"entities":[{"na"#;
        let result = extract_json_array(text).unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        // Should recover the 2 complete objects, drop the truncated 3rd
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0]["entities"][0]["name"], "a");
        assert_eq!(parsed[1]["entities"][0]["name"], "b");
    }

    /// Markdown fence + truncated array (real 9B failure case).
    #[test]
    fn test_extract_json_array_fence_truncated() {
        let text = "```json\n[\n  {\"i\": 0, \"entities\": []},\n  {\"i\": 1, \"entities\":";
        let result = extract_json_array(text).unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.len(), 1);
    }

    /// Strategy-3 validation: garbage input that finds `{` and `}` but produces
    /// invalid JSON when wrapped must return None instead of unparseable output.
    #[test]
    fn test_extract_json_array_strategy3_invalid_returns_none() {
        // Contains { and } but is not valid JSON — Strategy 3 must reject it.
        let text = "garbage{ broken } stuff { incomplete";
        assert_eq!(extract_json_array(text), None);
    }
}
