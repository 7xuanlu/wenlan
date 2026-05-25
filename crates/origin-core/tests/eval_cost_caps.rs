// SPDX-License-Identifier: Apache-2.0
#![cfg(feature = "eval-harness")]
//! Cost-cap parsing + cumulative-spend tests.
//!
//! NOTE: Several tests mutate the `EVAL_I_REALLY_MEAN_IT` environment variable.
//! Run with `--test-threads=1` to avoid races between tests in this file.

use origin_core::eval::anthropic::parse_eval_max_usd;

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
