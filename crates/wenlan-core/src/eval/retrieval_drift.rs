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
const RBO_P: f64 = 0.9; // persistence: top-2 swap ~0.85, deep swap ~0.99 — top weighted ~10× depth-10
#[allow(dead_code)]
const CURRENT_RBO_THRESHOLD: f64 = 0.95; // 0.95: above a benign deep swap (~0.99), below a top-2 swap (~0.85)
#[allow(dead_code)]
const ANCHOR_RBO_THRESHOLD: f64 = 0.85; // 0.85: looser floor — full-reverse lands ~0.25, well below this

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

    fn ids(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("m{i}")).collect()
    }

    #[test]
    fn injected_reverse_top2_is_red() {
        let g = ids(10);
        let mut bad = g.clone();
        bad.swap(0, 1);
        let s = crate::eval::rank_overlap::rbo(&g, &bad, super::RBO_P);
        assert!(
            s < super::CURRENT_RBO_THRESHOLD,
            "reverse-top-2 must trip current floor (got {s:.4})"
        );
    }
    #[test]
    fn injected_drop_best_is_red() {
        let g = ids(10);
        let mut bad = g[1..].to_vec();
        bad.push("mX".to_string());
        let s = crate::eval::rank_overlap::rbo(&g, &bad, super::RBO_P);
        assert!(
            s < super::CURRENT_RBO_THRESHOLD,
            "drop-best must trip current floor (got {s:.4})"
        );
    }
    #[test]
    fn injected_full_reverse_is_red_even_at_anchor() {
        let g = ids(10);
        let mut bad = g.clone();
        bad.reverse();
        let s = crate::eval::rank_overlap::rbo(&g, &bad, super::RBO_P);
        assert!(
            s < super::ANCHOR_RBO_THRESHOLD,
            "full reverse must trip even the looser anchor floor (got {s:.4})"
        );
    }
    #[test]
    fn identical_is_green() {
        let g = ids(10);
        let s = crate::eval::rank_overlap::rbo(&g, &g, super::RBO_P);
        assert!(
            s >= super::CURRENT_RBO_THRESHOLD,
            "identical must pass (got {s:.4})"
        );
    }
    #[test]
    fn benign_deep_neartie_swap_is_green() {
        let g = ids(10);
        let mut benign = g.clone();
        benign.swap(7, 8);
        let s = crate::eval::rank_overlap::rbo(&g, &benign, super::RBO_P);
        assert!(
            s >= super::CURRENT_RBO_THRESHOLD,
            "deep near-tie swap must stay green (got {s:.4})"
        );
    }

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
