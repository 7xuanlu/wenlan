// SPDX-License-Identifier: Apache-2.0
//! Per-query latency capture — P50/P99 percentile summary.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct LatencySummary {
    pub p50_ms: u64,
    pub p99_ms: u64,
    pub total_ms: u64,
    pub sample_count: usize,
}

impl LatencySummary {
    /// Compute P50/P99 over a slice of per-query latencies in microseconds.
    /// Converts to milliseconds for storage (rounds down).
    pub fn from_micros(samples: &[u64]) -> Self {
        if samples.is_empty() {
            return LatencySummary::default();
        }
        let samples_ms: Vec<u64> = samples.iter().map(|&us| us / 1000).collect();
        latency_summary(&samples_ms)
    }
}

/// Sort-based percentile. O(n log n); fine for eval (n < 10k).
pub fn latency_summary(samples_ms: &[u64]) -> LatencySummary {
    if samples_ms.is_empty() {
        return LatencySummary::default();
    }
    let mut sorted = samples_ms.to_vec();
    sorted.sort_unstable();
    let n = sorted.len();
    let p50_idx = (n * 50).saturating_sub(1) / 100;
    let p99_idx = (n * 99).saturating_sub(1) / 100;
    LatencySummary {
        p50_ms: sorted[p50_idx],
        p99_ms: sorted[p99_idx],
        total_ms: samples_ms.iter().sum(),
        sample_count: n,
    }
}
