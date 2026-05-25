// SPDX-License-Identifier: Apache-2.0
#![cfg(feature = "eval-harness")]
//! Cost-cap parsing + cumulative-spend tests.

use origin_core::eval::anthropic::parse_eval_max_usd;

/// Serialize tests that mutate EVAL_I_REALLY_MEAN_IT / EVAL_MAX_USD env vars,
/// mirroring the pattern in crates/origin-core/tests/eval_harness.rs:3770
/// (PR #160). Without this, parallel test execution races on shared process env.
static EVAL_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn parse_eval_max_usd_garbage_fails_loudly() {
    let err = parse_eval_max_usd(Some("garbage")).unwrap_err();
    let msg = format!("{}", err);
    assert!(
        msg.contains("EVAL_MAX_USD"),
        "msg should mention env var: {}",
        msg
    );
    assert!(
        msg.contains("parse"),
        "msg should mention parse failure: {}",
        msg
    );
}

#[test]
fn parse_eval_max_usd_above_10_refused_without_override() {
    let _guard = EVAL_ENV_LOCK.lock().unwrap();
    std::env::remove_var("EVAL_I_REALLY_MEAN_IT");
    let err = parse_eval_max_usd(Some("50.0")).unwrap_err();
    let msg = format!("{}", err);
    assert!(
        msg.contains("EVAL_I_REALLY_MEAN_IT"),
        "should mention override: {}",
        msg
    );
}

#[test]
fn parse_eval_max_usd_above_10_allowed_with_override() {
    let _guard = EVAL_ENV_LOCK.lock().unwrap();
    std::env::set_var("EVAL_I_REALLY_MEAN_IT", "1");
    let cap = parse_eval_max_usd(Some("50.0")).unwrap();
    assert_eq!(cap, Some(50.0));
    std::env::remove_var("EVAL_I_REALLY_MEAN_IT");
}

#[test]
fn parse_eval_max_usd_none_means_no_cap() {
    assert_eq!(parse_eval_max_usd(None).unwrap(), None);
}

#[test]
fn parse_eval_max_usd_negative_fails() {
    let err = parse_eval_max_usd(Some("-1")).unwrap_err();
    assert!(format!("{}", err).contains("must be positive"));
}

#[test]
fn parse_eval_max_usd_normal_value_works() {
    assert_eq!(parse_eval_max_usd(Some("1.5")).unwrap(), Some(1.5));
}

use origin_core::eval::cost::RunCostTracker;

#[test]
fn run_cost_tracker_accumulates_in_millicents() {
    let t = RunCostTracker::new(Some(5.0));
    t.record_usd(1.50).unwrap();
    t.record_usd(2.25).unwrap();
    assert_eq!(t.total_usd(), 3.75);
}

#[test]
fn run_cost_tracker_aborts_when_cap_exceeded() {
    let t = RunCostTracker::new(Some(1.0));
    t.record_usd(0.50).unwrap();
    let err = t.record_usd(0.75).unwrap_err();
    assert!(format!("{}", err).contains("EVAL_MAX_USD_RUN"));
    assert!(
        format!("{}", err).contains("1.25"),
        "should include attempted total"
    );
}

#[test]
fn run_cost_tracker_no_cap_never_aborts() {
    let t = RunCostTracker::new(None);
    for _ in 0..100 {
        t.record_usd(1.0).unwrap();
    }
    assert_eq!(t.total_usd(), 100.0);
}

#[test]
fn run_cost_tracker_thread_safe() {
    use std::sync::Arc;
    let t = Arc::new(RunCostTracker::new(Some(10.0)));
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let t = t.clone();
            std::thread::spawn(move || {
                for _ in 0..100 {
                    let _ = t.record_usd(0.01);
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    // 4 threads × 100 iters × $0.01 = $4.00 — well under cap
    assert_eq!(t.total_usd(), 4.0);
}

#[test]
fn run_cost_tracker_refunds_on_cap_overage() {
    let t = RunCostTracker::new(Some(1.0));
    t.record_usd(0.50).unwrap();
    let _err = t.record_usd(0.75).unwrap_err();
    // Total should reflect successful spend only — failed record was refunded.
    assert_eq!(
        t.total_usd(),
        0.50,
        "failed record_usd must not corrupt total"
    );
    // Subsequent records within remaining cap should succeed.
    t.record_usd(0.40).unwrap();
    assert_eq!(t.total_usd(), 0.90);
}

#[test]
#[cfg(not(debug_assertions))]
fn run_cost_tracker_negative_cap_saturates_to_zero() {
    // Defensive: nonsense cap shouldn't wrap to giant u64.
    // Only runs in release (debug_assert fires under debug, which is the designed behavior).
    let t = RunCostTracker::new(Some(-1.0));
    // First record_usd of any positive amount should bail (cap effectively 0).
    let err = t.record_usd(0.01).unwrap_err();
    assert!(format!("{}", err).contains("EVAL_MAX_USD_RUN"));
}

use origin_core::eval::anthropic::reconcile_cost_usd;

#[test]
fn reconcile_cost_matches_haiku_batch_pricing() {
    // 1M input tokens at $0.25/MTok (batch-discounted) = $0.25
    assert!((reconcile_cost_usd(1_000_000, 0) - 0.25).abs() < 1e-9);
    // 1M output at $1.25/MTok = $1.25
    assert!((reconcile_cost_usd(0, 1_000_000) - 1.25).abs() < 1e-9);
    // Mixed
    assert!((reconcile_cost_usd(500_000, 100_000) - (0.125 + 0.125)).abs() < 1e-9);
    // Zero
    assert_eq!(reconcile_cost_usd(0, 0), 0.0);
}
