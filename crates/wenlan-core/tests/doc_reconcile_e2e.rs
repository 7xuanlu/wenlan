// SPDX-License-Identifier: Apache-2.0
//! Doc-grounded revisions (L3 reconcile) — DB substrate + full-pipeline e2e.
//! Style of folder_ingest_e2e: in-process MemoryDB, no server, no network LLM.

use std::sync::Arc;
use wenlan_core::db::MemoryDB;
use wenlan_core::events::NoopEmitter;
use wenlan_types::RawDocument;

async fn temp_db() -> (tempfile::TempDir, MemoryDB) {
    let dir = tempfile::tempdir().unwrap();
    let db = MemoryDB::new(dir.path(), Arc::new(NoopEmitter))
        .await
        .expect("open temp MemoryDB");
    (dir, db)
}

/// A confirmed agent capture (frontier-b shape).
fn capture(id: &str, content: &str, space: Option<&str>, ts: i64) -> RawDocument {
    RawDocument {
        source: "memory".to_string(),
        source_id: id.to_string(),
        title: content.chars().take(40).collect(),
        content: content.to_string(),
        last_modified: ts,
        space: space.map(str::to_string),
        confirmed: Some(true),
        ..Default::default()
    }
}

/// A doc chunk row as L1 folder ingest stamps it (frontier-a shape).
fn doc(file_id: &str, content: &str, hash: &str, ts: i64) -> RawDocument {
    RawDocument {
        source: "memory".to_string(),
        source_id: file_id.to_string(),
        title: content.chars().take(40).collect(),
        content: content.to_string(),
        last_modified: ts,
        source_agent: Some("folder".to_string()),
        confirmed: None,
        content_hash: Some(hash.to_string()),
        ..Default::default()
    }
}

#[tokio::test]
async fn frontier_queries_select_and_paginate_by_watermark() {
    let (_dir, db) = temp_db().await;
    db.upsert_documents(vec![
        capture("mem_cap1", "The daemon listens on port 9999.", None, 100),
        capture(
            "mem_cap2",
            "Coffee preference: oat milk flat white.",
            None,
            200,
        ),
        doc(
            "src_f1::notes/net.md",
            "The daemon listens on port 7878.",
            "hash_v1",
            150,
        ),
    ])
    .await
    .unwrap();

    // Docs frontier: only the folder row.
    let docs = db.reconcile_doc_frontier(0, "", -1, 50).await.unwrap();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].source_id, "src_f1::notes/net.md");
    assert_eq!(docs[0].content_hash.as_deref(), Some("hash_v1"));

    // Watermark past it: empty.
    let after = db
        .reconcile_doc_frontier(
            docs[0].last_modified,
            &docs[0].source_id,
            docs[0].chunk_index,
            50,
        )
        .await
        .unwrap();
    assert!(after.is_empty(), "watermark must exclude processed rows");

    // Captures frontier: both captures, ascending last_modified; no folder rows.
    let caps = db.reconcile_capture_frontier(0, "", -1, 50).await.unwrap();
    let ids: Vec<&str> = caps.iter().map(|c| c.source_id.as_str()).collect();
    assert_eq!(ids, vec!["mem_cap1", "mem_cap2"]);

    // Limit paginates.
    let first = db.reconcile_capture_frontier(0, "", -1, 1).await.unwrap();
    assert_eq!(first.len(), 1);
    let second = db
        .reconcile_capture_frontier(
            first[0].last_modified,
            &first[0].source_id,
            first[0].chunk_index,
            1,
        )
        .await
        .unwrap();
    assert_eq!(second[0].source_id, "mem_cap2");
}

#[tokio::test]
async fn capture_frontier_excludes_folder_reconcile_unconfirmed_and_pending() {
    let (_dir, db) = temp_db().await;
    let mut reconcile_row = capture("mem_rec1", "accepted revision", None, 100);
    reconcile_row.source_agent = Some("reconcile".to_string());
    let mut unconfirmed = capture("mem_unc1", "unconfirmed capture", None, 100);
    unconfirmed.confirmed = None;
    let mut pending = capture("mem_pend1", "pending revision row", None, 100);
    pending.pending_revision = true;
    pending.supersedes = Some("mem_cap1".to_string());
    db.upsert_documents(vec![
        capture("mem_cap1", "a live capture", None, 100),
        doc("src_f1::a.md", "a doc", "h1", 100),
        reconcile_row,
        unconfirmed,
        pending,
    ])
    .await
    .unwrap();

    let caps = db.reconcile_capture_frontier(0, "", -1, 50).await.unwrap();
    let ids: Vec<&str> = caps.iter().map(|c| c.source_id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["mem_cap1"],
        "only live confirmed non-folder non-reconcile captures"
    );
}

#[tokio::test]
async fn pair_and_pending_guards() {
    let (_dir, db) = temp_db().await;
    let mut revision = capture("mem_rev1", "revised text", None, 100);
    revision.source_agent = Some("reconcile".to_string());
    revision.confirmed = None;
    revision.pending_revision = true;
    revision.supersedes = Some("mem_cap1".to_string());
    revision.structured_fields = Some(
        serde_json::json!({
            "revises": "mem_cap1",
            "grounded_in": "src_f1::a.md",
            "grounded_chunk": 0,
            "doc_hash": "hash_v1",
        })
        .to_string(),
    );
    db.upsert_documents(vec![capture("mem_cap1", "wrong claim", None, 50), revision])
        .await
        .unwrap();

    assert!(db
        .reconcile_pair_exists("mem_cap1", "src_f1::a.md", "hash_v1")
        .await
        .unwrap());
    // New doc hash = new pair (dismiss binds to a doc version).
    assert!(!db
        .reconcile_pair_exists("mem_cap1", "src_f1::a.md", "hash_v2")
        .await
        .unwrap());
    // Quoted-LIKE: no substring false positive on a prefix id.
    assert!(!db
        .reconcile_pair_exists("mem_cap", "src_f1::a.md", "hash_v1")
        .await
        .unwrap());

    assert!(db.capture_has_pending_revision("mem_cap1").await.unwrap());
    assert!(!db.capture_has_pending_revision("mem_cap2").await.unwrap());

    assert!(
        !db.pending_reconcile_at_cap(1).await.unwrap(),
        "1 pending, cap 1: not OVER cap"
    );
    assert!(
        db.pending_reconcile_at_cap(0).await.unwrap(),
        "1 pending, cap 0: over cap"
    );
}
