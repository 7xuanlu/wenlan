// SPDX-License-Identifier: Apache-2.0
//! LoCoMo benchmark adapter — converts locomo10.json into Origin eval cases.
//!
//! LoCoMo (Long Conversational Memory) contains 10 conversations with
//! pre-extracted observations and ~1,986 QA pairs in 5 categories:
//!   1 = multi-hop, 2 = temporal, 3 = open-domain, 4 = single-hop, 5 = adversarial
//!
//! Dataset: <https://github.com/snap-research/locomo>

use crate::db::MemoryDB;
use crate::error::OriginError;
use crate::eval::fixtures::{EvalCase, SeedMemory};
use crate::eval::metrics;
use crate::quality_gate::QualityGate;
use crate::sources::RawDocument;
use crate::tuning::GateConfig;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct LocomoSample {
    pub sample_id: String,
    /// We only read speakers from conversation; the full dialogue is unused.
    pub conversation: serde_json::Value,
    pub qa: Vec<LocomoQA>,
    /// Nested: { "session_N_observation": { "Speaker": [["fact", "dia_id"], ...] } }
    pub observation: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct LocomoQA {
    pub question: String,
    /// Present for non-adversarial questions (categories 1-4).
    /// Can be a string or integer in the dataset, so we use Value.
    pub answer: Option<serde_json::Value>,
    /// Present for adversarial questions (category 5).
    pub adversarial_answer: Option<serde_json::Value>,
    pub evidence: Vec<String>,
    /// 1=multi-hop, 2=temporal, 3=open-domain, 4=single-hop, 5=adversarial
    pub category: u8,
}

/// A single observation extracted from the LoCoMo dataset.
#[derive(Debug, Clone)]
pub struct LocomoMemory {
    pub content: String,
    pub speaker: String,
    pub session_num: usize,
    pub dia_id: String,
    pub sample_id: String,
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

/// Load LoCoMo dataset from a local JSON file.
pub fn load_locomo(path: &Path) -> Result<Vec<LocomoSample>, OriginError> {
    let data = std::fs::read_to_string(path)
        .map_err(|e| OriginError::Generic(format!("Failed to read LoCoMo file: {e}")))?;
    let samples: Vec<LocomoSample> = serde_json::from_str(&data)
        .map_err(|e| OriginError::Generic(format!("Failed to parse LoCoMo JSON: {e}")))?;
    Ok(samples)
}

/// Truncate the loaded LoCoMo samples in place if `EVAL_LOCOMO_LIMIT` is set
/// to a positive integer. Used by every `run_locomo_eval*` variant so a
/// developer can run a small pre-flight subset (~30min) before committing
/// to a full multi-hour run.
fn apply_locomo_limit(samples: &mut Vec<LocomoSample>) {
    apply_limit_from_env(samples, "EVAL_LOCOMO_LIMIT", "locomo", "conversations");
}

/// Shared helper for `apply_locomo_limit`. Parameterized on the env var name so
/// unit tests can exercise the behavior without racing the production var.
fn apply_limit_from_env<T>(samples: &mut Vec<T>, env_var: &str, bench_tag: &str, unit_label: &str) {
    let Some(limit) = std::env::var(env_var)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
    else {
        return;
    };
    let total = samples.len();
    if limit < total {
        samples.truncate(limit);
        log::warn!(
            "[eval/{}] {}={} active -- running on {} of {} {}",
            bench_tag,
            env_var,
            limit,
            samples.len(),
            total,
            unit_label
        );
    }
}

// ---------------------------------------------------------------------------
// Observation extraction
// ---------------------------------------------------------------------------

/// Extract all observation facts from a LoCoMo sample.
///
/// Iterates `session_N_observation` keys, then speaker keys, then `[fact, dia_id]` pairs.
pub fn extract_observations(sample: &LocomoSample) -> Vec<LocomoMemory> {
    let mut memories = Vec::new();
    let obs = match sample.observation.as_object() {
        Some(o) => o,
        None => return memories,
    };

    for (session_key, speakers_val) in obs {
        // Parse session number from keys like "session_1_observation"
        let session_num = parse_session_num(session_key).unwrap_or(0);

        let speakers = match speakers_val.as_object() {
            Some(s) => s,
            None => continue,
        };

        for (speaker, facts_val) in speakers {
            let facts = match facts_val.as_array() {
                Some(f) => f,
                None => continue,
            };

            for fact_pair in facts {
                let pair = match fact_pair.as_array() {
                    Some(p) if p.len() >= 2 => p,
                    _ => continue,
                };

                let content = match pair[0].as_str() {
                    Some(s) => s.to_string(),
                    None => continue,
                };
                let dia_id = match pair[1].as_str() {
                    Some(s) => s.to_string(),
                    None => continue,
                };

                memories.push(LocomoMemory {
                    content,
                    speaker: speaker.clone(),
                    session_num,
                    dia_id,
                    sample_id: sample.sample_id.clone(),
                });
            }
        }
    }

    memories
}

/// Parse session number from a key like "session_3_observation" -> 3.
fn parse_session_num(key: &str) -> Option<usize> {
    // Expected format: session_N_observation
    let stripped = key.strip_prefix("session_")?;
    let num_str = stripped.split('_').next()?;
    num_str.parse().ok()
}

// ---------------------------------------------------------------------------
// Conversion to eval cases
// ---------------------------------------------------------------------------

/// Convert a LoCoMo sample into eval cases for Origin's runner.
///
/// For each non-adversarial QA pair (category != 5):
/// - Observations whose `dia_id` matches the QA evidence become seeds with `relevance=3`
/// - All other observations from the same conversation become seeds with `relevance=1`
///
/// Category 5 (adversarial) is skipped — we don't test abstention yet.
pub fn sample_to_eval_cases(sample: &LocomoSample, memories: &[LocomoMemory]) -> Vec<EvalCase> {
    let mut cases = Vec::new();

    for qa in &sample.qa {
        // Skip adversarial questions
        if qa.category == 5 {
            continue;
        }

        let evidence_set: std::collections::HashSet<&str> =
            qa.evidence.iter().map(|s| s.as_str()).collect();

        let mut seeds = Vec::new();
        for (i, mem) in memories.iter().enumerate() {
            let relevance = if evidence_set.contains(mem.dia_id.as_str()) {
                3
            } else {
                1
            };

            seeds.push(SeedMemory {
                id: format!("locomo_{}_{}", sample.sample_id, i),
                content: mem.content.clone(),
                memory_type: "fact".to_string(),
                space: Some("conversation".to_string()),
                relevance,
                structured_fields: None,
                confidence: None,
                confirmed: None,
                quality: None,
                is_recap: None,
                source_agent: None,
                age_days: None,
                supersedes: None,
            });
        }

        cases.push(EvalCase {
            query: qa.question.clone(),
            space: Some("conversation".to_string()),
            seeds,
            negative_seeds: vec![],
            entities: vec![],
            empty_set: false,
        });
    }

    cases
}

/// Category name for display/reporting.
pub fn category_name(cat: u8) -> &'static str {
    match cat {
        1 => "multi-hop",
        2 => "temporal",
        3 => "open-domain",
        4 => "single-hop",
        5 => "adversarial",
        _ => "unknown",
    }
}

/// Build a `CaseResult` for one LoCoMo QA pair. Metrics not computed by the
/// LoCoMo runner (map_at_10, precision_at_3, negatives) are zero-filled.
#[allow(clippy::too_many_arguments)]
fn build_locomo_case_result(
    question: &str,
    category: u8,
    ndcg_5: f64,
    ndcg_10: f64,
    mrr_val: f64,
    recall_5: f64,
    hr_1: f64,
) -> crate::eval::report::CaseResult {
    crate::eval::report::CaseResult {
        query: question.to_string(),
        ndcg_at_10: ndcg_10,
        ndcg_at_5: ndcg_5,
        map_at_10: 0.0,
        mrr: mrr_val,
        recall_at_5: recall_5,
        hit_rate_at_1: hr_1,
        precision_at_3: 0.0,
        negative_leakage: 0,
        neg_above_relevant: 0,
        category: Some(category_name(category).to_string()),
    }
}

// ---------------------------------------------------------------------------
// Benchmark result structs
// ---------------------------------------------------------------------------

/// Baseline metrics for LoCoMo benchmark comparison across runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocomoBaseline {
    pub ndcg_at_10: f64,
    pub mrr: f64,
    pub recall_at_5: f64,
    pub hit_rate_at_1: f64,
    pub per_category: Vec<crate::eval::report::CategoryBaseline>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage: Option<crate::eval::report::CoverageRecall>,
}

/// Per-category results
#[derive(Debug, Clone, Serialize)]
pub struct LocomoCategoryResult {
    pub category: u8,
    pub name: String,
    pub count: usize,
    pub ndcg_at_5: f64,
    pub ndcg_at_10: f64,
    pub mrr: f64,
    pub recall_at_5: f64,
    pub hit_rate_at_1: f64,
}

/// Results for one LoCoMo conversation
#[derive(Debug, Clone, Serialize)]
pub struct LocomoConversationResult {
    pub sample_id: String,
    pub memories_seeded: usize,
    pub questions_evaluated: usize,
    pub overall_ndcg_at_10: f64,
    pub overall_mrr: f64,
    pub overall_recall_at_5: f64,
    pub per_category: Vec<LocomoCategoryResult>,
}

/// Full LoCoMo benchmark report
#[derive(Debug, Clone, Serialize)]
pub struct LocomoReport {
    pub conversations: Vec<LocomoConversationResult>,
    pub aggregate_ndcg_at_10: f64,
    pub aggregate_mrr: f64,
    pub aggregate_recall_at_5: f64,
    pub aggregate_hit_rate_at_1: f64,
    pub total_questions: usize,
    pub total_memories: usize,
    pub per_category_aggregate: Vec<LocomoCategoryResult>,
    /// Placeholder for future LLM-as-judge QA accuracy (J-score).
    /// Currently None — requires an LLM to generate answers from retrieved context.
    pub qa_accuracy: Option<f64>,
    /// Baseline comparison from a previous run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline: Option<LocomoBaseline>,
    /// Run environment capture (schema v1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<crate::eval::report::ReportEnv>,
    /// Per-question results for paired comparison (McNemar etc.).
    /// Populated by each runner variant; empty by default for back-compat.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub per_case: Vec<crate::eval::report::CaseResult>,
    /// Source-expanded coverage recall (page-channel `_from_db` runner only;
    /// None elsewhere). See [`crate::eval::report::CoverageRecall`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage: Option<crate::eval::report::CoverageRecall>,
}

impl LocomoReport {
    /// Format as terminal-friendly text.
    pub fn to_terminal(&self) -> String {
        let mut out = String::new();
        out.push_str("LoCoMo Benchmark\n");
        out.push_str("================\n");
        out.push_str(&format!("Conversations: {}\n", self.conversations.len()));
        out.push_str(&format!("Total questions: {}\n", self.total_questions));
        out.push_str(&format!("Total memories: {}\n\n", self.total_memories));

        out.push_str(&format!(
            "  NDCG@10:     {:.4}  <- primary\n",
            self.aggregate_ndcg_at_10
        ));
        out.push_str(&format!("  MRR:         {:.4}\n", self.aggregate_mrr));
        out.push_str(&format!(
            "  Recall@5:    {:.4}\n",
            self.aggregate_recall_at_5
        ));
        out.push_str(&format!(
            "  Hit Rate@1:  {:.4}\n",
            self.aggregate_hit_rate_at_1
        ));

        if let Some(ref cov) = self.coverage {
            out.push_str(&format!(
                "  Coverage recall (set-based, page-source-expanded):\n    blind:    {:.4}\n    expanded: {:.4}\n    delta:    {:+.4}  <- page contribution\n",
                cov.blind,
                cov.expanded,
                cov.expanded - cov.blind
            ));
        }

        if let Some(ref b) = self.baseline {
            out.push_str("\nBaseline comparison:\n");
            let delta = |name: &str, old: f64, new: f64| -> String {
                let pct = ((new - old) / old.max(0.001)) * 100.0;
                format!("  {:<12} {:.3} -> {:.3} ({:+.1}%)\n", name, old, new, pct)
            };
            out.push_str(&delta("NDCG@10:", b.ndcg_at_10, self.aggregate_ndcg_at_10));
            out.push_str(&delta("MRR:", b.mrr, self.aggregate_mrr));
            out.push_str(&delta(
                "Recall@5:",
                b.recall_at_5,
                self.aggregate_recall_at_5,
            ));
            out.push_str(&delta(
                "HR@1:",
                b.hit_rate_at_1,
                self.aggregate_hit_rate_at_1,
            ));

            if !b.per_category.is_empty() {
                out.push_str("  Per-category:\n");
                for cat_bl in &b.per_category {
                    if let Some(cat_new) = self
                        .per_category_aggregate
                        .iter()
                        .find(|c| c.name == cat_bl.name)
                    {
                        let pct = ((cat_new.ndcg_at_10 - cat_bl.ndcg_at_10)
                            / cat_bl.ndcg_at_10.max(0.001))
                            * 100.0;
                        out.push_str(&format!(
                            "    {}: {:.3} -> {:.3} ({:+.1}%)\n",
                            cat_bl.name, cat_bl.ndcg_at_10, cat_new.ndcg_at_10, pct
                        ));
                    }
                }
            }
        }

        out.push_str("\nPer category:\n");
        for cat in &self.per_category_aggregate {
            out.push_str(&format!(
                "  {} (n={:>3}): NDCG@10={:.3} MRR={:.3} R@5={:.3} HR@1={:.3}\n",
                cat.name, cat.count, cat.ndcg_at_10, cat.mrr, cat.recall_at_5, cat.hit_rate_at_1,
            ));
        }

        // Per-conversation summary
        if !self.conversations.is_empty() {
            out.push_str("\nPer conversation:\n");
            for conv in &self.conversations {
                out.push_str(&format!(
                    "  {} (n={}, mem={}): NDCG@10={:.3} MRR={:.3} R@5={:.3}\n",
                    conv.sample_id,
                    conv.questions_evaluated,
                    conv.memories_seeded,
                    conv.overall_ndcg_at_10,
                    conv.overall_mrr,
                    conv.overall_recall_at_5,
                ));
            }
        }
        out
    }

    /// Save current metrics as baseline for future comparison.
    pub fn save_baseline(&self, path: &Path) -> Result<(), std::io::Error> {
        let per_category: Vec<crate::eval::report::CategoryBaseline> = self
            .per_category_aggregate
            .iter()
            .map(|c| crate::eval::report::CategoryBaseline {
                name: c.name.clone(),
                ndcg_at_10: c.ndcg_at_10,
                mrr: c.mrr,
                recall_at_5: c.recall_at_5,
            })
            .collect();
        let baseline = LocomoBaseline {
            ndcg_at_10: self.aggregate_ndcg_at_10,
            mrr: self.aggregate_mrr,
            recall_at_5: self.aggregate_recall_at_5,
            hit_rate_at_1: self.aggregate_hit_rate_at_1,
            per_category,
            coverage: self.coverage.clone(),
        };
        let json = serde_json::to_string_pretty(&baseline).map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }

    /// Load baseline from a previous run for comparison.
    pub fn load_baseline(path: &Path) -> Option<LocomoBaseline> {
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Encode retrieval variant + provider + fixture-hash into baseline filename.
    /// Falls back to base + ".json" if env is missing (back-compat).
    pub fn baseline_filename(&self, base: &str) -> String {
        crate::eval::report::encode_baseline_filename(self.env.as_ref(), base)
    }

    /// Project this LocomoReport onto the flat `EvalReport` shape so the
    /// P0b layered baseline path (`save_full_report`) can consume it.
    ///
    /// **Mapping notes:**
    /// - LoCoMo retrieval metrics map onto `ndcg_at_10` / `mrr` / `recall_at_5`
    ///   / `hit_rate_at_1` directly.
    /// - Metrics not surfaced by the LoCoMo runner (ndcg_at_5, map_at_5/10,
    ///   recall_at_1/3, precision_at_3/5, negative analysis) are zero-filled.
    /// - `search_mode` is taken from the env stamp when available.
    /// - `per_case` is left empty; LoCoMo's per-conversation breakdown lives
    ///   on the strongly-typed report. Consumers wanting per-conversation
    ///   data should keep using `LocomoReport` directly.
    pub fn to_eval_report(&self) -> crate::eval::report::EvalReport {
        let search_mode = self
            .env
            .as_ref()
            .map(|e| e.retrieval_method.clone())
            .unwrap_or_else(|| "locomo".to_string());
        crate::eval::report::EvalReport {
            fixture_count: self.total_questions,
            file_count: self.conversations.len(),
            search_mode,
            ndcg_at_10: self.aggregate_ndcg_at_10,
            ndcg_at_5: 0.0,
            map_at_5: 0.0,
            map_at_10: 0.0,
            mrr: self.aggregate_mrr,
            recall_at_1: 0.0,
            recall_at_3: 0.0,
            recall_at_5: self.aggregate_recall_at_5,
            hit_rate_at_1: self.aggregate_hit_rate_at_1,
            hit_rate_at_3: 0.0,
            precision_at_3: 0.0,
            precision_at_5: 0.0,
            neg_above_relevant: 0,
            total_negatives: 0,
            negative_leakage: 0,
            gate_content_filtered: 0,
            gate_novelty_filtered: 0,
            empty_set_count: 0,
            empty_set_false_confidence: None,
            score_gap: None,
            temporal_ordering_total: 0,
            temporal_ordering_correct: 0,
            temporal_ordering_rate: None,
            baseline: None,
            per_case: self.per_case.clone(),
            env: self.env.clone(),
            latency: None,
            total_scenarios: self.conversations.len(),
            skipped_scenarios: Vec::new(),
            enrichment_failures: 0,
            truncated_reason: None,
        }
    }
}

// ---------------------------------------------------------------------------
// ReportEnv builder
// ---------------------------------------------------------------------------

/// Build a `ReportEnv` for a LoCoMo runner variant.
///
/// Fills both the legacy 9 fields (needed by `encode_baseline_filename`) and
/// the new P0a additive fields. The `llm_provider_class` / `llm_model` legacy
/// fields and the new P0a fields carry the same information so both views of
/// the data stay consistent.
fn build_locomo_env(
    variant: &str,
    path: &std::path::Path,
    retrieval_method: &str,
    llm_provider_class: &str,
    llm_model: &str,
    judge_model: Option<String>,
) -> crate::eval::report::ReportEnv {
    let fixture_revision =
        crate::eval::fixtures::fixture_revision_hash(path).unwrap_or_else(|_| "unknown".into());
    let n_runs: u32 = 1;
    let run_id = Some(format!(
        "run_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let now = chrono::Utc::now();
    let timestamp_utc = Some(now.to_rfc3339());
    crate::eval::report::ReportEnv {
        // Legacy fields (needed by encode_baseline_filename and existing callers)
        fixture_revision,
        embedder_model: "BGE-Base-EN-v1.5-Q".into(),
        embedder_revision: "768d".into(),
        retrieval_method: retrieval_method.to_string(),
        llm_provider_class: llm_provider_class.to_string(),
        llm_model: llm_model.to_string(),
        judge_model: judge_model.clone(),
        origin_version: env!("CARGO_PKG_VERSION").into(),
        eval_timestamp_unix: now.timestamp(),
        // P0a additive fields
        layer: Some(crate::eval::EvalLayer::L1Db),
        task: Some("locomo".to_string()),
        variant: Some(variant.to_string()),
        embed_dim: Some(768),
        similarity_fn_name: "cosine".to_string(),
        judge_model_id: judge_model,
        mcp_schema_hash: None,
        skill_prompt_hash: None,
        schema_version: 1,
        schema_db_version: Some(crate::db::SCHEMA_VERSION),
        migrations_hash: option_env!("ORIGIN_MIGRATIONS_HASH").map(String::from),
        n_runs,
        is_single_run: n_runs == 1,
        run_id,
        timestamp_utc,
        git_sha: option_env!("ORIGIN_GIT_SHA").map(String::from),
        warmup_iterations: 0,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// End-to-end benchmark runner
// ---------------------------------------------------------------------------

/// Run LoCoMo benchmark. For each conversation:
/// 1. Create fresh ephemeral DB
/// 2. Seed ALL observations as memories
/// 3. For each non-adversarial QA pair, search and score
/// 4. Aggregate per-category and overall metrics
pub async fn run_locomo_eval(path: &Path) -> Result<LocomoReport, OriginError> {
    let mut samples = load_locomo(path)?;
    apply_locomo_limit(&mut samples);
    let mut conversations = Vec::new();
    // (category, ndcg_5, ndcg_10, mrr, recall_5, hit_rate_1)
    let mut all_scores: Vec<(u8, f64, f64, f64, f64, f64)> = Vec::new();
    let mut per_case: Vec<crate::eval::report::CaseResult> = Vec::new();

    for sample in &samples {
        let memories = extract_observations(sample);

        // Create ephemeral DB for this conversation
        let tmp = tempfile::tempdir().map_err(|e| OriginError::Generic(format!("tempdir: {e}")))?;
        let db = MemoryDB::new(tmp.path(), std::sync::Arc::new(crate::events::NoopEmitter)).await?;

        // Seed all observations as memories
        let docs: Vec<RawDocument> = memories
            .iter()
            .enumerate()
            .map(|(i, mem)| RawDocument {
                content: mem.content.clone(),
                source_id: format!("locomo_{}_obs_{}", sample.sample_id, i),
                source: "memory".to_string(),
                title: format!("{} session {}", mem.speaker, mem.session_num),
                memory_type: Some("fact".to_string()),
                space: Some("conversation".to_string()),
                last_modified: chrono::Utc::now().timestamp(),
                ..Default::default()
            })
            .collect();
        db.upsert_documents(docs).await?;

        // Map dia_id to source_id for relevance judgments
        let dia_to_source: HashMap<String, String> = memories
            .iter()
            .enumerate()
            .map(|(i, m)| {
                (
                    m.dia_id.clone(),
                    format!("locomo_{}_obs_{}", sample.sample_id, i),
                )
            })
            .collect();

        let mut conv_scores: Vec<(u8, f64, f64, f64, f64, f64)> = Vec::new();

        for qa in &sample.qa {
            if qa.category == 5 {
                continue;
            }

            let results = db
                .search_memory(&qa.question, 10, None, None, None, None, None, None)
                .await?;

            // Build relevance judgments: evidence dia_ids -> source_ids = relevant
            let relevant_ids: HashSet<String> = qa
                .evidence
                .iter()
                .filter_map(|did| dia_to_source.get(did).cloned())
                .collect();

            if relevant_ids.is_empty() {
                continue; // Skip if no mappable evidence
            }

            let result_ids: Vec<&str> = results.iter().map(|r| r.source_id.as_str()).collect();

            // Binary relevance: 1 if in evidence set, 0 otherwise
            let grades: HashMap<&str, u8> = result_ids
                .iter()
                .map(|id| (*id, if relevant_ids.contains(*id) { 1 } else { 0 }))
                .collect();

            let relevant_set: HashSet<&str> = relevant_ids.iter().map(|s| s.as_str()).collect();

            let ndcg_10 = metrics::ndcg_at_k(&result_ids, &grades, 10);
            let ndcg_5 = metrics::ndcg_at_k(&result_ids, &grades, 5);
            let mrr_val = metrics::mrr(&result_ids, &relevant_set);
            let recall_5 = metrics::recall_at_k(&result_ids, &relevant_set, 5);
            let hr_1 = metrics::hit_rate_at_k(&result_ids, &relevant_set, 1);

            conv_scores.push((qa.category, ndcg_5, ndcg_10, mrr_val, recall_5, hr_1));
            all_scores.push((qa.category, ndcg_5, ndcg_10, mrr_val, recall_5, hr_1));
            per_case.push(build_locomo_case_result(
                &qa.question,
                qa.category,
                ndcg_5,
                ndcg_10,
                mrr_val,
                recall_5,
                hr_1,
            ));
        }

        // Per-category for this conversation
        let per_cat = aggregate_by_category(&conv_scores);

        let n = conv_scores.len();
        conversations.push(LocomoConversationResult {
            sample_id: sample.sample_id.clone(),
            memories_seeded: memories.len(),
            questions_evaluated: n,
            overall_ndcg_at_10: avg_field(&conv_scores, |s| s.2),
            overall_mrr: avg_field(&conv_scores, |s| s.3),
            overall_recall_at_5: avg_field(&conv_scores, |s| s.4),
            per_category: per_cat,
        });
    }

    // Global aggregates
    let per_cat_agg = aggregate_by_category(&all_scores);

    // TODO: LLM-as-judge QA accuracy requires:
    // 1. Generate answer from retrieved context using an LLM
    // 2. Judge answer correctness against ground truth using LLM-as-judge
    // 3. This is what competitors report as "J-score" or "accuracy"
    // Currently we report retrieval metrics (NDCG, MRR, Recall) which measure
    // whether the right memories are found, not whether the final answer is correct.
    let mut report = LocomoReport {
        conversations,
        aggregate_ndcg_at_10: avg_field(&all_scores, |s| s.2),
        aggregate_mrr: avg_field(&all_scores, |s| s.3),
        aggregate_recall_at_5: avg_field(&all_scores, |s| s.4),
        aggregate_hit_rate_at_1: avg_field(&all_scores, |s| s.5),
        total_questions: all_scores.len(),
        total_memories: samples.iter().map(|s| extract_observations(s).len()).sum(),
        per_category_aggregate: per_cat_agg,
        qa_accuracy: None,
        baseline: None,
        env: None,
        per_case,
        coverage: None,
    };
    report.env = Some(build_locomo_env(
        "base",
        path,
        "search_memory",
        "none",
        "none",
        None,
    ));
    Ok(report)
}

// ---------------------------------------------------------------------------
// Reranked benchmark runner — same as run_locomo_eval but uses search_memory_llm_rerank
// ---------------------------------------------------------------------------

/// Same seeding/scoring logic as `run_locomo_eval`, but retrieval uses
/// `search_memory_llm_rerank` with the supplied LLM for per-query reranking.
#[allow(deprecated)] // search_memory_llm_rerank retained for eval baseline lineage
pub async fn run_locomo_eval_reranked(
    path: &Path,
    llm: std::sync::Arc<dyn crate::llm_provider::LlmProvider>,
) -> Result<LocomoReport, OriginError> {
    let mut samples = load_locomo(path)?;
    apply_locomo_limit(&mut samples);
    let mut conversations = Vec::new();
    // (category, ndcg_5, ndcg_10, mrr, recall_5, hit_rate_1)
    let mut all_scores: Vec<(u8, f64, f64, f64, f64, f64)> = Vec::new();
    let mut per_case: Vec<crate::eval::report::CaseResult> = Vec::new();

    for sample in &samples {
        let memories = extract_observations(sample);

        // Create ephemeral DB for this conversation
        let tmp = tempfile::tempdir().map_err(|e| OriginError::Generic(format!("tempdir: {e}")))?;
        let db = MemoryDB::new(tmp.path(), std::sync::Arc::new(crate::events::NoopEmitter)).await?;

        // Seed all observations as memories
        let docs: Vec<RawDocument> = memories
            .iter()
            .enumerate()
            .map(|(i, mem)| RawDocument {
                content: mem.content.clone(),
                source_id: format!("locomo_{}_obs_{}", sample.sample_id, i),
                source: "memory".to_string(),
                title: format!("{} session {}", mem.speaker, mem.session_num),
                memory_type: Some("fact".to_string()),
                space: Some("conversation".to_string()),
                last_modified: chrono::Utc::now().timestamp(),
                ..Default::default()
            })
            .collect();
        db.upsert_documents(docs).await?;

        // Map dia_id to source_id for relevance judgments
        let dia_to_source: HashMap<String, String> = memories
            .iter()
            .enumerate()
            .map(|(i, m)| {
                (
                    m.dia_id.clone(),
                    format!("locomo_{}_obs_{}", sample.sample_id, i),
                )
            })
            .collect();

        let mut conv_scores: Vec<(u8, f64, f64, f64, f64, f64)> = Vec::new();

        for qa in &sample.qa {
            if qa.category == 5 {
                continue;
            }

            let results = db
                .search_memory_llm_rerank(&qa.question, 10, None, None, None, Some(llm.clone()))
                .await?;

            // Build relevance judgments: evidence dia_ids -> source_ids = relevant
            let relevant_ids: HashSet<String> = qa
                .evidence
                .iter()
                .filter_map(|did| dia_to_source.get(did).cloned())
                .collect();

            if relevant_ids.is_empty() {
                continue; // Skip if no mappable evidence
            }

            let result_ids: Vec<&str> = results.iter().map(|r| r.source_id.as_str()).collect();

            // Binary relevance: 1 if in evidence set, 0 otherwise
            let grades: HashMap<&str, u8> = result_ids
                .iter()
                .map(|id| (*id, if relevant_ids.contains(*id) { 1 } else { 0 }))
                .collect();

            let relevant_set: HashSet<&str> = relevant_ids.iter().map(|s| s.as_str()).collect();

            let ndcg_10 = metrics::ndcg_at_k(&result_ids, &grades, 10);
            let ndcg_5 = metrics::ndcg_at_k(&result_ids, &grades, 5);
            let mrr_val = metrics::mrr(&result_ids, &relevant_set);
            let recall_5 = metrics::recall_at_k(&result_ids, &relevant_set, 5);
            let hr_1 = metrics::hit_rate_at_k(&result_ids, &relevant_set, 1);

            conv_scores.push((qa.category, ndcg_5, ndcg_10, mrr_val, recall_5, hr_1));
            all_scores.push((qa.category, ndcg_5, ndcg_10, mrr_val, recall_5, hr_1));
            per_case.push(build_locomo_case_result(
                &qa.question,
                qa.category,
                ndcg_5,
                ndcg_10,
                mrr_val,
                recall_5,
                hr_1,
            ));
        }

        // Per-category for this conversation
        let per_cat = aggregate_by_category(&conv_scores);

        let n = conv_scores.len();
        conversations.push(LocomoConversationResult {
            sample_id: sample.sample_id.clone(),
            memories_seeded: memories.len(),
            questions_evaluated: n,
            overall_ndcg_at_10: avg_field(&conv_scores, |s| s.2),
            overall_mrr: avg_field(&conv_scores, |s| s.3),
            overall_recall_at_5: avg_field(&conv_scores, |s| s.4),
            per_category: per_cat,
        });
    }

    // Global aggregates
    let per_cat_agg = aggregate_by_category(&all_scores);

    let mut report = LocomoReport {
        conversations,
        aggregate_ndcg_at_10: avg_field(&all_scores, |s| s.2),
        aggregate_mrr: avg_field(&all_scores, |s| s.3),
        aggregate_recall_at_5: avg_field(&all_scores, |s| s.4),
        aggregate_hit_rate_at_1: avg_field(&all_scores, |s| s.5),
        total_questions: all_scores.len(),
        total_memories: samples.iter().map(|s| extract_observations(s).len()).sum(),
        per_category_aggregate: per_cat_agg,
        qa_accuracy: None,
        baseline: None,
        env: None,
        per_case,
        coverage: None,
    };
    report.env = Some(build_locomo_env(
        "reranked",
        path,
        "search_memory_reranked",
        llm.kind(),
        &llm.model_id(),
        None,
    ));
    Ok(report)
}

// ---------------------------------------------------------------------------
// Cross-encoder rerank benchmark runner — same as run_locomo_eval_reranked but
// swaps the LLM reranker for a cross-encoder model (fastembed TextRerank).
// ---------------------------------------------------------------------------

/// Same seeding/scoring logic as `run_locomo_eval_reranked`, but retrieval uses
/// `search_memory_cross_rerank` driven by a cross-encoder reranker
/// (typically `BGERerankerV2M3`). Lets the eval sweep compare LLM-as-judge
/// reranking against a purpose-built cross-encoder on identical fixtures.
pub async fn run_locomo_eval_cross_rerank(
    path: &Path,
    reranker: std::sync::Arc<dyn crate::reranker::Reranker>,
) -> Result<LocomoReport, OriginError> {
    let mut samples = load_locomo(path)?;
    apply_locomo_limit(&mut samples);
    let mut conversations = Vec::new();
    // (category, ndcg_5, ndcg_10, mrr, recall_5, hit_rate_1)
    let mut all_scores: Vec<(u8, f64, f64, f64, f64, f64)> = Vec::new();
    let mut per_case: Vec<crate::eval::report::CaseResult> = Vec::new();

    for sample in &samples {
        let memories = extract_observations(sample);

        // Create ephemeral DB for this conversation
        let tmp = tempfile::tempdir().map_err(|e| OriginError::Generic(format!("tempdir: {e}")))?;
        let db = MemoryDB::new(tmp.path(), std::sync::Arc::new(crate::events::NoopEmitter)).await?;

        // Seed all observations as memories
        let docs: Vec<RawDocument> = memories
            .iter()
            .enumerate()
            .map(|(i, mem)| RawDocument {
                content: mem.content.clone(),
                source_id: format!("locomo_{}_obs_{}", sample.sample_id, i),
                source: "memory".to_string(),
                title: format!("{} session {}", mem.speaker, mem.session_num),
                memory_type: Some("fact".to_string()),
                space: Some("conversation".to_string()),
                last_modified: chrono::Utc::now().timestamp(),
                ..Default::default()
            })
            .collect();
        db.upsert_documents(docs).await?;

        // Map dia_id to source_id for relevance judgments
        let dia_to_source: HashMap<String, String> = memories
            .iter()
            .enumerate()
            .map(|(i, m)| {
                (
                    m.dia_id.clone(),
                    format!("locomo_{}_obs_{}", sample.sample_id, i),
                )
            })
            .collect();

        let mut conv_scores: Vec<(u8, f64, f64, f64, f64, f64)> = Vec::new();

        for qa in &sample.qa {
            if qa.category == 5 {
                continue;
            }

            let results = db
                .search_memory_cross_rerank(
                    &qa.question,
                    10,
                    None,
                    None,
                    None,
                    Some(reranker.clone()),
                )
                .await?;

            let relevant_ids: HashSet<String> = qa
                .evidence
                .iter()
                .filter_map(|did| dia_to_source.get(did).cloned())
                .collect();

            if relevant_ids.is_empty() {
                continue;
            }

            let result_ids: Vec<&str> = results.iter().map(|r| r.source_id.as_str()).collect();

            let grades: HashMap<&str, u8> = result_ids
                .iter()
                .map(|id| (*id, if relevant_ids.contains(*id) { 1 } else { 0 }))
                .collect();

            let relevant_set: HashSet<&str> = relevant_ids.iter().map(|s| s.as_str()).collect();

            let ndcg_10 = metrics::ndcg_at_k(&result_ids, &grades, 10);
            let ndcg_5 = metrics::ndcg_at_k(&result_ids, &grades, 5);
            let mrr_val = metrics::mrr(&result_ids, &relevant_set);
            let recall_5 = metrics::recall_at_k(&result_ids, &relevant_set, 5);
            let hr_1 = metrics::hit_rate_at_k(&result_ids, &relevant_set, 1);

            conv_scores.push((qa.category, ndcg_5, ndcg_10, mrr_val, recall_5, hr_1));
            all_scores.push((qa.category, ndcg_5, ndcg_10, mrr_val, recall_5, hr_1));
            per_case.push(build_locomo_case_result(
                &qa.question,
                qa.category,
                ndcg_5,
                ndcg_10,
                mrr_val,
                recall_5,
                hr_1,
            ));
        }

        let per_cat = aggregate_by_category(&conv_scores);

        let n = conv_scores.len();
        conversations.push(LocomoConversationResult {
            sample_id: sample.sample_id.clone(),
            memories_seeded: memories.len(),
            questions_evaluated: n,
            overall_ndcg_at_10: avg_field(&conv_scores, |s| s.2),
            overall_mrr: avg_field(&conv_scores, |s| s.3),
            overall_recall_at_5: avg_field(&conv_scores, |s| s.4),
            per_category: per_cat,
        });
    }

    let per_cat_agg = aggregate_by_category(&all_scores);

    let mut report = LocomoReport {
        conversations,
        aggregate_ndcg_at_10: avg_field(&all_scores, |s| s.2),
        aggregate_mrr: avg_field(&all_scores, |s| s.3),
        aggregate_recall_at_5: avg_field(&all_scores, |s| s.4),
        aggregate_hit_rate_at_1: avg_field(&all_scores, |s| s.5),
        total_questions: all_scores.len(),
        total_memories: samples.iter().map(|s| extract_observations(s).len()).sum(),
        per_category_aggregate: per_cat_agg,
        qa_accuracy: None,
        baseline: None,
        env: None,
        per_case,
        coverage: None,
    };
    report.env = Some(build_locomo_env(
        "cross_rerank",
        path,
        "search_memory_with_reranker",
        "cross-encoder",
        &format!("cross-encoder:{}", reranker.model_id()),
        None,
    ));
    Ok(report)
}

// ---------------------------------------------------------------------------
// Cross-encoder rerank runner against a pre-seeded consolidated DB (PR-B)
// ---------------------------------------------------------------------------

/// Like `run_locomo_eval_cross_rerank`, but scores against a PRE-SEEDED
/// consolidated scenario DB (no per-conversation ephemeral DB, no ingest).
/// Used by PR-B's page-channel eval to surface distilled pages that the
/// fullpipeline harness wrote into the cache.
///
/// `db` MUST already contain memories with `source_id` formatted as
/// `locomo_<sample_id>_obs_<i>` (matches both the in-tree fullpipeline
/// seed path and the ephemeral seed in `run_locomo_eval_cross_rerank`).
/// Page-channel ON/OFF is controlled by the caller via the
/// `ORIGIN_ENABLE_PAGE_CHANNEL` env var (read inside
/// `search_memory_cross_rerank`).
pub async fn run_locomo_eval_cross_rerank_from_db(
    db: &MemoryDB,
    path: &Path,
    reranker: std::sync::Arc<dyn crate::reranker::Reranker>,
) -> Result<LocomoReport, OriginError> {
    let mut samples = load_locomo(path)?;
    apply_locomo_limit(&mut samples);
    let mut conversations = Vec::new();
    // (category, ndcg_5, ndcg_10, mrr, recall_5, hit_rate_1)
    let mut all_scores: Vec<(u8, f64, f64, f64, f64, f64)> = Vec::new();
    let mut per_case: Vec<crate::eval::report::CaseResult> = Vec::new();
    let mut cov_blind_acc: Vec<f64> = Vec::new();
    let mut cov_expanded_acc: Vec<f64> = Vec::new();

    for sample in &samples {
        let memories = extract_observations(sample);

        // Re-derive the source_id mapping — matches the fullpipeline seed path.
        let dia_to_source: HashMap<String, String> = memories
            .iter()
            .enumerate()
            .map(|(i, m)| {
                (
                    m.dia_id.clone(),
                    format!("locomo_{}_obs_{}", sample.sample_id, i),
                )
            })
            .collect();

        let mut conv_scores: Vec<(u8, f64, f64, f64, f64, f64)> = Vec::new();

        for qa in &sample.qa {
            if qa.category == 5 {
                continue;
            }

            let results = db
                .search_memory_cross_rerank(
                    &qa.question,
                    10,
                    None,
                    None,
                    None,
                    Some(reranker.clone()),
                )
                .await?;

            let relevant_ids: HashSet<String> = qa
                .evidence
                .iter()
                .filter_map(|did| dia_to_source.get(did).cloned())
                .collect();

            if relevant_ids.is_empty() {
                continue;
            }

            let result_ids: Vec<&str> = results.iter().map(|r| r.source_id.as_str()).collect();

            let grades: HashMap<&str, u8> = result_ids
                .iter()
                .map(|id| (*id, if relevant_ids.contains(*id) { 1 } else { 0 }))
                .collect();

            let relevant_set: HashSet<&str> = relevant_ids.iter().map(|s| s.as_str()).collect();

            let ndcg_10 = metrics::ndcg_at_k(&result_ids, &grades, 10);
            let ndcg_5 = metrics::ndcg_at_k(&result_ids, &grades, 5);
            let mrr_val = metrics::mrr(&result_ids, &relevant_set);
            let recall_5 = metrics::recall_at_k(&result_ids, &relevant_set, 5);
            let hr_1 = metrics::hit_rate_at_k(&result_ids, &relevant_set, 1);

            conv_scores.push((qa.category, ndcg_5, ndcg_10, mrr_val, recall_5, hr_1));
            all_scores.push((qa.category, ndcg_5, ndcg_10, mrr_val, recall_5, hr_1));
            per_case.push(build_locomo_case_result(
                &qa.question,
                qa.category,
                ndcg_5,
                ndcg_10,
                mrr_val,
                recall_5,
                hr_1,
            ));

            // Source-expanded coverage recall (page provenance, eval-only).
            // Pages contribute the memory source ids they were distilled from;
            // memories contribute their own id. Set-based + deduped so a gold
            // id counts once (no double-count). Reads the full result bundle
            // (memories in [0..limit] + appended pages) — pages ride after the
            // limit by the iter2 partition.
            let page_src_owned: Vec<(String, Vec<String>)> = {
                let mut v = Vec::new();
                for r in &results {
                    if r.source == "page" {
                        let srcs: Vec<String> = db
                            .get_page_sources(&r.source_id)
                            .await
                            .unwrap_or_default()
                            .into_iter()
                            .map(|ps| ps.memory_source_id)
                            .collect();
                        v.push((r.source_id.clone(), srcs));
                    }
                }
                v
            };
            let page_sources_map: HashMap<&str, Vec<&str>> = page_src_owned
                .iter()
                .map(|(pid, srcs)| (pid.as_str(), srcs.iter().map(|s| s.as_str()).collect()))
                .collect();
            let units: Vec<(&str, &str)> = results
                .iter()
                .map(|r| (r.source.as_str(), r.source_id.as_str()))
                .collect();
            let cov_blind = metrics::coverage_recall(
                &metrics::build_coverage_set(&units, &HashMap::new()),
                &relevant_set,
            );
            let cov_expanded = metrics::coverage_recall(
                &metrics::build_coverage_set(&units, &page_sources_map),
                &relevant_set,
            );
            cov_blind_acc.push(cov_blind);
            cov_expanded_acc.push(cov_expanded);
        }

        let per_cat = aggregate_by_category(&conv_scores);
        let n = conv_scores.len();
        conversations.push(LocomoConversationResult {
            sample_id: sample.sample_id.clone(),
            memories_seeded: memories.len(),
            questions_evaluated: n,
            overall_ndcg_at_10: avg_field(&conv_scores, |s| s.2),
            overall_mrr: avg_field(&conv_scores, |s| s.3),
            overall_recall_at_5: avg_field(&conv_scores, |s| s.4),
            per_category: per_cat,
        });
    }

    let per_cat_agg = aggregate_by_category(&all_scores);
    let coverage = if cov_blind_acc.is_empty() {
        None
    } else {
        Some(crate::eval::report::CoverageRecall {
            blind: cov_blind_acc.iter().sum::<f64>() / cov_blind_acc.len() as f64,
            expanded: cov_expanded_acc.iter().sum::<f64>() / cov_expanded_acc.len() as f64,
        })
    };
    let mut report = LocomoReport {
        conversations,
        aggregate_ndcg_at_10: avg_field(&all_scores, |s| s.2),
        aggregate_mrr: avg_field(&all_scores, |s| s.3),
        aggregate_recall_at_5: avg_field(&all_scores, |s| s.4),
        aggregate_hit_rate_at_1: avg_field(&all_scores, |s| s.5),
        total_questions: all_scores.len(),
        total_memories: samples.iter().map(|s| extract_observations(s).len()).sum(),
        per_category_aggregate: per_cat_agg,
        qa_accuracy: None,
        baseline: None,
        env: None,
        per_case,
        coverage,
    };

    // Branch variant_tag on ORIGIN_ENABLE_PAGE_CHANNEL + ORIGIN_MAGNITUDE_FUSION
    // so each variant produces distinct baseline filenames (comparable_hash uses
    // the variant string). magfusion appends `_magfusion` when enabled.
    let page_channel_state = if crate::db::page_channel_enabled() {
        "on"
    } else {
        "off"
    };
    let magfusion_state = if crate::db::magnitude_fusion_enabled() {
        "on"
    } else {
        "off"
    };
    let mut variant_tag = if page_channel_state == "off" {
        "cross_rerank_v2_no_pages".to_string()
    } else {
        "cross_rerank_v2_pages".to_string()
    };
    if magfusion_state == "on" {
        variant_tag.push_str("_magfusion");
    }
    // T9: append __graph_seed_d{depth} suffix when ORIGIN_ENABLE_GRAPH_SEED is on.
    let graph_seed_depth = if crate::db::graph_seed_enabled() {
        let depth = crate::retrieval::signals::parse_hop_depth(
            std::env::var("ORIGIN_GRAPH_HOP_DEPTH").ok().as_deref(),
        );
        Some(depth)
    } else {
        None
    };
    if let Some(depth) = graph_seed_depth {
        variant_tag.push_str(&format!("__graph_seed_d{}", depth));
    }
    // T4b: append __graph_khop_d{depth} suffix when ORIGIN_ENABLE_GRAPH_KHOP is on.
    // Honest config stamp: this runner calls search_memory_cross_rerank ->
    // augment_with_graph, where the k-hop expansion lives, so the flag genuinely
    // changes the retrieval path. No accuracy claim is encoded by the tag.
    let graph_khop_depth = if crate::db::khop_traversal_enabled() {
        Some(crate::retrieval::traversal::parse_khop_depth(
            std::env::var("ORIGIN_GRAPH_KHOP_DEPTH").ok().as_deref(),
        ))
    } else {
        None
    };
    if let Some(depth) = graph_khop_depth {
        variant_tag.push_str(&format!("__graph_khop_d{}", depth));
    }
    // T19: append __query_intent suffix when ORIGIN_ENABLE_QUERY_INTENT is on.
    let query_intent_state = if crate::retrieval::query_intent::query_intent_enabled() {
        variant_tag.push_str("__query_intent");
        "on"
    } else {
        "off"
    };
    // T8: append __salience suffix when ORIGIN_ENABLE_SALIENCE_PRIOR is on, so
    // salience-ON and salience-OFF baselines get distinct baseline filenames.
    let salience_state = if crate::db::salience_prior_enabled() {
        variant_tag.push_str("__salience");
        "on"
    } else {
        "off"
    };
    // T2: append __episode suffix when ORIGIN_ENABLE_EPISODE_CHANNEL is on, so
    // episode-ON and episode-OFF baselines get distinct baseline filenames.
    let episode_state = if crate::db::episode_channel_enabled() {
        variant_tag.push_str("__episode");
        "on"
    } else {
        "off"
    };
    // T15a: append __fact suffix when ORIGIN_ENABLE_FACT_CHANNEL is on, so
    // fact-ON and fact-OFF baselines get distinct baseline filenames.
    let fact_state = if crate::retrieval::fact_channel::fact_channel_enabled() {
        variant_tag.push_str("__fact");
        "on"
    } else {
        "off"
    };
    let mut env_stamp = build_locomo_env(
        &variant_tag,
        path,
        "search_memory_with_reranker",
        "cross-encoder",
        &format!("cross-encoder:{}", reranker.model_id()),
        None,
    );
    env_stamp
        .flags
        .push(format!("page_channel={}", page_channel_state));
    env_stamp
        .flags
        .push(format!("magnitude_fusion={}", magfusion_state));
    if let Some(depth) = graph_seed_depth {
        env_stamp.flags.push(format!("graph_seed=on_d{}", depth));
    } else {
        env_stamp.flags.push("graph_seed=off".to_string());
    }
    if let Some(depth) = graph_khop_depth {
        env_stamp.flags.push(format!("graph_khop=on_d{}", depth));
    } else {
        env_stamp.flags.push("graph_khop=off".to_string());
    }
    env_stamp
        .flags
        .push(format!("query_intent={}", query_intent_state));
    env_stamp
        .flags
        .push(format!("salience_prior={}", salience_state));
    env_stamp
        .flags
        .push(format!("episode_channel={}", episode_state));
    env_stamp.flags.push(format!("fact_channel={}", fact_state));
    env_stamp.flags.push("scenario_db=consolidated".to_string());
    report.env = Some(env_stamp);
    Ok(report)
}

// ---------------------------------------------------------------------------
/// Retrieval eval over a pre-seeded DB using the base `search_memory` path
/// (vector + FTS + RRF + graph augmentation) — the path the graph gate
/// (`ORIGIN_ENABLE_GRAPH_GATE`) acts on. Mirrors `run_locomo_eval_cross_rerank_from_db`
/// but without the cross-encoder/page channel, so the only LLM-free retrieval
/// signal under test is the graph augmentation + its gate. Used for the T3
/// graph-gate A/B experiment.
pub async fn run_locomo_eval_from_db(
    db: &MemoryDB,
    path: &Path,
) -> Result<LocomoReport, OriginError> {
    let mut samples = load_locomo(path)?;
    apply_locomo_limit(&mut samples);
    let mut conversations = Vec::new();
    let mut all_scores: Vec<(u8, f64, f64, f64, f64, f64)> = Vec::new();
    let mut cov_acc: Vec<f64> = Vec::new();
    let gate_on = crate::db::graph_gate_enabled();
    let (mut gate_skipped, mut gate_total) = (0usize, 0usize);

    for sample in &samples {
        let memories = extract_observations(sample);
        let dia_to_source: HashMap<String, String> = memories
            .iter()
            .enumerate()
            .map(|(i, m)| {
                (
                    m.dia_id.clone(),
                    format!("locomo_{}_obs_{}", sample.sample_id, i),
                )
            })
            .collect();

        let mut conv_scores: Vec<(u8, f64, f64, f64, f64, f64)> = Vec::new();
        for qa in &sample.qa {
            if qa.category == 5 {
                continue;
            }
            gate_total += 1;
            if gate_on && !crate::retrieval::signals::query_warrants_graph(&qa.question) {
                gate_skipped += 1;
            }
            let results = db
                .search_memory(&qa.question, 10, None, None, None, None, None, None)
                .await?;

            let relevant_ids: HashSet<String> = qa
                .evidence
                .iter()
                .filter_map(|did| dia_to_source.get(did).cloned())
                .collect();
            if relevant_ids.is_empty() {
                continue;
            }
            let result_ids: Vec<&str> = results.iter().map(|r| r.source_id.as_str()).collect();
            let grades: HashMap<&str, u8> = result_ids
                .iter()
                .map(|id| (*id, if relevant_ids.contains(*id) { 1 } else { 0 }))
                .collect();
            let relevant_set: HashSet<&str> = relevant_ids.iter().map(|s| s.as_str()).collect();

            let ndcg_10 = metrics::ndcg_at_k(&result_ids, &grades, 10);
            let ndcg_5 = metrics::ndcg_at_k(&result_ids, &grades, 5);
            let mrr_val = metrics::mrr(&result_ids, &relevant_set);
            let recall_5 = metrics::recall_at_k(&result_ids, &relevant_set, 5);
            let hr_1 = metrics::hit_rate_at_k(&result_ids, &relevant_set, 1);

            conv_scores.push((qa.category, ndcg_5, ndcg_10, mrr_val, recall_5, hr_1));
            all_scores.push((qa.category, ndcg_5, ndcg_10, mrr_val, recall_5, hr_1));

            let units: Vec<(&str, &str)> = results
                .iter()
                .map(|r| (r.source.as_str(), r.source_id.as_str()))
                .collect();
            cov_acc.push(metrics::coverage_recall(
                &metrics::build_coverage_set(&units, &HashMap::new()),
                &relevant_set,
            ));
        }

        let per_cat = aggregate_by_category(&conv_scores);
        let n = conv_scores.len();
        conversations.push(LocomoConversationResult {
            sample_id: sample.sample_id.clone(),
            memories_seeded: memories.len(),
            questions_evaluated: n,
            overall_ndcg_at_10: avg_field(&conv_scores, |s| s.2),
            overall_mrr: avg_field(&conv_scores, |s| s.3),
            overall_recall_at_5: avg_field(&conv_scores, |s| s.4),
            per_category: per_cat,
        });
    }

    if gate_on {
        eprintln!(
            "[locomo] graph-gate skipped {gate_skipped}/{gate_total} queries ({:.1}%)",
            100.0 * gate_skipped as f64 / gate_total.max(1) as f64
        );
    }
    let per_cat_agg = aggregate_by_category(&all_scores);
    let coverage = if cov_acc.is_empty() {
        None
    } else {
        Some(crate::eval::report::CoverageRecall {
            blind: cov_acc.iter().sum::<f64>() / cov_acc.len() as f64,
            expanded: cov_acc.iter().sum::<f64>() / cov_acc.len() as f64,
        })
    };
    let mut report = LocomoReport {
        conversations,
        aggregate_ndcg_at_10: avg_field(&all_scores, |s| s.2),
        aggregate_mrr: avg_field(&all_scores, |s| s.3),
        aggregate_recall_at_5: avg_field(&all_scores, |s| s.4),
        aggregate_hit_rate_at_1: avg_field(&all_scores, |s| s.5),
        total_questions: all_scores.len(),
        total_memories: samples.iter().map(|s| extract_observations(s).len()).sum(),
        per_category_aggregate: per_cat_agg,
        qa_accuracy: None,
        baseline: None,
        env: None,
        per_case: Vec::new(),
        coverage,
    };
    let graph_gate = if crate::db::graph_gate_enabled() {
        "on"
    } else {
        "off"
    };
    let mut env_stamp = build_locomo_env(
        if graph_gate == "off" {
            "search_memory_gate_off"
        } else {
            "search_memory_gate_on"
        },
        path,
        "search_memory",
        "none",
        "none",
        None,
    );
    env_stamp.flags.push(format!("graph_gate={graph_gate}"));
    env_stamp.flags.push("scenario_db=consolidated".to_string());
    report.env = Some(env_stamp);
    Ok(report)
}

// Expanded benchmark runner -- same as run_locomo_eval but uses search_memory_expanded
// ---------------------------------------------------------------------------

/// Same seeding/scoring logic as `run_locomo_eval`, but retrieval uses
/// `search_memory_expanded` with the supplied LLM for query expansion before search.
pub async fn run_locomo_eval_expanded(
    path: &Path,
    llm: std::sync::Arc<dyn crate::llm_provider::LlmProvider>,
) -> Result<LocomoReport, OriginError> {
    let mut samples = load_locomo(path)?;
    apply_locomo_limit(&mut samples);
    let mut conversations = Vec::new();
    // (category, ndcg_5, ndcg_10, mrr, recall_5, hit_rate_1)
    let mut all_scores: Vec<(u8, f64, f64, f64, f64, f64)> = Vec::new();
    let mut per_case: Vec<crate::eval::report::CaseResult> = Vec::new();

    for sample in &samples {
        let memories = extract_observations(sample);

        // Create ephemeral DB for this conversation
        let tmp = tempfile::tempdir().map_err(|e| OriginError::Generic(format!("tempdir: {e}")))?;
        let db = MemoryDB::new(tmp.path(), std::sync::Arc::new(crate::events::NoopEmitter)).await?;

        // Seed all observations as memories
        let docs: Vec<RawDocument> = memories
            .iter()
            .enumerate()
            .map(|(i, mem)| RawDocument {
                content: mem.content.clone(),
                source_id: format!("locomo_{}_obs_{}", sample.sample_id, i),
                source: "memory".to_string(),
                title: format!("{} session {}", mem.speaker, mem.session_num),
                memory_type: Some("fact".to_string()),
                space: Some("conversation".to_string()),
                last_modified: chrono::Utc::now().timestamp(),
                ..Default::default()
            })
            .collect();
        db.upsert_documents(docs).await?;

        // Map dia_id to source_id for relevance judgments
        let dia_to_source: HashMap<String, String> = memories
            .iter()
            .enumerate()
            .map(|(i, m)| {
                (
                    m.dia_id.clone(),
                    format!("locomo_{}_obs_{}", sample.sample_id, i),
                )
            })
            .collect();

        let mut conv_scores: Vec<(u8, f64, f64, f64, f64, f64)> = Vec::new();

        for qa in &sample.qa {
            if qa.category == 5 {
                continue;
            }

            let results = db
                .search_memory_expanded(&qa.question, 10, None, None, None, Some(llm.clone()))
                .await?;

            // Build relevance judgments: evidence dia_ids -> source_ids = relevant
            let relevant_ids: HashSet<String> = qa
                .evidence
                .iter()
                .filter_map(|did| dia_to_source.get(did).cloned())
                .collect();

            if relevant_ids.is_empty() {
                continue; // Skip if no mappable evidence
            }

            let result_ids: Vec<&str> = results.iter().map(|r| r.source_id.as_str()).collect();

            // Binary relevance: 1 if in evidence set, 0 otherwise
            let grades: HashMap<&str, u8> = result_ids
                .iter()
                .map(|id| (*id, if relevant_ids.contains(*id) { 1 } else { 0 }))
                .collect();

            let relevant_set: HashSet<&str> = relevant_ids.iter().map(|s| s.as_str()).collect();

            let ndcg_10 = metrics::ndcg_at_k(&result_ids, &grades, 10);
            let ndcg_5 = metrics::ndcg_at_k(&result_ids, &grades, 5);
            let mrr_val = metrics::mrr(&result_ids, &relevant_set);
            let recall_5 = metrics::recall_at_k(&result_ids, &relevant_set, 5);
            let hr_1 = metrics::hit_rate_at_k(&result_ids, &relevant_set, 1);

            conv_scores.push((qa.category, ndcg_5, ndcg_10, mrr_val, recall_5, hr_1));
            all_scores.push((qa.category, ndcg_5, ndcg_10, mrr_val, recall_5, hr_1));
            per_case.push(build_locomo_case_result(
                &qa.question,
                qa.category,
                ndcg_5,
                ndcg_10,
                mrr_val,
                recall_5,
                hr_1,
            ));
        }

        // Per-category for this conversation
        let per_cat = aggregate_by_category(&conv_scores);

        let n = conv_scores.len();
        conversations.push(LocomoConversationResult {
            sample_id: sample.sample_id.clone(),
            memories_seeded: memories.len(),
            questions_evaluated: n,
            overall_ndcg_at_10: avg_field(&conv_scores, |s| s.2),
            overall_mrr: avg_field(&conv_scores, |s| s.3),
            overall_recall_at_5: avg_field(&conv_scores, |s| s.4),
            per_category: per_cat,
        });
    }

    // Global aggregates
    let per_cat_agg = aggregate_by_category(&all_scores);

    let mut report = LocomoReport {
        conversations,
        aggregate_ndcg_at_10: avg_field(&all_scores, |s| s.2),
        aggregate_mrr: avg_field(&all_scores, |s| s.3),
        aggregate_recall_at_5: avg_field(&all_scores, |s| s.4),
        aggregate_hit_rate_at_1: avg_field(&all_scores, |s| s.5),
        total_questions: all_scores.len(),
        total_memories: samples.iter().map(|s| extract_observations(s).len()).sum(),
        per_category_aggregate: per_cat_agg,
        qa_accuracy: None,
        baseline: None,
        env: None,
        per_case,
        coverage: None,
    };
    report.env = Some(build_locomo_env(
        "expanded",
        path,
        "search_memory_expanded",
        llm.kind(),
        &llm.model_id(),
        None,
    ));
    Ok(report)
}

// ---------------------------------------------------------------------------
// Gated benchmark runner — clean / noisy / gated comparison
// ---------------------------------------------------------------------------

/// Controls how noise is handled in the LoCoMo benchmark.
#[derive(Debug, Clone, Copy)]
pub enum LocomoGateMode {
    /// No noise — only observations (baseline).
    Clean,
    /// Noise added alongside observations, no gate filtering.
    Noisy,
    /// Noise added, but each noise doc passes through the quality gate before insertion.
    Gated,
}

/// Generate domain-relevant noise documents proportional to observation count.
///
/// For every 3 observations, 1 noise memory is generated (33% ratio).
/// Noise is designed to compete with real personal-conversation memories:
///
/// - **Category 1**: System prompt fragments about conversation memory
///   (should be caught by the content gate's preamble detection)
/// - **Category 2**: Vague restates of common conversation topics
///   (competes semantically with real memories)
/// - **Category 3**: Hallucinated personal facts using actual speaker names
///   (maximally confusing — plausible but wrong)
/// - **Category 4**: Meta-commentary about conversation processing
///   (should be caught by content gate patterns or novelty)
/// - **Category 5**: Transient status about conversation processing
fn generate_noise(sample: &LocomoSample, observation_count: usize) -> Vec<RawDocument> {
    let noise_count = observation_count / 3; // 33% noise ratio

    // Extract speaker names from the conversation metadata
    let speaker_a = sample
        .conversation
        .get("speaker_a")
        .and_then(|v| v.as_str())
        .unwrap_or("Speaker A");
    let speaker_b = sample
        .conversation
        .get("speaker_b")
        .and_then(|v| v.as_str())
        .unwrap_or("Speaker B");

    // Category 1: System prompt fragments (should be caught by content gate)
    let sys_prompt_templates: Vec<&str> = vec![
        "You are a personal conversation memory assistant. Your role is to remember details about people's lives relationships and activities.",
        "As an AI you must accurately recall personal facts about family members friends and their daily activities.",
        "Your role is to track relationships between people including their family members friends and colleagues mentioned in conversations.",
    ];

    // Category 2: Vague restates of common conversation topics
    let vague_templates: Vec<&str> = vec![
        "Someone mentioned something about a family gathering or holiday celebration recently.",
        "There was a discussion about travel plans and visiting family members sometime soon.",
        "The speakers talked about their children and school activities at some point.",
        "Something about cooking or trying a new restaurant was mentioned in the conversation.",
        "There were some health-related concerns discussed between the speakers.",
        "Plans were made regarding an upcoming birthday party or celebration event.",
        "The conversation included discussion about work stress and career changes.",
        "Someone mentioned moving to a new house or apartment in the near future.",
        "Weekend plans involving outdoor activities or sports were discussed briefly.",
        "A discussion about pets or animals came up during the conversation session.",
    ];

    // Category 3: Hallucinated personal facts using actual speaker names
    // {a} and {b} are replaced with real names below
    let hallucinated_templates: Vec<String> = vec![
        "{a} enjoys playing tennis on weekends with neighborhood friends regularly.",
        "{b} mentioned planning to visit parents in Florida next month for holidays.",
        "{a} and {b} discussed their shared interest in cooking Italian food together.",
        "{a} recently started learning guitar as a new creative hobby this year.",
        "{b} is training for a local charity run happening in the spring season.",
        "{a} talked about attending a concert with friends downtown last weekend.",
        "{b} mentioned getting a new puppy from the local animal shelter recently.",
        "{a} said the family is planning a camping trip to the national park.",
        "{b} discussed redecorating the living room with new furniture and paint.",
        "{a} mentioned starting a book club with coworkers at the office monthly.",
    ]
    .into_iter()
    .map(|t| t.replace("{a}", speaker_a).replace("{b}", speaker_b))
    .collect();

    // Category 4: Meta-commentary about the conversation
    let meta_templates: Vec<&str> = vec![
        "I just stored several personal facts from this conversation about family and hobbies.",
        "The conversation contained interesting details about upcoming travel and social plans.",
        "Updated memory with new observations about the speakers' relationships and activities.",
        "Currently processing this dialogue to extract key personal facts and preferences.",
        "Analyzing the conversation context to identify family relationships and important events.",
    ];

    // Category 5: Transient status about conversation processing
    let transient_templates: Vec<&str> = vec![
        "Working on extracting personal details from the latest conversation session data.",
        "Reviewing the dialogue for mentions of names dates and relationship information.",
        "Processing conversation turns to identify facts worth storing in long-term memory.",
    ];

    // Build a combined cycle: interleave categories for variety
    let mut all_noise: Vec<(String, &str)> = Vec::new();
    for t in &sys_prompt_templates {
        all_noise.push((t.to_string(), "sys_prompt"));
    }
    for t in &vague_templates {
        all_noise.push((t.to_string(), "vague"));
    }
    for t in &hallucinated_templates {
        all_noise.push((t.clone(), "hallucinated"));
    }
    for t in &meta_templates {
        all_noise.push((t.to_string(), "meta"));
    }
    for t in &transient_templates {
        all_noise.push((t.to_string(), "transient"));
    }

    let mut docs = Vec::new();
    for i in 0..noise_count {
        let (content, _noise_type) = &all_noise[i % all_noise.len()];
        docs.push(RawDocument {
            content: content.clone(),
            source_id: format!("noise_{}_{}", sample.sample_id, i),
            source: "memory".to_string(),
            title: format!("noise_{}", i),
            memory_type: Some("fact".to_string()),
            space: Some("conversation".to_string()),
            last_modified: chrono::Utc::now().timestamp(),
            ..Default::default()
        });
    }
    docs
}

/// Run LoCoMo benchmark with noise + quality gate comparison.
///
/// Three modes:
/// - **Clean**: Only observations seeded (baseline).
/// - **Noisy**: Observations + synthetic noise, all inserted without filtering.
/// - **Gated**: Observations inserted first, then each noise doc is run through
///   `QualityGate::evaluate()` (content patterns + novelty check) and only
///   inserted if admitted.
pub async fn run_locomo_eval_with_gate(
    path: &Path,
    mode: LocomoGateMode,
) -> Result<LocomoReport, OriginError> {
    let mut samples = load_locomo(path)?;
    apply_locomo_limit(&mut samples);
    let mut conversations = Vec::new();
    let mut all_scores: Vec<(u8, f64, f64, f64, f64, f64)> = Vec::new();
    let mut per_case: Vec<crate::eval::report::CaseResult> = Vec::new();
    let mut total_memories_inserted: usize = 0;

    let gate = match mode {
        LocomoGateMode::Gated => Some(QualityGate::new(GateConfig::default())),
        _ => None,
    };

    for sample in &samples {
        let memories = extract_observations(sample);

        // Create ephemeral DB for this conversation
        let tmp = tempfile::tempdir().map_err(|e| OriginError::Generic(format!("tempdir: {e}")))?;
        let db = MemoryDB::new(tmp.path(), std::sync::Arc::new(crate::events::NoopEmitter)).await?;

        // Seed all observations as memories (ground truth — always inserted)
        let obs_docs: Vec<RawDocument> = memories
            .iter()
            .enumerate()
            .map(|(i, mem)| RawDocument {
                content: mem.content.clone(),
                source_id: format!("locomo_{}_obs_{}", sample.sample_id, i),
                source: "memory".to_string(),
                title: format!("{} session {}", mem.speaker, mem.session_num),
                memory_type: Some("fact".to_string()),
                space: Some("conversation".to_string()),
                last_modified: chrono::Utc::now().timestamp(),
                ..Default::default()
            })
            .collect();
        db.upsert_documents(obs_docs).await?;

        let mut memories_in_db = memories.len();

        // For Noisy/Gated modes, generate and process noise
        match mode {
            LocomoGateMode::Clean => { /* no noise */ }
            LocomoGateMode::Noisy => {
                let noise = generate_noise(sample, memories.len());
                let noise_count = noise.len();
                db.upsert_documents(noise).await?;
                memories_in_db += noise_count;
            }
            LocomoGateMode::Gated => {
                let noise = generate_noise(sample, memories.len());
                let gate = gate.as_ref().unwrap();
                let mut admitted_docs = Vec::new();
                for doc in &noise {
                    let (result, _similar_id) = gate.evaluate(&doc.content, &db).await?;
                    if result.admitted {
                        admitted_docs.push(doc.clone());
                    }
                }
                let admitted_count = admitted_docs.len();
                if !admitted_docs.is_empty() {
                    db.upsert_documents(admitted_docs).await?;
                }
                memories_in_db += admitted_count;
            }
        }

        total_memories_inserted += memories_in_db;

        // Map dia_id to source_id for relevance judgments
        let dia_to_source: HashMap<String, String> = memories
            .iter()
            .enumerate()
            .map(|(i, m)| {
                (
                    m.dia_id.clone(),
                    format!("locomo_{}_obs_{}", sample.sample_id, i),
                )
            })
            .collect();

        let mut conv_scores: Vec<(u8, f64, f64, f64, f64, f64)> = Vec::new();

        for qa in &sample.qa {
            if qa.category == 5 {
                continue;
            }

            let results = db
                .search_memory(&qa.question, 10, None, None, None, None, None, None)
                .await?;

            // Build relevance judgments: evidence dia_ids -> source_ids = relevant
            let relevant_ids: HashSet<String> = qa
                .evidence
                .iter()
                .filter_map(|did| dia_to_source.get(did).cloned())
                .collect();

            if relevant_ids.is_empty() {
                continue;
            }

            let result_ids: Vec<&str> = results.iter().map(|r| r.source_id.as_str()).collect();

            // Binary relevance: 1 if in evidence set, 0 otherwise
            let grades: HashMap<&str, u8> = result_ids
                .iter()
                .map(|id| (*id, if relevant_ids.contains(*id) { 1 } else { 0 }))
                .collect();

            let relevant_set: HashSet<&str> = relevant_ids.iter().map(|s| s.as_str()).collect();

            let ndcg_10 = metrics::ndcg_at_k(&result_ids, &grades, 10);
            let ndcg_5 = metrics::ndcg_at_k(&result_ids, &grades, 5);
            let mrr_val = metrics::mrr(&result_ids, &relevant_set);
            let recall_5 = metrics::recall_at_k(&result_ids, &relevant_set, 5);
            let hr_1 = metrics::hit_rate_at_k(&result_ids, &relevant_set, 1);

            conv_scores.push((qa.category, ndcg_5, ndcg_10, mrr_val, recall_5, hr_1));
            all_scores.push((qa.category, ndcg_5, ndcg_10, mrr_val, recall_5, hr_1));
            per_case.push(build_locomo_case_result(
                &qa.question,
                qa.category,
                ndcg_5,
                ndcg_10,
                mrr_val,
                recall_5,
                hr_1,
            ));
        }

        let per_cat = aggregate_by_category(&conv_scores);

        conversations.push(LocomoConversationResult {
            sample_id: sample.sample_id.clone(),
            memories_seeded: memories_in_db,
            questions_evaluated: conv_scores.len(),
            overall_ndcg_at_10: avg_field(&conv_scores, |s| s.2),
            overall_mrr: avg_field(&conv_scores, |s| s.3),
            overall_recall_at_5: avg_field(&conv_scores, |s| s.4),
            per_category: per_cat,
        });
    }

    let per_cat_agg = aggregate_by_category(&all_scores);

    // TODO: LLM-as-judge QA accuracy requires:
    // 1. Generate answer from retrieved context using an LLM
    // 2. Judge answer correctness against ground truth using LLM-as-judge
    // 3. This is what competitors report as "J-score" or "accuracy"
    // Currently we report retrieval metrics (NDCG, MRR, Recall) which measure
    // whether the right memories are found, not whether the final answer is correct.
    let mut report = LocomoReport {
        conversations,
        aggregate_ndcg_at_10: avg_field(&all_scores, |s| s.2),
        aggregate_mrr: avg_field(&all_scores, |s| s.3),
        aggregate_recall_at_5: avg_field(&all_scores, |s| s.4),
        aggregate_hit_rate_at_1: avg_field(&all_scores, |s| s.5),
        total_questions: all_scores.len(),
        total_memories: total_memories_inserted,
        per_category_aggregate: per_cat_agg,
        qa_accuracy: None,
        baseline: None,
        env: None,
        per_case,
        coverage: None,
    };
    report.env = Some(build_locomo_env(
        "gated",
        path,
        "search_memory",
        "none",
        "none",
        None,
    ));
    Ok(report)
}

/// Average a field across a score slice.
fn avg_field(
    scores: &[(u8, f64, f64, f64, f64, f64)],
    f: impl Fn(&(u8, f64, f64, f64, f64, f64)) -> f64,
) -> f64 {
    if scores.is_empty() {
        return 0.0;
    }
    let sum: f64 = scores.iter().map(&f).sum();
    sum / scores.len() as f64
}

/// Aggregate scores by category.
fn aggregate_by_category(scores: &[(u8, f64, f64, f64, f64, f64)]) -> Vec<LocomoCategoryResult> {
    let mut results = Vec::new();
    for cat in [1u8, 2, 3, 4] {
        let cat_scores: Vec<_> = scores.iter().filter(|s| s.0 == cat).cloned().collect();
        if cat_scores.is_empty() {
            continue;
        }
        results.push(LocomoCategoryResult {
            category: cat,
            name: category_name(cat).to_string(),
            count: cat_scores.len(),
            ndcg_at_5: avg_field(&cat_scores, |s| s.1),
            ndcg_at_10: avg_field(&cat_scores, |s| s.2),
            mrr: avg_field(&cat_scores, |s| s.3),
            recall_at_5: avg_field(&cat_scores, |s| s.4),
            hit_rate_at_1: avg_field(&cat_scores, |s| s.5),
        });
    }
    results
}

// ---------------------------------------------------------------------------
// Backward-compat re-exports (prompt functions now live in judge.rs)
// ---------------------------------------------------------------------------

pub use crate::eval::judge::locomo_judge_prompt;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_locomo_sample() {
        let json = r#"[{
            "sample_id": "test-conv",
            "conversation": {"speaker_a": "Alice", "speaker_b": "Bob"},
            "observation": {
                "session_1_observation": {
                    "Alice": [["Alice likes hiking in the mountains.", "D1:3"]],
                    "Bob": [["Bob is training for a marathon.", "D1:5"]]
                }
            },
            "qa": [
                {"question": "What does Alice like?", "answer": "hiking in the mountains", "evidence": ["D1:3"], "category": 4},
                {"question": "Unanswerable", "adversarial_answer": "not mentioned", "evidence": ["D1:1"], "category": 5}
            ]
        }]"#;

        let samples: Vec<LocomoSample> = serde_json::from_str(json).unwrap();
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].sample_id, "test-conv");
        assert_eq!(samples[0].qa.len(), 2);
        assert_eq!(samples[0].qa[0].category, 4);
        assert!(samples[0].qa[1].answer.is_none());
    }

    #[test]
    fn test_extract_observations() {
        let json = r#"{
            "sample_id": "test-conv",
            "conversation": {"speaker_a": "Alice", "speaker_b": "Bob"},
            "observation": {
                "session_1_observation": {
                    "Alice": [["Alice likes hiking in the mountains.", "D1:3"]],
                    "Bob": [["Bob is training for a marathon.", "D1:5"]]
                }
            },
            "qa": []
        }"#;

        let sample: LocomoSample = serde_json::from_str(json).unwrap();
        let memories = extract_observations(&sample);
        assert_eq!(memories.len(), 2);

        // Check that both speakers are represented
        let speakers: Vec<&str> = memories.iter().map(|m| m.speaker.as_str()).collect();
        assert!(speakers.contains(&"Alice"));
        assert!(speakers.contains(&"Bob"));

        // Check dia_ids
        let dia_ids: Vec<&str> = memories.iter().map(|m| m.dia_id.as_str()).collect();
        assert!(dia_ids.contains(&"D1:3"));
        assert!(dia_ids.contains(&"D1:5"));

        // Check session number
        for mem in &memories {
            assert_eq!(mem.session_num, 1);
            assert_eq!(mem.sample_id, "test-conv");
        }
    }

    #[test]
    fn test_extract_multi_session_observations() {
        let json = r#"{
            "sample_id": "conv-multi",
            "conversation": {},
            "observation": {
                "session_1_observation": {
                    "Alice": [["Fact from session 1.", "D1:2"]]
                },
                "session_3_observation": {
                    "Alice": [["Fact from session 3.", "D3:4"]],
                    "Bob": [["Bob fact session 3.", "D3:7"]]
                }
            },
            "qa": []
        }"#;

        let sample: LocomoSample = serde_json::from_str(json).unwrap();
        let memories = extract_observations(&sample);
        assert_eq!(memories.len(), 3);

        let session_nums: Vec<usize> = memories.iter().map(|m| m.session_num).collect();
        assert!(session_nums.contains(&1));
        assert!(session_nums.contains(&3));
    }

    #[test]
    fn test_sample_to_eval_cases_skips_adversarial() {
        let json = r#"{
            "sample_id": "test-conv",
            "conversation": {},
            "observation": {
                "session_1_observation": {
                    "Alice": [["Alice likes hiking in the mountains.", "D1:3"]],
                    "Bob": [["Bob is training for a marathon.", "D1:5"]]
                }
            },
            "qa": [
                {"question": "What does Alice like?", "answer": "hiking in the mountains", "evidence": ["D1:3"], "category": 4},
                {"question": "Unanswerable", "adversarial_answer": "not mentioned", "evidence": ["D1:1"], "category": 5}
            ]
        }"#;

        let sample: LocomoSample = serde_json::from_str(json).unwrap();
        let memories = extract_observations(&sample);
        let cases = sample_to_eval_cases(&sample, &memories);

        // Only the non-adversarial question should produce an eval case
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].query, "What does Alice like?");
    }

    #[test]
    fn test_sample_to_eval_cases_relevance_grading() {
        let json = r#"{
            "sample_id": "test-conv",
            "conversation": {},
            "observation": {
                "session_1_observation": {
                    "Alice": [["Alice likes hiking in the mountains.", "D1:3"]],
                    "Bob": [["Bob is training for a marathon.", "D1:5"]]
                }
            },
            "qa": [
                {"question": "What does Alice like?", "answer": "hiking in the mountains", "evidence": ["D1:3"], "category": 4}
            ]
        }"#;

        let sample: LocomoSample = serde_json::from_str(json).unwrap();
        let memories = extract_observations(&sample);
        let cases = sample_to_eval_cases(&sample, &memories);

        assert_eq!(cases.len(), 1);
        // All observations are seeds (evidence + non-evidence)
        assert_eq!(cases[0].seeds.len(), 2);

        // Find the matching evidence seed (D1:3) and the non-matching one (D1:5)
        let evidence_seed = cases[0]
            .seeds
            .iter()
            .find(|s| s.content.contains("hiking"))
            .unwrap();
        let other_seed = cases[0]
            .seeds
            .iter()
            .find(|s| s.content.contains("marathon"))
            .unwrap();

        assert_eq!(evidence_seed.relevance, 3);
        assert_eq!(other_seed.relevance, 1);
    }

    #[test]
    fn test_parse_session_num() {
        assert_eq!(parse_session_num("session_1_observation"), Some(1));
        assert_eq!(parse_session_num("session_12_observation"), Some(12));
        assert_eq!(parse_session_num("not_a_session"), None);
    }

    #[test]
    fn test_category_name() {
        assert_eq!(category_name(1), "multi-hop");
        assert_eq!(category_name(4), "single-hop");
        assert_eq!(category_name(5), "adversarial");
        assert_eq!(category_name(99), "unknown");
    }

    #[test]
    fn test_multi_evidence_qa() {
        let json = r#"{
            "sample_id": "test-multi",
            "conversation": {},
            "observation": {
                "session_1_observation": {
                    "Alice": [
                        ["Alice works at Google.", "D1:2"],
                        ["Alice moved to NYC.", "D1:5"]
                    ],
                    "Bob": [["Bob likes coffee.", "D1:3"]]
                }
            },
            "qa": [
                {"question": "Where does Alice work and live?", "answer": "Google in NYC", "evidence": ["D1:2", "D1:5"], "category": 1}
            ]
        }"#;

        let sample: LocomoSample = serde_json::from_str(json).unwrap();
        let memories = extract_observations(&sample);
        assert_eq!(memories.len(), 3);

        let cases = sample_to_eval_cases(&sample, &memories);
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].seeds.len(), 3);

        // Two seeds should have relevance=3 (both evidence dia_ids match)
        let high_rel: Vec<_> = cases[0].seeds.iter().filter(|s| s.relevance == 3).collect();
        assert_eq!(high_rel.len(), 2);

        // One seed should have relevance=1 (Bob's non-evidence observation)
        let low_rel: Vec<_> = cases[0].seeds.iter().filter(|s| s.relevance == 1).collect();
        assert_eq!(low_rel.len(), 1);
        assert!(low_rel[0].content.contains("coffee"));
    }

    #[test]
    fn test_baseline_save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("locomo_baseline.json");

        let report = LocomoReport {
            conversations: vec![],
            aggregate_ndcg_at_10: 0.750,
            aggregate_mrr: 0.700,
            aggregate_recall_at_5: 0.520,
            aggregate_hit_rate_at_1: 0.450,
            total_questions: 100,
            total_memories: 500,
            per_category_aggregate: vec![
                LocomoCategoryResult {
                    category: 1,
                    name: "multi-hop".to_string(),
                    count: 25,
                    ndcg_at_5: 0.550,
                    ndcg_at_10: 0.560,
                    mrr: 0.500,
                    recall_at_5: 0.480,
                    hit_rate_at_1: 0.400,
                },
                LocomoCategoryResult {
                    category: 3,
                    name: "open-domain".to_string(),
                    count: 30,
                    ndcg_at_5: 0.430,
                    ndcg_at_10: 0.441,
                    mrr: 0.410,
                    recall_at_5: 0.380,
                    hit_rate_at_1: 0.350,
                },
            ],
            qa_accuracy: None,
            baseline: None,
            env: None,
            per_case: vec![],
            coverage: None,
        };

        report.save_baseline(&path).unwrap();
        let loaded = LocomoReport::load_baseline(&path).unwrap();

        assert!((loaded.ndcg_at_10 - 0.750).abs() < 0.001);
        assert!((loaded.mrr - 0.700).abs() < 0.001);
        assert!((loaded.recall_at_5 - 0.520).abs() < 0.001);
        assert!((loaded.hit_rate_at_1 - 0.450).abs() < 0.001);

        // Per-category baselines
        assert_eq!(loaded.per_category.len(), 2);
        assert_eq!(loaded.per_category[0].name, "multi-hop");
        assert!((loaded.per_category[0].ndcg_at_10 - 0.560).abs() < 0.001);
        assert_eq!(loaded.per_category[1].name, "open-domain");
        assert!((loaded.per_category[1].mrr - 0.410).abs() < 0.001);
    }

    #[test]
    fn locomo_env_records_judge_model_when_batch_runs() {
        let mut report = LocomoReport {
            conversations: vec![],
            aggregate_ndcg_at_10: 0.0,
            aggregate_mrr: 0.0,
            aggregate_recall_at_5: 0.0,
            aggregate_hit_rate_at_1: 0.0,
            total_questions: 0,
            total_memories: 0,
            per_category_aggregate: vec![],
            qa_accuracy: None,
            baseline: None,
            env: None,
            per_case: vec![],
            coverage: None,
        };
        report.env = Some(crate::eval::report::ReportEnv {
            fixture_revision: "deadbeef".into(),
            embedder_model: "bge-base-en-v1.5-q".into(),
            embedder_revision: "768d".into(),
            retrieval_method: "search_memory".into(),
            llm_provider_class: "on-device".into(),
            llm_model: "qwen3-4b".into(),
            judge_model: None,
            origin_version: env!("CARGO_PKG_VERSION").into(),
            eval_timestamp_unix: 0,
            ..Default::default()
        });
        crate::eval::judge::stamp_judge_model(&mut report.env, "claude-haiku-4-5-20251001");
        assert_eq!(
            report.env.as_ref().and_then(|e| e.judge_model.clone()),
            Some("claude-haiku-4-5-20251001".to_string())
        );
    }

    #[test]
    fn test_to_terminal_with_baseline() {
        let report = LocomoReport {
            conversations: vec![],
            aggregate_ndcg_at_10: 0.660,
            aggregate_mrr: 0.610,
            aggregate_recall_at_5: 0.540,
            aggregate_hit_rate_at_1: 0.470,
            total_questions: 100,
            total_memories: 500,
            per_category_aggregate: vec![LocomoCategoryResult {
                category: 3,
                name: "open-domain".to_string(),
                count: 30,
                ndcg_at_5: 0.470,
                ndcg_at_10: 0.480,
                mrr: 0.440,
                recall_at_5: 0.400,
                hit_rate_at_1: 0.370,
            }],
            qa_accuracy: None,
            baseline: Some(LocomoBaseline {
                ndcg_at_10: 0.750,
                mrr: 0.700,
                recall_at_5: 0.520,
                hit_rate_at_1: 0.450,
                per_category: vec![crate::eval::report::CategoryBaseline {
                    name: "open-domain".to_string(),
                    ndcg_at_10: 0.441,
                    mrr: 0.410,
                    recall_at_5: 0.380,
                }],
                coverage: None,
            }),
            env: None,
            per_case: vec![],
            coverage: None,
        };

        let text = report.to_terminal();
        assert!(text.contains("LoCoMo Benchmark"));
        assert!(text.contains("NDCG@10"));
        assert!(text.contains("Baseline comparison:"));
        assert!(text.contains("open-domain"));
        // Verify delta printing is present
        assert!(text.contains("->"));
    }

    /// Build a vec of `n` minimal `LocomoSample`s for env-limit tests.
    fn mock_samples(n: usize) -> Vec<LocomoSample> {
        (0..n)
            .map(|i| {
                let json = format!(
                    r#"{{
                        "sample_id": "mock-{i}",
                        "conversation": {{}},
                        "observation": {{}},
                        "qa": []
                    }}"#
                );
                serde_json::from_str::<LocomoSample>(&json).unwrap()
            })
            .collect()
    }

    #[test]
    fn eval_locomo_limit_truncates_when_set() {
        // Unique env var name so the test doesn't race the real EVAL_LOCOMO_LIMIT.
        let var = "EVAL_LOCOMO_LIMIT_TEST_TRUNCATE";
        let mut samples = mock_samples(10);
        std::env::set_var(var, "2");
        apply_limit_from_env(&mut samples, var, "locomo", "conversations");
        std::env::remove_var(var);
        assert_eq!(samples.len(), 2, "limit=2 should truncate 10 down to 2");
        assert_eq!(samples[0].sample_id, "mock-0");
        assert_eq!(samples[1].sample_id, "mock-1");
    }

    #[test]
    fn eval_locomo_limit_no_op_when_unset() {
        let var = "EVAL_LOCOMO_LIMIT_TEST_NOOP";
        // Defensive: ensure the var is unset before the call.
        std::env::remove_var(var);
        let mut samples = mock_samples(5);
        apply_limit_from_env(&mut samples, var, "locomo", "conversations");
        assert_eq!(samples.len(), 5, "unset env var must leave samples intact");
    }

    #[test]
    fn to_terminal_prints_coverage_delta_when_present() {
        let mut report = LocomoReport {
            conversations: vec![],
            aggregate_ndcg_at_10: 0.0,
            aggregate_mrr: 0.0,
            aggregate_recall_at_5: 0.0,
            aggregate_hit_rate_at_1: 0.0,
            total_questions: 0,
            total_memories: 0,
            per_category_aggregate: vec![],
            qa_accuracy: None,
            baseline: None,
            env: None,
            per_case: vec![],
            coverage: Some(crate::eval::report::CoverageRecall {
                blind: 0.40,
                expanded: 0.55,
            }),
        };
        let out = report.to_terminal();
        assert!(
            out.contains("Coverage recall"),
            "missing coverage header:\n{out}"
        );
        assert!(out.contains("0.4000"), "missing blind:\n{out}");
        assert!(out.contains("0.5500"), "missing expanded:\n{out}");
        assert!(out.contains("+0.1500"), "missing delta:\n{out}");
        report.coverage = None;
        assert!(!report.to_terminal().contains("Coverage recall"));
    }
}
