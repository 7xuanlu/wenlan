use super::{check, metric, run_at, REFINEMENTS};
use crate::db::tests::test_db;
use crate::synthesis::refinement_queue::process_refinement_queue;
use wenlan_types::lint::{LintMetricCode, LintOutcome};

async fn process(db: &crate::db::MemoryDB) -> usize {
    process_refinement_queue(
        db,
        None,
        &crate::prompts::PromptRegistry::default(),
        &crate::tuning::RefineryConfig::default(),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn daemon_dismissed_dedup_merge_is_valid_inventory() {
    // Given
    let (db, _tmp) = test_db().await;
    db.insert_refinement_proposal(
        "dedup",
        "dedup_merge",
        &["left".into(), "right".into()],
        None,
        0.8,
    )
    .await
    .unwrap();

    // When
    assert_eq!(process(&db).await, 1);
    let report = run_at(&db, &[], chrono::Utc::now().timestamp()).await;

    // Then
    assert_eq!(
        db.get_refinement_proposal("dedup")
            .await
            .unwrap()
            .unwrap()
            .status,
        "dismissed"
    );
    assert_eq!(check(&report, REFINEMENTS).outcome(), LintOutcome::Pass);
}

#[tokio::test]
async fn daemon_promoted_suggest_entity_is_valid_inventory() {
    // Given
    let (db, _tmp) = test_db().await;
    db.insert_refinement_proposal(
        "suggest",
        "suggest_entity",
        &["memory".into()],
        Some("Entity"),
        0.8,
    )
    .await
    .unwrap();

    // When
    assert_eq!(process(&db).await, 1);
    let report = run_at(&db, &[], chrono::Utc::now().timestamp()).await;

    // Then
    assert_eq!(
        db.get_refinement_proposal("suggest")
            .await
            .unwrap()
            .unwrap()
            .status,
        "awaiting_review"
    );
    assert_eq!(check(&report, REFINEMENTS).outcome(), LintOutcome::Pass);
}

#[tokio::test]
async fn producer_shaped_consolidate_duplicate_stays_valid_pending_inventory() {
    // Given
    let (db, _tmp) = test_db().await;
    db.insert_refinement_proposal(
        "consolidate",
        "consolidate_duplicate",
        &["incoming".into(), "duplicate".into()],
        None,
        0.85,
    )
    .await
    .unwrap();

    // When
    assert_eq!(process(&db).await, 0);
    let report = run_at(&db, &[], chrono::Utc::now().timestamp()).await;

    // Then
    assert_eq!(
        db.get_refinement_proposal("consolidate")
            .await
            .unwrap()
            .unwrap()
            .status,
        "pending"
    );
    let result = check(&report, REFINEMENTS);
    assert_eq!(result.outcome(), LintOutcome::Pass);
    assert_eq!(metric(result, LintMetricCode::OperationPending), 1);
}
