// SPDX-License-Identifier: Apache-2.0

use super::source_annotations::SourceAnnotationWrite;
use crate::db::tests::test_db;
use crate::sources::RawDocument;
use std::sync::Arc;

async fn seed_memory_in_space(db: &super::MemoryDB, id: &str, space: &str) {
    let conn = db.conn.lock().await;
    conn.execute(
        "INSERT INTO memories (id, content, source, source_id, title, chunk_index, \
         last_modified, chunk_type, space) VALUES (?1, 'content', 'memory', ?1, 'title', 0, 1, \
         'text', ?2)",
        libsql::params![id, space],
    )
    .await
    .unwrap();
}

async fn memory_space(db: &super::MemoryDB, id: &str) -> String {
    let conn = db.conn.lock().await;
    let mut rows = conn
        .query(
            "SELECT space FROM memories WHERE id = ?1",
            libsql::params![id],
        )
        .await
        .unwrap();
    rows.next().await.unwrap().unwrap().get(0).unwrap()
}

fn webpage_document(source_id: &str, content: &str) -> RawDocument {
    RawDocument {
        source: "webpage".to_string(),
        source_id: source_id.to_string(),
        title: "Saved webpage".to_string(),
        content: content.to_string(),
        url: Some(source_id.to_string()),
        ..Default::default()
    }
}

#[tokio::test]
async fn migration_135_is_idempotent_and_keeps_existing_rows() {
    let (db, _temp) = test_db().await;
    let source_id = "https://example.test/articles/a-very-long-full-source-identity";
    let saved = db
        .write_source_annotation(
            "webpage",
            source_id,
            "global",
            0,
            Some("personal note"),
            None,
        )
        .await
        .unwrap();
    assert!(matches!(saved, SourceAnnotationWrite::Saved(_)));

    let conn = db.test_primary_session().await;
    conn.execute("PRAGMA user_version = 134", ()).await.unwrap();
    drop(conn);
    db.migrate_135_source_annotations().await.unwrap();
    db.migrate_135_source_annotations().await.unwrap();

    let annotation = db
        .get_source_annotation("webpage", source_id, "global")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(annotation.source_id, source_id);
    assert_eq!(annotation.notes, "personal note");
    assert_eq!(annotation.version, 1);
}

#[tokio::test]
async fn source_reingest_keeps_annotation_keyed_by_full_source_identity() {
    let (db, _temp) = test_db().await;
    let source_id =
        "https://example.test/articles/a-very-long-full-source-identity?lang=en#section";
    db.upsert_documents(vec![webpage_document(
        source_id,
        "original saved page content",
    )])
    .await
    .unwrap();
    db.write_source_annotation(
        "webpage",
        source_id,
        "global",
        0,
        Some("personal note"),
        None,
    )
    .await
    .unwrap();

    db.upsert_documents(vec![webpage_document(
        source_id,
        "refreshed saved page content",
    )])
    .await
    .unwrap();

    let annotation = db
        .get_source_annotation("webpage", source_id, "global")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(annotation.source_id, source_id);
    assert_eq!(annotation.notes, "personal note");
    assert_eq!(annotation.version, 1);
}

#[tokio::test]
async fn cas_patches_preserve_other_fields_clear_notes_and_reject_stale_writes() {
    let (db, _temp) = test_db().await;
    let page_ids = vec!["page-a".to_string()];
    let first = db
        .write_source_annotation(
            "webpage",
            "https://example.test/full/url/path?query=value",
            "global",
            0,
            Some("keep this note"),
            Some(&page_ids),
        )
        .await
        .unwrap();
    let SourceAnnotationWrite::Saved(first) = first else {
        panic!("initial CAS should save")
    };
    assert_eq!(
        first.source_id,
        "https://example.test/full/url/path?query=value"
    );

    let stale = db
        .write_source_annotation(
            "webpage",
            &first.source_id,
            "global",
            0,
            Some("stale"),
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        stale,
        SourceAnnotationWrite::VersionConflict { current_version: 1 }
    );

    let cleared = db
        .write_source_annotation("webpage", &first.source_id, "global", 1, Some(""), None)
        .await
        .unwrap();
    let SourceAnnotationWrite::Saved(cleared) = cleared else {
        panic!("clear should save")
    };
    assert_eq!(cleared.notes, "");
    assert_eq!(cleared.page_ids, page_ids);

    let list = vec!["page-b".to_string()];
    let changed = db
        .write_source_annotation("webpage", &first.source_id, "global", 2, None, Some(&list))
        .await
        .unwrap();
    let SourceAnnotationWrite::Saved(changed) = changed else {
        panic!("list patch should save")
    };
    assert_eq!(changed.notes, "");
    assert_eq!(changed.page_ids, list);
}

#[tokio::test]
async fn simultaneous_field_patches_have_one_winner_without_lost_update() {
    let (db, _temp) = test_db().await;
    let db = Arc::new(db);
    let first = db
        .write_source_annotation(
            "webpage",
            "https://example.test/a",
            "global",
            0,
            Some("old"),
            None,
        )
        .await
        .unwrap();
    assert!(matches!(first, SourceAnnotationWrite::Saved(_)));

    let left_db = Arc::clone(&db);
    let right_db = Arc::clone(&db);
    let left = tokio::spawn(async move {
        left_db
            .write_source_annotation(
                "webpage",
                "https://example.test/a",
                "global",
                1,
                Some("new note"),
                None,
            )
            .await
            .unwrap()
    });
    let right = tokio::spawn(async move {
        right_db
            .write_source_annotation(
                "webpage",
                "https://example.test/a",
                "global",
                1,
                None,
                Some(&["page-1".to_string()]),
            )
            .await
            .unwrap()
    });
    let results = [left.await.unwrap(), right.await.unwrap()];
    assert_eq!(
        results
            .iter()
            .filter(|value| matches!(value, SourceAnnotationWrite::Saved(_)))
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|value| matches!(value, SourceAnnotationWrite::VersionConflict { .. }))
            .count(),
        1
    );
    let current = db
        .get_source_annotation("webpage", "https://example.test/a", "global")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(current.version, 2);
}

#[tokio::test]
async fn annotation_context_key_is_independent_and_stable_across_space_rename() {
    let (db, _temp) = test_db().await;
    let space = db
        .create_space("annotations-work", None, false)
        .await
        .unwrap();
    let source_id = "https://example.test/one";
    let initial = db
        .write_source_annotation("webpage", source_id, &space.id, 0, Some("space note"), None)
        .await
        .unwrap();
    assert!(matches!(initial, SourceAnnotationWrite::Saved(_)));
    let isolated = db
        .get_source_annotation("webpage", source_id, "global")
        .await
        .unwrap();
    assert!(isolated.is_none());

    db.update_space("annotations-work", "annotations-renamed", None)
        .await
        .unwrap();
    let persisted = db
        .get_source_annotation("webpage", source_id, &space.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(persisted.notes, "space note");
}

#[tokio::test]
async fn reassign_moves_annotations_and_advances_version() {
    let (db, _temp) = test_db().await;
    let from = db
        .create_space("annotation-move-from", None, false)
        .await
        .unwrap();
    let to = db
        .create_space("annotation-move-to", None, false)
        .await
        .unwrap();
    let source_id = "https://example.test/move";
    let pages = vec!["page-a".to_string()];
    db.write_source_annotation(
        "webpage",
        source_id,
        &from.id,
        0,
        Some("keep with moved Space"),
        Some(&pages),
    )
    .await
    .unwrap();

    db.reassign_memories_space("annotation-move-from", "annotation-move-to")
        .await
        .unwrap();

    assert!(db
        .get_source_annotation("webpage", source_id, &from.id)
        .await
        .unwrap()
        .is_none());
    let moved = db
        .get_source_annotation("webpage", source_id, &to.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(moved.notes, "keep with moved Space");
    assert_eq!(moved.page_ids, pages);
    assert_eq!(moved.version, 2);
    assert_eq!(
        db.write_source_annotation("webpage", source_id, &to.id, 1, Some("stale"), None)
            .await
            .unwrap(),
        SourceAnnotationWrite::VersionConflict { current_version: 2 }
    );
}

#[tokio::test]
async fn conflicting_annotation_blocks_reassign_without_partial_memory_move() {
    let (db, _temp) = test_db().await;
    let from = db
        .create_space("annotation-conflict-from", None, false)
        .await
        .unwrap();
    let to = db
        .create_space("annotation-conflict-to", None, false)
        .await
        .unwrap();
    let source_id = "https://example.test/conflict";
    db.write_source_annotation("webpage", source_id, &from.id, 0, Some("source note"), None)
        .await
        .unwrap();
    db.write_source_annotation(
        "webpage",
        source_id,
        &to.id,
        0,
        Some("different destination note"),
        None,
    )
    .await
    .unwrap();
    seed_memory_in_space(&db, "conflict-memory", "annotation-conflict-from").await;

    let error = db
        .reassign_memories_space("annotation-conflict-from", "annotation-conflict-to")
        .await
        .unwrap_err();
    assert!(matches!(error, crate::WenlanError::Conflict(_)));
    assert_eq!(
        memory_space(&db, "conflict-memory").await,
        "annotation-conflict-from"
    );
    assert_eq!(
        db.get_source_annotation("webpage", source_id, &from.id)
            .await
            .unwrap()
            .unwrap()
            .notes,
        "source note"
    );
    assert_eq!(
        db.get_source_annotation("webpage", source_id, &to.id)
            .await
            .unwrap()
            .unwrap()
            .notes,
        "different destination note"
    );
}

#[tokio::test]
async fn identical_annotations_coalesce_with_a_version_bump() {
    let (db, _temp) = test_db().await;
    let from = db
        .create_space("annotation-coalesce-from", None, false)
        .await
        .unwrap();
    let to = db
        .create_space("annotation-coalesce-to", None, false)
        .await
        .unwrap();
    let source_id = "https://example.test/coalesce";
    let pages = vec!["page-a".to_string(), "page-b".to_string()];
    for space_id in [&from.id, &to.id] {
        db.write_source_annotation(
            "webpage",
            source_id,
            space_id,
            0,
            Some("same notes"),
            Some(&pages),
        )
        .await
        .unwrap();
    }

    db.reassign_memories_space("annotation-coalesce-from", "annotation-coalesce-to")
        .await
        .unwrap();

    assert!(db
        .get_source_annotation("webpage", source_id, &from.id)
        .await
        .unwrap()
        .is_none());
    let coalesced = db
        .get_source_annotation("webpage", source_id, &to.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(coalesced.notes, "same notes");
    assert_eq!(coalesced.page_ids, pages);
    assert_eq!(coalesced.version, 2);
}

#[tokio::test]
async fn delete_move_conflict_keeps_space_and_memories_unchanged() {
    let (db, _temp) = test_db().await;
    let from = db
        .create_space("annotation-delete-move-from", None, false)
        .await
        .unwrap();
    let to = db
        .create_space("annotation-delete-move-to", None, false)
        .await
        .unwrap();
    let source_id = "https://example.test/delete-move-conflict";
    db.write_source_annotation("webpage", source_id, &from.id, 0, Some("source"), None)
        .await
        .unwrap();
    db.write_source_annotation("webpage", source_id, &to.id, 0, Some("target"), None)
        .await
        .unwrap();
    seed_memory_in_space(&db, "delete-move-memory", "annotation-delete-move-from").await;

    let error = db
        .delete_space(
            "annotation-delete-move-from",
            "move:annotation-delete-move-to",
        )
        .await
        .unwrap_err();
    assert!(matches!(error, crate::WenlanError::Conflict(_)));
    assert_eq!(
        memory_space(&db, "delete-move-memory").await,
        "annotation-delete-move-from"
    );
    assert_eq!(
        db.get_space("annotation-delete-move-from")
            .await
            .unwrap()
            .unwrap()
            .id,
        from.id
    );
    assert_eq!(
        db.get_source_annotation("webpage", source_id, &to.id)
            .await
            .unwrap()
            .unwrap()
            .notes,
        "target"
    );
}

#[tokio::test]
async fn unassign_fails_closed_when_it_would_hide_source_annotations() {
    let (db, _temp) = test_db().await;
    let space = db
        .create_space("annotation-unassign", None, false)
        .await
        .unwrap();
    let source_id = "https://example.test/unassign";
    db.write_source_annotation(
        "webpage",
        source_id,
        &space.id,
        0,
        Some("preserve this note"),
        None,
    )
    .await
    .unwrap();
    seed_memory_in_space(&db, "unassign-memory", "annotation-unassign").await;

    let error = db
        .delete_space("annotation-unassign", "unassign")
        .await
        .unwrap_err();
    assert!(matches!(error, crate::WenlanError::Conflict(_)));
    assert_eq!(
        memory_space(&db, "unassign-memory").await,
        "annotation-unassign"
    );
    assert_eq!(
        db.get_source_annotation("webpage", source_id, &space.id)
            .await
            .unwrap()
            .unwrap()
            .notes,
        "preserve this note"
    );
    assert_eq!(
        db.get_space("annotation-unassign")
            .await
            .unwrap()
            .unwrap()
            .id,
        space.id
    );
}

#[tokio::test]
async fn keep_then_recreate_rehomes_annotations_without_reusing_space_id() {
    let (db, _temp) = test_db().await;
    let old_space = db
        .create_space("annotation-kept", None, false)
        .await
        .unwrap();
    let source_id = "https://example.test/kept";
    let pages = vec!["page-kept".to_string()];
    db.write_source_annotation(
        "webpage",
        source_id,
        &old_space.id,
        0,
        Some("kept note"),
        Some(&pages),
    )
    .await
    .unwrap();

    db.delete_space("annotation-kept", "keep").await.unwrap();
    assert!(db
        .get_source_annotation("webpage", source_id, &old_space.id)
        .await
        .unwrap()
        .is_none());
    let orphan_key = super::source_annotations::orphaned_space_context_key("annotation-kept");
    assert_eq!(
        db.get_source_annotation("webpage", source_id, &orphan_key)
            .await
            .unwrap()
            .unwrap()
            .version,
        2
    );

    let recreated = db
        .create_space("annotation-kept", None, false)
        .await
        .unwrap();
    assert_ne!(recreated.id, old_space.id);
    let adopted = db
        .get_source_annotation("webpage", source_id, &recreated.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(adopted.notes, "kept note");
    assert_eq!(adopted.page_ids, pages);
    assert_eq!(adopted.version, 3);
    assert!(db
        .get_source_annotation("webpage", source_id, &orphan_key)
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        db.write_source_annotation(
            "webpage",
            source_id,
            &recreated.id,
            1,
            Some("stale client"),
            None,
        )
        .await
        .unwrap(),
        SourceAnnotationWrite::VersionConflict { current_version: 3 }
    );
}
