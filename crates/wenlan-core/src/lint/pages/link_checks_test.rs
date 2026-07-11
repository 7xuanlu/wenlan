use super::*;
use crate::db::tests::test_db;
use crate::lint::context::{
    AppliedScope, CancellationToken, ExecutionGate, LintClock, LintContext,
};
use crate::lint::pages::fs::scan_page_root;
use crate::lint::runner::LintRunner;
use crate::lint::snapshot::LintReadSnapshot;
use std::fs;
use std::path::Path;
use tempfile::TempDir;
use wenlan_types::lint::{
    LintApplicability, LintMetricCode, LintMetricValue, LintOpaqueId, LintOutcome, LintQuery,
    LintSeverity,
};

#[tokio::test]
async fn orphan_totals_and_truncation_are_exact_at_zero_hundred_and_101() {
    for (count, truncated) in [(0_u64, false), (100, false), (101, true)] {
        let (db, _tmp) = test_db().await;
        let conn = db._db.connect().unwrap();
        insert_page(&conn, "page-source", Some("workspace-a"), "active").await;
        for ordinal in 0..count {
            conn.execute(
                "INSERT INTO page_links (source_page_id, target_page_id, label_key, label) \
                 VALUES ('page-source', NULL, ?1, ?1)",
                libsql::params![format!("private-label-{ordinal:03}")],
            )
            .await
            .unwrap();
        }
        let before = link_row_count(&conn).await;
        let snapshot = db.open_lint_snapshot().await.unwrap();
        let scope = AppliedScope::global();
        let clock = LintClock::fixed();
        let gate = ExecutionGate::new(CancellationToken::new());
        let context = LintContext::new(&snapshot, &scope, None, &clock, &gate);
        let result = load_orphans(&context)
            .await
            .unwrap()
            .result(ORPHAN_LABELS_ID, 0)
            .unwrap();
        assert_eq!(result.coverage().denominator(), count);
        assert_eq!(result.coverage().evaluated(), count);
        assert_eq!(result.coverage().truncated(), truncated);
        assert_eq!(
            result.evidence().len(),
            usize::try_from(count.min(100)).unwrap()
        );
        assert_eq!(
            result.outcome(),
            if count == 0 {
                LintOutcome::Pass
            } else {
                LintOutcome::Finding
            }
        );
        if count > 0 {
            assert_eq!(result.severity(), LintSeverity::Warning);
        }
        assert_eq!(
            metric_value(&result, LintMetricCode::PageOrphanLabels),
            count
        );
        assert!(!serde_json::to_string(&result)
            .unwrap()
            .contains("private-label"));
        drop(snapshot);
        assert_eq!(link_row_count(&conn).await, before);
    }
}

#[tokio::test]
async fn orphan_scope_is_anchored_to_active_source_workspace() {
    let (db, _tmp) = test_db().await;
    let conn = db._db.connect().unwrap();
    insert_page(&conn, "page-a", Some("workspace-a"), "active").await;
    insert_page(&conn, "page-b", Some("workspace-b"), "active").await;
    insert_page(&conn, "page-archived", Some("workspace-a"), "archived").await;
    for (page, label) in [
        ("page-a", "private-a"),
        ("page-b", "private-b"),
        ("page-archived", "private-archived"),
    ] {
        conn.execute(
            "INSERT INTO page_links (source_page_id, target_page_id, label_key, label) \
             VALUES (?1, NULL, ?2, ?2)",
            libsql::params![page, label],
        )
        .await
        .unwrap();
    }
    let snapshot = db.open_lint_snapshot().await.unwrap();
    let scope = AppliedScope::registered(
        LintOpaqueId::from_sorted_position(0).unwrap(),
        "workspace-a".to_string(),
    );
    let clock = LintClock::fixed();
    let gate = ExecutionGate::new(CancellationToken::new());
    let context = LintContext::new(&snapshot, &scope, None, &clock, &gate);
    let result = load_orphans(&context)
        .await
        .unwrap()
        .result(ORPHAN_LABELS_ID, 0)
        .unwrap();
    assert_eq!(result.coverage().denominator(), 1);
}

#[tokio::test]
async fn orphan_query_failure_propagates_without_a_mutating_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let database = libsql::Builder::new_local(dir.path().join("empty.db"))
        .build()
        .await
        .unwrap();
    let snapshot = LintReadSnapshot::open(&database).await.unwrap();
    let scope = AppliedScope::global();
    let clock = LintClock::fixed();
    let gate = ExecutionGate::new(CancellationToken::new());
    let context = LintContext::new(&snapshot, &scope, None, &clock, &gate);
    assert!(load_orphans(&context).await.is_err());
}

#[test]
fn manifest_divergence_is_inventory_but_invalid_json_is_error() {
    let root = TempDir::new().unwrap();
    write(
        root.path(),
        "_sources/.manifest.json",
        br#"{"pages":{"page-private":["mem-expected"]}}"#,
    );
    write(root.path(), "_sources/mem-extra.md", b"stub");
    let scan = scan_page_root(root.path()).unwrap();
    let result = assess_manifest(&scan, false)
        .result(MANIFEST_ID, 0)
        .unwrap();
    assert_eq!(result.outcome(), LintOutcome::Pass);
    assert_eq!(result.applicability(), LintApplicability::Inventory);
    assert_eq!(metric_value(&result, LintMetricCode::PageManifestPages), 1);
    assert_eq!(
        metric_value(&result, LintMetricCode::PageManifestDivergences),
        2
    );
    assert!(!serde_json::to_string(&result).unwrap().contains("private"));

    write(root.path(), "_sources/.manifest.json", b"{");
    let invalid = scan_page_root(root.path()).unwrap();
    let result = assess_manifest(&invalid, false)
        .result(MANIFEST_ID, 0)
        .unwrap();
    assert_eq!(result.outcome(), LintOutcome::Finding);
    assert_eq!(result.severity(), LintSeverity::Error);
}

#[cfg(unix)]
#[test]
fn source_symlink_escape_is_a_manifest_containment_error() {
    use std::os::unix::fs::symlink;

    let root = TempDir::new().unwrap();
    fs::create_dir_all(root.path().join("_sources")).unwrap();
    symlink(
        "../../private-outside",
        root.path().join("_sources/escape.md"),
    )
    .unwrap();
    let scan = scan_page_root(root.path()).unwrap();
    let result = assess_manifest(&scan, false)
        .result(MANIFEST_ID, 0)
        .unwrap();
    assert_eq!(result.outcome(), LintOutcome::Finding);
    assert_eq!(result.severity(), LintSeverity::Error);
    assert!(!serde_json::to_string(&result)
        .unwrap()
        .contains("private-outside"));
}

#[test]
fn oversized_manifest_is_bounded_and_classified_as_invalid() {
    let root = TempDir::new().unwrap();
    write(
        root.path(),
        "_sources/.manifest.json",
        &vec![b'x'; 1024 * 1024 + 1],
    );
    let scan = scan_page_root(root.path()).unwrap();
    let result = assess_manifest(&scan, false)
        .result(MANIFEST_ID, 0)
        .unwrap();
    assert_eq!(result.outcome(), LintOutcome::Finding);
    assert_eq!(result.severity(), LintSeverity::Error);
}

#[cfg(unix)]
#[tokio::test]
async fn selected_runner_keeps_global_manifest_errors_without_evidence() {
    use std::os::unix::fs::symlink;

    let (db, _tmp) = test_db().await;
    db.conn
        .lock()
        .await
        .execute(
            "INSERT INTO spaces (id, name, created_at, updated_at) \
             VALUES ('lint-space', 'alpha', 1, 1)",
            (),
        )
        .await
        .unwrap();
    let root = TempDir::new().unwrap();
    fs::create_dir_all(root.path().join("_sources")).unwrap();
    symlink(
        "../../private-outside",
        root.path().join("_sources/escape.md"),
    )
    .unwrap();
    let report = LintRunner::new(LintClock::fixed(), CancellationToken::new())
        .run(
            &db,
            &LintQuery {
                space: Some("alpha".to_string()),
            },
            Some(root.path()),
            true,
        )
        .await
        .unwrap();
    let manifest = report
        .checks()
        .iter()
        .find(|check| check.check_id() == MANIFEST_ID)
        .unwrap();
    assert_eq!(manifest.outcome(), LintOutcome::Finding);
    assert_eq!(manifest.severity(), LintSeverity::Error);
    assert!(manifest.evidence().is_empty());
}

#[tokio::test]
async fn optional_project_artifacts_are_neutral_inventory_with_structural_counts() {
    let (db, _tmp) = test_db().await;
    let conn = db._db.connect().unwrap();
    insert_page(&conn, "page-active", None, "active").await;
    insert_page(&conn, "page-target", None, "active").await;
    insert_page(&conn, "page-archived-target", None, "archived").await;
    conn.execute(
        "INSERT INTO page_links (source_page_id, target_page_id, label_key, label) \
         VALUES ('page-active', 'page-target', 'resolved', 'resolved'), \
                ('page-active', 'page-missing', 'missing', 'missing'), \
                ('page-active', 'page-archived-target', 'archived', 'archived'), \
                ('page-active', NULL, 'unresolved', 'unresolved')",
        (),
    )
    .await
    .unwrap();
    let root = TempDir::new().unwrap();
    write(root.path(), "purpose.md", b"purpose");
    write(root.path(), "wiki/_index.md", b"index");
    write(root.path(), "wiki/Overview.md", b"overview");
    write(root.path(), "_sources/mem-one.md", b"stub");
    let scan = scan_page_root(root.path()).unwrap();
    let counts = ArtifactCounts::from_scan(&scan);
    assert_eq!(counts.purpose, 1);
    assert_eq!(counts.schema, 0);
    assert_eq!(counts.index, 1);
    assert_eq!(counts.log, 0);
    assert_eq!(counts.overview, 1);
    assert_eq!(counts.source_stubs, 1);

    let snapshot = db.open_lint_snapshot().await.unwrap();
    let scope = AppliedScope::global();
    let clock = LintClock::fixed();
    let gate = ExecutionGate::new(CancellationToken::new());
    let context = LintContext::new(&snapshot, &scope, Some(&scan), &clock, &gate);
    let result = load_artifacts(&context)
        .await
        .unwrap()
        .result(ARTIFACT_ID, 0)
        .unwrap();
    assert_eq!(result.outcome(), LintOutcome::Finding);
    assert_eq!(result.severity(), LintSeverity::Warning);
    assert_eq!(
        metric_value(&result, LintMetricCode::ProjectArchiveRecords),
        1
    );
    assert_eq!(
        metric_value(&result, LintMetricCode::ProjectOutboundLinks),
        4
    );
    assert_eq!(
        metric_value(&result, LintMetricCode::ProjectInboundLinks),
        1
    );
    assert_eq!(metric_value(&result, LintMetricCode::ProjectBrokenLinks), 2);
}

async fn insert_page(conn: &libsql::Connection, id: &str, workspace: Option<&str>, status: &str) {
    conn.execute(
        "INSERT INTO pages (id, title, content, source_memory_ids, version, status, created_at, last_compiled, last_modified, workspace, creation_kind, review_status) \
         VALUES (?1, ?1, 'body', '[]', 1, ?3, 'now', 'now', 'now', ?2, 'distilled', 'confirmed')",
        libsql::params![id, workspace, status],
    )
    .await
    .unwrap();
}

async fn link_row_count(conn: &libsql::Connection) -> i64 {
    let mut rows = conn
        .query("SELECT COUNT(*) FROM page_links", ())
        .await
        .unwrap();
    rows.next().await.unwrap().unwrap().get(0).unwrap()
}

fn metric_value(result: &LintCheckResult, code: LintMetricCode) -> u64 {
    result
        .metrics()
        .iter()
        .find_map(|metric| {
            if metric.code() != code {
                return None;
            }
            match metric.value() {
                LintMetricValue::Count { value } => Some(*value),
                _ => None,
            }
        })
        .unwrap()
}

fn write(root: &Path, relative: &str, bytes: &[u8]) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, bytes).unwrap();
}
