// SPDX-License-Identifier: Apache-2.0
//! Signal composition with degenerate-skip and panic-safe fallback.

use crate::tuning::CompositeWeights;
use std::collections::HashMap;

#[allow(dead_code)]
#[derive(Hash, Eq, PartialEq, Clone, Copy, Debug)]
pub(crate) enum SignalKind {
    Semantic,
    Bm25,
    GraphDistance,
    Activation,
    Temporal,
    Trust,
    Recency,
    AccessFrequency,
}

/// Compose per-signal score vectors into a single ranked score vector.
///
/// Signals whose entire score vector is zero (degenerate) are skipped. The
/// remaining active signals are renormalized so their weights sum to 1.0. Each
/// active signal is min-max normalized before weighting; if the range is
/// effectively zero the fallback value 0.5 is used so the result is bounded.
///
/// Fallback: if no signal is active, returns the raw semantic vec (if present)
/// or an empty Vec.
#[allow(dead_code)]
pub(crate) fn compose(
    scores_per_signal: &HashMap<SignalKind, Vec<f64>>,
    weights: &CompositeWeights,
) -> Vec<f64> {
    let pool_size = scores_per_signal
        .values()
        .next()
        .map(|v| v.len())
        .unwrap_or(0);

    let all_kinds: Vec<(SignalKind, f64)> = vec![
        (SignalKind::Semantic, weights.semantic),
        (SignalKind::Bm25, weights.bm25),
        (SignalKind::GraphDistance, weights.graph_distance),
        (SignalKind::Activation, weights.activation),
        (SignalKind::Temporal, weights.temporal),
        (SignalKind::Trust, weights.trust),
        (SignalKind::Recency, weights.recency),
        (SignalKind::AccessFrequency, weights.access_frequency),
    ];

    let active: Vec<(SignalKind, f64)> = all_kinds
        .into_iter()
        .filter(|(kind, _w)| {
            scores_per_signal
                .get(kind)
                .map(|s| s.iter().any(|x| x.abs() > 1e-9))
                .unwrap_or(false)
        })
        .filter(|(_kind, w)| *w > 0.0)
        .collect();

    if active.is_empty() {
        return scores_per_signal
            .get(&SignalKind::Semantic)
            .cloned()
            .unwrap_or_else(|| vec![0.0; pool_size]);
    }

    let weight_sum: f64 = active.iter().map(|(_, w)| w).sum();
    let renormalized: Vec<(SignalKind, f64)> = active
        .into_iter()
        .map(|(k, w)| (k, w / weight_sum))
        .collect();

    let mut out = vec![0.0; pool_size];
    for (kind, w) in &renormalized {
        let raw = match scores_per_signal.get(kind) {
            Some(v) => v,
            None => continue,
        };
        let min = raw.iter().fold(f64::INFINITY, |a, &b| a.min(b));
        let max = raw.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
        let range = max - min;
        for (i, &v) in raw.iter().enumerate() {
            let nv = if range < 1e-9 { 0.5 } else { (v - min) / range };
            out[i] += w * nv;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tuning::CompositeWeights;

    #[test]
    fn compose_all_zero_signal_skipped() {
        let mut scores: HashMap<SignalKind, Vec<f64>> = HashMap::new();
        scores.insert(SignalKind::Semantic, vec![0.5, 0.7, 0.3]);
        scores.insert(SignalKind::Temporal, vec![0.0, 0.0, 0.0]); // degenerate
        let weights = CompositeWeights {
            semantic: 0.5,
            temporal: 0.5,
            ..CompositeWeights::default_zero()
        };
        let out = compose(&scores, &weights);
        // Temporal skipped; semantic re-normalized to weight 1.0.
        // After min-max norm semantic = [0.5, 1.0, 0.0]. Weighted sum = those values directly.
        assert!(out[1] > out[0]);
        assert!(out[0] > out[2]);
    }

    #[test]
    fn compose_all_degenerate_falls_back_to_semantic() {
        let mut scores: HashMap<SignalKind, Vec<f64>> = HashMap::new();
        scores.insert(SignalKind::Semantic, vec![0.5, 0.7, 0.3]);
        let weights = CompositeWeights::default_zero(); // all weights zero
        let out = compose(&scores, &weights);
        assert_eq!(out, vec![0.5, 0.7, 0.3]);
    }

    #[test]
    fn compose_all_degenerate_no_semantic_returns_zero_vec() {
        let scores: HashMap<SignalKind, Vec<f64>> = HashMap::new();
        let weights = CompositeWeights::default_zero();
        let out = compose(&scores, &weights);
        assert_eq!(out, Vec::<f64>::new());
    }

    #[test]
    fn compose_renormalizes_two_active_when_three_degenerate() {
        // 5 signals in map; 3 are all-zero (degenerate); 2 active.
        // Active: Semantic weight 0.3, Bm25 weight 0.5. Sum = 0.8.
        // After renorm: Semantic = 0.3/0.8 = 0.375, Bm25 = 0.5/0.8 = 0.625.
        //
        // pool = [[1.0, 0.0], [0.0, 1.0]] for Semantic and Bm25 respectively.
        // After min-max norm: Semantic [1.0,0.0], Bm25 [0.0,1.0].
        // out[0] = 0.375 * 1.0 + 0.625 * 0.0 = 0.375
        // out[1] = 0.375 * 0.0 + 0.625 * 1.0 = 0.625
        let mut scores: HashMap<SignalKind, Vec<f64>> = HashMap::new();
        scores.insert(SignalKind::Semantic, vec![1.0, 0.0]);
        scores.insert(SignalKind::Bm25, vec![0.0, 1.0]);
        scores.insert(SignalKind::Temporal, vec![0.0, 0.0]);
        scores.insert(SignalKind::Trust, vec![0.0, 0.0]);
        scores.insert(SignalKind::Recency, vec![0.0, 0.0]);
        let weights = CompositeWeights {
            semantic: 0.3,
            bm25: 0.5,
            ..CompositeWeights::default_zero()
        };
        let out = compose(&scores, &weights);
        let eps = 1e-9;
        assert!((out[0] - 0.375).abs() < eps, "out[0]={}", out[0]);
        assert!((out[1] - 0.625).abs() < eps, "out[1]={}", out[1]);
    }

    #[test]
    fn compose_min_max_range_zero_handled() {
        let mut scores: HashMap<SignalKind, Vec<f64>> = HashMap::new();
        scores.insert(SignalKind::Semantic, vec![0.5, 0.5, 0.5]);
        let weights = CompositeWeights {
            semantic: 1.0,
            ..CompositeWeights::default_zero()
        };
        let out = compose(&scores, &weights);
        // range == 0; fallback 0.5; renorm weight = 1.0 (only active signal)
        assert_eq!(out, vec![0.5, 0.5, 0.5]);
    }
    // Note: deterministic tiebreak lives at the caller site (search_memory_composite),
    // not inside compose. No assertions to write here — contract is caller-enforced.
}
