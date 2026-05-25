// SPDX-License-Identifier: Apache-2.0
#![cfg(feature = "eval-harness")]

use origin_core::eval::report::{save_full_report, EvalReport, ReportEnv};
use origin_core::eval::EvalLayer;

fn sample_env() -> ReportEnv {
    ReportEnv {
        layer: Some(EvalLayer::L1Db),
        task: Some("locomo".to_string()),
        variant: Some("base".to_string()),
        fixture_revision: "x".to_string(),
        embedder_revision: "x".to_string(),
        llm_provider_class: "on-device".to_string(),
        llm_model: "qwen3-4b".to_string(),
        schema_version: 1,
        ..ReportEnv::default()
    }
}

#[test]
fn save_refuses_no_env() {
    let report = EvalReport {
        env: None,
        ..EvalReport::default()
    };
    let tmp = tempfile::tempdir().unwrap();
    let err = save_full_report(tmp.path(), &report).unwrap_err();
    assert!(
        format!("{}", err).contains("env is required"),
        "unexpected error: {}",
        err
    );
}

#[test]
fn save_refuses_nan_in_metrics() {
    let mut report = EvalReport {
        env: Some(sample_env()),
        ..EvalReport::default()
    };
    report.ndcg_at_10 = f64::NAN;
    let tmp = tempfile::tempdir().unwrap();
    let err = save_full_report(tmp.path(), &report).unwrap_err();
    assert!(
        format!("{}", err).contains("non-finite"),
        "unexpected error: {}",
        err
    );
}

#[test]
fn save_refuses_skip_rate_above_5pct() {
    let mut report = EvalReport {
        env: Some(sample_env()),
        ..EvalReport::default()
    };
    report.total_scenarios = 100;
    report.skipped_scenarios = (0..10).map(|i| format!("s{}", i)).collect();
    let tmp = tempfile::tempdir().unwrap();
    let err = save_full_report(tmp.path(), &report).unwrap_err();
    assert!(
        format!("{}", err).contains("skip"),
        "unexpected error: {}",
        err
    );
}

#[test]
fn save_writes_to_correct_path_with_atomic_rename() {
    let report = EvalReport {
        env: Some(sample_env()),
        ..EvalReport::default()
    };
    let tmp = tempfile::tempdir().unwrap();
    let path = save_full_report(tmp.path(), &report).unwrap();
    assert!(path.exists(), "output file should exist at {:?}", path);
    assert!(
        path.to_string_lossy().contains("/l1_db/locomo/base__"),
        "expected layered path, got {:?}",
        path
    );
    let parent = path.parent().unwrap();
    let leftovers: Vec<_> = std::fs::read_dir(parent)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".tmp."))
        .collect();
    assert!(leftovers.is_empty(), "stale .tmp file(s): {:?}", leftovers);
}
