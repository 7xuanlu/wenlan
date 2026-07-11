use super::{check, metric, run, IMPORTS, MAINTENANCE, NOW, REFINEMENTS, REJECTIONS};
use crate::db::tests::test_db;
use crate::lint::test_support::assert_no_privacy_canaries;
use wenlan_types::lint::{LintMetricCode, LintOutcome};

#[tokio::test]
async fn terminal_imports_and_every_review_state_are_inventory_or_findings() {
    // Given
    let (db, _tmp) = test_db().await;
    let conn = db.conn.lock().await;
    for (index, stage) in ["parsing", "stage_a", "stage_b", "done", "error"]
        .iter()
        .enumerate()
    {
        conn.execute(
            "INSERT INTO import_state
             (id,vendor,source_path,total_conversations,processed_conversations,stage,error_message,started_at,updated_at)
             VALUES (?1,'claude','/Users/canary/CANARY_HOME_7f31',10,?2,?3,?4,'2023-11-14T22:13:20Z','2023-11-14T22:13:20Z')",
            libsql::params![format!("import-{index}"), i64::try_from(index).unwrap(), *stage,
                (*stage == "error").then_some("nested error at /private/CANARY_ERROR_7f31")],
        ).await.unwrap();
    }
    for (index, status) in [
        "pending",
        "awaiting_review",
        "auto_applied",
        "resolved",
        "dismissed",
    ]
    .iter()
    .enumerate()
    {
        conn.execute(
            "INSERT INTO refinement_queue (id,action,source_ids,status,created_at) VALUES (?1,'detect_contradiction','[]',?2,'2023-11-14 22:13:20')",
            libsql::params![format!("ref-{index}"), *status],
        ).await.unwrap();
    }
    conn.execute(
        "INSERT INTO rejected_memories (id,content,rejection_reason,rejection_detail,created_at)
         VALUES ('rej','CANARY_MEMORY_CONTENT_7f31','noise_pattern','CANARY_SESSION_SUMMARY_7f31',?1)",
        libsql::params![NOW],
    ).await.unwrap();
    drop(conn);

    // When
    let report = run(&db, &[]).await;

    // Then
    assert_eq!(check(&report, IMPORTS).outcome(), LintOutcome::Finding);
    assert_eq!(
        metric(
            check(&report, IMPORTS),
            LintMetricCode::OperationTerminalFailures
        ),
        1
    );
    assert_eq!(
        metric(
            check(&report, REFINEMENTS),
            LintMetricCode::OperationPending
        ),
        1
    );
    assert_eq!(
        metric(
            check(&report, REFINEMENTS),
            LintMetricCode::OperationAwaitingReview
        ),
        1
    );
    assert_eq!(
        metric(
            check(&report, REFINEMENTS),
            LintMetricCode::OperationTerminal
        ),
        3
    );
    assert_eq!(check(&report, REJECTIONS).outcome(), LintOutcome::Pass);
    assert_eq!(
        metric(check(&report, REJECTIONS), LintMetricCode::ObservedRecords),
        1
    );
    assert_no_privacy_canaries(&serde_json::to_string(&report).unwrap());
}

#[tokio::test]
async fn durable_failure_oracle_finds_no_progress_and_absence_is_a_limitation() {
    // Given
    let (db, _tmp) = test_db().await;

    // When
    let limited = run(&db, &[]).await;
    db.conn.lock().await.execute_batch(
        "INSERT INTO app_metadata(key,value) VALUES
         ('reconcile_frontier_docs','{\"ts\":1,\"id\":\"opaque\",\"chunk\":0,\"stuck_id\":\"private-host-7f31.invalid\",\"failures\":2}'),
         ('compile_queue_depth_v1','7'),
         ('last_daily_steep_ts','1699996400');"
    ).await.unwrap();
    let proven = run(&db, &[]).await;

    // Then
    let limitation = check(&limited, MAINTENANCE);
    assert_eq!(limitation.outcome(), LintOutcome::Pass);
    assert_eq!(
        metric(limitation, LintMetricCode::OperationMissingProgressOracles),
        2
    );
    let finding = check(&proven, MAINTENANCE);
    assert_eq!(finding.outcome(), LintOutcome::Finding);
    assert_eq!(
        metric(finding, LintMetricCode::OperationDurableNoProgress),
        1
    );
    assert_eq!(metric(finding, LintMetricCode::PendingRecords), 7);
    assert_no_privacy_canaries(&serde_json::to_string(&proven).unwrap());
}
