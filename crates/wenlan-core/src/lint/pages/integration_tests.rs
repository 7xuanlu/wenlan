use super::{uses_cross_store, uses_filesystem};
use crate::db::tests::test_db;
use crate::lint::context::{CancellationToken, LintClock};
use crate::lint::runner::{LintRunner, TestScenario};
use crate::lint::test_support::{DbBytesFingerprint, PageBytesFingerprint};
use std::time::Duration;
use wenlan_types::lint::{LintOutcome, LintQuery};

#[tokio::test]
async fn active_writer_makes_only_cross_store_checks_incomplete_without_blocking() {
    let (db, _tmp) = test_db().await;
    let page_root = tempfile::tempdir().unwrap();
    let guard = db.begin_page_projection_write();
    let report = tokio::time::timeout(
        Duration::from_secs(2),
        LintRunner::new(LintClock::fixed(), CancellationToken::new()).run(
            &db,
            &LintQuery { space: None },
            Some(page_root.path()),
            true,
        ),
    )
    .await
    .expect("lint must not wait for an active Page writer")
    .unwrap();

    assert!(!report.complete());
    for check in report.checks() {
        if uses_cross_store(check.check_id()) {
            assert_eq!(check.outcome(), LintOutcome::InconsistentSnapshot);
        } else {
            assert_ne!(check.outcome(), LintOutcome::InconsistentSnapshot);
        }
    }
    drop(guard);
    assert!(!db.page_projection_tracker().sample().has_active_writes());
}

#[tokio::test]
async fn completed_writer_generation_drift_is_detected_between_receipts() {
    let (db, _tmp) = test_db().await;
    let page_root = tempfile::tempdir().unwrap();
    let report = LintRunner::new(LintClock::fixed(), CancellationToken::new())
        .with_test_scenario(TestScenario::ProjectionGenerationDrift)
        .run(
            &db,
            &LintQuery { space: None },
            Some(page_root.path()),
            true,
        )
        .await
        .unwrap();
    assert!(!report.complete());
    assert!(report.checks().iter().all(|check| {
        !uses_cross_store(check.check_id()) || check.outcome() == LintOutcome::InconsistentSnapshot
    }));
}

#[tokio::test]
async fn guard_completion_is_nonblocking_and_clears_active_state() {
    let (db, _tmp) = test_db().await;
    let tracker = db.page_projection_tracker();
    let guard = tracker.begin_write();
    let task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(10)).await;
        drop(guard);
    });
    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .expect("writer guard completes within two seconds")
        .unwrap();
    assert!(!tracker.sample().has_active_writes());
    assert_eq!(tracker.sample().generation(), 1);
}

#[tokio::test]
async fn stable_repeated_run_is_byte_identical_and_keeps_all_checks_conclusive() {
    let (db, _tmp) = test_db().await;
    let page_root = tempfile::tempdir().unwrap();
    let first = LintRunner::new(LintClock::fixed(), CancellationToken::new())
        .run(
            &db,
            &LintQuery { space: None },
            Some(page_root.path()),
            true,
        )
        .await
        .unwrap();
    let second = LintRunner::new(LintClock::fixed(), CancellationToken::new())
        .run(
            &db,
            &LintQuery { space: None },
            Some(page_root.path()),
            true,
        )
        .await
        .unwrap();
    assert!(first
        .checks()
        .iter()
        .all(|check| check.outcome() != LintOutcome::InconsistentSnapshot));
    assert_eq!(
        serde_json::to_vec(&first).unwrap(),
        serde_json::to_vec(&second).unwrap()
    );
}

#[tokio::test]
async fn full_page_group_does_not_mutate_database_or_projection_tree() {
    let (db, db_root) = test_db().await;
    let page_root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(page_root.path().join(".wenlan")).unwrap();
    std::fs::write(
        page_root.path().join(".wenlan/state.json"),
        b"{\"schema_version\":2,\"pages\":{}}",
    )
    .unwrap();
    std::fs::create_dir_all(page_root.path().join("_sources")).unwrap();
    std::fs::write(
        page_root.path().join("_sources/.manifest.json"),
        b"{\"pages\":{}}",
    )
    .unwrap();
    let db_before = DbBytesFingerprint::capture(db_root.path()).unwrap();
    let page_before = PageBytesFingerprint::capture(page_root.path()).unwrap();

    for _ in 0..2 {
        LintRunner::new(LintClock::fixed(), CancellationToken::new())
            .run(
                &db,
                &LintQuery { space: None },
                Some(page_root.path()),
                true,
            )
            .await
            .unwrap();
    }

    assert_eq!(
        db_before,
        DbBytesFingerprint::capture(db_root.path()).unwrap()
    );
    assert_eq!(
        page_before,
        PageBytesFingerprint::capture(page_root.path()).unwrap()
    );
}

#[tokio::test]
async fn knowledge_writer_rejects_a_guard_from_another_tracker() {
    let (db, _tmp) = test_db().await;
    let page_root = tempfile::tempdir().unwrap();
    let writer = crate::export::knowledge::KnowledgeWriter::new(
        page_root.path().to_path_buf(),
        db.page_projection_tracker(),
    );
    let other = crate::page_projection_tracker::PageProjectionTracker::new();
    let wrong_guard = other.begin_write();
    let error = writer
        .remove_page(&wrong_guard, "page-private")
        .unwrap_err();
    assert!(error.to_string().contains("guard"));
}

#[test]
fn page_check_store_classification_is_closed_over_the_catalog() {
    for id in [
        "pages.projection.identity",
        "pages.projection.version_alignment",
        "pages.project.artifact_inventory",
    ] {
        assert!(uses_cross_store(id));
        assert!(uses_filesystem(id));
    }
    for id in [
        "pages.projection.state_contract",
        "pages.projection.manifest_inventory",
    ] {
        assert!(!uses_cross_store(id));
        assert!(uses_filesystem(id));
    }
    for id in [
        "pages.db.partitions",
        "pages.citations.partitions",
        "pages.links.orphan_labels",
    ] {
        assert!(!uses_cross_store(id));
        assert!(!uses_filesystem(id));
    }
}
