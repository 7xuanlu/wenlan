// SPDX-License-Identifier: Apache-2.0
//! Eval report formatting — terminal table and JSON output.

use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct ReportEnv {
    pub fixture_revision: String,
    pub embedder_model: String,
    pub embedder_revision: String,
    pub retrieval_method: String,
    pub llm_provider_class: String,
    pub llm_model: String,
    pub judge_model: Option<String>,
    pub origin_version: String,
    pub eval_timestamp_unix: i64,

    // P0a additive fields:
    #[serde(default)]
    pub layer: Option<crate::eval::EvalLayer>,
    #[serde(default)]
    pub task: Option<String>,
    #[serde(default)]
    pub variant: Option<String>,
    #[serde(default)]
    pub embed_dim: Option<u32>,
    #[serde(default = "default_similarity_fn")]
    pub similarity_fn_name: String,
    #[serde(default)]
    pub judge_model_id: Option<String>,
    #[serde(default)]
    pub mcp_schema_hash: Option<String>,
    #[serde(default)]
    pub skill_prompt_hash: Option<String>,
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub schema_db_version: Option<u32>,
    #[serde(default)]
    pub migrations_hash: Option<String>,
    #[serde(default = "default_n_runs")]
    pub n_runs: u32,
    #[serde(default)]
    pub is_single_run: bool,
    #[serde(default)]
    pub run_id: Option<String>,
    #[serde(default)]
    pub timestamp_utc: Option<String>,
    #[serde(default)]
    pub git_sha: Option<String>,
    #[serde(default)]
    pub warmup_iterations: u32,
    #[serde(default)]
    pub eval_max_usd_baseline_cap: Option<f64>,
    #[serde(default)]
    pub eval_max_usd_run_cap: Option<f64>,
    #[serde(default)]
    pub eval_max_wall_secs_cap: Option<u64>,
    #[serde(default)]
    pub total_cost_usd: f64,
    #[serde(default)]
    pub total_wall_secs: u64,
}

fn default_similarity_fn() -> String {
    "cosine".to_string()
}
fn default_schema_version() -> u32 {
    1
}
fn default_n_runs() -> u32 {
    1
}

impl Default for ReportEnv {
    fn default() -> Self {
        Self {
            // 9 legacy fields
            fixture_revision: String::new(),
            embedder_model: String::new(),
            embedder_revision: String::new(),
            retrieval_method: String::new(),
            llm_provider_class: String::new(),
            llm_model: String::new(),
            judge_model: None,
            origin_version: String::new(),
            eval_timestamp_unix: 0,
            // P0a additive fields — mirror serde default attributes
            layer: None,
            task: None,
            variant: None,
            embed_dim: None,
            similarity_fn_name: default_similarity_fn(),
            judge_model_id: None,
            mcp_schema_hash: None,
            skill_prompt_hash: None,
            schema_version: default_schema_version(),
            schema_db_version: None,
            migrations_hash: None,
            n_runs: default_n_runs(),
            is_single_run: false,
            run_id: None,
            timestamp_utc: None,
            git_sha: None,
            warmup_iterations: 0,
            eval_max_usd_baseline_cap: None,
            eval_max_usd_run_cap: None,
            eval_max_wall_secs_cap: None,
            total_cost_usd: 0.0,
            total_wall_secs: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, Default)]
pub struct EvalReport {
    pub fixture_count: usize,
    pub file_count: usize,
    pub search_mode: String,
    // Primary (BEIR/MTEB headline)
    pub ndcg_at_10: f64,
    // Ranking quality
    pub ndcg_at_5: f64,
    pub map_at_5: f64,
    pub map_at_10: f64,
    pub mrr: f64,
    // Recall gradient
    pub recall_at_1: f64,
    pub recall_at_3: f64,
    pub recall_at_5: f64,
    // Hit rate
    pub hit_rate_at_1: f64,
    pub hit_rate_at_3: f64,
    // Precision
    pub precision_at_3: f64,
    pub precision_at_5: f64,
    // Negative analysis
    pub neg_above_relevant: usize,
    pub total_negatives: usize,
    pub negative_leakage: usize,
    // Quality gate
    #[serde(default)]
    pub gate_content_filtered: usize,
    #[serde(default)]
    pub gate_novelty_filtered: usize,
    // Empty-set precision
    #[serde(default)]
    pub empty_set_count: usize,
    #[serde(default)]
    pub empty_set_false_confidence: Option<f64>,
    #[serde(default)]
    pub score_gap: Option<f64>,
    // Temporal ordering
    #[serde(default)]
    pub temporal_ordering_total: usize,
    #[serde(default)]
    pub temporal_ordering_correct: usize,
    #[serde(default)]
    pub temporal_ordering_rate: Option<f64>,
    // Comparison
    pub baseline: Option<BaselineComparison>,
    pub per_case: Vec<CaseResult>,
    // Environment capture (schema v1)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub env: Option<ReportEnv>,
    // Per-query latency summary
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub latency: Option<crate::eval::latency::LatencySummary>,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct BaselineComparison {
    pub ndcg_at_10: f64,
    pub map_at_10: f64,
    pub mrr: f64,
    pub recall_at_5: f64,
    pub neg_above_relevant: usize,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct CaseResult {
    pub query: String,
    pub ndcg_at_10: f64,
    pub ndcg_at_5: f64,
    pub map_at_10: f64,
    pub mrr: f64,
    pub recall_at_5: f64,
    pub hit_rate_at_1: f64,
    pub precision_at_3: f64,
    pub negative_leakage: usize,
    pub neg_above_relevant: usize,
}

/// Hash the subset of `ReportEnv` fields that determine baseline comparability.
///
/// Includes: fixture_revision, embedder_revision, llm_provider_class,
/// llm_model, mcp_schema_hash, skill_prompt_hash, schema_version,
/// schema_db_version, similarity_fn_name.
///
/// Excludes: layer (path component), variant (path component), n_runs,
/// run_id, timestamp, costs, latency fields. These vary across runs of
/// the same eval setup, so cross-run comparison of metrics requires the
/// COMPARABLE subset to match.
pub fn comparable_env_hash(env: &ReportEnv) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(env.fixture_revision.as_bytes());
    h.update(b"|");
    h.update(env.embedder_revision.as_bytes());
    h.update(b"|");
    h.update(env.llm_provider_class.as_bytes());
    h.update(b"|");
    h.update(env.llm_model.as_bytes());
    h.update(b"|");
    h.update(env.mcp_schema_hash.as_deref().unwrap_or("").as_bytes());
    h.update(b"|");
    h.update(env.skill_prompt_hash.as_deref().unwrap_or("").as_bytes());
    h.update(b"|");
    h.update(env.schema_version.to_string().as_bytes());
    h.update(b"|");
    h.update(
        env.schema_db_version
            .map(|v| v.to_string())
            .unwrap_or_default()
            .as_bytes(),
    );
    h.update(b"|");
    h.update(env.similarity_fn_name.as_bytes());
    let hex = format!("{:x}", h.finalize());
    hex.chars().take(8).collect()
}

/// Encode retrieval variant + provider + fixture-hash into a baseline filename.
/// Shared by EvalReport, LocomoReport, and LongMemEvalReport.
/// Falls back to `base + ".json"` when `env` is None (back-compat).
pub fn encode_baseline_filename(env: Option<&ReportEnv>, base: &str) -> String {
    let Some(env) = env else {
        return format!("{}.json", base);
    };
    let variant = env
        .retrieval_method
        .trim_start_matches("search_memory")
        .trim_start_matches('_');
    let variant = if variant.is_empty() { "base" } else { variant };
    format!(
        "{}__{}__{}__{}.json",
        base, variant, env.llm_provider_class, env.fixture_revision
    )
}

impl EvalReport {
    /// Encode retrieval variant + provider + fixture-hash into baseline filename.
    /// Falls back to base + ".json" if env is missing (back-compat).
    pub fn baseline_filename(&self, base: &str) -> String {
        encode_baseline_filename(self.env.as_ref(), base)
    }

    /// Format report as terminal-friendly text.
    pub fn to_terminal(&self) -> String {
        let mut out = String::new();
        out.push_str("Origin Memory Eval\n");
        out.push_str("==================\n");
        out.push_str(&format!(
            "Fixtures: {} cases from {} files\n",
            self.fixture_count, self.file_count
        ));
        out.push_str(&format!("Search mode: {}\n\n", self.search_mode));

        out.push_str(&format!(
            "  NDCG@10:       {:.3}        <- primary\n",
            self.ndcg_at_10
        ));
        out.push_str(&format!("  NDCG@5:        {:.3}\n", self.ndcg_at_5));
        out.push_str(&format!("  MAP@10:        {:.3}\n", self.map_at_10));
        out.push_str(&format!("  MAP@5:         {:.3}\n", self.map_at_5));
        out.push_str(&format!("  MRR:           {:.3}\n", self.mrr));
        out.push_str(&format!("  Recall@1:      {:.3}\n", self.recall_at_1));
        out.push_str(&format!("  Recall@3:      {:.3}\n", self.recall_at_3));
        out.push_str(&format!("  Recall@5:      {:.3}\n", self.recall_at_5));
        out.push_str(&format!("  Hit Rate@1:    {:.3}\n", self.hit_rate_at_1));
        out.push_str(&format!("  Hit Rate@3:    {:.3}\n", self.hit_rate_at_3));
        out.push_str(&format!("  P@3:           {:.3}\n", self.precision_at_3));
        out.push_str(&format!("  P@5:           {:.3}\n", self.precision_at_5));
        out.push_str(&format!(
            "  Neg>relevant:  {}/{}\n",
            self.neg_above_relevant, self.total_negatives
        ));
        if self.gate_content_filtered > 0 || self.gate_novelty_filtered > 0 {
            out.push_str(&format!(
                "  Gate (content): {}\n",
                self.gate_content_filtered
            ));
            out.push_str(&format!(
                "  Gate (novelty): {}\n",
                self.gate_novelty_filtered
            ));
        }

        if self.empty_set_count > 0 {
            out.push_str(&format!(
                "\nEmpty-set precision ({} cases):\n",
                self.empty_set_count
            ));
            if let Some(fc) = self.empty_set_false_confidence {
                out.push_str(&format!("  False confidence: {:.3} (lower = better)\n", fc));
            }
            if let Some(sg) = self.score_gap {
                out.push_str(&format!(
                    "  Score gap:        {:.3} (higher = better)\n",
                    sg
                ));
            }
        }

        if self.temporal_ordering_total > 0 {
            out.push_str(&format!(
                "\nTemporal ordering: {}/{} correct",
                self.temporal_ordering_correct, self.temporal_ordering_total
            ));
            if let Some(rate) = self.temporal_ordering_rate {
                out.push_str(&format!(" (rate={:.3})", rate));
            }
            out.push('\n');
        }

        if let Some(ref b) = self.baseline {
            out.push_str("\nBaseline comparison:\n");
            let delta = |name: &str, old: f64, new: f64| -> String {
                let pct = ((new - old) / old.max(0.001)) * 100.0;
                format!("  {:<12} {:.3} -> {:.3} ({:+.1}%)\n", name, old, new, pct)
            };
            out.push_str(&delta("NDCG@10:", b.ndcg_at_10, self.ndcg_at_10));
            out.push_str(&delta("MAP@10:", b.map_at_10, self.map_at_10));
            out.push_str(&delta("MRR:", b.mrr, self.mrr));
            out.push_str(&delta("Recall@5:", b.recall_at_5, self.recall_at_5));
            out.push_str(&format!(
                "  Neg>rel:     {} -> {}\n",
                b.neg_above_relevant, self.neg_above_relevant
            ));
        }

        // Per-case breakdown sorted by NDCG@10 ascending (worst first)
        if !self.per_case.is_empty() {
            out.push_str("\nPer-case (worst first):\n");
            let mut sorted: Vec<_> = self.per_case.iter().collect();
            sorted.sort_by(|a, b| {
                a.ndcg_at_10
                    .partial_cmp(&b.ndcg_at_10)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            for c in sorted {
                let query_short: String = c.query.chars().take(55).collect();
                out.push_str(&format!(
                    "  NDCG={:.2} MAP={:.2} MRR={:.2} R@5={:.2} Nab={} | {}\n",
                    c.ndcg_at_10,
                    c.map_at_10,
                    c.mrr,
                    c.recall_at_5,
                    c.neg_above_relevant,
                    query_short
                ));
            }
        }

        out
    }

    /// Save current metrics as baseline for future comparison.
    pub fn save_baseline(&self, path: &Path) -> Result<(), std::io::Error> {
        let baseline = BaselineComparison {
            ndcg_at_10: self.ndcg_at_10,
            map_at_10: self.map_at_10,
            mrr: self.mrr,
            recall_at_5: self.recall_at_5,
            neg_above_relevant: self.neg_above_relevant,
        };
        let json = serde_json::to_string_pretty(&baseline).map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }

    /// Load baseline from a previous run for comparison.
    pub fn load_baseline(path: &Path) -> Option<BaselineComparison> {
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }
}

// ---------------------------------------------------------------------------
// Shared JSONL history append
// ---------------------------------------------------------------------------

/// A single benchmark run entry for the JSONL history file.
#[derive(Debug, Serialize, serde::Deserialize)]
pub struct BenchmarkHistoryEntry {
    pub timestamp: String,
    pub git_sha: String,
    pub benchmark: String,
    pub ndcg_at_10: f64,
    pub mrr: f64,
    pub recall_at_5: f64,
    pub hit_rate_at_1: f64,
}

/// Append a benchmark run to a JSONL history file.
///
/// Each call writes one JSON line. The file is created if it doesn't exist.
pub fn append_history(path: &Path, entry: &BenchmarkHistoryEntry) -> Result<(), std::io::Error> {
    use std::io::Write;
    let json = serde_json::to_string(entry).map_err(std::io::Error::other)?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{}", json)
}

// ---------------------------------------------------------------------------
// Shared baseline category struct
// ---------------------------------------------------------------------------

/// Per-category baseline metrics shared across benchmark types.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct CategoryBaseline {
    pub name: String,
    pub ndcg_at_10: f64,
    pub mrr: f64,
    pub recall_at_5: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalLayer;

    fn sample_env(layer: EvalLayer, variant: &str) -> ReportEnv {
        ReportEnv {
            layer: Some(layer),
            task: Some("locomo".to_string()),
            variant: Some(variant.to_string()),
            fixture_revision: "fixture_aaaa".to_string(),
            embedder_revision: "BGE-Base-EN-v1.5-Q".to_string(),
            llm_provider_class: "on-device".to_string(),
            llm_model: "qwen3-4b".to_string(),
            mcp_schema_hash: None,
            skill_prompt_hash: None,
            schema_version: 1,
            schema_db_version: Some(46),
            similarity_fn_name: "cosine".to_string(),
            ..ReportEnv::default()
        }
    }

    #[test]
    fn comparable_hash_excludes_layer_and_variant() {
        // L1 base vs L2 base: same comparable fields → same hash across layers.
        let h1 = comparable_env_hash(&sample_env(EvalLayer::L1Db, "base"));
        let h2 = comparable_env_hash(&sample_env(EvalLayer::L2Http, "base"));
        assert_eq!(h1, h2, "same comparable fields → same hash across layers");

        // base vs reranked at L1: also same hash (variant excluded).
        let h3 = comparable_env_hash(&sample_env(EvalLayer::L1Db, "reranked"));
        assert_eq!(h1, h3, "variant should not affect comparable hash");
    }

    #[test]
    fn comparable_hash_changes_when_fixture_changes() {
        let e1 = sample_env(EvalLayer::L1Db, "base");
        let mut e2 = sample_env(EvalLayer::L1Db, "base");
        e2.fixture_revision = "different_fixture".to_string();
        assert_ne!(comparable_env_hash(&e1), comparable_env_hash(&e2));
    }

    #[test]
    fn comparable_hash_changes_when_schema_version_bumps() {
        let e1 = sample_env(EvalLayer::L1Db, "base");
        let mut e2 = sample_env(EvalLayer::L1Db, "base");
        e2.schema_version = 2;
        assert_ne!(comparable_env_hash(&e1), comparable_env_hash(&e2));
    }

    #[test]
    fn comparable_hash_stable_across_calls() {
        let e = sample_env(EvalLayer::L1Db, "base");
        assert_eq!(comparable_env_hash(&e), comparable_env_hash(&e));
        assert_eq!(comparable_env_hash(&e).len(), 8, "expected sha256[..8]");
    }

    #[test]
    fn test_baseline_save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("baseline.json");

        let report = EvalReport {
            fixture_count: 10,
            file_count: 3,
            search_mode: "search_memory".to_string(),
            ndcg_at_10: 0.95,
            ndcg_at_5: 0.93,
            map_at_5: 0.88,
            map_at_10: 0.90,
            mrr: 0.97,
            recall_at_1: 0.85,
            recall_at_3: 0.92,
            recall_at_5: 0.96,
            hit_rate_at_1: 0.90,
            hit_rate_at_3: 0.95,
            precision_at_3: 0.80,
            precision_at_5: 0.75,
            neg_above_relevant: 5,
            total_negatives: 20,
            negative_leakage: 15,
            gate_content_filtered: 0,
            gate_novelty_filtered: 0,
            empty_set_count: 0,
            empty_set_false_confidence: None,
            score_gap: None,
            temporal_ordering_total: 0,
            temporal_ordering_correct: 0,
            temporal_ordering_rate: None,
            baseline: None,
            per_case: vec![],
            env: None,
            latency: None,
        };

        report.save_baseline(&path).unwrap();
        let loaded = EvalReport::load_baseline(&path).unwrap();

        assert!((loaded.ndcg_at_10 - 0.95).abs() < 0.001);
        assert!((loaded.mrr - 0.97).abs() < 0.001);
        assert_eq!(loaded.neg_above_relevant, 5);
    }

    #[test]
    fn test_append_history() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.jsonl");

        let entry1 = BenchmarkHistoryEntry {
            timestamp: "2026-04-05T12:00:00Z".to_string(),
            git_sha: "abc1234".to_string(),
            benchmark: "fixtures".to_string(),
            ndcg_at_10: 0.95,
            mrr: 0.90,
            recall_at_5: 0.85,
            hit_rate_at_1: 0.80,
        };
        let entry2 = BenchmarkHistoryEntry {
            timestamp: "2026-04-05T13:00:00Z".to_string(),
            git_sha: "def5678".to_string(),
            benchmark: "locomo".to_string(),
            ndcg_at_10: 0.65,
            mrr: 0.60,
            recall_at_5: 0.55,
            hit_rate_at_1: 0.50,
        };

        append_history(&path, &entry1).unwrap();
        append_history(&path, &entry2).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2, "expected 2 JSONL lines");

        // Parse each line as valid JSON
        let parsed1: BenchmarkHistoryEntry = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(parsed1.benchmark, "fixtures");
        assert!((parsed1.ndcg_at_10 - 0.95).abs() < 0.001);

        let parsed2: BenchmarkHistoryEntry = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(parsed2.benchmark, "locomo");
        assert_eq!(parsed2.git_sha, "def5678");
    }
}
