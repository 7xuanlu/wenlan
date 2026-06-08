// SPDX-License-Identifier: Apache-2.0
//! P2 typed-evidence integration tests.
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

/// Seed a memory via the canonical upsert path (replaces the plan's
/// non-existent `store_memory_for_test`).
async fn seed_memory(db: &MemoryDB, id: &str, content: &str) {
    let doc = RawDocument {
        source: "memory".to_string(),
        source_id: id.to_string(),
        title: id.to_string(),
        summary: None,
        content: content.to_string(),
        url: None,
        last_modified: chrono::Utc::now().timestamp(),
        memory_type: Some("fact".to_string()),
        space: Some("technology".to_string()),
        source_agent: Some("test-agent".to_string()),
        confidence: None,
        confirmed: None,
        supersedes: None,
        pending_revision: false,
        ..Default::default()
    };
    db.upsert_documents(vec![doc]).await.expect("seed memory");
}

#[tokio::test]
async fn page_evidence_backfill_matches_legacy_page_sources() {
    let (db, _d) = make_db().await;
    seed_memory(&db, "mem_a", "alpha content about rust").await;
    seed_memory(&db, "mem_b", "beta content about rust").await;
    let now = chrono::Utc::now().to_rfc3339();
    db.insert_page(
        "page_1",
        "Rust",
        Some("rust topic"),
        "body",
        None,
        None,
        &["mem_a", "mem_b"],
        &now,
    )
    .await
    .unwrap();
    let ev = db.get_page_evidence("page_1").await.unwrap();
    let mut locs: Vec<String> = ev
        .iter()
        .filter(|e| e.source_kind == "memory")
        .map(|e| e.locator.clone().unwrap())
        .collect();
    locs.sort();
    assert_eq!(locs, vec!["mem_a".to_string(), "mem_b".to_string()]);
    assert!(ev.iter().all(|e| e.source_kind == "memory"));
}
