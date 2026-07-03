// SPDX-License-Identifier: Apache-2.0
//! Doc-grounded revisions (L3 reconcile) — DB substrate + full-pipeline e2e.
//! Style of folder_ingest_e2e: in-process MemoryDB, no server, no network LLM.

use std::sync::Arc;
use wenlan_core::db::MemoryDB;
use wenlan_core::events::NoopEmitter;
use wenlan_core::llm_provider::{LlmBackend, LlmError, LlmProvider, LlmRequest};
use wenlan_core::prompts::PromptRegistry;
use wenlan_core::reconcile::{
    run_reconcile_tick, write_revision, RevisionInput, RECONCILE_PENDING_CAP,
};
use wenlan_core::tuning::{DistillationConfig, RefineryConfig};
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

#[tokio::test]
async fn candidates_match_same_space_opposite_side_above_gate() {
    let (_dir, db) = temp_db().await;
    db.upsert_documents(vec![
        capture(
            "mem_cap1",
            "The daemon listens on port 9999 for HTTP requests.",
            None,
            100,
        ),
        capture(
            "mem_other_space",
            "The daemon listens on port 9999 for HTTP requests.",
            Some("work"),
            100,
        ),
        capture(
            "mem_unrelated",
            "Favorite hiking trail is the coastal ridge loop.",
            None,
            100,
        ),
        doc(
            "src_f1::net.md",
            "The daemon listens on port 7878 for HTTP requests.",
            "h1",
            150,
        ),
    ])
    .await
    .unwrap();

    // Doc frontier item -> capture candidates, NULL space only.
    let cands = db
        .reconcile_candidates("src_f1::net.md", 0, None, false, 5, 0.70)
        .await
        .unwrap();
    let ids: Vec<&str> = cands.iter().map(|c| c.source_id.as_str()).collect();
    assert!(
        ids.contains(&"mem_cap1"),
        "near-identical same-space capture matches"
    );
    assert!(
        !ids.contains(&"mem_other_space"),
        "space='work' never matches NULL space"
    );
    assert!(!ids.contains(&"src_f1::net.md"), "item itself excluded");
    for c in &cands {
        assert!(c.cosine >= 0.70, "gate enforced, got {}", c.cosine);
    }

    // Capture frontier item -> doc candidates.
    let doc_cands = db
        .reconcile_candidates("mem_cap1", 0, None, true, 5, 0.70)
        .await
        .unwrap();
    assert_eq!(doc_cands.len(), 1);
    assert_eq!(doc_cands[0].source_id, "src_f1::net.md");
    assert_eq!(doc_cands[0].content_hash.as_deref(), Some("h1"));
}

#[tokio::test]
async fn write_revision_stages_embedded_pending_row_with_provenance() {
    let (_dir, db) = temp_db().await;
    db.upsert_documents(vec![
        capture(
            "mem_cap1",
            "The daemon listens on port 9999.",
            Some("work"),
            100,
        ),
        doc(
            "src_f1::net.md",
            "The daemon listens on port 7878.",
            "hash_v1",
            150,
        ),
    ])
    .await
    .unwrap();

    let rev_id = write_revision(
        &db,
        RevisionInput {
            capture_source_id: "mem_cap1",
            capture_space: Some("work"),
            doc_file_source_id: "src_f1::net.md",
            doc_chunk_index: 0,
            doc_hash: "hash_v1",
            revised_content: "The daemon listens on port 7878.",
        },
        None, // no LLM: Phase-1 placeholders, still embedded + stored
        &PromptRegistry::default(),
        &RefineryConfig::default(),
        &DistillationConfig::default(),
    )
    .await
    .unwrap();

    // Surfaced on the existing pending-revisions queue.
    let pending = db.list_pending_revisions(10).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].target_source_id, "mem_cap1");
    assert_eq!(pending[0].revision_source_id, rev_id);
    assert_eq!(pending[0].source_agent.as_deref(), Some("reconcile"));
    assert_eq!(
        pending[0].grounded_in.as_deref(),
        Some("src_f1::net.md"),
        "revision card carries its grounding doc"
    );

    // Canonical-path contract: the revision row is embedded (never a dead row).
    let missing = db.count_unembedded_chunks("memory", &rev_id).await.unwrap();
    assert_eq!(missing, 0, "revision must be embedded at store time");

    // Pair key present for the treadmill guard.
    assert!(db
        .reconcile_pair_exists("mem_cap1", "src_f1::net.md", "hash_v1")
        .await
        .unwrap());
    // Same space as the capture.
    assert!(db.capture_has_pending_revision("mem_cap1").await.unwrap());
}

/// Deterministic judge: flags candidate 0 as contradicted, rewrites to the doc's fact.
struct StubJudge;

#[async_trait::async_trait]
impl LlmProvider for StubJudge {
    async fn generate(&self, req: LlmRequest) -> Result<String, LlmError> {
        if req.label.as_deref() == Some("doc_reconcile") {
            Ok(
                r#"{"conflicts":[{"idx":0,"revised_content":"The daemon listens on port 7878."}]}"#
                    .to_string(),
            )
        } else {
            // Phase-1 classify etc. during write_revision: harmless empty object.
            Ok("{}".to_string())
        }
    }
    fn is_available(&self) -> bool {
        true
    }
    fn name(&self) -> &str {
        "stub-judge"
    }
    fn backend(&self) -> LlmBackend {
        LlmBackend::Api
    }
}

#[tokio::test]
async fn doc_reconcile_full_pipeline_e2e() {
    let (_dir, db) = temp_db().await;
    let llm: Arc<dyn LlmProvider> = Arc::new(StubJudge);
    let prompts = PromptRegistry::default();
    let refinery = RefineryConfig::default();
    let distillation = DistillationConfig::default();

    // Seed: wrong capture + contradicting doc (same NULL space) + negative
    // control in another space.
    db.upsert_documents(vec![
        capture("mem_wrong", "The daemon listens on port 9999.", None, 100),
        capture(
            "mem_other",
            "The daemon listens on port 9999.",
            Some("work"),
            100,
        ),
    ])
    .await
    .unwrap();
    db.upsert_documents(vec![doc(
        "src_f1::notes/net.md",
        "The daemon listens on port 7878.",
        "hash_v1",
        200,
    )])
    .await
    .unwrap();
    let doc_content_before: String = {
        // Raw content of the doc row, to prove byte-identity after the sweep.
        let all = db.reconcile_doc_frontier(0, "", -1, 50).await.unwrap();
        all[0].content.clone()
    };

    // Tick 1: exactly one pending revision, correctly linked + grounded.
    let r1 = run_reconcile_tick(&db, &llm, &prompts, &refinery, &distillation)
        .await
        .unwrap();
    assert!(r1.judged >= 1);
    assert_eq!(r1.proposed, 1, "one conflict -> one staged revision");
    let pending = db.list_pending_revisions(10).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].target_source_id, "mem_wrong");
    assert_eq!(pending[0].source_agent.as_deref(), Some("reconcile"));
    assert_eq!(
        pending[0].grounded_in.as_deref(),
        Some("src_f1::notes/net.md")
    );
    assert_eq!(
        pending[0].revision_content,
        "The daemon listens on port 7878."
    );

    // Negative control: the other-space capture is never flagged.
    assert!(!db.capture_has_pending_revision("mem_other").await.unwrap());

    // Doc row untouched.
    let docs_after = db.reconcile_doc_frontier(0, "", -1, 50).await.unwrap();
    assert_eq!(
        docs_after[0].content, doc_content_before,
        "doc rows are never written"
    );
    assert_eq!(docs_after[0].content_hash.as_deref(), Some("hash_v1"));

    // Tick 2: watermark + pair guard -> nothing new.
    let r2 = run_reconcile_tick(&db, &llm, &prompts, &refinery, &distillation)
        .await
        .unwrap();
    assert_eq!(
        r2.proposed, 0,
        "treadmill guard + watermark: no re-proposal"
    );
    assert_eq!(db.list_pending_revisions(10).await.unwrap().len(), 1);

    // Accept: revision becomes the active cited capture; the wrong capture is
    // suppressed. (HTTP handler is a thin wrapper over this same call.)
    let rev_id = pending[0].revision_source_id.clone();
    db.accept_pending_revision(&rev_id).await.unwrap();
    assert!(db.list_pending_revisions(10).await.unwrap().is_empty());
    // The accepted revision is now a live capture; the old one is not.
    let caps = db.reconcile_capture_frontier(0, "", -1, 50).await.unwrap();
    let ids: Vec<&str> = caps.iter().map(|c| c.source_id.as_str()).collect();
    assert!(
        !ids.contains(&"mem_wrong"),
        "suppressed capture (confirmed=0) leaves the frontier"
    );
    assert!(
        !ids.contains(&rev_id.as_str()),
        "accepted reconcile row is excluded from frontier (b) — no self-churn"
    );
}

#[tokio::test]
async fn back_pressure_holds_sweep_at_pending_cap() {
    let (_dir, db) = temp_db().await;
    // Stage RECONCILE_PENDING_CAP + 1 fake pending reconcile rows.
    let mut rows = Vec::new();
    for i in 0..=RECONCILE_PENDING_CAP {
        let mut r = capture(&format!("mem_rev{i}"), "r", None, 100);
        r.source_agent = Some("reconcile".to_string());
        r.confirmed = None;
        r.pending_revision = true;
        r.supersedes = Some(format!("mem_target{i}"));
        rows.push(r);
    }
    db.upsert_documents(rows).await.unwrap();

    let llm: Arc<dyn LlmProvider> = Arc::new(StubJudge);
    let r = run_reconcile_tick(
        &db,
        &llm,
        &PromptRegistry::default(),
        &RefineryConfig::default(),
        &DistillationConfig::default(),
    )
    .await
    .unwrap();
    assert!(r.skipped_backpressure);
    assert_eq!(r.judged, 0, "watermarks hold, nothing judged");
}

/// A judge that returns the capture's text unchanged — observed live with
/// small on-device models. A no-op "revision" must never reach the human
/// queue.
struct EchoJudge;

#[async_trait::async_trait]
impl LlmProvider for EchoJudge {
    async fn generate(&self, req: LlmRequest) -> Result<String, LlmError> {
        if req.label.as_deref() == Some("doc_reconcile") {
            Ok(
                r#"{"conflicts":[{"idx":0,"revised_content":"The daemon listens on port 9999."}]}"#
                    .to_string(),
            )
        } else {
            Ok("{}".to_string())
        }
    }
    fn is_available(&self) -> bool {
        true
    }
    fn name(&self) -> &str {
        "echo-judge"
    }
    fn backend(&self) -> LlmBackend {
        LlmBackend::Api
    }
}

#[tokio::test]
async fn echoed_capture_text_is_not_staged() {
    let (_dir, db) = temp_db().await;
    let llm: Arc<dyn LlmProvider> = Arc::new(EchoJudge);
    let prompts = PromptRegistry::default();
    let refinery = RefineryConfig::default();
    let distillation = DistillationConfig::default();

    db.upsert_documents(vec![capture(
        "mem_wrong",
        "The daemon listens on port 9999.",
        None,
        100,
    )])
    .await
    .unwrap();
    db.upsert_documents(vec![doc(
        "src_f1::notes/net.md",
        "The daemon listens on port 7878.",
        "hash_v1",
        200,
    )])
    .await
    .unwrap();

    let r = run_reconcile_tick(&db, &llm, &prompts, &refinery, &distillation)
        .await
        .unwrap();
    assert!(r.judged >= 1, "the pair is judged");
    assert_eq!(
        r.proposed, 0,
        "echoed capture text is a no-op, never staged"
    );
    assert!(db.list_pending_revisions(10).await.unwrap().is_empty());
}
