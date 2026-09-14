// SPDX-License-Identifier: Apache-2.0
use wenlan_types::repair_current::{CurrentRepairChoice, PrepareCurrentRepairRequest};

#[tokio::test]
#[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
async fn current_prepare_rename_succeeds_and_preserves_exact_review_binding() {
    let fixture = rename_fixture().await;
    let current = PrepareCurrentRepairRequest {
        lint_scope: fixture.request.lint_scope().clone(),
        choice: CurrentRepairChoice::rename_page_title(
            fixture.review_id.clone(),
            "page-a".to_string(),
            "Origin".to_string(),
            "Origin Git Workflow Gotchas".to_string(),
        )
        .unwrap(),
    };
    let manifest = crate::repair::current::prepare_current_repair_with_pages(
        &fixture.db,
        &RepairArtifactStore::new(fixture.repair_root.path().to_path_buf()),
        current,
        fixture.request.general_report().clone(),
        None,
        Some(fixture.page_root.path()),
        1_721_000_001,
    )
    .await
    .unwrap();

    assert_eq!(manifest.writer(), RepairWriter::RenamePageTitle);
    assert_eq!(
        manifest.source().review_binding().unwrap().review_id(),
        fixture.review_id
    );
    assert!(matches!(
        manifest.mutation(),
        RepairMutation::RenamePageTitle { before_title, after_title, .. }
            if before_title == "Origin" && after_title == "Origin Git Workflow Gotchas"
    ));
}

#[tokio::test]
#[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
async fn current_prepare_rename_rejects_stale_page_projection() {
    let fixture = rename_fixture().await;
    std::fs::write(fixture.page_root.path().join("unrelated.txt"), b"changed").unwrap();
    let current = PrepareCurrentRepairRequest {
        lint_scope: fixture.request.lint_scope().clone(),
        choice: CurrentRepairChoice::rename_page_title(
            fixture.review_id.clone(),
            "page-a".to_string(),
            "Origin".to_string(),
            "Origin Git Workflow Gotchas".to_string(),
        )
        .unwrap(),
    };
    let repair_root = tempfile::tempdir().unwrap();
    let result = crate::repair::current::prepare_current_repair_with_pages(
        &fixture.db,
        &RepairArtifactStore::new(repair_root.path().to_path_buf()),
        current,
        fixture.request.general_report().clone(),
        None,
        Some(fixture.page_root.path()),
        1_721_000_001,
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
async fn current_prepare_rename_rejects_review_owner_rebinding() {
    let fixture = rename_fixture().await;
    let current = PrepareCurrentRepairRequest {
        lint_scope: fixture.request.lint_scope().clone(),
        choice: CurrentRepairChoice::rename_page_title(
            fixture.review_id.clone(),
            "page-b".to_string(),
            "origin".to_string(),
            "Origin Git Workflow Gotchas".to_string(),
        )
        .unwrap(),
    };
    let repair_root = tempfile::tempdir().unwrap();
    let result = crate::repair::current::prepare_current_repair_with_pages(
        &fixture.db,
        &RepairArtifactStore::new(repair_root.path().to_path_buf()),
        current,
        fixture.request.general_report().clone(),
        None,
        Some(fixture.page_root.path()),
        1_721_000_001,
    )
    .await;

    assert!(matches!(
        result,
        Err(WenlanError::Conflict(message)) if message == "repair_target_stale"
    ));
}
