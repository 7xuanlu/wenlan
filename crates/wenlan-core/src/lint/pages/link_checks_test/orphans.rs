use super::*;

#[tokio::test]
async fn totals_and_truncation_are_exact_at_zero_hundred_and_101() {
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
async fn scope_is_anchored_to_active_source_workspace() {
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
async fn query_failure_propagates_without_a_mutating_fallback() {
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
