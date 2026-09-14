// SPDX-License-Identifier: Apache-2.0
use super::*;
use wenlan_types::repair_current::{CurrentRepairChoice, PrepareCurrentRepairRequest};

#[tokio::test]
#[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
async fn current_prepare_entity_extraction_succeeds_and_preserves_exact_owner_binding() {
    let fixture = entity_extraction_fixture().await;
    let current = PrepareCurrentRepairRequest {
        lint_scope: fixture.request.lint_scope().clone(),
        choice: CurrentRepairChoice::complete_entity_extraction(
            fixture.review_id.clone(),
            "mem-entity".to_string(),
            vec!["ent-new".to_string()],
        )
        .unwrap(),
    };
    let manifest = crate::repair::current::prepare_current_repair_with_pages(
        &fixture.db,
        &RepairArtifactStore::new(fixture.repair_root.path().to_path_buf()),
        current,
        fixture.request.general_report().clone(),
        None,
        None,
        1_721_000_001,
    )
    .await
    .unwrap();

    assert_eq!(manifest.writer(), RepairWriter::CompleteEntityExtraction);
    let binding = manifest.source().review_binding().unwrap();
    assert_eq!(binding.review_id(), fixture.review_id);
    assert_eq!(binding.owner_ids(), ["mem-entity"]);
    assert!(matches!(
        manifest.mutation(),
        RepairMutation::CompleteEntityExtraction { entity_ids }
            if entity_ids.as_slice() == ["ent-new"]
    ));
}

#[tokio::test]
#[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
async fn current_prepare_entity_extraction_rejects_stale_target() {
    let fixture = entity_extraction_fixture().await;
    fixture
        .db
        .test_primary_session()
        .await
        .execute(
            "UPDATE enrichment_steps SET status='ok' WHERE source_id='mem-entity' AND step_name='entity_extract'",
            (),
        )
        .await
        .unwrap();
    let current = PrepareCurrentRepairRequest {
        lint_scope: fixture.request.lint_scope().clone(),
        choice: CurrentRepairChoice::complete_entity_extraction(
            fixture.review_id.clone(),
            "mem-entity".to_string(),
            vec!["ent-new".to_string()],
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
        None,
        1_721_000_001,
    )
    .await;

    assert!(matches!(
        result,
        Err(WenlanError::Conflict(message)) if message == "repair_target_stale"
    ));
    assert_eq!(std::fs::read_dir(repair_root.path()).unwrap().count(), 0);
}
