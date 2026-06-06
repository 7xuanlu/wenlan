// SPDX-License-Identifier: Apache-2.0
//! Cross-encoder ⊕ RRF score blending for two-stage rerank. Replaces the prior
//! REPLACE behavior (CE logit overwrote the fused score, erasing temporal/
//! recency/salience boosters) with α·σ(CE)+(1−α)·norm(WRRF), matching
//! SuperLocalMemory's query-type-weighted blend.

use crate::router::classify::{RELATIONAL_KEYWORDS, TEMPORAL_KEYWORDS};

/// Logistic sigmoid: maps a raw CE logit to (0,1).
#[allow(dead_code)]
pub fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

/// Min-max normalize to [0,1]. Empty → empty. All-equal (incl. single) → all
/// 0.5 (neutral; no spurious ordering). Else (x-min)/(max-min).
#[allow(dead_code)]
pub fn minmax_normalize(scores: &[f32]) -> Vec<f32> {
    if scores.is_empty() {
        return Vec::new();
    }
    let min = scores.iter().copied().fold(f32::INFINITY, f32::min);
    let max = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let span = max - min;
    if span <= f32::EPSILON {
        return vec![0.5; scores.len()];
    }
    scores.iter().map(|s| (s - min) / span).collect()
}

/// Query-type-dependent blend weight α (weight on the CE term).
/// SuperLocalMemory: 0.5 for multi-hop/temporal, 0.75 single-hop. Origin maps
/// temporal/relational keyword presence → 0.5 (let boosted-RRF matter), else 0.75.
#[allow(dead_code)]
pub fn alpha_for_query(query: &str) -> f32 {
    let lower = query.to_lowercase();
    let needs_rrf = TEMPORAL_KEYWORDS.iter().any(|kw| lower.contains(kw))
        || RELATIONAL_KEYWORDS.iter().any(|kw| lower.contains(kw));
    if needs_rrf {
        0.5
    } else {
        0.75
    }
}

/// Blend a CE logit with a pre-normalized RRF score: α·σ(CE)+(1−α)·norm_rrf.
#[allow(dead_code)]
pub fn blend_score(ce_logit: f32, norm_rrf: f32, alpha: f32) -> f32 {
    alpha * sigmoid(ce_logit) + (1.0 - alpha) * norm_rrf
}

/// Opt-in: blend rerank only when `ORIGIN_ENABLE_RERANK_BLEND` is truthy
/// (default OFF → legacy REPLACE). Mirrors the other channel flags' parser.
#[allow(dead_code)]
pub fn rerank_blend_enabled() -> bool {
    let val = std::env::var("ORIGIN_ENABLE_RERANK_BLEND")
        .unwrap_or_default()
        .to_ascii_lowercase();
    val == "1" || val == "true" || val == "yes"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sigmoid_midpoint_and_saturation() {
        assert!((sigmoid(0.0) - 0.5).abs() < 1e-6);
        assert!(sigmoid(-2.0) < sigmoid(0.0) && sigmoid(0.0) < sigmoid(2.0));
        assert!(sigmoid(10.0) > 0.99 && sigmoid(-10.0) < 0.01);
    }

    #[test]
    fn minmax_empty_single_equal_and_range() {
        assert!(minmax_normalize(&[]).is_empty());
        assert_eq!(minmax_normalize(&[3.0]), vec![0.5]);
        assert_eq!(minmax_normalize(&[2.0, 2.0, 2.0]), vec![0.5, 0.5, 0.5]);
        let n = minmax_normalize(&[0.0, 5.0, 10.0]);
        assert!((n[0]).abs() < 1e-6 && (n[1] - 0.5).abs() < 1e-6 && (n[2] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn alpha_temporal_relational_vs_single() {
        assert_eq!(alpha_for_query("what changed recently"), 0.5);
        assert_eq!(alpha_for_query("relationship between Alice and Bob"), 0.5);
        assert_eq!(alpha_for_query("what is my favorite color"), 0.75);
    }

    #[test]
    fn blend_endpoints_and_mix() {
        assert!((blend_score(0.0, 0.9, 1.0) - 0.5).abs() < 1e-6);
        assert!((blend_score(5.0, 0.3, 0.0) - 0.3).abs() < 1e-6);
        let expect = 0.5 * sigmoid(2.0) + 0.5 * 0.4;
        assert!((blend_score(2.0, 0.4, 0.5) - expect).abs() < 1e-6);
    }
}
