use super::*;
use crate::db::tests::test_db;
use crate::lint::context::{
    AppliedScope, CancellationToken, ExecutionGate, LintClock, LintContext,
};
use wenlan_types::lint::{LintApplicability, LintOpaqueId, LintOutcome, LintSeverity};

fn page(title: &str, status: &str, creation: &str, review: &str) -> PageRow {
    PageRow {
        title_key: title.to_lowercase(),
        status: status.to_string(),
        creation_kind: creation.to_string(),
        review_status: review.to_string(),
    }
}

#[test]
fn cartesian_partitions_and_all_unconfirmed_kinds_are_exact_inventory() {
    let mut rows = Vec::new();
    for status in STATUSES {
        for creation in CREATION_KINDS {
            for review in REVIEW_STATUSES {
                rows.push(page(
                    &format!("{status}-{creation}-{review}"),
                    status,
                    creation,
                    review,
                ));
            }
        }
    }
    let partitions = Partitions::from_rows(&rows);
    assert!(partitions.is_exact(20));
    assert!(!partitions.is_exact(19));
    let partition_result = assess_partitions(&rows).result(PARTITIONS_ID, 0).unwrap();
    assert_eq!(partition_result.outcome(), LintOutcome::Pass);
    assert_eq!(partition_result.coverage().denominator(), 20);
    let review_result = assess_review(&rows).result(REVIEW_ID, 0).unwrap();
    assert_eq!(review_result.outcome(), LintOutcome::Pass);
    assert_eq!(review_result.applicability(), LintApplicability::Inventory);
    assert_eq!(review_result.coverage().denominator(), 20);
    for creation in CREATION_KINDS {
        let one = [page("private", "active", creation, "unconfirmed")];
        assert_eq!(
            assess_review(&one).result(REVIEW_ID, 0).unwrap().outcome(),
            LintOutcome::Pass
        );
    }
}

#[test]
fn unknown_storage_values_are_error_findings() {
    for row in [
        page("a", "future", "distilled", "confirmed"),
        page("b", "active", "future", "confirmed"),
        page("c", "active", "distilled", "future"),
    ] {
        let partition = assess_partitions(std::slice::from_ref(&row))
            .result(PARTITIONS_ID, 0)
            .unwrap();
        assert_eq!(partition.outcome(), LintOutcome::Finding);
        assert_eq!(partition.severity(), LintSeverity::Error);
    }
}

#[test]
fn active_duplicates_warn_but_archived_rows_remain_inventory() {
    let rows = [
        page("same", "active", "distilled", "confirmed"),
        page("SAME", "active", "authored", "unconfirmed"),
        page("same", "archived", "research", "unconfirmed"),
    ];
    let duplicates = assess_duplicates(&rows)
        .result(DUPLICATE_TITLES_ID, 0)
        .unwrap();
    assert_eq!(duplicates.outcome(), LintOutcome::Finding);
    assert_eq!(duplicates.severity(), LintSeverity::Warning);
    assert_eq!(duplicates.coverage().denominator(), 2);
    let archive = assess_archive(&rows).result(ARCHIVE_ID, 0).unwrap();
    assert_eq!(archive.outcome(), LintOutcome::Pass);
    assert_eq!(archive.applicability(), LintApplicability::Inventory);
    assert_eq!(archive.coverage().denominator(), 1);
}

#[tokio::test]
async fn workspace_scope_prevents_cross_workspace_duplicate_and_title_leak() {
    let (db, _tmp) = test_db().await;
    let conn = db._db.connect().unwrap();
    insert_page(
        &conn,
        "page-a",
        "Private Shared Title",
        "active",
        Some("workspace-a"),
    )
    .await;
    insert_page(
        &conn,
        "page-b",
        "private shared title",
        "active",
        Some("workspace-b"),
    )
    .await;
    insert_page(
        &conn,
        "page-archived",
        "Private Shared Title",
        "archived",
        Some("workspace-a"),
    )
    .await;
    insert_page(&conn, "page-none", "uncategorized", "active", None).await;
    let snapshot = db.open_lint_snapshot().await.unwrap();
    let clock = LintClock::fixed();
    let gate = ExecutionGate::new(CancellationToken::new());
    let selected_scope = AppliedScope::registered(
        LintOpaqueId::from_sorted_position(0).unwrap(),
        "workspace-a".to_string(),
    );
    let selected_context = LintContext::new(
        &snapshot,
        &selected_scope,
        None,
        &clock,
        &gate,
        wenlan_types::lint::LintProfile::General,
    );
    let selected = load_rows(&selected_context).await.unwrap();
    assert_eq!(selected.len(), 2);
    let selected_result = assess_duplicates(&selected)
        .result(DUPLICATE_TITLES_ID, 0)
        .unwrap();
    assert_eq!(selected_result.outcome(), LintOutcome::Pass);
    assert_eq!(selected_result.coverage().denominator(), 1);

    let global_scope = AppliedScope::global();
    let global_context = LintContext::new(
        &snapshot,
        &global_scope,
        None,
        &clock,
        &gate,
        wenlan_types::lint::LintProfile::General,
    );
    let global = load_rows(&global_context).await.unwrap();
    let global_result = assess_duplicates(&global)
        .result(DUPLICATE_TITLES_ID, 0)
        .unwrap();
    assert_eq!(global_result.outcome(), LintOutcome::Finding);
    assert_eq!(global_result.severity(), LintSeverity::Warning);
    let json = serde_json::to_string(&global_result).unwrap();
    assert!(!json.contains("Private"));
    assert!(!json.contains("private shared"));

    let uncategorized_scope = AppliedScope::uncategorized();
    let uncategorized_context = LintContext::new(
        &snapshot,
        &uncategorized_scope,
        None,
        &clock,
        &gate,
        wenlan_types::lint::LintProfile::General,
    );
    let uncategorized = load_rows(&uncategorized_context).await.unwrap();
    assert_eq!(uncategorized.len(), 1);
    assert_eq!(uncategorized[0].title_key, "uncategorized");
}

async fn insert_page(
    conn: &libsql::Connection,
    id: &str,
    title: &str,
    status: &str,
    workspace: Option<&str>,
) {
    conn.execute(
        "INSERT INTO pages (id, title, content, source_memory_ids, version, status, created_at, last_compiled, last_modified, workspace, creation_kind, review_status) VALUES (?1, ?2, 'body', '[]', 1, ?3, 'now', 'now', 'now', ?4, 'distilled', 'confirmed')",
        libsql::params![id, title, status, workspace],
    )
    .await
    .unwrap();
}
