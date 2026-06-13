// SPDX-License-Identifier: Apache-2.0
//! LLM-as-judge infrastructure: types, functions, prompts, Batch API judge.

use crate::error::OriginError;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

// ===== Judge Prompt (shared between CLI and Batch API paths) =====

/// Task-specific judge prompt dispatcher. Both `judge_single_tuple_model` (CLI)
/// and `judge_with_batch_api` (Batch API) call this, so judge behavior is
/// identical regardless of path.
///
/// Dispatches to benchmark-sourced prompts based on `category`:
/// - `temporal-reasoning`: off-by-one tolerance for day/week/month counts
/// - `knowledge-update`: accepts old+new answers if updated answer is correct
/// - `single-session-preference`: rubric-based (not exact-match)
/// - Everything else (LoCoMo categories + LME SSU/SSA/MS): standard benchmark prompt
pub fn task_judge_prompt(
    category: &str,
    question: &str,
    ground_truth: &str,
    answer: &str,
) -> String {
    lme_anscheck_prompt(category, question, ground_truth, answer)
}

// ===== LLM-as-Judge Types =====

/// A single E2E answer to be judged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgmentTuple {
    pub question: String,
    pub ground_truth: String,
    pub approach: String,
    pub answer: String,
    pub context_tokens: usize,
    /// Task category for task-specific judge prompts (e.g. "temporal-reasoning",
    /// "single-hop"). Defaults to empty for backward compat with existing JSON.
    #[serde(default)]
    pub category: String,
}

/// Result from the LLM judge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgmentResult {
    pub question: String,
    pub approach: String,
    /// 0 or 1.
    pub score: u8,
    pub reason: String,
    pub context_tokens: usize,
}

// Cost telemetry has been moved to `cli_batch::CliCostInfo` and is shared across
// all CLI batched eval routes. Keep `JudgeCostInfo` as an alias for backward compat
// with any external caller; new code should use `CliCostInfo` directly.
pub use crate::eval::cli_batch::CliCostInfo as JudgeCostInfo;

/// Per-approach aggregated result in a judged E2E report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgedApproachResult {
    pub approach: String,
    /// Fraction of questions scoring 1.
    pub accuracy: f64,
    pub total: usize,
    pub correct: usize,
    pub mean_context_tokens: f64,
}

/// Full judged E2E report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgedE2EReport {
    pub judge_model: String,
    pub total_judged: usize,
    pub results_by_approach: Vec<JudgedApproachResult>,
}

// ===== LLM-as-Judge Functions =====

/// Save E2E answer tuples to JSON for offline judging.
pub fn save_judgment_tuples(tuples: &[JudgmentTuple], path: &Path) -> Result<(), std::io::Error> {
    let json = serde_json::to_string_pretty(tuples).map_err(std::io::Error::other)?;
    std::fs::write(path, json)
}

/// Load previously saved judgment tuples from JSON.
pub fn load_judgment_tuples(path: &Path) -> Result<Vec<JudgmentTuple>, std::io::Error> {
    let content = std::fs::read_to_string(path)?;
    serde_json::from_str(&content).map_err(std::io::Error::other)
}

/// Judge answer tuples using Claude via the `claude -p` CLI.
///
/// Requires Claude Code CLI installed (`claude --version` must succeed).
/// Uses Haiku via the user's existing Max subscription — no API key needed.
/// Runs up to `concurrency` judgments in parallel.
pub async fn judge_with_claude(
    tuples: &[JudgmentTuple],
    concurrency: usize,
) -> Result<Vec<JudgmentResult>, OriginError> {
    judge_with_claude_model(tuples, concurrency, "haiku").await
}

/// Judge tuples with a specific Claude model (e.g. "haiku", "sonnet").
///
/// In-memory only. For long runs prefer `judge_with_claude_model_persistent`,
/// which caches each judgment to disk so partial failures aren't lost.
pub async fn judge_with_claude_model(
    tuples: &[JudgmentTuple],
    concurrency: usize,
    model: &str,
) -> Result<Vec<JudgmentResult>, OriginError> {
    use tokio::sync::Semaphore;

    let semaphore = Arc::new(Semaphore::new(concurrency));
    let mut handles = Vec::new();

    for tuple in tuples {
        let sem = semaphore.clone();
        let tuple = tuple.clone();
        let model = model.to_string();

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            judge_single_tuple_model(&tuple, &model).await
        });
        handles.push(handle);
    }

    let mut results = Vec::new();
    let mut fail_count = 0usize;
    for handle in handles {
        match handle.await {
            Ok(Ok(result)) => results.push(result),
            Ok(Err(e)) => {
                fail_count += 1;
                eprintln!("[judge] FAILED: {}", e);
            }
            Err(e) => {
                fail_count += 1;
                eprintln!("[judge] PANICKED: {}", e);
            }
        }
    }
    eprintln!(
        "[judge] {} succeeded, {} failed (no persistence — failures lost)",
        results.len(),
        fail_count
    );

    Ok(results)
}

/// Judge tuples with on-disk persistence + retry + resume.
///
/// Behavior:
/// - Loads any existing judgments from `cache_path` (JSONL, one `JudgmentResult` per line)
/// - Skips tuples whose (question, approach) pair is already judged
/// - Retries each unsuccessful tuple up to `max_retries` times with exponential backoff
/// - Appends each successful judgment to `cache_path` immediately (under file mutex)
/// - Prints visible per-failure messages + final summary to stderr
///
/// Safe to ctrl-C and re-run: re-running picks up where it left off.
pub async fn judge_with_claude_model_persistent(
    tuples: &[JudgmentTuple],
    concurrency: usize,
    model: &str,
    cache_path: &Path,
    max_retries: u32,
) -> Result<Vec<JudgmentResult>, OriginError> {
    use std::collections::HashSet;
    use std::fs::OpenOptions;
    use std::io::{BufRead, BufReader, Write};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::{Mutex as AsyncMutex, Semaphore};

    let cached: Vec<JudgmentResult> = if cache_path.exists() {
        let f = std::fs::File::open(cache_path).map_err(OriginError::from)?;
        BufReader::new(f)
            .lines()
            .map_while(|l| l.ok())
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str::<JudgmentResult>(&l).ok())
            .collect()
    } else {
        Vec::new()
    };

    let cached_keys: HashSet<(String, String)> = cached
        .iter()
        .map(|r| (r.question.clone(), r.approach.clone()))
        .collect();

    let todo: Vec<JudgmentTuple> = tuples
        .iter()
        .filter(|t| !cached_keys.contains(&(t.question.clone(), t.approach.clone())))
        .cloned()
        .collect();

    eprintln!(
        "[judge] cache: {} existing, {} to judge ({} total)",
        cached.len(),
        todo.len(),
        tuples.len()
    );

    if todo.is_empty() {
        return Ok(cached);
    }

    if let Some(parent) = cache_path.parent() {
        std::fs::create_dir_all(parent).map_err(OriginError::from)?;
    }
    let cache_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(cache_path)
        .map_err(OriginError::from)?;
    let cache_writer = Arc::new(AsyncMutex::new(cache_file));

    let semaphore = Arc::new(Semaphore::new(concurrency));
    let retry_count = Arc::new(AtomicUsize::new(0));
    let fail_count = Arc::new(AtomicUsize::new(0));
    let succ_count = Arc::new(AtomicUsize::new(0));
    let progress = Arc::new(AtomicUsize::new(0));
    let total_cost_usd = Arc::new(std::sync::atomic::AtomicU64::new(0)); // cost * 1_000_000
    let total_in_tokens = Arc::new(AtomicUsize::new(0));
    let total_out_tokens = Arc::new(AtomicUsize::new(0));
    let total_cache_create = Arc::new(AtomicUsize::new(0));
    let total_cache_read = Arc::new(AtomicUsize::new(0));
    let total = todo.len();

    let mut handles = Vec::with_capacity(todo.len());
    for tuple in todo {
        let sem = semaphore.clone();
        let model = model.to_string();
        let writer = cache_writer.clone();
        let retries = retry_count.clone();
        let fails = fail_count.clone();
        let succs = succ_count.clone();
        let prog = progress.clone();

        let cost_us = total_cost_usd.clone();
        let in_tok = total_in_tokens.clone();
        let out_tok = total_out_tokens.clone();
        let cc_tok = total_cache_create.clone();
        let cr_tok = total_cache_read.clone();

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            let mut last_err: Option<String> = None;
            let mut result: Option<JudgmentResult> = None;
            for attempt in 0..=max_retries {
                match judge_single_tuple_model_with_cost(&tuple, &model).await {
                    Ok((r, cost)) => {
                        if let Some(c) = cost {
                            cost_us.fetch_add((c.cost_usd * 1_000_000.0) as u64, Ordering::Relaxed);
                            in_tok.fetch_add(c.input_tokens as usize, Ordering::Relaxed);
                            out_tok.fetch_add(c.output_tokens as usize, Ordering::Relaxed);
                            cc_tok.fetch_add(c.cache_creation_tokens as usize, Ordering::Relaxed);
                            cr_tok.fetch_add(c.cache_read_tokens as usize, Ordering::Relaxed);
                        }
                        result = Some(r);
                        break;
                    }
                    Err(e) => {
                        last_err = Some(e.to_string());
                        if attempt < max_retries {
                            retries.fetch_add(1, Ordering::Relaxed);
                            let delay_ms = 500u64 * (1 << attempt);
                            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                        }
                    }
                }
            }

            let done = prog.fetch_add(1, Ordering::Relaxed) + 1;
            if done.is_multiple_of(50) || done == total {
                eprintln!(
                    "[judge] progress {}/{} (succ={}, fail={}, retries={})",
                    done,
                    total,
                    succs.load(Ordering::Relaxed),
                    fails.load(Ordering::Relaxed),
                    retries.load(Ordering::Relaxed)
                );
            }

            match result {
                Some(r) => {
                    let line = match serde_json::to_string(&r) {
                        Ok(s) => s,
                        Err(e) => {
                            fails.fetch_add(1, Ordering::Relaxed);
                            eprintln!("[judge] serialize FAIL for {:?}: {}", tuple.question, e);
                            return None;
                        }
                    };
                    let mut guard = writer.lock().await;
                    if let Err(e) = writeln!(*guard, "{}", line) {
                        fails.fetch_add(1, Ordering::Relaxed);
                        eprintln!("[judge] write FAIL for {:?}: {}", tuple.question, e);
                        return None;
                    }
                    let _ = guard.flush();
                    drop(guard);
                    succs.fetch_add(1, Ordering::Relaxed);
                    Some(r)
                }
                None => {
                    fails.fetch_add(1, Ordering::Relaxed);
                    let q_preview: String = tuple.question.chars().take(60).collect();
                    eprintln!(
                        "[judge] FAILED after {} retries: {:?} — last_err: {}",
                        max_retries,
                        q_preview,
                        last_err.unwrap_or_else(|| "?".into())
                    );
                    None
                }
            }
        });
        handles.push(handle);
    }

    let mut new_results = Vec::new();
    for handle in handles {
        if let Ok(Some(r)) = handle.await {
            new_results.push(r);
        }
    }

    let cost_total_dollars = total_cost_usd.load(Ordering::Relaxed) as f64 / 1_000_000.0;
    let succ_n = succ_count.load(Ordering::Relaxed) as f64;
    let mean_cost = if succ_n > 0.0 {
        cost_total_dollars / succ_n
    } else {
        0.0
    };
    eprintln!(
        "[judge] DONE: {} succ, {} fail, {} retries (cache: {} → {})",
        succ_count.load(Ordering::Relaxed),
        fail_count.load(Ordering::Relaxed),
        retry_count.load(Ordering::Relaxed),
        cached.len(),
        cached.len() + new_results.len()
    );
    eprintln!(
        "[judge] cost: ${:.4} total (${:.5}/call) | in={} out={} cache_create={} cache_read={}",
        cost_total_dollars,
        mean_cost,
        total_in_tokens.load(Ordering::Relaxed),
        total_out_tokens.load(Ordering::Relaxed),
        total_cache_create.load(Ordering::Relaxed),
        total_cache_read.load(Ordering::Relaxed),
    );
    if mean_cost > 0.05 {
        eprintln!(
            "[judge] WARN: mean cost ${:.5}/call is high — claude -p re-creates the system-prompt cache each call. Consider Anthropic API direct (~1000x cheaper) for full runs.",
            mean_cost
        );
    }

    let mut all = cached;
    all.extend(new_results);
    Ok(all)
}

/// Judge a single (question, ground_truth, answer) tuple via `claude -p`.
///
/// Passes the prompt via stdin and disables all tools (`--allowedTools ""`), which
/// prevents Claude Code's agentic tool-calling loop and gets a direct text/JSON response.
/// OAuth auth from the user's existing login is used (no API key required).
pub async fn judge_single_tuple(tuple: &JudgmentTuple) -> Result<JudgmentResult, OriginError> {
    judge_single_tuple_model(tuple, "haiku").await
}

/// Judge a single tuple with a specific Claude model.
///
/// Backward-compat wrapper that drops cost telemetry. New callers should prefer
/// `judge_single_tuple_model_with_cost`.
pub async fn judge_single_tuple_model(
    tuple: &JudgmentTuple,
    model: &str,
) -> Result<JudgmentResult, OriginError> {
    judge_single_tuple_model_with_cost(tuple, model)
        .await
        .map(|(r, _)| r)
}

/// Judge a single tuple, also extracting cost telemetry from the CLI envelope.
pub async fn judge_single_tuple_model_with_cost(
    tuple: &JudgmentTuple,
    model: &str,
) -> Result<(JudgmentResult, Option<JudgeCostInfo>), OriginError> {
    use tokio::io::AsyncWriteExt;
    use tokio::process::Command;

    let prompt = task_judge_prompt(
        &tuple.category,
        &tuple.question,
        &tuple.ground_truth,
        &tuple.answer,
    );

    let json_schema = r#"{"type":"object","properties":{"score":{"type":"integer","enum":[0,1]},"reason":{"type":"string"}},"required":["score","reason"]}"#;

    // No system prompt — all instructions are in the user prompt (shared with batch API).
    // Strip ANTHROPIC_API_KEY so Claude Code uses Max-plan OAuth instead of an empty
    // API balance (mirrors ClaudeCliProvider in llm_provider.rs).
    let mut child = Command::new("claude")
        .env_remove("ANTHROPIC_API_KEY")
        .args([
            "-p",
            "--model",
            model,
            "--output-format",
            "json",
            "--json-schema",
            json_schema,
            "--no-session-persistence",
            "--allowedTools",
            "",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| OriginError::Generic(format!("claude -p failed to start: {}", e)))?;

    // Write prompt to stdin then close it.
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(prompt.as_bytes())
            .await
            .map_err(|e| OriginError::Generic(format!("write to claude stdin failed: {}", e)))?;
        // drop closes stdin
    }

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| OriginError::Generic(format!("claude -p wait failed: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout_preview = String::from_utf8_lossy(&output.stdout);
        return Err(OriginError::Generic(format!(
            "claude -p exited with error: stderr={} stdout={}",
            stderr.trim(),
            stdout_preview.trim()
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Try to also extract cost telemetry from the envelope (best-effort, never fatal).
    let cost = serde_json::from_str::<serde_json::Value>(stdout.trim())
        .ok()
        .map(|env| {
            let usage = env.get("usage");
            JudgeCostInfo {
                cost_usd: env
                    .get("total_cost_usd")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0),
                input_tokens: usage
                    .and_then(|u| u.get("input_tokens"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
                output_tokens: usage
                    .and_then(|u| u.get("output_tokens"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
                cache_creation_tokens: usage
                    .and_then(|u| u.get("cache_creation_input_tokens"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
                cache_read_tokens: usage
                    .and_then(|u| u.get("cache_read_input_tokens"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
            }
        });

    // Parse the JSON response. `--output-format json` returns an envelope; the structured
    // output lives in the `structured_output` field when `--json-schema` is used.
    let parsed: serde_json::Value = parse_judge_json(&stdout).map_err(|e| {
        OriginError::Generic(format!(
            "judge response parse error: {} — raw: {}",
            e, stdout
        ))
    })?;

    let score = parsed.get("score").and_then(|v| v.as_u64()).unwrap_or(0) as u8;
    let reason = parsed
        .get("reason")
        .and_then(|v| v.as_str())
        .unwrap_or("no reason")
        .to_string();

    Ok((
        JudgmentResult {
            question: tuple.question.clone(),
            approach: tuple.approach.clone(),
            score,
            reason,
            context_tokens: tuple.context_tokens,
        },
        cost,
    ))
}

/// Strict batch judge prompt for N tuples per call.
///
/// Explicit conservative scoring rules counter the leniency bias observed when
/// the model judges multiple tuples in one call (probe 2026-04-29 measured
/// 90% agreement with single-call gold for standard prompt vs 100% for strict).
pub fn strict_batch_judge_prompt(tuples: &[JudgmentTuple]) -> String {
    let mut s = String::with_capacity(2048 + tuples.len() * 256);
    s.push_str(
        "You will judge multiple (question, ground_truth, model_answer) tuples. Be CONSERVATIVE and STRICT.\n\n",
    );
    s.push_str("Scoring rules:\n");
    s.push_str("- Score 1 ONLY if model_answer explicitly contains the exact information in ground_truth\n");
    s.push_str("- Score 0 if answer is a vague paraphrase, implication, or generic equivalent\n");
    s.push_str(
        "- Score 0 if answer is missing any required element of a multi-part ground_truth\n",
    );
    s.push_str(
        "- Score 0 if answer hedges on a key fact (date, name, number) the question requires\n",
    );
    s.push_str(
        "- Off-by-one tolerance allowed only for explicit numeric temporal counts (days/weeks/months)\n\n",
    );
    s.push_str(&format!(
        "Return JSON object with a 'results' array containing exactly {} entries in input order.\n\nTuples:\n",
        tuples.len()
    ));
    for (i, t) in tuples.iter().enumerate() {
        s.push_str(&format!(
            "[{}] Q: {}\n    GT: {}\n    A: {}\n\n",
            i + 1,
            t.question,
            t.ground_truth,
            t.answer
        ));
    }
    s
}

// Markdown fence stripper has moved to `cli_batch::strip_markdown_fence`.
use crate::eval::cli_batch::strip_markdown_fence;

/// Defensive parser for batch judge envelope. Returns Vec<(score, reason)> or None.
///
/// Tries multiple strategies because `--json-schema` enforcement is unreliable
/// after several --resume turns (model drifts to conversational mode and returns
/// markdown-wrapped JSON in `.result` field instead of `.structured_output`).
pub fn parse_batch_judge_envelope(stdout: &str) -> Option<Vec<(u8, String)>> {
    let trimmed = stdout.trim();
    let env: serde_json::Value = serde_json::from_str(trimmed).ok()?;

    // Strategy 1: structured_output.results (primary)
    if let Some(results) = env
        .get("structured_output")
        .and_then(|v| v.get("results"))
        .and_then(|v| v.as_array())
    {
        if !results.is_empty() {
            return Some(extract_score_reason_pairs(results));
        }
    }

    // Strategy 2: result field with markdown fence (schema drift fallback)
    if let Some(result_str) = env.get("result").and_then(|v| v.as_str()) {
        let stripped = strip_markdown_fence(result_str);
        if let Ok(inner) = serde_json::from_str::<serde_json::Value>(&stripped) {
            if let Some(results) = inner.get("results").and_then(|v| v.as_array()) {
                if !results.is_empty() {
                    return Some(extract_score_reason_pairs(results));
                }
            }
        }
    }

    None
}

fn extract_score_reason_pairs(arr: &[serde_json::Value]) -> Vec<(u8, String)> {
    arr.iter()
        .map(|v| {
            let score = v.get("score").and_then(|x| x.as_u64()).unwrap_or(0) as u8;
            let reason = v
                .get("reason")
                .and_then(|x| x.as_str())
                .unwrap_or("no reason")
                .to_string();
            (score, reason)
        })
        .collect()
}

/// Run one batched judge subprocess call.
///
/// If `session_id` is `Some`, uses `--resume` for cache reuse. Returns
/// `(results, cost, new_session_id)`. The new_session_id is the session of THIS
/// call (caller passes it into the next call's `session_id` for --resume).
async fn run_batch_judge_subprocess(
    prompt: &str,
    model: &str,
    session_id: Option<&str>,
) -> Result<(Vec<(u8, String)>, Option<JudgeCostInfo>, Option<String>), OriginError> {
    use crate::eval::cli_batch::run_cli_batch_subprocess;

    let json_schema = r#"{"type":"object","properties":{"results":{"type":"array","items":{"type":"object","properties":{"score":{"type":"integer","enum":[0,1]},"reason":{"type":"string"}},"required":["score","reason"]}}},"required":["results"]}"#;

    let (stdout, cost, new_sid) =
        run_cli_batch_subprocess(prompt, model, json_schema, session_id).await?;

    let results = parse_batch_judge_envelope(&stdout).ok_or_else(|| {
        OriginError::Generic(format!(
            "batch judge parse error — raw envelope head: {}",
            stdout.chars().take(200).collect::<String>()
        ))
    })?;

    Ok((results, cost, new_sid))
}

/// Batched persistent judging with --resume cache reuse + session rotation.
///
/// Cost-optimized path:
/// - Each subprocess call judges `batch_size` tuples (10 = empirical sweet spot).
/// - Within a session, second+ calls use `--resume` so claude-code re-loads cached
///   system prompt cheaply (cache_create drops ~92k → ~3k after first call).
/// - Every `rotation_calls` calls (default 3), session is reset to avoid schema
///   drift — claude treats long --resume conversations as casual chat and stops
///   honoring `--json-schema`, falling back to markdown-wrapped output.
/// - Defensive parser handles markdown fence fallback if schema drift slips through.
/// - Strict prompt counters the leniency bias observed in standard batched judging.
///
/// Persistence + retry semantics match `judge_with_claude_model_persistent`:
/// JSONL append-only cache, resume across runs, exp-backoff retry per batch.
pub async fn judge_with_claude_model_batched_persistent(
    tuples: &[JudgmentTuple],
    batch_size: usize,
    rotation_calls: usize,
    model: &str,
    cache_path: &Path,
    max_retries: u32,
) -> Result<Vec<JudgmentResult>, OriginError> {
    use std::collections::HashSet;
    use std::fs::OpenOptions;
    use std::io::{BufRead, BufReader, Write};

    let cached: Vec<JudgmentResult> = if cache_path.exists() {
        let f = std::fs::File::open(cache_path).map_err(OriginError::from)?;
        BufReader::new(f)
            .lines()
            .map_while(|l| l.ok())
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str::<JudgmentResult>(&l).ok())
            .collect()
    } else {
        Vec::new()
    };

    let cached_keys: HashSet<(String, String)> = cached
        .iter()
        .map(|r| (r.question.clone(), r.approach.clone()))
        .collect();

    let todo: Vec<&JudgmentTuple> = tuples
        .iter()
        .filter(|t| !cached_keys.contains(&(t.question.clone(), t.approach.clone())))
        .collect();

    eprintln!(
        "[judge-batch] cache: {} existing, {} to judge ({} total) | batch_size={} rotation={}",
        cached.len(),
        todo.len(),
        tuples.len(),
        batch_size,
        rotation_calls
    );

    if todo.is_empty() {
        return Ok(cached);
    }

    if let Some(parent) = cache_path.parent() {
        std::fs::create_dir_all(parent).map_err(OriginError::from)?;
    }
    let mut cache_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(cache_path)
        .map_err(OriginError::from)?;

    let total_batches = todo.len().div_ceil(batch_size);
    let mut new_results: Vec<JudgmentResult> = Vec::new();
    let mut session_id: Option<String> = None;
    let mut calls_in_session: usize = 0;
    let mut total_cost_usd: f64 = 0.0;
    let mut total_cache_create: u64 = 0;
    let mut total_cache_read: u64 = 0;
    let mut total_input: u64 = 0;
    let mut total_output: u64 = 0;
    let mut succ_tuples: usize = 0;
    let mut fail_batches: usize = 0;
    let mut retry_count: usize = 0;

    for (batch_i, chunk) in todo.chunks(batch_size).enumerate() {
        let chunk_owned: Vec<JudgmentTuple> = chunk.iter().map(|t| (*t).clone()).collect();
        let prompt = strict_batch_judge_prompt(&chunk_owned);

        // Rotate session if we hit the cap.
        if calls_in_session >= rotation_calls {
            session_id = None;
            calls_in_session = 0;
        }

        #[allow(clippy::type_complexity)]
        let mut batch_result: Option<(
            Vec<(u8, String)>,
            Option<JudgeCostInfo>,
            Option<String>,
        )> = None;
        let mut last_err: Option<String> = None;

        for attempt in 0..=max_retries {
            match run_batch_judge_subprocess(&prompt, model, session_id.as_deref()).await {
                Ok((results, cost, sid)) if results.len() == chunk_owned.len() => {
                    batch_result = Some((results, cost, sid));
                    break;
                }
                Ok((results, _, _)) => {
                    last_err = Some(format!(
                        "expected {} results, got {}",
                        chunk_owned.len(),
                        results.len()
                    ));
                    if attempt < max_retries {
                        retry_count += 1;
                        // Reset session — schema drift suspected.
                        session_id = None;
                        calls_in_session = 0;
                        let delay_ms = 500u64 * (1 << attempt);
                        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                    }
                }
                Err(e) => {
                    last_err = Some(e.to_string());
                    if attempt < max_retries {
                        retry_count += 1;
                        // Reset session on error — may be transient state corruption.
                        session_id = None;
                        calls_in_session = 0;
                        let delay_ms = 500u64 * (1 << attempt);
                        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                    }
                }
            }
        }

        match batch_result {
            Some((results, cost, sid)) => {
                if let Some(c) = cost {
                    total_cost_usd += c.cost_usd;
                    total_cache_create += c.cache_creation_tokens;
                    total_cache_read += c.cache_read_tokens;
                    total_input += c.input_tokens;
                    total_output += c.output_tokens;
                }
                if session_id.is_none() {
                    session_id = sid;
                    calls_in_session = 1;
                } else {
                    calls_in_session += 1;
                }

                for (i, (score, reason)) in results.iter().enumerate() {
                    let tuple = &chunk_owned[i];
                    let r = JudgmentResult {
                        question: tuple.question.clone(),
                        approach: tuple.approach.clone(),
                        score: *score,
                        reason: reason.clone(),
                        context_tokens: tuple.context_tokens,
                    };
                    let line = match serde_json::to_string(&r) {
                        Ok(s) => s,
                        Err(e) => {
                            eprintln!("[judge-batch] serialize FAIL: {}", e);
                            continue;
                        }
                    };
                    if let Err(e) = writeln!(cache_file, "{}", line) {
                        eprintln!("[judge-batch] write FAIL: {}", e);
                        continue;
                    }
                    new_results.push(r);
                    succ_tuples += 1;
                }
                let _ = cache_file.flush();
                eprintln!(
                    "[judge-batch] batch {}/{} ok | call_in_session={} | cost so far: ${:.4} | tuples: {}",
                    batch_i + 1,
                    total_batches,
                    calls_in_session,
                    total_cost_usd,
                    succ_tuples
                );
            }
            None => {
                fail_batches += 1;
                eprintln!(
                    "[judge-batch] batch {}/{} FAILED after {} retries: {}",
                    batch_i + 1,
                    total_batches,
                    max_retries,
                    last_err.unwrap_or_else(|| "?".into())
                );
                // Reset session for next batch
                session_id = None;
                calls_in_session = 0;
            }
        }
    }

    let mean_cost_per_call = if total_batches > fail_batches {
        total_cost_usd / (total_batches - fail_batches) as f64
    } else {
        0.0
    };
    eprintln!(
        "[judge-batch] DONE: {} succ tuples / {} target | {} batches failed | {} retries | sessions={} | total_cost=${:.4} (${:.4}/call)",
        succ_tuples,
        todo.len(),
        fail_batches,
        retry_count,
        total_batches.div_ceil(rotation_calls),
        total_cost_usd,
        mean_cost_per_call
    );
    eprintln!(
        "[judge-batch] tokens: input={} output={} cache_create={} cache_read={}",
        total_input, total_output, total_cache_create, total_cache_read
    );

    let mut all = cached;
    all.extend(new_results);
    Ok(all)
}

/// Try to extract the judgment JSON object from `claude -p` output.
///
/// When `--output-format json` is combined with `--json-schema`, Claude Code returns an
/// envelope like:
/// ```json
/// {"type":"result", "structured_output": {"score":1, "reason":"..."}, ...}
/// ```
/// We try several strategies to locate the score/reason object:
/// 1. `structured_output` field in the envelope (primary path).
/// 2. `result` field in the envelope (fallback for older CLI versions).
/// 3. Top-level object if it already contains `score`.
/// 4. Extract any `{...}` substring (last resort).
pub fn parse_judge_json(stdout: &str) -> Result<serde_json::Value, serde_json::Error> {
    let trimmed = stdout.trim();

    if let Ok(envelope) = serde_json::from_str::<serde_json::Value>(trimmed) {
        // Strategy 1: structured_output field (primary — used when --json-schema is set).
        if let Some(so) = envelope.get("structured_output") {
            if so.get("score").is_some() {
                return Ok(so.clone());
            }
        }
        // Strategy 2: result field (text-mode fallback).
        if let Some(result) = envelope.get("result") {
            if result.get("score").is_some() {
                return Ok(result.clone());
            }
            // result may be a JSON string — try to parse it.
            if let Some(s) = result.as_str() {
                if let Ok(inner) = serde_json::from_str::<serde_json::Value>(s) {
                    if inner.get("score").is_some() {
                        return Ok(inner);
                    }
                }
            }
        }
        // Strategy 3: top-level already has score.
        if envelope.get("score").is_some() {
            return Ok(envelope);
        }
    }

    // Strategy 4: extract the first balanced {...} block.
    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        if start <= end {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&trimmed[start..=end]) {
                if v.get("score").is_some() {
                    return Ok(v);
                }
            }
        }
    }

    // Final fallback: return a parse error to surface the raw output.
    serde_json::from_str(trimmed)
}

/// Aggregate judgment results into a report sorted by accuracy descending.
pub fn aggregate_judgments(results: &[JudgmentResult], judge_model: &str) -> JudgedE2EReport {
    let mut by_approach: HashMap<String, Vec<&JudgmentResult>> = HashMap::new();
    for r in results {
        by_approach.entry(r.approach.clone()).or_default().push(r);
    }

    let mut approach_results: Vec<JudgedApproachResult> = by_approach
        .iter()
        .map(|(approach, items)| {
            let total = items.len();
            let correct = items.iter().filter(|r| r.score == 1).count();
            let accuracy = correct as f64 / total.max(1) as f64;
            let mean_tokens =
                items.iter().map(|r| r.context_tokens as f64).sum::<f64>() / total.max(1) as f64;
            JudgedApproachResult {
                approach: approach.clone(),
                accuracy,
                total,
                correct,
                mean_context_tokens: mean_tokens,
            }
        })
        .collect();

    approach_results.sort_by(|a, b| {
        b.accuracy
            .partial_cmp(&a.accuracy)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    JudgedE2EReport {
        judge_model: judge_model.to_string(),
        total_judged: results.len(),
        results_by_approach: approach_results,
    }
}

// ===== Task-Specific Prompt Functions =====

/// LongMemEval answer-check prompt. Returns the appropriate judge prompt for the task type.
///
/// Rubric text is mirrored verbatim from the LongMemEval canonical evaluator:
/// <https://raw.githubusercontent.com/xiaowu0162/longmemeval/main/src/evaluation/evaluate_qa.py>
/// (function `get_anscheck_prompt`, lines ~20-50). Every LME canonical task category
/// has an explicit branch so we never silently fall through to a default. See the
/// audit test `judge_prompt_has_branch_for_every_lme_task_category` in
/// `crates/origin-core/tests/eval_harness.rs`.
pub fn lme_anscheck_prompt(task: &str, question: &str, answer: &str, response: &str) -> String {
    // Each branch begins with an explicit "Task category:" tag so the
    // category branched on is visible in the rendered prompt and discoverable
    // by audit tests. The instruction body that follows is the verbatim
    // canonical LME rubric for that category.
    match task {
        // LME "temporal-reasoning" + LoCoMo "temporal" — both test temporal recall.
        // Canonical: evaluate_qa.py line 30, prompt for 'temporal-reasoning'.
        "temporal-reasoning" | "temporal" => {
            format!(
                "Task category: temporal-reasoning\n\n\
                 I will give you a question, a correct answer, and a response from a model. \
                 Please answer yes if the response contains the correct answer. Otherwise, answer no. \
                 If the response is equivalent to the correct answer or contains all the intermediate \
                 steps to get the correct answer, you should also answer yes. If the response only \
                 contains a subset of the information required by the answer, answer no. In addition, \
                 do not penalize off-by-one errors for the number of days. If the question asks for \
                 the number of days/weeks/months, etc., and the model makes off-by-one errors \
                 (e.g., predicting 19 days when the answer is 18), the model's response is still \
                 correct. \n\nQuestion: {}\n\nCorrect Answer: {}\n\nModel Response: {}\n\n\
                 Is the model response correct? Answer yes or no only.",
                question, answer, response
            )
        }
        // Canonical: evaluate_qa.py line 34, prompt for 'knowledge-update'.
        "knowledge-update" => {
            format!(
                "Task category: knowledge-update\n\n\
                 I will give you a question, a correct answer, and a response from a model. \
                 Please answer yes if the response contains the correct answer. Otherwise, answer no. \
                 If the response contains some previous information along with an updated answer, the \
                 response should be considered as correct as long as the updated answer is the required \
                 answer.\n\nQuestion: {}\n\nCorrect Answer: {}\n\nModel Response: {}\n\n\
                 Is the model response correct? Answer yes or no only.",
                question, answer, response
            )
        }
        // Canonical: evaluate_qa.py line 38, prompt for 'single-session-preference'.
        "single-session-preference" => {
            format!(
                "Task category: single-session-preference\n\n\
                 I will give you a question, a rubric for desired personalized response, and a \
                 response from a model. Please answer yes if the response satisfies the desired \
                 response. Otherwise, answer no. The model does not need to reflect all the points \
                 in the rubric. The response is correct as long as it recalls and utilizes the \
                 user's personal information correctly.\n\n\
                 Question: {}\n\nRubric: {}\n\nModel Response: {}\n\n\
                 Is the model response correct? Answer yes or no only.",
                question, answer, response
            )
        }
        // LME canonical groups single-session-user, single-session-assistant, and
        // multi-session under the same base prompt (evaluate_qa.py line 26). We
        // mirror that grouping but split into named branches so each LME category
        // has explicit coverage and shows up as its own branch in the audit.
        "single-session-user" => {
            format!(
                "Task category: single-session-user\n\n\
                 {}",
                lme_base_prompt(question, answer, response)
            )
        }
        "single-session-assistant" => {
            format!(
                "Task category: single-session-assistant\n\n\
                 {}",
                lme_base_prompt(question, answer, response)
            )
        }
        "multi-session" => {
            format!(
                "Task category: multi-session\n\n\
                 {}",
                lme_base_prompt(question, answer, response)
            )
        }
        _ => {
            // Standard benchmark prompt — same as LoCoMo and LME SSU/SSA/MS.
            // Includes equivalence + subset guidance for fair evaluation.
            lme_base_prompt(question, answer, response)
        }
    }
}

/// The base LongMemEval answer-check prompt body, used verbatim from
/// `evaluate_qa.py` line 26 (single-session-user / single-session-assistant /
/// multi-session) and reused as the default fallback.
fn lme_base_prompt(question: &str, answer: &str, response: &str) -> String {
    format!(
        "I will give you a question, a correct answer, and a response from a model. \
         Please answer yes if the response contains the correct answer. Otherwise, answer no. \
         If the response is equivalent to the correct answer or contains all the intermediate \
         steps to get the correct answer, you should also answer yes. If the response only \
         contains a subset of the information required by the answer, answer no.\n\n\
         Question: {}\n\nCorrect Answer: {}\n\nModel Response: {}\n\n\
         Is the model response correct? Answer yes or no only.",
        question, answer, response
    )
}

/// LongMemEval answer prompt. Returns (user_prompt, system_prompt) for generating answers.
pub fn lme_answer_prompt(question: &str, context: &str, question_type: &str) -> (String, String) {
    if context.is_empty() {
        return (
            format!(
                "Question: {}\n\nAnswer the question as best you can. Be specific and concise.",
                question
            ),
            "Be specific and concise. Respond in 1-3 sentences.".to_string(),
        );
    }
    match question_type {
        "single-session-preference" => {
            let prompt = format!(
                "The following context contains information about a user's preferences, \
                 interests, and past choices:\n\n{}\n\nQuestion: {}\n\n\
                 Use the user's preferences and interests from the context to \
                 personalize your response. Apply their known preferences even if \
                 this specific scenario isn't mentioned.",
                context, question
            );
            let sys = "You are a personalized assistant. Use the user's known preferences \
                to tailor your response. Be specific and concise. Respond in 1-3 sentences."
                .to_string();
            (prompt, sys)
        }
        _ => {
            let prompt = format!(
                "Context:\n{}\n\nQuestion: {}\n\nAnswer the question based on the context provided. \
                 Be specific and concise.",
                context, question
            );
            let sys =
                "Answer the question based on the provided context. Be specific and concise. \
                Respond in 1-3 sentences."
                    .to_string();
            (prompt, sys)
        }
    }
}

/// LoCoMo judge prompt. Standard binary yes/no judge for LoCoMo eval.
pub fn locomo_judge_prompt(question: &str, ground_truth: &str, model_answer: &str) -> String {
    format!(
        "I will give you a question, a correct answer, and a response from a model. \
         Please answer yes if the response contains the correct answer. Otherwise, answer no. \
         If the response is equivalent to the correct answer or contains all the intermediate \
         steps to get the correct answer, you should also answer yes. If the response only \
         contains a subset of the information required by the answer, answer no.\n\n\
         Question: {}\n\nCorrect Answer: {}\n\nModel Response: {}\n\n\
         Is the model response correct? Answer yes or no only.",
        question, ground_truth, model_answer
    )
}

// ===== Batch API Judge =====

/// Judge answer tuples using Anthropic Batch API.
///
/// 50% cheaper than direct API, no rate limits. Cost cap via
/// `EVAL_COST_CAP` env var (default $2).
pub async fn judge_with_batch_api(
    tuples: &[JudgmentTuple],
    judge_model: &str,
    cost_cap: Option<f64>,
) -> Result<Vec<JudgmentResult>, crate::error::OriginError> {
    use crate::eval::anthropic::{
        download_batch_results_structured, poll_batch, submit_batch_with_tool,
    };

    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .map_err(|_| crate::error::OriginError::Generic("ANTHROPIC_API_KEY not set".into()))?;
    let cost_cap = cost_cap.unwrap_or_else(|| {
        std::env::var("EVAL_COST_CAP")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2.0)
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| crate::error::OriginError::Generic(format!("reqwest build: {e}")))?;

    // Build batch requests — each tuple becomes a judge prompt
    let requests: Vec<(String, String, Option<String>, usize)> = tuples
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let prompt = task_judge_prompt(&t.category, &t.question, &t.ground_truth, &t.answer);
            (format!("judge_{i}"), prompt, None, 128usize)
        })
        .collect();

    eprintln!(
        "[judge_batch] Submitting {} requests (model={judge_model})...",
        requests.len()
    );

    let batch_id = submit_batch_with_tool(
        &client,
        &api_key,
        requests,
        verdict_tool(),
        "record_verdict",
        judge_model,
        cost_cap,
    )
    .await
    .map_err(|e| crate::error::OriginError::Generic(format!("batch submit: {e}")))?;

    let results_url = poll_batch(&client, &api_key, &batch_id)
        .await
        .map_err(|e| crate::error::OriginError::Generic(format!("batch poll: {e}")))?;

    let raw_results = download_batch_results_structured(&client, &api_key, &results_url)
        .await
        .map_err(|e| crate::error::OriginError::Generic(format!("batch download: {e}")))?;

    let mut results = Vec::new();
    for (i, tuple) in tuples.iter().enumerate() {
        let id = format!("judge_{i}");
        let (score, reason) = match raw_results.get(&id) {
            Some(content) => extract_tool_verdict(content),
            None => {
                eprintln!("[judge_batch] missing result for {id}");
                (0u8, "judge error: missing result".to_string())
            }
        };
        results.push(JudgmentResult {
            question: tuple.question.clone(),
            approach: tuple.approach.clone(),
            score,
            reason,
            context_tokens: tuple.context_tokens,
        });
    }

    eprintln!(
        "[judge_batch] Done. {}/{} judged.",
        results.len(),
        tuples.len()
    );
    Ok(results)
}

/// Tool schema for the binary judge verdict. Passed as a forced tool_choice in
/// the batch judge so every response is structured JSON, not free text.
pub(crate) fn verdict_tool() -> serde_json::Value {
    serde_json::json!({
        "name": "record_verdict",
        "description": "Record the binary verdict for the model response.",
        "input_schema": {
            "type": "object",
            "properties": {
                "verdict": {
                    "type": "string",
                    "enum": ["yes", "no"],
                    "description": "yes if response is correct per the rubric, otherwise no"
                },
                "verdict_reason": {
                    "type": "string",
                    "description": "one-sentence justification"
                }
            },
            "required": ["verdict", "verdict_reason"]
        }
    })
}

/// Extract a binary verdict + reason from a batch response content array.
/// Returns `(score, reason)`. Score is 1 for yes, 0 for everything else
/// including parse failure. Parse failures eprintln so the operator sees them.
///
/// Happy path: find the first `tool_use` block named `record_verdict`, read
/// `input.verdict` (case-insensitive `yes`/`no`).
///
/// Fallback: if no tool_use block matches, try a `text` block via
/// `parse_judge_output`. This handles the rare case where the model bypasses
/// the forced tool_choice or returns mixed content.
pub(crate) fn extract_tool_verdict(content: &serde_json::Value) -> (u8, String) {
    if let Some(blocks) = content.as_array() {
        for block in blocks {
            if block["type"] == "tool_use" && block["name"] == "record_verdict" {
                if let Some(input) = block["input"].as_object() {
                    let verdict_raw = input.get("verdict").and_then(|v| v.as_str()).unwrap_or("");
                    let reason = input
                        .get("verdict_reason")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    let normalized = verdict_raw.trim().to_ascii_lowercase();
                    let score = match normalized.as_str() {
                        "yes" => 1u8,
                        "no" => 0u8,
                        other => {
                            eprintln!(
                                "[judge_batch] tool_use verdict not in enum: {other:?}; scoring 0"
                            );
                            0u8
                        }
                    };
                    return (score, reason);
                } else {
                    eprintln!("[judge_batch] tool_use input not an object; scoring 0");
                    return (0, String::new());
                }
            }
        }
    }

    let fallback_text = content
        .as_array()
        .and_then(|blocks| blocks.iter().find(|b| b["type"] == "text"))
        .and_then(|b| b["text"].as_str())
        .unwrap_or("");

    if fallback_text.is_empty() {
        eprintln!("[judge_batch] tool_use absent and no text fallback; scoring 0");
        return (0, String::new());
    }

    eprintln!("[judge_batch] tool_use absent; falling back to text parse");
    let score = parse_judge_output(fallback_text)
        .map(|p| {
            if p.verdict == JudgeVerdict::Yes {
                1u8
            } else {
                0u8
            }
        })
        .unwrap_or(0);
    (score, fallback_text.to_string())
}

/// Stamp the judge model id on a Report's env (no-op when env is None).
/// Used by runners to record which judge produced the answer-quality metrics.
pub fn stamp_judge_model(env: &mut Option<crate::eval::report::ReportEnv>, model: &str) {
    if let Some(e) = env.as_mut() {
        e.judge_model = Some(model.to_string());
    }
}

// ===== Per-row JudgeVerdict stamping =====
//
// `StampedVerdict` captures which judge produced each individual verdict.
// Useful for replay + audit when a baseline run is later disputed: we can
// re-query the judge with the same question_id and confirm the verdict
// matches.
//
// **Naming.** Plan §Task 3 originally specified `JudgeVerdict` as the
// per-row struct name. The existing enum at line 1313 already owns that
// name (Yes/No tag), so the per-row type lands as `StampedVerdict` and
// the inner pre-stamp shape as `StampedVerdictCore`. Same intent.
//
// **NOT YET WIRED.** As of PR #192 these symbols exist only as
// unit-tested scaffolding. The batch-judge iteration in
// `crates/origin-core/src/eval/answer_quality.rs` does not call
// `stamp_verdict_row` or `write_judge_verdicts_jsonl`. Wiring is
// deferred to the next PR that regenerates answer-quality baselines,
// where the JSONL is actually consumed. Until then, treat these as
// API surface only — no JSONL audit trail will be produced.

/// Per-row, fully-stamped judge verdict. Serialized into the per-run
/// JSONL at `<baselines_root>/judge_verdicts/<run_id>.jsonl`.
///
/// NOT YET WIRED into the batch-judge loop — see module-level note above.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StampedVerdict {
    pub question_id: String,
    pub verdict: bool,
    pub raw_output: String,
    pub judge_model_used: String,
    pub judge_response_id: Option<String>,
}

/// Pre-stamp shape: caller builds this from batch result data and
/// passes it to `stamp_verdict_row` along with the judge model id.
#[derive(Debug, Clone)]
pub struct StampedVerdictCore {
    pub question_id: String,
    pub verdict: bool,
    pub raw_output: String,
}

/// Stamp a `StampedVerdictCore` with the judge model id + optional
/// upstream response id (e.g. Anthropic batch `id` field).
pub fn stamp_verdict_row(
    core: StampedVerdictCore,
    judge_model: &str,
    response_id: Option<String>,
) -> StampedVerdict {
    StampedVerdict {
        question_id: core.question_id,
        verdict: core.verdict,
        raw_output: core.raw_output,
        judge_model_used: judge_model.to_string(),
        judge_response_id: response_id,
    }
}

/// Write `verdicts` to `<baselines_root>/judge_verdicts/<run_id>.jsonl`.
/// Returns the path written.
pub fn write_judge_verdicts_jsonl(
    baselines_root: &Path,
    run_id: &str,
    verdicts: &[StampedVerdict],
) -> std::io::Result<PathBuf> {
    use std::io::Write;
    let out_dir = baselines_root.join("judge_verdicts");
    std::fs::create_dir_all(&out_dir)?;
    let path = out_dir.join(format!("{}.jsonl", run_id));
    let file = std::fs::File::create(&path)?;
    let mut writer = std::io::BufWriter::new(file);
    for v in verdicts {
        let line = serde_json::to_string(v).map_err(std::io::Error::other)?;
        writeln!(writer, "{}", line)?;
    }
    writer.flush()?;
    Ok(path)
}

// ===== Structured Judge Output =====

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JudgeVerdict {
    Yes,
    No,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct StructuredJudgeOutput {
    #[serde(default)]
    pub rubric_scores: BTreeMap<String, f64>,
    #[serde(default)]
    pub verdict_reason: String,
    pub verdict: JudgeVerdict,
}

/// Parse a judge response into a structured verdict.
///
/// Priority order:
/// 1. Direct JSON object matching `StructuredJudgeOutput`.
/// 2. Markdown-fenced JSON (```json ... ```).
/// 3. Legacy bare token — exact match only ("yes" | "correct" | "no" | "incorrect").
///
/// Tier 3 is strict (exact match, not substring) to prevent regressions like
/// "corrects" classifying as Yes. The `partial` token is intentionally absent
/// since v2 verdict space is binary; the back-compat `correct`/`incorrect`
/// tokens cover any pre-rewrite JSONL strings stored by the branch's
/// structured-output era.
///
/// Multi-line raw judge text like "No.\n\nThe model response..." (the shape
/// stored in cached `lme_accuracy_*.json` `judge_response` fields) is NOT
/// supported and will error. Replay of those caches requires re-judging.
pub fn parse_judge_output(raw: &str) -> Result<StructuredJudgeOutput, String> {
    let trimmed = raw.trim();

    // Tier 1: direct JSON.
    if let Ok(s) = serde_json::from_str::<StructuredJudgeOutput>(trimmed) {
        return Ok(s);
    }

    // Tier 2: markdown-fenced JSON.
    if let Some(fenced) = strip_json_fence(trimmed) {
        if let Ok(s) = serde_json::from_str::<StructuredJudgeOutput>(fenced) {
            return Ok(s);
        }
    }

    // Tier 3: exact-match legacy bare token.
    let normalized = trimmed.to_ascii_lowercase();
    let legacy = match normalized.as_str() {
        "yes" | "correct" => Some(JudgeVerdict::Yes),
        "no" | "incorrect" => Some(JudgeVerdict::No),
        _ => None,
    };
    legacy
        .map(|v| StructuredJudgeOutput {
            rubric_scores: Default::default(),
            verdict_reason: String::new(),
            verdict: v,
        })
        .ok_or_else(|| {
            format!(
                "unparseable judge output: {}",
                trimmed.chars().take(80).collect::<String>()
            )
        })
}

fn strip_json_fence(s: &str) -> Option<&str> {
    let s = s.strip_prefix("```json")?.trim_start();
    s.strip_suffix("```").map(|inner| inner.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_judge_output_json_yes() {
        let raw = r#"{"rubric_scores": {}, "verdict_reason": "ok", "verdict": "yes"}"#;
        assert_eq!(parse_judge_output(raw).unwrap().verdict, JudgeVerdict::Yes);
    }

    #[test]
    fn parse_judge_output_json_no() {
        let raw = r#"{"rubric_scores": {}, "verdict_reason": "no", "verdict": "no"}"#;
        assert_eq!(parse_judge_output(raw).unwrap().verdict, JudgeVerdict::No);
    }

    #[test]
    fn parse_judge_output_fenced_json() {
        let raw =
            "```json\n{\"rubric_scores\":{},\"verdict_reason\":\"x\",\"verdict\":\"no\"}\n```";
        assert_eq!(parse_judge_output(raw).unwrap().verdict, JudgeVerdict::No);
    }

    #[test]
    fn parse_judge_output_legacy_yes_no_strings_exact_match() {
        assert_eq!(
            parse_judge_output("yes").unwrap().verdict,
            JudgeVerdict::Yes
        );
        assert_eq!(parse_judge_output("No").unwrap().verdict, JudgeVerdict::No);
    }

    #[test]
    fn parse_judge_output_legacy_correct_incorrect_back_compat() {
        assert_eq!(
            parse_judge_output("correct").unwrap().verdict,
            JudgeVerdict::Yes
        );
        assert_eq!(
            parse_judge_output("incorrect").unwrap().verdict,
            JudgeVerdict::No
        );
    }

    #[test]
    fn parse_judge_output_drops_partial() {
        assert!(parse_judge_output("partial").is_err());
    }

    // Regression test: branch's parse_judge_output_legacy_rejects_substring_false_positives
    // was added specifically to catch silent classification of misleading inputs.
    // Spec v2 restored exact-match semantics; this test guards that.
    #[test]
    fn parse_judge_output_rejects_substring_false_positives() {
        assert!(parse_judge_output("corrects").is_err());
        assert!(parse_judge_output("incorrect-ish").is_err());
        assert!(parse_judge_output("yes please").is_err());
        assert!(parse_judge_output("yeses").is_err());
        assert!(parse_judge_output("nope").is_err());
    }

    #[test]
    fn parse_judge_output_rejects_multiline_text() {
        // Documents that multi-line raw judge text is intentionally not handled.
        // Cached pre-rewrite JSONLs cannot be replayed; re-judge is required.
        assert!(parse_judge_output("No.\n\nThe model response is wrong.").is_err());
    }

    #[test]
    fn parse_judge_output_garbage_errors() {
        assert!(parse_judge_output("¯\\_(ツ)_/¯").is_err());
    }

    #[test]
    fn extract_tool_verdict_happy_path_yes() {
        let content = serde_json::json!([{
            "type": "tool_use",
            "name": "record_verdict",
            "input": {"verdict": "yes", "verdict_reason": "matches gold"}
        }]);
        let (score, reason) = extract_tool_verdict(&content);
        assert_eq!(score, 1);
        assert_eq!(reason, "matches gold");
    }

    #[test]
    fn extract_tool_verdict_happy_path_no() {
        let content = serde_json::json!([{
            "type": "tool_use",
            "name": "record_verdict",
            "input": {"verdict": "no", "verdict_reason": "missing required fact"}
        }]);
        let (score, _) = extract_tool_verdict(&content);
        assert_eq!(score, 0);
    }

    #[test]
    fn extract_tool_verdict_content_null_scores_zero() {
        let content = serde_json::Value::Null;
        let (score, _) = extract_tool_verdict(&content);
        assert_eq!(score, 0);
    }

    #[test]
    fn extract_tool_verdict_wrong_tool_name_falls_through() {
        let content = serde_json::json!([{
            "type": "tool_use",
            "name": "wrong_tool",
            "input": {"verdict": "yes"}
        }]);
        // No matching tool_use block + no text block → score 0.
        let (score, _) = extract_tool_verdict(&content);
        assert_eq!(score, 0);
    }

    #[test]
    fn extract_tool_verdict_input_is_string_not_object() {
        let content = serde_json::json!([{
            "type": "tool_use",
            "name": "record_verdict",
            "input": "yes"
        }]);
        let (score, _) = extract_tool_verdict(&content);
        assert_eq!(score, 0);
    }

    #[test]
    fn extract_tool_verdict_missing_verdict_key_scores_zero() {
        let content = serde_json::json!([{
            "type": "tool_use",
            "name": "record_verdict",
            "input": {"verdict_reason": "no verdict provided"}
        }]);
        let (score, _) = extract_tool_verdict(&content);
        assert_eq!(score, 0);
    }

    #[test]
    fn extract_tool_verdict_unexpected_verdict_value_scores_zero() {
        let content = serde_json::json!([{
            "type": "tool_use",
            "name": "record_verdict",
            "input": {"verdict": "maybe", "verdict_reason": "unsure"}
        }]);
        let (score, _) = extract_tool_verdict(&content);
        assert_eq!(score, 0);
    }

    #[test]
    fn extract_tool_verdict_case_insensitive_yes() {
        let content = serde_json::json!([{
            "type": "tool_use",
            "name": "record_verdict",
            "input": {"verdict": "YES", "verdict_reason": "exact"}
        }]);
        let (score, _) = extract_tool_verdict(&content);
        assert_eq!(score, 1);
    }

    #[test]
    fn extract_tool_verdict_falls_back_to_text_when_no_tool_block() {
        let content = serde_json::json!([{"type": "text", "text": "yes"}]);
        let (score, _) = extract_tool_verdict(&content);
        assert_eq!(score, 1);
    }

    #[test]
    fn extract_tool_verdict_text_fallback_strict_match() {
        // parse_judge_output is exact-match; multi-line text scores 0.
        let content =
            serde_json::json!([{"type": "text", "text": "Yes.\n\nThe model response is correct."}]);
        let (score, _) = extract_tool_verdict(&content);
        assert_eq!(score, 0);
    }

    #[test]
    fn stamp_verdict_row_carries_model_and_id() {
        let core = StampedVerdictCore {
            question_id: "q1".into(),
            verdict: true,
            raw_output: "yes".into(),
        };
        let stamped = stamp_verdict_row(core, "claude-haiku-4-5", Some("msg_abc".into()));
        assert_eq!(stamped.question_id, "q1");
        assert!(stamped.verdict);
        assert_eq!(stamped.raw_output, "yes");
        assert_eq!(stamped.judge_model_used, "claude-haiku-4-5");
        assert_eq!(stamped.judge_response_id.as_deref(), Some("msg_abc"));
    }

    #[test]
    fn write_judge_verdicts_jsonl_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let verdicts = vec![
            stamp_verdict_row(
                StampedVerdictCore {
                    question_id: "q1".into(),
                    verdict: true,
                    raw_output: "yes".into(),
                },
                "haiku",
                None,
            ),
            stamp_verdict_row(
                StampedVerdictCore {
                    question_id: "q2".into(),
                    verdict: false,
                    raw_output: "no".into(),
                },
                "haiku",
                Some("msg_xyz".into()),
            ),
        ];
        let path = write_judge_verdicts_jsonl(tmp.path(), "run_test", &verdicts).unwrap();
        assert!(path.ends_with("judge_verdicts/run_test.jsonl"));
        let body = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = body.lines().collect();
        assert_eq!(lines.len(), 2);
        let back: StampedVerdict = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(back, verdicts[0]);
        let back2: StampedVerdict = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(back2, verdicts[1]);
    }
}

// ===== Paired McNemar answer-accuracy comparator =====

/// Aggregate + per-category McNemar test on paired CE answer-accuracy A/B results.
///
/// `results` must contain entries with `approach` prefixed by `"ce_off_"` or `"ce_on_"`.
/// Pairs are joined on `question`. Unpaired questions are excluded with a log line.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McNemarReport {
    /// Aggregate OFF accuracy (fraction of pairs where OFF arm scored 1).
    pub off_acc: f64,
    /// Aggregate ON accuracy (fraction of pairs where ON arm scored 1).
    pub on_acc: f64,
    /// on_acc − off_acc.
    pub delta: f64,
    /// Pairs where OFF=1, ON=0.
    pub b: u64,
    /// Pairs where OFF=0, ON=1.
    pub c: u64,
    /// Total complete pairs used.
    pub n_pairs: u64,
    /// Test statistic (χ² or exact-binomial n_min for small-n path).
    pub stat: f64,
    /// Two-sided p-value.
    pub p_value: f64,
    /// "mcnemar_chi2" or "exact_binomial" (when b+c < 25).
    pub test_used: String,
    /// Per-category breakdown (category parsed from approach suffix after `ce_off_`/`ce_on_`).
    pub by_category: Vec<McNemarCategory>,
}

/// Per-category slice of a McNemar report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McNemarCategory {
    pub category: String,
    pub off_acc: f64,
    pub on_acc: f64,
    pub delta: f64,
    pub b: u64,
    pub c: u64,
    pub n_pairs: u64,
}

/// Compute `McNemarReport` from a slice of `JudgmentResult`s.
///
/// Arms are identified by approach prefix (`ce_off_<category>` / `ce_on_<category>`).
/// Pairs are joined on `question`. Unpaired entries are skipped.
pub fn paired_answer_mcnemar(results: &[JudgmentResult]) -> McNemarReport {
    use std::collections::HashMap;

    // Collect per-question score for each arm. Key = question text.
    let mut off_map: HashMap<&str, (u8, String)> = HashMap::new(); // question -> (score, category)
    let mut on_map: HashMap<&str, u8> = HashMap::new();

    for r in results {
        if let Some(cat) = r.approach.strip_prefix("ce_off_") {
            off_map.insert(r.question.as_str(), (r.score, cat.to_string()));
        } else if r.approach.starts_with("ce_on_") {
            on_map.insert(r.question.as_str(), r.score);
        }
    }

    // Build complete pairs.
    let mut agg_b = 0u64; // off=1, on=0
    let mut agg_c = 0u64; // off=0, on=1
    let mut agg_off_correct = 0u64;
    let mut agg_on_correct = 0u64;
    let mut agg_n = 0u64;

    // Per-category accumulation.
    let mut cat_b: HashMap<String, u64> = HashMap::new();
    let mut cat_c: HashMap<String, u64> = HashMap::new();
    let mut cat_off: HashMap<String, u64> = HashMap::new();
    let mut cat_on: HashMap<String, u64> = HashMap::new();
    let mut cat_n: HashMap<String, u64> = HashMap::new();

    for (question, (off_score, category)) in &off_map {
        if let Some(on_score) = on_map.get(question) {
            agg_n += 1;
            agg_off_correct += u64::from(*off_score);
            agg_on_correct += u64::from(*on_score);
            if *off_score == 1 && *on_score == 0 {
                agg_b += 1;
            } else if *off_score == 0 && *on_score == 1 {
                agg_c += 1;
            }

            *cat_n.entry(category.clone()).or_default() += 1;
            *cat_off.entry(category.clone()).or_default() += u64::from(*off_score);
            *cat_on.entry(category.clone()).or_default() += u64::from(*on_score);
            if *off_score == 1 && *on_score == 0 {
                *cat_b.entry(category.clone()).or_default() += 1;
            } else if *off_score == 0 && *on_score == 1 {
                *cat_c.entry(category.clone()).or_default() += 1;
            }
        }
    }

    let unpaired_off = off_map.len().saturating_sub(agg_n as usize);
    let unpaired_on = on_map.len().saturating_sub(agg_n as usize);
    if unpaired_off > 0 || unpaired_on > 0 {
        log::warn!(
            "[mcnemar] {} OFF-only and {} ON-only entries excluded (unpaired)",
            unpaired_off,
            unpaired_on
        );
    }

    let (stat, p_value, test_used) = mcnemar_stat(agg_b, agg_c);

    let off_acc = if agg_n > 0 {
        agg_off_correct as f64 / agg_n as f64
    } else {
        0.0
    };
    let on_acc = if agg_n > 0 {
        agg_on_correct as f64 / agg_n as f64
    } else {
        0.0
    };

    // Build per-category vec, sorted by category name for determinism.
    let mut all_cats: Vec<String> = cat_n.keys().cloned().collect();
    all_cats.sort();
    let by_category: Vec<McNemarCategory> = all_cats
        .into_iter()
        .map(|cat| {
            let n = *cat_n.get(&cat).unwrap_or(&0);
            let off_c = *cat_off.get(&cat).unwrap_or(&0);
            let on_c = *cat_on.get(&cat).unwrap_or(&0);
            let b = *cat_b.get(&cat).unwrap_or(&0);
            let c = *cat_c.get(&cat).unwrap_or(&0);
            McNemarCategory {
                category: cat,
                off_acc: if n > 0 { off_c as f64 / n as f64 } else { 0.0 },
                on_acc: if n > 0 { on_c as f64 / n as f64 } else { 0.0 },
                delta: if n > 0 {
                    on_c as f64 / n as f64 - off_c as f64 / n as f64
                } else {
                    0.0
                },
                b,
                c,
                n_pairs: n,
            }
        })
        .collect();

    McNemarReport {
        off_acc,
        on_acc,
        delta: on_acc - off_acc,
        b: agg_b,
        c: agg_c,
        n_pairs: agg_n,
        stat,
        p_value,
        test_used,
        by_category,
    }
}

/// Compute McNemar statistic + two-sided p-value.
///
/// Uses exact two-sided binomial when `b + c < 25` (standard small-n threshold),
/// McNemar χ²=(|b−c|−1)²/(b+c) with continuity correction otherwise.
fn mcnemar_stat(b: u64, c: u64) -> (f64, f64, String) {
    let n_disc = b + c;
    if n_disc == 0 {
        // No discordant pairs — cannot distinguish arms.
        return (0.0, 1.0, "mcnemar_chi2".to_string());
    }

    if n_disc < 25 {
        // Exact two-sided binomial: P(X <= min(b,c)) * 2, clamped to [0,1].
        // Under H0, B ~ Binomial(n=b+c, p=0.5).
        let n = n_disc;
        let k = b.min(c);
        let p_one_tail = binomial_cdf(n, k, 0.5);
        let p = (p_one_tail * 2.0).min(1.0);
        return (k as f64, p, "exact_binomial".to_string());
    }

    // McNemar χ² with continuity correction.
    let diff = (b as f64 - c as f64).abs() - 1.0;
    let chi2 = (diff * diff) / n_disc as f64;
    let p = chi2_df1_p_value(chi2);
    (chi2, p, "mcnemar_chi2".to_string())
}

/// Cumulative binomial P(X <= k) for X ~ Bin(n, 0.5), computed EXACTLY.
///
/// McNemar's exact path only ever uses `p = 0.5` with `n = b + c < 25`
/// (the `n >= 25` case takes the chi² path). For that regime the CDF is the
/// closed form `sum_{i=0}^{k} C(n,i) / 2^n`, which f64 represents exactly:
/// the largest term `C(24,12) ≈ 2.7e6` and `2^24` both fit with no rounding,
/// so there is no need for a regularized-incomplete-beta approximation.
///
/// `p` is accepted for signature clarity but must be `0.5`; any other value
/// would be a programming error (debug-asserted).
fn binomial_cdf(n: u64, k: u64, p: f64) -> f64 {
    debug_assert!((p - 0.5).abs() < 1e-12, "binomial_cdf only supports p=0.5");
    if k >= n {
        return 1.0;
    }
    let mut sum: f64 = 0.0;
    for i in 0..=k {
        sum += binomial_coeff(n, i);
    }
    sum / 2f64.powi(n as i32)
}

/// Exact binomial coefficient C(n, k) as f64. Exact for the small `n < 25`
/// McNemar regime (values stay well under 2^53).
fn binomial_coeff(n: u64, k: u64) -> f64 {
    if k > n {
        return 0.0;
    }
    let k = k.min(n - k); // symmetry: C(n,k) = C(n,n-k)
    let mut result: f64 = 1.0;
    for i in 0..k {
        result = result * (n - i) as f64 / (i + 1) as f64;
    }
    result
}

/// Two-sided p-value from a chi-squared(df=1) statistic via the complementary error function.
///
/// P(chi2(1) > t) = erfc(sqrt(t/2)).
fn chi2_df1_p_value(chi2: f64) -> f64 {
    if chi2 <= 0.0 {
        return 1.0;
    }
    erfc(chi2.sqrt() / std::f64::consts::SQRT_2)
}

/// Complementary error function erfc(x) = 1 - erf(x).
/// Implemented via polynomial approximation; accurate to ~1e-7.
fn erfc(x: f64) -> f64 {
    if x < 0.0 {
        return 2.0 - erfc(-x);
    }
    if x > 10.0 {
        return 0.0;
    }
    // Use asymptotic expansion for large x.
    if x >= 4.0 {
        let t = 1.0 / (x * x);
        let series = 1.0 - t * (0.5 - t * (0.75 - t * (1.875 - t * (6.5625 - t * 29.53125))));
        return series * (-x * x).exp() / (x * std::f64::consts::PI.sqrt());
    }
    // For 0 <= x < 4, use Horner's polynomial (Abramowitz & Stegun 7.1.26, p=0.47047).
    let p = 0.47047_f64;
    let a1 = 0.3480242_f64;
    let a2 = -0.0958798_f64;
    let a3 = 0.7478556_f64;
    let t = 1.0 / (1.0 + p * x);
    let erf_x = 1.0 - (a1 * t + a2 * t * t + a3 * t * t * t) * (-x * x).exp();
    1.0 - erf_x
}

#[cfg(test)]
mod mcnemar_tests {
    use super::*;

    fn make_result(question: &str, approach: &str, score: u8) -> JudgmentResult {
        JudgmentResult {
            question: question.to_string(),
            approach: approach.to_string(),
            score,
            reason: String::new(),
            context_tokens: 0,
        }
    }

    #[test]
    fn mcnemar_clear_on_win() {
        // ON wins on 10 questions, OFF wins on 0. c=10, b=0, n=10.
        let mut results = Vec::new();
        for i in 0..10 {
            let q = format!("question_{i}");
            results.push(make_result(&q, "ce_off_single-hop", 0));
            results.push(make_result(&q, "ce_on_single-hop", 1));
        }
        let report = paired_answer_mcnemar(&results);
        assert_eq!(report.n_pairs, 10);
        assert_eq!(report.b, 0);
        assert_eq!(report.c, 10);
        assert!(report.delta > 0.0, "ON should win: delta={}", report.delta);
        assert_eq!(report.test_used, "exact_binomial", "n_disc=10 < 25");
        // Exact binomial with k=0, n=10: p = 2*(0.5^10) ~ 0.002
        assert!(report.p_value < 0.05, "p={}", report.p_value);
        assert_eq!(report.by_category.len(), 1);
        assert_eq!(report.by_category[0].category, "single-hop");
        assert_eq!(report.by_category[0].c, 10);
    }

    #[test]
    fn mcnemar_clear_off_win() {
        // OFF wins on 8 questions. b=8, c=0.
        let mut results = Vec::new();
        for i in 0..8 {
            let q = format!("q_{i}");
            results.push(make_result(&q, "ce_off_temporal-reasoning", 1));
            results.push(make_result(&q, "ce_on_temporal-reasoning", 0));
        }
        let report = paired_answer_mcnemar(&results);
        assert_eq!(report.b, 8);
        assert_eq!(report.c, 0);
        assert!(report.delta < 0.0, "OFF should win");
        assert_eq!(report.test_used, "exact_binomial");
        assert!(report.p_value < 0.05);
    }

    #[test]
    fn mcnemar_tie_no_discordant() {
        // Both arms always agree — no discordant pairs. b=c=0.
        let mut results = Vec::new();
        for i in 0..20 {
            let q = format!("q_{i}");
            let score = if i % 2 == 0 { 1 } else { 0 };
            results.push(make_result(&q, "ce_off_single-hop", score));
            results.push(make_result(&q, "ce_on_single-hop", score));
        }
        let report = paired_answer_mcnemar(&results);
        assert_eq!(report.b, 0);
        assert_eq!(report.c, 0);
        assert_eq!(report.p_value, 1.0, "no discordant pairs -> p=1");
        assert!((report.delta).abs() < 1e-10, "tie -> delta=0");
    }

    #[test]
    fn mcnemar_small_n_exact_binomial_path() {
        // 5 discordant pairs (b+c=5 < 25) -> exact_binomial path.
        let mut results = Vec::new();
        // 10 concordant pairs (both score 1) — ignored by McNemar.
        for i in 0..10 {
            let q = format!("q_{i}");
            results.push(make_result(&q, "ce_off_multi-session", 1));
            results.push(make_result(&q, "ce_on_multi-session", 1));
        }
        // 5 discordant ON-wins (off=0, on=1).
        for i in 10..15 {
            let q = format!("q_{i}");
            results.push(make_result(&q, "ce_off_multi-session", 0));
            results.push(make_result(&q, "ce_on_multi-session", 1));
        }
        let report = paired_answer_mcnemar(&results);
        assert_eq!(report.test_used, "exact_binomial", "5 discordant -> exact");
        assert_eq!(report.c, 5);
        assert_eq!(report.b, 0);
    }

    #[test]
    fn mcnemar_per_category_split() {
        // Two categories: ON wins single-hop, tie on temporal-reasoning.
        let mut results = Vec::new();
        for i in 0..5 {
            let q = format!("sh_{i}");
            results.push(make_result(&q, "ce_off_single-hop", 0));
            results.push(make_result(&q, "ce_on_single-hop", 1));
        }
        for i in 0..5 {
            let q = format!("tr_{i}");
            results.push(make_result(&q, "ce_off_temporal-reasoning", 1));
            results.push(make_result(&q, "ce_on_temporal-reasoning", 1));
        }
        let report = paired_answer_mcnemar(&results);
        assert_eq!(report.n_pairs, 10);
        assert_eq!(report.by_category.len(), 2);

        let sh = report
            .by_category
            .iter()
            .find(|c| c.category == "single-hop")
            .expect("single-hop category");
        assert_eq!(sh.c, 5);
        assert_eq!(sh.b, 0);
        assert!((sh.on_acc - 1.0).abs() < 1e-9);
        assert!((sh.off_acc).abs() < 1e-9);

        let tr = report
            .by_category
            .iter()
            .find(|c| c.category == "temporal-reasoning")
            .expect("temporal-reasoning category");
        assert_eq!(tr.b, 0);
        assert_eq!(tr.c, 0);
        assert!((tr.delta).abs() < 1e-9);
    }

    #[test]
    fn mcnemar_large_n_chi2_path() {
        // 30 discordant ON-wins (b+c=30 >= 25) -> chi2 path.
        let mut results = Vec::new();
        for i in 0..30 {
            let q = format!("q_{i}");
            results.push(make_result(&q, "ce_off_single-hop", 0));
            results.push(make_result(&q, "ce_on_single-hop", 1));
        }
        let report = paired_answer_mcnemar(&results);
        assert_eq!(report.test_used, "mcnemar_chi2", "30 discordant -> chi2");
        assert!(report.stat > 0.0);
        assert!(report.p_value < 0.05, "p={}", report.p_value);
    }

    // ----- value-pinned numeric guards (regression-protect beta-CF / erfc) -----

    /// Helper: build `b` OFF-wins + `c` ON-wins (all discordant) and return the report.
    fn report_for_bc(b: usize, c: usize) -> McNemarReport {
        let mut results = Vec::new();
        let mut i = 0;
        for _ in 0..b {
            let q = format!("b_{i}");
            results.push(make_result(&q, "ce_off_single-hop", 1));
            results.push(make_result(&q, "ce_on_single-hop", 0));
            i += 1;
        }
        for _ in 0..c {
            let q = format!("c_{i}");
            results.push(make_result(&q, "ce_off_single-hop", 0));
            results.push(make_result(&q, "ce_on_single-hop", 1));
            i += 1;
        }
        paired_answer_mcnemar(&results)
    }

    #[test]
    fn mcnemar_pinned_exact_binomial_0_10() {
        // b=0, c=10: exact two-sided binomial p = 2*(0.5)^10 = 0.001953125.
        let r = report_for_bc(0, 10);
        assert_eq!(r.test_used, "exact_binomial");
        assert!(
            (r.p_value - 0.001_953_125).abs() < 1e-9,
            "p={} expected 0.001953125",
            r.p_value
        );
    }

    #[test]
    fn mcnemar_pinned_exact_binomial_0_8() {
        // b=0, c=8: p = 2*(0.5)^8 = 0.0078125.
        let r = report_for_bc(0, 8);
        assert_eq!(r.test_used, "exact_binomial");
        assert!(
            (r.p_value - 0.007_812_5).abs() < 1e-9,
            "p={} expected 0.0078125",
            r.p_value
        );
    }

    #[test]
    fn mcnemar_pinned_balanced_discordance_clamps_to_one() {
        // b=c=5 (b+c=10 < 25): exact-binomial path; 2*P(X<=5) > 1 so clamps to 1.0.
        // This exercises the `(p*2).min(1.0)` clamp and the b==c symmetry, which
        // the no-discordant tie test (b=c=0) short-circuits before reaching.
        let r = report_for_bc(5, 5);
        assert_eq!(r.test_used, "exact_binomial");
        assert!(
            (r.p_value - 1.0).abs() < 1e-12,
            "p={} expected exactly 1.0 (clamped)",
            r.p_value
        );
    }

    #[test]
    fn mcnemar_pinned_chi2_0_30() {
        // b=0, c=30 (b+c=30 >= 25): chi2 with continuity = (|0-30|-1)^2/30 = 28.0333…,
        // p = erfc(sqrt(chi2/2)) ≈ 1.1924e-7. Guards erfc accuracy in the small-p tail.
        let r = report_for_bc(0, 30);
        assert_eq!(r.test_used, "mcnemar_chi2");
        assert!(
            (r.stat - 28.033_333).abs() < 1e-4,
            "stat={} expected ~28.0333",
            r.stat
        );
        assert!(
            (r.p_value - 1.192_436_7e-7).abs() < 1e-8,
            "p={} expected ~1.1924e-7",
            r.p_value
        );
    }
}
