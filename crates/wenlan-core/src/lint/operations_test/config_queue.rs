use super::{check, metric, run, source, NOW, QUEUE, SOURCE_CONFIG};
use crate::db::tests::test_db;
use crate::lint::test_support::assert_no_privacy_canaries;
use wenlan_types::lint::{LintMetricCode, LintOutcome};
use wenlan_types::sources::SyncStatus;

#[tokio::test]
async fn source_config_failures_are_findings_with_redacted_evidence() {
    // Given
    let (db, _tmp) = test_db().await;
    let sources = vec![
        source(
            "duplicate",
            "/private/source/CANARY_IMPORT_7f31.md",
            SyncStatus::Active,
        ),
        source(
            "duplicate",
            "private-host-7f31.invalid",
            SyncStatus::Error("nested error at /private/CANARY_ERROR_7f31".into()),
        ),
    ];

    // When
    let report = run(&db, &sources).await;

    // Then
    let result = check(&report, SOURCE_CONFIG);
    assert_eq!(result.outcome(), LintOutcome::Finding);
    assert_eq!(
        metric(result, LintMetricCode::SourceInvalidConfigurations),
        2
    );
    assert_eq!(metric(result, LintMetricCode::SourceTerminalFailures), 1);
    assert_no_privacy_canaries(&serde_json::to_string(&report).unwrap());
}

#[tokio::test]
async fn retry_boundary_and_queue_ages_use_closed_buckets() {
    // Given
    let (db, _tmp) = test_db().await;
    let conn = db.conn.lock().await;
    for (id, retry_at, enqueued_at) in
        [("active", NOW + 1, NOW - 10), ("expired", NOW, NOW - 3_600)]
    {
        conn.execute(
            "INSERT INTO document_enrichment_queue
             (source_id,file_path,status,last_completed_chunk,attempt_count,next_retry_at,error_detail,enqueued_at,updated_at)
             VALUES (?1,?1,'paused',0,1,?2,'CANARY_ENV_VALUE_7f31',?3,?3)",
            libsql::params![id, retry_at, enqueued_at],
        ).await.unwrap();
    }
    conn.execute(
        "INSERT INTO document_enrichment_queue
         (source_id,file_path,status,last_completed_chunk,attempt_count,enqueued_at,updated_at)
         VALUES ('old','old','pending',-1,0,?1,?1)",
        libsql::params![NOW - 604_800],
    )
    .await
    .unwrap();
    drop(conn);

    // When
    let report = run(&db, &[]).await;

    // Then
    let result = check(&report, QUEUE);
    assert_eq!(result.outcome(), LintOutcome::Finding);
    assert_eq!(metric(result, LintMetricCode::OperationActiveRetries), 1);
    assert_eq!(metric(result, LintMetricCode::OperationExpiredRetries), 1);
    assert_eq!(metric(result, LintMetricCode::OperationAgeUnderHour), 1);
    assert_eq!(metric(result, LintMetricCode::OperationAgeOneTo24Hours), 1);
    assert_eq!(
        metric(result, LintMetricCode::OperationAgeSevenDaysOrMore),
        1
    );
    let json = serde_json::to_string(&report).unwrap();
    assert!(!json.contains("age_seconds"));
    assert_no_privacy_canaries(&json);
}
