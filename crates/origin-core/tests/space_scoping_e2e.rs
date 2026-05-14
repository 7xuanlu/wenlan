// SPDX-License-Identifier: Apache-2.0
//! Acceptance gate for PR-A: space filter end-to-end at DB layer.
//!
//! Tests that `search_memory` correctly scopes results by space:
//! - Memories tagged with space=A are returned when filter=A.
//! - Memories tagged with space=B are excluded when filter=A.
//! - The special filter value "uncategorized" matches rows where space IS NULL.

use origin_core::db::MemoryDB;
use origin_core::sources::RawDocument;
use origin_core::{EventEmitter, NoopEmitter};
use std::sync::Arc;

async fn make_db() -> (Arc<MemoryDB>, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let emitter: Arc<dyn EventEmitter> = Arc::new(NoopEmitter);
    let db = MemoryDB::new(&db_path, emitter)
        .await
        .expect("MemoryDB::new");
    (Arc::new(db), dir)
}

fn make_memory(source_id: &str, content: &str, space: Option<&str>) -> RawDocument {
    RawDocument {
        source: "memory".to_string(),
        source_id: source_id.to_string(),
        title: source_id.to_string(),
        content: content.to_string(),
        memory_type: Some("fact".to_string()),
        space: space.map(|s| s.to_string()),
        last_modified: 1_700_000_000,
        ..Default::default()
    }
}

#[tokio::test]
async fn space_scoping_excludes_other_space() {
    let (db, _dir) = make_db().await;

    db.upsert_documents(vec![
        make_memory("mem_alpha_1", "foo fact alpha-only", Some("alpha")),
        make_memory("mem_beta_1", "bar fact beta-only", Some("beta")),
    ])
    .await
    .unwrap();

    let r = db
        .search_memory("fact", 10, None, Some("alpha"), None, None, None, None)
        .await
        .unwrap();
    let texts: Vec<&str> = r.iter().map(|x| x.content.as_str()).collect();

    assert!(
        texts.iter().any(|t| t.contains("foo")),
        "alpha hit missing -- search_memory should return memories scoped to space=alpha; got: {texts:?}"
    );
    assert!(
        !texts.iter().any(|t| t.contains("bar")),
        "beta leaked into alpha results -- space filter not applied correctly; got: {texts:?}"
    );
}

#[tokio::test]
async fn space_uncategorized_matches_null() {
    let (db, _dir) = make_db().await;

    db.upsert_documents(vec![
        make_memory("mem_orphan_1", "orphan fact no space", None),
        make_memory("mem_alpha_2", "alpha fact tagged", Some("alpha")),
    ])
    .await
    .unwrap();

    let r = db
        .search_memory(
            "fact",
            10,
            None,
            Some("uncategorized"),
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
    let texts: Vec<&str> = r.iter().map(|x| x.content.as_str()).collect();

    assert!(
        texts.iter().any(|t| t.contains("orphan")),
        "uncategorized must match rows where space IS NULL; got: {texts:?}"
    );
    assert!(
        !texts.iter().any(|t| t.contains("alpha fact tagged")),
        "alpha-tagged memory must not appear under uncategorized filter; got: {texts:?}"
    );
}
