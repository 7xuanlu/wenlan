// SPDX-License-Identifier: Apache-2.0

use super::tests::test_db;
use super::MemoryDB;
use crate::error::WenlanError;
use crate::pages::{PageDraftDeleteOutcome, PageDraftUpdateOutcome};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::{Barrier, Notify};

pub(super) mod transaction_test_hooks {
    use super::*;

    struct Pause {
        reached: Arc<Notify>,
        resume: Arc<Notify>,
    }

    static CREATE_AFTER_INSERT: OnceLock<Mutex<HashMap<String, Pause>>> = OnceLock::new();
    static AFTER_SPACE_VALIDATION: OnceLock<Mutex<HashMap<String, Pause>>> = OnceLock::new();
    static AFTER_SPACE_CASCADE: OnceLock<Mutex<HashMap<String, Pause>>> = OnceLock::new();

    fn install_pause(
        pauses: &OnceLock<Mutex<HashMap<String, Pause>>>,
        id: &str,
    ) -> (Arc<Notify>, Arc<Notify>) {
        let reached = Arc::new(Notify::new());
        let resume = Arc::new(Notify::new());
        pauses.get_or_init(Default::default).lock().unwrap().insert(
            id.to_string(),
            Pause {
                reached: Arc::clone(&reached),
                resume: Arc::clone(&resume),
            },
        );
        (reached, resume)
    }

    async fn reach_pause(pauses: &OnceLock<Mutex<HashMap<String, Pause>>>, id: &str) {
        let pause = pauses
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .remove(id);
        if let Some(pause) = pause {
            pause.reached.notify_one();
            pause.resume.notified().await;
        }
    }

    pub(super) fn pause_create_after_insert(id: &str) -> (Arc<Notify>, Arc<Notify>) {
        install_pause(&CREATE_AFTER_INSERT, id)
    }

    pub(crate) async fn after_create_insert(id: &str) {
        reach_pause(&CREATE_AFTER_INSERT, id).await;
    }

    pub(super) fn pause_after_space_validation(id: &str) -> (Arc<Notify>, Arc<Notify>) {
        install_pause(&AFTER_SPACE_VALIDATION, id)
    }

    pub(crate) async fn after_space_validation(id: &str) {
        reach_pause(&AFTER_SPACE_VALIDATION, id).await;
    }

    pub(super) fn pause_after_space_cascade(key: &str) -> (Arc<Notify>, Arc<Notify>) {
        install_pause(&AFTER_SPACE_CASCADE, key)
    }

    pub(crate) async fn after_space_cascade(key: &str) {
        reach_pause(&AFTER_SPACE_CASCADE, key).await;
    }
}

async fn scalar_i64(db: &MemoryDB, sql: &str, id: &str) -> i64 {
    let conn = db.conn.lock().await;
    let mut rows = conn.query(sql, libsql::params![id]).await.unwrap();
    rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap()
}

async fn seed_non_draft_page(
    db: &MemoryDB,
    id: &str,
    status: &str,
    space: &str,
    version: i64,
    last_modified: &str,
) {
    let conn = db.conn.lock().await;
    conn.execute(
        "INSERT INTO pages (
            id, title, content, space, source_memory_ids, version, status,
            created_at, last_compiled, last_modified, creation_kind,
            review_status, workspace
         ) VALUES (?1, ?2, 'body', ?3, '[]', ?4, ?5,
            ?6, ?6, ?6, 'authored', 'unconfirmed', ?3)",
        libsql::params![
            id,
            format!("{status} page"),
            space,
            version,
            status,
            last_modified
        ],
    )
    .await
    .unwrap();
}

async fn page_version_and_modified(db: &MemoryDB, id: &str) -> (i64, String) {
    let conn = db.conn.lock().await;
    let mut rows = conn
        .query(
            "SELECT version, last_modified FROM pages WHERE id=?1",
            libsql::params![id],
        )
        .await
        .unwrap();
    let row = rows.next().await.unwrap().unwrap();
    (row.get(0).unwrap(), row.get(1).unwrap())
}

#[tokio::test]
async fn create_rejects_empty_but_persists_meaningful_partial_snapshots() {
    let (db, _tmp) = test_db().await;

    for (title, content, space) in [
        ("", "", None),
        (" \t", "\n", None),
        ("", "", Some("work")),
        (
            "",
            "<!-- origin:sources:start -->owned<!-- origin:sources:end -->",
            None,
        ),
    ] {
        assert!(matches!(
            db.create_page_draft(title, content, space, space).await,
            Err(WenlanError::Validation(_))
        ));
    }

    let title_only = db
        .create_page_draft("  Working title  ", "", None, None)
        .await
        .unwrap();
    let body_only = db
        .create_page_draft("", "  Opening paragraph  ", Some("work"), Some("work"))
        .await
        .unwrap();

    assert_eq!(title_only.title, "  Working title  ");
    assert_eq!(title_only.content, "");
    assert_eq!(body_only.title, "");
    assert_eq!(body_only.content, "  Opening paragraph  ");
    for page in [&title_only, &body_only] {
        assert_eq!(page.status, "draft");
        assert_eq!(page.creation_kind, "authored");
        assert_eq!(page.review_status, "unconfirmed");
        assert_eq!(page.source_memory_ids, Vec::<String>::new());
        assert!(page.citations.is_empty());
        assert!(page.entity_id.is_none());
        assert!(page.summary.is_none());
        assert_eq!(page.version, 1);
    }
}

#[tokio::test]
async fn client_uuid_is_validated_idempotent_and_collision_safe() {
    let (db, _tmp) = test_db().await;
    for invalid in [
        "",
        "not-a-page",
        "page_not-a-uuid",
        "page_00000000-0000-0000-0000-000000000000",
        "page_00000000-0000-4000-0000-000000000001",
        "page_00000000000040008000000000000001",
        "page_00000000-0000-4000-8000-000000000001-extra",
    ] {
        assert!(matches!(
            db.create_page_draft_with_id(invalid, "Draft", "Body", None, None)
                .await,
            Err(WenlanError::Validation(_))
        ));
    }

    let id = "page_00000000-0000-4000-8000-000000000001";
    let first = db
        .create_page_draft_with_id(id, "Draft", "Body  \n", Some("work"), Some("work"))
        .await
        .unwrap();
    let replay = db
        .create_page_draft_with_id(id, "Draft", "Body  \n", Some("work"), Some("work"))
        .await
        .unwrap();
    assert_eq!(first.id, id);
    assert_eq!(replay.version, first.version);
    assert_eq!(
        scalar_i64(&db, "SELECT COUNT(*) FROM pages WHERE id=?1", id).await,
        1
    );
    assert!(matches!(
        db.create_page_draft_with_id(id, "Draft", "Different", Some("work"), Some("work"))
            .await,
        Err(WenlanError::PageDraftIdConflict(conflict_id)) if conflict_id == id
    ));

    seed_non_draft_page(
        &db,
        "page_00000000-0000-4000-8000-000000000002",
        "active",
        "work",
        1,
        "2026-01-01T00:00:00Z",
    )
    .await;
    assert!(matches!(
        db.create_page_draft_with_id(
            "page_00000000-0000-4000-8000-000000000002",
            "Draft",
            "Body",
            None,
            None,
        )
        .await,
        Err(WenlanError::PageDraftIdConflict(_))
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn simultaneous_same_id_creates_are_idempotent_or_conflict_by_snapshot() {
    let (db, _tmp) = test_db().await;
    let db = Arc::new(db);

    let identical_id = "page_00000000-0000-4000-8000-000000000003";
    let barrier = Arc::new(Barrier::new(3));
    let mut identical = Vec::new();
    for _ in 0..2 {
        let db = Arc::clone(&db);
        let barrier = Arc::clone(&barrier);
        identical.push(tokio::spawn(async move {
            barrier.wait().await;
            db.create_page_draft_with_id(identical_id, "Draft", "Body", None, None)
                .await
        }));
    }
    barrier.wait().await;
    let identical = [
        identical.remove(0).await.unwrap().unwrap(),
        identical.remove(0).await.unwrap().unwrap(),
    ];
    assert_eq!(identical[0].id, identical[1].id);
    assert_eq!(identical[0].version, identical[1].version);
    assert_eq!(
        scalar_i64(&db, "SELECT COUNT(*) FROM pages WHERE id=?1", identical_id,).await,
        1
    );

    let divergent_id = "page_00000000-0000-4000-8000-000000000004";
    let barrier = Arc::new(Barrier::new(3));
    let mut divergent = Vec::new();
    for (title, content) in [("First", "First body"), ("Second", "Second body")] {
        let db = Arc::clone(&db);
        let barrier = Arc::clone(&barrier);
        divergent.push(tokio::spawn(async move {
            barrier.wait().await;
            db.create_page_draft_with_id(divergent_id, title, content, None, None)
                .await
        }));
    }
    barrier.wait().await;
    let outcomes = [
        divergent.remove(0).await.unwrap(),
        divergent.remove(0).await.unwrap(),
    ];
    assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(
                outcome,
                Err(WenlanError::PageDraftIdConflict(id)) if id == divergent_id
            ))
            .count(),
        1
    );
    assert_eq!(
        scalar_i64(&db, "SELECT COUNT(*) FROM pages WHERE id=?1", divergent_id,).await,
        1
    );
}

#[tokio::test]
async fn create_preserves_bytes_null_embedding_and_has_no_derived_rows() {
    let (db, _tmp) = test_db().await;
    let content =
        "Before\n<!-- origin:sources:start -->\nowned\n<!-- origin:sources:end -->\nAfter  \n";
    let page = db
        .create_page_draft("  Draft  ", content, None, None)
        .await
        .unwrap();

    assert_eq!(page.title, "  Draft  ");
    assert_eq!(page.content, content);
    assert_eq!(
        scalar_i64(
            &db,
            "SELECT COUNT(*) FROM pages WHERE id=?1 AND embedding IS NULL",
            &page.id,
        )
        .await,
        1
    );
    for table in ["page_sources", "page_evidence", "page_links"] {
        let sql = format!(
            "SELECT COUNT(*) FROM {table} WHERE {}=?1",
            if table == "page_links" {
                "source_page_id"
            } else {
                "page_id"
            }
        );
        assert_eq!(scalar_i64(&db, &sql, &page.id).await, 0);
    }
}

#[tokio::test]
async fn update_supports_exact_retry_and_rejects_stale_active_missing_and_empty() {
    let (db, _tmp) = test_db().await;
    db.create_space("retry-space", None, false).await.unwrap();
    let draft = db
        .create_page_draft("Draft", "Original body", None, None)
        .await
        .unwrap();

    let first = db
        .update_page_draft_in_registered_space(
            &draft.id,
            1,
            "  Revised title  ",
            "Revised body  \n",
            Some(" retry-space "),
        )
        .await
        .unwrap();
    let PageDraftUpdateOutcome::Updated(first) = first else {
        panic!("expected initial update");
    };
    assert_eq!(first.version, 2);
    db.delete_space("retry-space", "keep").await.unwrap();

    let retry = db
        .update_page_draft_in_registered_space(
            &draft.id,
            1,
            "  Revised title  ",
            "Revised body  \n",
            Some(" retry-space "),
        )
        .await
        .unwrap();
    let PageDraftUpdateOutcome::Updated(retry) = retry else {
        panic!("exact retry must return the committed snapshot");
    };
    assert_eq!(retry.version, 2);
    assert_eq!(retry.content, "Revised body  \n");

    assert!(matches!(
        db.update_page_draft(&draft.id, 1, "stale", "different", None, None)
            .await
            .unwrap(),
        PageDraftUpdateOutcome::VersionConflict { current_version: 2 }
    ));
    assert!(matches!(
        db.update_page_draft(
            &draft.id,
            2,
            "",
            "<!-- origin:sources:start -->owned<!-- origin:sources:end -->",
            None,
            None,
        )
        .await,
        Err(WenlanError::Validation(_))
    ));

    seed_non_draft_page(
        &db,
        "page_active_update_guard",
        "active",
        "work",
        1,
        "2026-01-01T00:00:00Z",
    )
    .await;
    assert!(matches!(
        db.update_page_draft(
            "page_active_update_guard",
            1,
            "Changed",
            "Changed",
            None,
            None,
        )
        .await,
        Err(WenlanError::Validation(_))
    ));
    assert!(matches!(
        db.update_page_draft("page_missing", 1, "Title", "Body", None, None)
            .await,
        Err(WenlanError::NotFound(_))
    ));
}

#[tokio::test]
async fn delete_is_version_safe_and_rejects_active_and_missing_pages() {
    let (db, _tmp) = test_db().await;
    let draft = db
        .create_page_draft("Draft", "Body", None, None)
        .await
        .unwrap();
    let updated = db
        .update_page_draft(&draft.id, 1, "Revised", "Updated", None, None)
        .await
        .unwrap();
    assert!(matches!(updated, PageDraftUpdateOutcome::Updated(_)));
    assert!(matches!(
        db.delete_page_draft(&draft.id, 1).await.unwrap(),
        PageDraftDeleteOutcome::VersionConflict { current_version: 2 }
    ));
    assert!(matches!(
        db.delete_page_draft(&draft.id, 2).await.unwrap(),
        PageDraftDeleteOutcome::Deleted
    ));
    assert!(db.get_page(&draft.id).await.unwrap().is_none());

    seed_non_draft_page(
        &db,
        "page_active_delete_guard",
        "active",
        "work",
        1,
        "2026-01-01T00:00:00Z",
    )
    .await;
    assert!(matches!(
        db.delete_page_draft("page_active_delete_guard", 1).await,
        Err(WenlanError::Validation(_))
    ));
    assert!(matches!(
        db.delete_page_draft("page_missing", 1).await,
        Err(WenlanError::NotFound(_))
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn simultaneous_updates_allow_exactly_one_compare_and_swap_winner() {
    let (db, _tmp) = test_db().await;
    let db = Arc::new(db);
    let draft = db
        .create_page_draft("Draft", "Body", None, None)
        .await
        .unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let mut tasks = Vec::new();
    for (title, body) in [("First", "First body"), ("Second", "Second body")] {
        let db = Arc::clone(&db);
        let barrier = Arc::clone(&barrier);
        let id = draft.id.clone();
        tasks.push(tokio::spawn(async move {
            barrier.wait().await;
            db.update_page_draft(&id, 1, title, body, None, None)
                .await
                .unwrap()
        }));
    }
    barrier.wait().await;
    let outcomes = [
        tasks.remove(0).await.unwrap(),
        tasks.remove(0).await.unwrap(),
    ];
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, PageDraftUpdateOutcome::Updated(_)))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(
                outcome,
                PageDraftUpdateOutcome::VersionConflict { current_version: 2 }
            ))
            .count(),
        1
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_create_after_insert_rolls_back_before_retry() {
    let (db, _tmp) = test_db().await;
    let db = Arc::new(db);
    let id = "page_00000000-0000-4000-8000-000000000006";
    let (reached, _resume) = transaction_test_hooks::pause_create_after_insert(id);
    let task = {
        let db = Arc::clone(&db);
        tokio::spawn(async move {
            db.create_page_draft_with_id(id, "Draft", "Body", None, None)
                .await
        })
    };

    reached.notified().await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_eq!(
        scalar_i64(&db, "SELECT COUNT(*) FROM pages WHERE id=?1", id).await,
        0
    );
    db.create_page_draft_with_id(id, "Draft", "Body", None, None)
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn registered_space_validation_is_atomic_with_concurrent_rename() {
    let (db, _tmp) = test_db().await;
    assert!(matches!(
        db.create_page_draft_with_id_in_registered_space(
            "page_00000000-0000-4000-8000-000000000008",
            "Draft",
            "Body",
            Some("missing"),
        )
        .await,
        Err(WenlanError::Validation(_))
    ));

    db.create_space("old", None, false).await.unwrap();
    let db = Arc::new(db);
    let id = "page_00000000-0000-4000-8000-000000000007";
    let (reached, resume) = transaction_test_hooks::pause_after_space_validation(id);
    let create = {
        let db = Arc::clone(&db);
        tokio::spawn(async move {
            db.create_page_draft_with_id_in_registered_space(id, "Draft", "Body", Some("old"))
                .await
        })
    };

    reached.notified().await;
    let mut rename = {
        let db = Arc::clone(&db);
        tokio::spawn(async move { db.update_space("old", "renamed", None).await })
    };
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), &mut rename)
            .await
            .is_err()
    );
    resume.notify_one();
    create.await.unwrap().unwrap();
    rename.await.unwrap().unwrap();

    let persisted = db.get_page(id).await.unwrap().unwrap();
    assert_eq!(persisted.space.as_deref(), Some("renamed"));
    assert_eq!(persisted.workspace.as_deref(), Some("renamed"));
}

#[derive(Clone, Copy)]
enum SpacePath {
    Rename,
    DeleteMove,
    Reassign,
}

async fn assert_space_path_moves_all_statuses_but_only_bumps_draft(path: SpacePath) {
    let (db, _tmp) = test_db().await;
    db.create_space("src", None, false).await.unwrap();
    if !matches!(path, SpacePath::Rename) {
        db.create_space("dest", None, false).await.unwrap();
    }
    let draft = db
        .create_page_draft("Draft", "Body", Some("src"), Some("src"))
        .await
        .unwrap();
    seed_non_draft_page(
        &db,
        "page_active_space_control",
        "active",
        "src",
        7,
        "2026-01-01T00:00:00Z",
    )
    .await;
    seed_non_draft_page(
        &db,
        "page_archived_space_control",
        "archived",
        "src",
        9,
        "2026-01-02T00:00:00Z",
    )
    .await;
    let active_before = page_version_and_modified(&db, "page_active_space_control").await;
    let archived_before = page_version_and_modified(&db, "page_archived_space_control").await;

    match path {
        SpacePath::Rename => {
            db.update_space("src", "dest", None).await.unwrap();
        }
        SpacePath::DeleteMove => {
            db.delete_space("src", "move:dest").await.unwrap();
        }
        SpacePath::Reassign => {
            db.reassign_memories_space("src", "dest").await.unwrap();
        }
    }

    let moved_draft = db.get_page(&draft.id).await.unwrap().unwrap();
    assert_eq!(moved_draft.space.as_deref(), Some("dest"));
    assert_eq!(moved_draft.workspace.as_deref(), Some("dest"));
    assert_eq!(moved_draft.version, draft.version + 1);
    assert_ne!(moved_draft.last_modified, draft.last_modified);
    assert!(matches!(
        db.update_page_draft(&draft.id, draft.version, "Stale", "Stale", None, None)
            .await
            .unwrap(),
        PageDraftUpdateOutcome::VersionConflict { current_version }
            if current_version == draft.version + 1
    ));

    for (id, before) in [
        ("page_active_space_control", active_before),
        ("page_archived_space_control", archived_before),
    ] {
        let page = db.get_page(id).await.unwrap().unwrap();
        assert_eq!(page.space.as_deref(), Some("dest"));
        assert_eq!(page.workspace.as_deref(), Some("dest"));
        assert_eq!(page_version_and_modified(&db, id).await, before);
    }
}

async fn assert_cancelled_space_path_rolls_back_and_releases_connection(path: SpacePath) {
    let (source, destination, hook_key, reuse_name) = match path {
        SpacePath::Rename => (
            "abort-rename-src",
            "abort-rename-dest",
            "update_space:abort-rename-src",
            "after-abort-rename",
        ),
        SpacePath::DeleteMove => (
            "abort-delete-src",
            "abort-delete-dest",
            "delete_space:abort-delete-src",
            "after-abort-delete",
        ),
        SpacePath::Reassign => (
            "abort-reassign-src",
            "abort-reassign-dest",
            "reassign_memories_space:abort-reassign-src",
            "after-abort-reassign",
        ),
    };
    let (db, _tmp) = test_db().await;
    db.create_space(source, None, false).await.unwrap();
    if !matches!(path, SpacePath::Rename) {
        db.create_space(destination, None, false).await.unwrap();
    }
    let draft = db
        .create_page_draft("Draft", "Body", Some(source), Some(source))
        .await
        .unwrap();
    let before = page_version_and_modified(&db, &draft.id).await;
    let (reached, _resume) = transaction_test_hooks::pause_after_space_cascade(hook_key);
    let db = Arc::new(db);
    let operation = {
        let db = Arc::clone(&db);
        tokio::spawn(async move {
            match path {
                SpacePath::Rename => db.update_space(source, destination, None).await.map(|_| ()),
                SpacePath::DeleteMove => {
                    db.delete_space(source, &format!("move:{destination}"))
                        .await
                }
                SpacePath::Reassign => db
                    .reassign_memories_space(source, destination)
                    .await
                    .map(|_| ()),
            }
        })
    };

    reached.notified().await;
    operation.abort();
    assert!(operation.await.unwrap_err().is_cancelled());

    let persisted = db.get_page(&draft.id).await.unwrap().unwrap();
    assert_eq!(persisted.space.as_deref(), Some(source));
    assert_eq!(persisted.workspace.as_deref(), Some(source));
    assert_eq!(page_version_and_modified(&db, &draft.id).await, before);
    assert!(db.get_space(source).await.unwrap().is_some());
    if matches!(path, SpacePath::Rename) {
        assert!(db.get_space(destination).await.unwrap().is_none());
    } else {
        assert!(db.get_space(destination).await.unwrap().is_some());
    }
    db.create_space(reuse_name, None, false).await.unwrap();
}

#[tokio::test]
async fn rename_space_bumps_matching_draft_once_only() {
    assert_space_path_moves_all_statuses_but_only_bumps_draft(SpacePath::Rename).await;
}

#[tokio::test]
async fn delete_space_move_bumps_matching_draft_once_only() {
    assert_space_path_moves_all_statuses_but_only_bumps_draft(SpacePath::DeleteMove).await;
}

#[tokio::test]
async fn reassign_space_bumps_matching_draft_once_only() {
    assert_space_path_moves_all_statuses_but_only_bumps_draft(SpacePath::Reassign).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_space_cascades_rolls_back_and_releases_connection() {
    for path in [
        SpacePath::Rename,
        SpacePath::DeleteMove,
        SpacePath::Reassign,
    ] {
        assert_cancelled_space_path_rolls_back_and_releases_connection(path).await;
    }
}

#[tokio::test]
async fn description_delete_keep_and_failed_space_paths_do_not_bump_drafts() {
    let (db, _tmp) = test_db().await;
    db.create_space("src", None, false).await.unwrap();
    db.create_space("dest", None, false).await.unwrap();
    let draft = db
        .create_page_draft("Draft", "Body", Some("src"), Some("src"))
        .await
        .unwrap();
    let before = page_version_and_modified(&db, &draft.id).await;

    db.update_space("src", "src", Some("description"))
        .await
        .unwrap();
    assert_eq!(page_version_and_modified(&db, &draft.id).await, before);

    assert!(matches!(
        db.reassign_memories_space("src", "src").await,
        Err(WenlanError::Validation(_))
    ));
    assert_eq!(page_version_and_modified(&db, &draft.id).await, before);

    db.delete_space("src", "keep").await.unwrap();
    assert_eq!(page_version_and_modified(&db, &draft.id).await, before);

    assert!(db.update_space("missing", "dest", None).await.is_err());
    assert!(db.reassign_memories_space("missing", "dest").await.is_err());
    assert!(db.delete_space("missing", "move:dest").await.is_err());
    assert_eq!(page_version_and_modified(&db, &draft.id).await, before);
}
