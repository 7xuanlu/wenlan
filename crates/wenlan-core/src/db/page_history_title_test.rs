// SPDX-License-Identifier: Apache-2.0

use super::{tests::test_db, tests::test_db_at, MemoryDB};
use std::sync::Arc;

async fn page_titles(db: &MemoryDB, page_id: &str) -> Vec<super::PageHistoryEntry> {
    db.list_page_history(page_id, 100).await.unwrap()
}

async fn insert_legacy_page(
    conn: &libsql::Connection,
    id: &str,
    title: &str,
    version: i64,
    content: &str,
    source_memory_ids: &str,
) {
    conn.execute(
        "INSERT INTO pages
             (id,title,content,space,workspace,source_memory_ids,version,status,
              created_at,last_compiled,last_modified,creation_kind,review_status)
         VALUES (?1,?2,?3,'history-space','history-space',?4,?5,'active',
                 '2026-01-01T00:00:00Z','2026-01-01T00:00:00Z',
                 '2026-01-01T00:00:00Z','authored','confirmed')",
        libsql::params![id, title, content, source_memory_ids, version],
    )
    .await
    .unwrap();
}

async fn insert_legacy_history(
    conn: &libsql::Connection,
    page_id: &str,
    version: i64,
    content: &str,
    source_memory_ids: &str,
) {
    conn.execute(
        "INSERT INTO page_history
             (page_id,version,content,source_memory_ids,edited_by,created_at)
         VALUES (?1,?2,?3,?4,'legacy_fixture',1)",
        libsql::params![page_id, version, content, source_memory_ids],
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn migration_133_backfills_only_an_exact_current_snapshot_and_preserves_known_titles() {
    let (db, dir) = test_db_at(132).await;
    {
        let conn = db.conn.lock().await;
        conn.execute(
            "INSERT INTO spaces(id,name,created_at,updated_at)
             VALUES ('history-space','history-space',1,1)",
            (),
        )
        .await
        .unwrap();
        insert_legacy_page(
            &conn,
            "history-current",
            "Present title",
            3,
            "present body",
            "[\"present-source\"]",
        )
        .await;
        insert_legacy_history(
            &conn,
            "history-current",
            1,
            "older body",
            "[\"older-source\"]",
        )
        .await;
        insert_legacy_history(
            &conn,
            "history-current",
            3,
            "present body",
            "[\"present-source\"]",
        )
        .await;

        insert_legacy_page(
            &conn,
            "history-content-mismatch",
            "Current content title",
            5,
            "live body",
            "[]",
        )
        .await;
        insert_legacy_history(
            &conn,
            "history-content-mismatch",
            5,
            "different old body",
            "[]",
        )
        .await;
        insert_legacy_page(
            &conn,
            "history-source-mismatch",
            "Current source title",
            6,
            "same body",
            "[\"live-source\"]",
        )
        .await;
        insert_legacy_history(
            &conn,
            "history-source-mismatch",
            6,
            "same body",
            "[\"old-source\"]",
        )
        .await;
    }
    drop(db);

    let db = MemoryDB::new(dir.path(), Arc::new(crate::events::NoopEmitter))
        .await
        .unwrap();
    let current = page_titles(&db, "history-current").await;
    assert_eq!(current.len(), 2);
    assert_eq!(current[0].version, 3);
    assert_eq!(current[0].title.as_deref(), Some("Present title"));
    assert_eq!(current[0].content, "present body");
    assert_eq!(current[1].version, 1);
    assert_eq!(current[1].title, None);
    assert_eq!(
        page_titles(&db, "history-content-mismatch").await[0].title,
        None
    );
    assert_eq!(
        page_titles(&db, "history-source-mismatch").await[0].title,
        None
    );

    // Simulate an ALTER/backfill that committed before the migration stamp.
    // A known title is never overwritten by a retry.
    {
        let conn = db.conn.lock().await;
        conn.execute(
            "UPDATE page_history SET title='Known preserved title'
              WHERE page_id='history-current' AND version=3",
            (),
        )
        .await
        .unwrap();
        conn.execute("PRAGMA user_version=132", ()).await.unwrap();
    }
    drop(db);
    let db = MemoryDB::new(dir.path(), Arc::new(crate::events::NoopEmitter))
        .await
        .unwrap();
    let current = page_titles(&db, "history-current").await;
    assert_eq!(current[0].title.as_deref(), Some("Known preserved title"));
    assert_eq!(current[1].title, None);
    assert_eq!(
        page_titles(&db, "history-content-mismatch").await[0].title,
        None
    );
    assert_eq!(
        page_titles(&db, "history-source-mismatch").await[0].title,
        None
    );
}

#[tokio::test]
async fn page_history_records_creation_and_content_edit_titles() {
    let (db, _dir) = test_db().await;
    let now = chrono::Utc::now().to_rfc3339();
    db.insert_page(
        "history-write-paths",
        "Initial title",
        None,
        "Initial body",
        None,
        None,
        &[],
        &now,
    )
    .await
    .unwrap();
    db.update_page_content("history-write-paths", "Edited body", &[], "test_edit")
        .await
        .unwrap();

    let created_and_edited = page_titles(&db, "history-write-paths").await;
    assert_eq!(created_and_edited.len(), 2);
    assert_eq!(created_and_edited[0].version, 2);
    assert_eq!(
        created_and_edited[0].title.as_deref(),
        Some("Initial title")
    );
    assert_eq!(created_and_edited[0].content, "Edited body");
    assert_eq!(created_and_edited[1].version, 1);
    assert_eq!(
        created_and_edited[1].title.as_deref(),
        Some("Initial title")
    );
    assert_eq!(created_and_edited[1].content, "Initial body");
}

#[tokio::test]
async fn rolled_back_page_history_title_is_not_visible() {
    let (db, _dir) = test_db().await;
    let now = chrono::Utc::now().to_rfc3339();
    db.insert_page(
        "history-rollback",
        "Before rollback",
        None,
        "Stable body",
        None,
        None,
        &[],
        &now,
    )
    .await
    .unwrap();
    let before = page_titles(&db, "history-rollback").await;
    {
        let conn = db.conn.lock().await;
        conn.execute("BEGIN IMMEDIATE", ()).await.unwrap();
        conn.execute(
            "UPDATE pages SET title='Uncommitted title',version=version+1
              WHERE id='history-rollback'",
            (),
        )
        .await
        .unwrap();
        MemoryDB::append_page_history(
            &conn,
            "history-rollback",
            "test_rollback",
            chrono::Utc::now().timestamp(),
        )
        .await
        .unwrap();
        conn.execute("ROLLBACK", ()).await.unwrap();
    }
    assert_eq!(page_titles(&db, "history-rollback").await, before);
    assert_eq!(
        db.get_page("history-rollback")
            .await
            .unwrap()
            .unwrap()
            .title,
        "Before rollback"
    );
}
