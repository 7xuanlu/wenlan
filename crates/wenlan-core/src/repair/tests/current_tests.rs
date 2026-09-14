// SPDX-License-Identifier: Apache-2.0
use super::*;
use wenlan_types::repair_current::{CurrentRepairChoice, PrepareCurrentRepairRequest};
use wenlan_types::repair_plan::{RepairAffectedRecord, RepairAffectedRecordKind};

fn classification_current_request(
    request: &PrepareRepairRequest,
    review_id: String,
    memory_id: &str,
) -> PrepareCurrentRepairRequest {
    PrepareCurrentRepairRequest {
        lint_scope: request.lint_scope().clone(),
        choice: CurrentRepairChoice::reclassify_memory(
            review_id,
            memory_id.to_string(),
            MemoryType::Decision,
        )
        .unwrap(),
    }
}

fn classification_review_id(request: &PrepareRepairRequest, memory_id: &str) -> String {
    let finding = request.selected_finding().expect("classification finding");
    let affected =
        RepairAffectedRecord::try_new(RepairAffectedRecordKind::Memory, memory_id.to_string())
            .unwrap();
    let occurrence = crate::repair_plan::semantic_review_occurrence_digest(
        REPAIR_CLASSIFICATION_CHECK_ID,
        finding,
        &[affected],
    )
    .unwrap();
    format!("lint_review_{}", occurrence.as_str())
}

fn deep_with_classification_outcome(
    report: &wenlan_types::lint::LintReport,
    outcome: &str,
) -> wenlan_types::lint::LintReport {
    let mut value = serde_json::to_value(report).unwrap();
    let checks = value["checks"].as_array_mut().unwrap();
    let classification = checks
        .iter_mut()
        .find(|check| check["check_id"] == REPAIR_CLASSIFICATION_CHECK_ID)
        .expect("classification check");
    classification["outcome"] = serde_json::json!(outcome);
    classification["evidence"] = serde_json::json!([]);
    classification["coverage"]["evidence_returned"] = serde_json::json!(0);
    match outcome {
        "pass" => {
            classification["severity"] = serde_json::json!("info");
            classification["applicability"] = serde_json::json!("inventory");
            classification["precondition"] = serde_json::json!("ready");
        }
        "failed_to_run" => {
            classification["severity"] = serde_json::json!("error");
            classification["applicability"] = serde_json::json!("applicable");
            classification["precondition"] = serde_json::json!("ready");
        }
        "not_run_prerequisite" => {
            classification["severity"] = serde_json::json!("error");
            classification["applicability"] = serde_json::json!("not_applicable");
            classification["precondition"] = serde_json::json!("missing_prerequisite");
        }
        "inconsistent_snapshot" => {
            classification["severity"] = serde_json::json!("error");
            classification["applicability"] = serde_json::json!("applicable");
            classification["precondition"] = serde_json::json!("snapshot_unstable");
        }
        other => panic!("unsupported test outcome {other}"),
    }
    let passed = checks
        .iter()
        .filter(|check| check["outcome"] == "pass")
        .count();
    let findings = checks
        .iter()
        .filter(|check| check["outcome"] == "finding")
        .count();
    let actionable_findings = checks
        .iter()
        .filter(|check| check["outcome"] == "finding" && check["gate_effect"] == "actionable")
        .count();
    let incomplete = checks.len() - passed - findings;
    value["totals"] = serde_json::json!({
        "checks": checks.len(),
        "passed": passed,
        "findings": findings,
        "actionable_findings": actionable_findings,
        "advisory_findings": findings - actionable_findings,
        "incomplete": incomplete
    });
    value["complete"] = serde_json::json!(incomplete == 0);
    serde_json::from_value(value).unwrap()
}

fn deep_without_classification(
    report: &wenlan_types::lint::LintReport,
) -> wenlan_types::lint::LintReport {
    let checks = report
        .checks()
        .iter()
        .filter(|check| check.check_id() != REPAIR_CLASSIFICATION_CHECK_ID)
        .cloned()
        .collect();
    wenlan_types::lint::LintReport::try_new_for_profile_with_agent_work(
        report.profile(),
        report.scope().clone(),
        report.capability_context(),
        report.snapshots().clone(),
        report.config_fingerprint().clone(),
        report.producer_receipt().clone(),
        checks,
        report.agent_work().cloned(),
    )
    .unwrap()
}

#[tokio::test]
#[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
async fn current_prepare_classification_binds_exact_review_and_source_evidence() {
    let (db, _db_dir) = fixture().await;
    let source = request(&db).await;
    let review_id = classification_review_id(&source, "mem_target");
    let current = classification_current_request(&source, review_id.clone(), "mem_target");
    let repair_root = tempfile::tempdir().unwrap();
    let manifest = crate::repair::current::prepare_current_repair_with_pages(
        &db,
        &RepairArtifactStore::new(repair_root.path().to_path_buf()),
        current,
        source.general_report().clone(),
        source.deep_report().cloned(),
        None,
        1_721_000_000,
    )
    .await
    .unwrap();

    assert_eq!(manifest.target().memory_source_id(), "mem_target");
    assert_eq!(
        manifest.source().review_binding().unwrap().review_id(),
        review_id
    );
    assert_eq!(
        manifest.source().finding().unwrap().evidence_ids(),
        source.selected_finding().unwrap().evidence_ids()
    );
}

#[tokio::test]
#[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
async fn current_prepare_classification_rejects_cross_scope_report() {
    let (db, _db_dir) = fixture().await;
    let source = request(&db).await;
    let mut current = classification_current_request(
        &source,
        classification_review_id(&source, "mem_target"),
        "mem_target",
    );
    current.lint_scope = RepairLintScope::registered("work".to_string()).unwrap();
    let repair_root = tempfile::tempdir().unwrap();
    let result = crate::repair::current::prepare_current_repair_with_pages(
        &db,
        &RepairArtifactStore::new(repair_root.path().to_path_buf()),
        current,
        source.general_report().clone(),
        source.deep_report().cloned(),
        None,
        1_721_000_000,
    )
    .await;

    assert!(matches!(
        result,
        Err(WenlanError::Validation(message))
            if message == "invalid_prepare_current_repair_request"
    ));
    assert_eq!(std::fs::read_dir(repair_root.path()).unwrap().count(), 0);
}

#[tokio::test]
#[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
async fn current_prepare_classification_rejects_stale_source_snapshot() {
    let (db, _db_dir) = fixture().await;
    let source = request(&db).await;
    let current = classification_current_request(
        &source,
        classification_review_id(&source, "mem_target"),
        "mem_target",
    );
    db.test_primary_session()
        .await
        .execute(
            "UPDATE memories SET title='changed after lint' WHERE id='row-other'",
            (),
        )
        .await
        .unwrap();
    let repair_root = tempfile::tempdir().unwrap();
    let result = crate::repair::current::prepare_current_repair_with_pages(
        &db,
        &RepairArtifactStore::new(repair_root.path().to_path_buf()),
        current,
        source.general_report().clone(),
        source.deep_report().cloned(),
        None,
        1_721_000_000,
    )
    .await;

    assert!(matches!(
        result,
        Err(WenlanError::Conflict(message)) if message == "repair_source_reports_stale"
    ));
    assert_eq!(std::fs::read_dir(repair_root.path()).unwrap().count(), 0);
}

#[tokio::test]
#[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
async fn current_prepare_classification_rejects_requested_review_rebinding() {
    let (db, _db_dir) = fixture().await;
    let source = request(&db).await;
    let current = classification_current_request(
        &source,
        "lint_review_wrong_binding".to_string(),
        "mem_target",
    );
    let repair_root = tempfile::tempdir().unwrap();
    let result = crate::repair::current::prepare_current_repair_with_pages(
        &db,
        &RepairArtifactStore::new(repair_root.path().to_path_buf()),
        current,
        source.general_report().clone(),
        source.deep_report().cloned(),
        None,
        1_721_000_000,
    )
    .await;

    assert!(matches!(
        result,
        Err(WenlanError::Conflict(message))
            if message == "repair_current_review_binding_mismatch"
    ));
}

#[tokio::test]
#[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
async fn current_prepare_classification_rejects_unavailable_or_missing_check_without_stale_claim() {
    let (db, _db_dir) = fixture().await;
    let source = request(&db).await;
    let current = classification_current_request(
        &source,
        classification_review_id(&source, "mem_target"),
        "mem_target",
    );

    for outcome in [
        "failed_to_run",
        "not_run_prerequisite",
        "inconsistent_snapshot",
    ] {
        let repair_root = tempfile::tempdir().unwrap();
        let result = crate::repair::current::prepare_current_repair_with_pages(
            &db,
            &RepairArtifactStore::new(repair_root.path().to_path_buf()),
            current.clone(),
            source.general_report().clone(),
            Some(deep_with_classification_outcome(
                source.deep_report().unwrap(),
                outcome,
            )),
            None,
            1_721_000_000,
        )
        .await;
        assert!(matches!(
            result,
            Err(WenlanError::Conflict(message))
                if message == "repair_current_check_unavailable"
        ));
        assert_eq!(std::fs::read_dir(repair_root.path()).unwrap().count(), 0);
    }

    let repair_root = tempfile::tempdir().unwrap();
    let result = crate::repair::current::prepare_current_repair_with_pages(
        &db,
        &RepairArtifactStore::new(repair_root.path().to_path_buf()),
        current.clone(),
        source.general_report().clone(),
        Some(deep_without_classification(source.deep_report().unwrap())),
        None,
        1_721_000_000,
    )
    .await;
    assert!(matches!(
        result,
        Err(WenlanError::Conflict(message))
            if message == "repair_current_check_unavailable"
    ));
    assert_eq!(std::fs::read_dir(repair_root.path()).unwrap().count(), 0);
}

#[tokio::test]
#[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
async fn current_prepare_classification_pass_without_target_finding_is_stale() {
    let (db, _db_dir) = fixture().await;
    let source = request(&db).await;
    let current = classification_current_request(
        &source,
        classification_review_id(&source, "mem_target"),
        "mem_target",
    );
    let repair_root = tempfile::tempdir().unwrap();
    let result = crate::repair::current::prepare_current_repair_with_pages(
        &db,
        &RepairArtifactStore::new(repair_root.path().to_path_buf()),
        current,
        source.general_report().clone(),
        Some(deep_with_classification_outcome(
            source.deep_report().unwrap(),
            "pass",
        )),
        None,
        1_721_000_000,
    )
    .await;

    assert!(matches!(
        result,
        Err(WenlanError::Conflict(message)) if message == "repair_current_finding_missing"
    ));
    assert_eq!(std::fs::read_dir(repair_root.path()).unwrap().count(), 0);
}
