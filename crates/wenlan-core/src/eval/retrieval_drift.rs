// SPDX-License-Identifier: Apache-2.0
//! Differential / equivalence oracle for retrieval ranking (drift detector).
//! Freezes retrieval rankings over the eval fixtures as a committed golden, then
//! asserts the live ranking still agrees (top-weighted RBO) — catching silent
//! reorderings the noisy labeled NDCG eval (`eval::retrieval`) misses.
//! HONEST FRAMING: detects DRIFT from a frozen reference, NOT absolute correctness.
//! The golden is trustworthy only because the labeled NDCG eval vouches each baseline.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[allow(dead_code)]
const TOP_K: usize = 10;
#[allow(dead_code)]
const RBO_P: f64 = 0.9; // persistence — weights the top of the ranking; tuned later
#[allow(dead_code)]
const CURRENT_RBO_THRESHOLD: f64 = 0.95; // per-query floor vs current golden; tuned later
#[allow(dead_code)]
const ANCHOR_RBO_THRESHOLD: f64 = 0.85; // looser cumulative-drift floor vs old anchor; tuned later

/// Per-query frozen ranking snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryRanking {
    pub query: String,
    pub ranked_ids: Vec<String>,
    /// Stored for DIAGNOSIS only — never asserted exactly (cross-arch f32 ULP drift).
    pub scores: Vec<f32>,
}

/// Full golden snapshot for a fixture set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankingGolden {
    pub generated_env: String,
    pub fixture_set_hash: String,
    pub top_k: usize,
    pub queries: Vec<QueryRanking>,
}

/// Serialize a [`RankingGolden`] to pretty-printed JSON at `path`.
pub fn save_golden(golden: &RankingGolden, path: &Path) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(golden).map_err(std::io::Error::other)?;
    std::fs::write(path, json)
}

/// Deserialize a [`RankingGolden`] from a JSON file at `path`.
pub fn load_golden(path: &Path) -> std::io::Result<RankingGolden> {
    let raw = std::fs::read_to_string(path)?;
    serde_json::from_str(&raw).map_err(std::io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn golden_json_roundtrips() {
        let g = RankingGolden {
            generated_env: "linux-x86_64".to_string(),
            fixture_set_hash: "abc123".to_string(),
            top_k: 10,
            queries: vec![QueryRanking {
                query: "q".to_string(),
                ranked_ids: vec!["m1".to_string(), "m2".to_string()],
                scores: vec![0.91, 0.42],
            }],
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("g.json");
        save_golden(&g, &path).unwrap();
        let back = load_golden(&path).unwrap();
        assert_eq!(back.queries.len(), 1);
        assert_eq!(back.queries[0].ranked_ids, vec!["m1", "m2"]);
        assert_eq!(back.top_k, 10);
    }
}
