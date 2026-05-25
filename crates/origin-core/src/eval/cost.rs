// SPDX-License-Identifier: Apache-2.0
//! Run-level cost accumulation for EVAL_MAX_USD_RUN cap enforcement.

use std::sync::atomic::{AtomicU64, Ordering};

/// Tracks cumulative cost across all baselines in a single eval invocation.
///
/// Internally stored as millicents (USD × 100_000) in an atomic u64 so
/// concurrent baselines can record spend without locking. Cap check is racy
/// by design — if two threads race past the threshold, the cap may briefly
/// exceed by one batch's worth of cost. Acceptable because batch sizes are
/// bounded and the cap is a soft fence, not a hard wall.
pub struct RunCostTracker {
    cap_millicents: Option<u64>,
    total_millicents: AtomicU64,
}

impl RunCostTracker {
    pub fn new(cap_usd: Option<f64>) -> Self {
        if let Some(cap) = cap_usd {
            debug_assert!(
                cap.is_finite() && cap > 0.0,
                "cap_usd should be finite + positive; got {}",
                cap
            );
        }
        Self {
            cap_millicents: cap_usd.map(|usd| {
                let mc = (usd * 100_000.0).round();
                mc.max(0.0) as u64
            }),
            total_millicents: AtomicU64::new(0),
        }
    }

    /// Record a spend in USD. Returns Err if recording would push total past cap.
    pub fn record_usd(&self, amount_usd: f64) -> anyhow::Result<()> {
        if !amount_usd.is_finite() || amount_usd < 0.0 {
            anyhow::bail!(
                "record_usd: amount must be finite + non-negative; got {}",
                amount_usd
            );
        }
        let amount_mc = (amount_usd * 100_000.0).round() as u64;
        let new_total_mc = self.total_millicents.fetch_add(amount_mc, Ordering::SeqCst) + amount_mc;
        if let Some(cap_mc) = self.cap_millicents {
            if new_total_mc > cap_mc {
                // Refund — failed record shouldn't corrupt total.
                self.total_millicents.fetch_sub(amount_mc, Ordering::SeqCst);
                let new_total_usd = (new_total_mc as f64) / 100_000.0;
                let cap_usd = (cap_mc as f64) / 100_000.0;
                anyhow::bail!(
                    "EVAL_MAX_USD_RUN cap exceeded: attempted total ${:.2} > cap ${:.2}",
                    new_total_usd,
                    cap_usd
                );
            }
        }
        Ok(())
    }

    pub fn total_usd(&self) -> f64 {
        (self.total_millicents.load(Ordering::SeqCst) as f64) / 100_000.0
    }

    pub fn cap_usd(&self) -> Option<f64> {
        self.cap_millicents.map(|mc| (mc as f64) / 100_000.0)
    }
}
