use super::tests::test_db;
use crate::read_scope::ReadScope;
use crate::sources::RawDocument;

fn activity_memory(source_id: &str, space: &str) -> RawDocument {
    RawDocument {
        source: "memory".to_string(),
        source_id: source_id.to_string(),
        title: source_id.to_string(),
        content: format!("activity owner {source_id}"),
        space: Some(space.to_string()),
        memory_type: Some("fact".to_string()),
        confirmed: Some(true),
        ..Default::default()
    }
}

#[tokio::test]
async fn scoped_chunks_propagate_database_failures() {
    let (db, _temp) = test_db().await;
    let conn = db.conn.lock().await;
    conn.execute_batch("DROP TABLE memories;").await.unwrap();
    drop(conn);

    let result = db.get_chunks_scoped("missing", &ReadScope::Global).await;

    assert!(
        result.is_err(),
        "query failure must not become an empty success"
    );
}

#[tokio::test]
async fn scoped_briefing_stats_propagate_database_failures() {
    let (db, _temp) = test_db().await;
    let conn = db.conn.lock().await;
    conn.execute_batch("DROP TABLE memories;").await.unwrap();
    drop(conn);

    let result = db
        .get_briefing_stats_scoped(
            chrono::Utc::now().timestamp() - 3_600,
            &ReadScope::Space("work".to_string()),
        )
        .await;

    assert!(result.is_err(), "query failure must not become empty stats");
}

#[tokio::test]
async fn scoped_activity_searches_past_the_initial_candidate_window() {
    let (db, _temp) = test_db().await;
    db.upsert_documents(vec![
        activity_memory("work-owner", "work"),
        activity_memory("personal-owner", "personal"),
    ])
    .await
    .unwrap();
    let conn = db.conn.lock().await;
    conn.execute_batch(
        "WITH RECURSIVE seq(value) AS (
             SELECT 1 UNION ALL SELECT value + 1 FROM seq WHERE value < 101
         )
         INSERT INTO agent_activity
             (timestamp, agent_name, action, memory_ids, query, detail)
         SELECT 1000 + value, 'codex', 'search', 'personal-owner',
                'personal-query', 'personal-detail'
           FROM seq;
         INSERT INTO agent_activity
             (timestamp, agent_name, action, memory_ids, query, detail)
         VALUES (1, 'codex', 'search', 'work-owner', 'work-query', 'work-detail');",
    )
    .await
    .unwrap();
    drop(conn);
    let scope = ReadScope::Space("work".to_string());

    let retrievals = db.list_recent_retrievals_scoped(1, &scope).await.unwrap();
    let activities = db
        .list_agent_activity_scoped(1, None, None, &scope)
        .await
        .unwrap();

    assert_eq!(retrievals.len(), 1);
    assert_eq!(retrievals[0].query.as_deref(), Some("work-query"));
    assert_eq!(activities.len(), 1);
    assert_eq!(activities[0].query.as_deref(), Some("work-query"));
}

#[tokio::test]
async fn malformed_activity_owner_is_a_controlled_error() {
    let (db, _temp) = test_db().await;
    db.upsert_documents(vec![activity_memory("corrupt-owner", "work")])
        .await
        .unwrap();
    let conn = db.conn.lock().await;
    conn.execute(
        "UPDATE memories SET space = x'FF' WHERE source_id = 'corrupt-owner'",
        (),
    )
    .await
    .unwrap();
    conn.execute(
        "INSERT INTO agent_activity
             (timestamp, agent_name, action, memory_ids, query, detail)
         VALUES (1, 'codex', 'search', 'corrupt-owner', 'corrupt-query', 'corrupt-detail')",
        (),
    )
    .await
    .unwrap();
    drop(conn);

    let result = db
        .list_agent_activity_scoped(1, None, None, &ReadScope::Space("work".to_string()))
        .await;

    assert!(result.is_err(), "malformed owner storage must fail safely");
}

#[tokio::test]
async fn malformed_page_activity_owner_fails_both_scoped_feeds_safely() {
    let (db, _temp) = test_db().await;
    let now = chrono::Utc::now().to_rfc3339();
    db.insert_page_with_kind(
        "corrupt-page-owner",
        "Corrupt page owner",
        None,
        "page activity owner canary",
        None,
        None,
        &[],
        &now,
        "authored",
        "confirmed",
        Some("work"),
        None,
    )
    .await
    .unwrap();
    let conn = db.conn.lock().await;
    conn.execute(
        "UPDATE pages SET workspace = x'FF' WHERE id = 'corrupt-page-owner'",
        (),
    )
    .await
    .unwrap();
    conn.execute(
        "INSERT INTO agent_activity
             (timestamp, agent_name, action, memory_ids, query, detail)
         VALUES (1, 'codex', 'search', 'corrupt-page-owner', 'corrupt-query', 'corrupt-detail')",
        (),
    )
    .await
    .unwrap();
    drop(conn);
    let scope = ReadScope::Space("work".to_string());

    let retrievals = db.list_recent_retrievals_scoped(1, &scope).await;
    let activities = db.list_agent_activity_scoped(1, None, None, &scope).await;

    assert!(retrievals.is_err(), "retrieval feed must fail safely");
    assert!(activities.is_err(), "activity feed must fail safely");
}
