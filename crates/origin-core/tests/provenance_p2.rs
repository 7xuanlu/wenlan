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

/// Sorted memory-locator set from page_evidence vs sorted memory_source_id
/// set from page_sources for a page. They must be equal (dual-write contract).
async fn evidence_vs_sources(db: &MemoryDB, page_id: &str) -> (Vec<String>, Vec<String>) {
    let mut ev: Vec<String> = db
        .get_page_evidence(page_id)
        .await
        .unwrap()
        .into_iter()
        .filter(|e| e.source_kind == "memory")
        .filter_map(|e| e.locator)
        .collect();
    let mut ps: Vec<String> = db
        .get_page_sources(page_id)
        .await
        .unwrap()
        .into_iter()
        .map(|s| s.memory_source_id)
        .collect();
    ev.sort();
    ps.sort();
    (ev, ps)
}

#[tokio::test]
async fn dual_write_keeps_page_evidence_consistent_with_page_sources() {
    let (db, _d) = make_db().await;
    seed_memory(&db, "mem_a", "alpha").await;
    seed_memory(&db, "mem_b", "beta").await;
    seed_memory(&db, "mem_c", "gamma").await;
    let now = chrono::Utc::now().to_rfc3339();
    db.insert_page(
        "page_1",
        "T",
        Some("s"),
        "body",
        None,
        None,
        &["mem_a", "mem_b"],
        &now,
    )
    .await
    .unwrap();
    let (ev, ps) = evidence_vs_sources(&db, "page_1").await;
    assert_eq!(ev, ps, "insert_page diverged");
    db.update_page_content("page_1", "body2", &["mem_a", "mem_c"], "manual_edit")
        .await
        .unwrap();
    let (ev, ps) = evidence_vs_sources(&db, "page_1").await;
    assert_eq!(ev, ps, "update_page_content diverged");
    db.link_page_source("page_1", "mem_b", "page_growth")
        .await
        .unwrap();
    let (ev, ps) = evidence_vs_sources(&db, "page_1").await;
    assert_eq!(ev, ps, "link_page_source diverged");
}

#[tokio::test]
async fn update_prunes_memory_evidence_but_preserves_external() {
    let (db, _d) = make_db().await;
    seed_memory(&db, "mem_a", "alpha").await;
    seed_memory(&db, "mem_b", "beta").await;
    let now = chrono::Utc::now().to_rfc3339();
    db.insert_page(
        "page_x",
        "T",
        Some("s"),
        "body",
        None,
        None,
        &["mem_a", "mem_b"],
        &now,
    )
    .await
    .unwrap();
    // Attach a non-memory evidence row directly (the row a memory-source edit must NOT touch).
    db.link_page_evidence(
        "page_x",
        "external_url",
        Some("https://example.com"),
        Some("Example"),
        "manual",
    )
    .await
    .unwrap();

    // Edit drops mem_b. Memory rows reconcile; the external row must survive.
    db.update_page_content("page_x", "body2", &["mem_a"], "manual_edit")
        .await
        .unwrap();
    let ev = db.get_page_evidence("page_x").await.unwrap();
    let mem: Vec<String> = ev
        .iter()
        .filter(|e| e.source_kind == "memory")
        .filter_map(|e| e.locator.clone())
        .collect();
    assert_eq!(
        mem,
        vec!["mem_a".to_string()],
        "memory evidence reconciled to new set"
    );
    assert!(
        ev.iter().any(|e| e.source_kind == "external_url"
            && e.locator.as_deref() == Some("https://example.com")),
        "external evidence must survive a memory-source edit"
    );

    // Empty-source edit: prune ALL memory rows; external still preserved.
    db.update_page_content("page_x", "body3", &[], "manual_edit")
        .await
        .unwrap();
    let ev = db.get_page_evidence("page_x").await.unwrap();
    assert!(
        !ev.iter().any(|e| e.source_kind == "memory"),
        "empty-source edit prunes all memory evidence"
    );
    assert!(
        ev.iter().any(|e| e.source_kind == "external_url"),
        "external evidence preserved on empty-source edit"
    );
}

#[tokio::test]
async fn distilled_page_defaults_creation_kind_distilled() {
    let (db, _d) = make_db().await;
    seed_memory(&db, "mem_a", "alpha").await;
    let now = chrono::Utc::now().to_rfc3339();
    db.insert_page(
        "page_1",
        "T",
        Some("s"),
        "body",
        None,
        None,
        &["mem_a"],
        &now,
    )
    .await
    .unwrap();
    let p = db.get_page("page_1").await.unwrap().unwrap();
    assert_eq!(p.creation_kind, "distilled");
}

/// Guards the `dist` read index in `find_matching_page` against creation_kind
/// column-shift drift. After adding creation_kind at index 16, the vector
/// distance moved to index 17. If the read regresses to index 16, it parses
/// the creation_kind TEXT column as f64 -> unwrap_or(1.0) -> uniform distance
/// 1.0 -> similarity 0.0 < threshold -> `find_matching_page` returns None.
/// With the correct index 17 it reads the real distance and returns the near
/// page. The None-vs-Some(near) boundary is binary, so this is non-flaky.
#[tokio::test]
async fn find_matching_page_reads_real_distance_not_creation_kind() {
    let (db, _d) = make_db().await;
    let now = chrono::Utc::now().to_rfc3339();
    db.insert_page(
        "p_rust",
        "Rust",
        Some("rust async tokio runtime"),
        "Rust ownership and the borrow checker.",
        None,
        None,
        &[],
        &now,
    )
    .await
    .unwrap();
    db.insert_page(
        "p_py",
        "Python",
        Some("python decorators metaclasses"),
        "Python data model and descriptors.",
        None,
        None,
        &[],
        &now,
    )
    .await
    .unwrap();

    // Query embedding near the Rust page (title + summary is what insert_page embeds).
    let q = db
        .generate_embeddings(&["Rust rust async tokio runtime ownership".to_string()])
        .unwrap()
        .remove(0);

    // entity_id=None forces the embedding path (skips the entity-first branch).
    // Threshold 0.5: the near page clears it; under a misread (uniform distance
    // 1.0 -> similarity 0.0) NO page clears it, so the result would be None.
    let matched = db.find_matching_page(None, &q, 0.5).await.unwrap();
    let page = matched.expect("near page must clear the similarity threshold");
    assert_eq!(
        page.id, "p_rust",
        "must match the embedding-nearest page, proving real distance was read"
    );
}

#[tokio::test]
async fn distilled_page_defaults_review_status_confirmed() {
    let (db, _d) = make_db().await;
    seed_memory(&db, "mem_a", "alpha").await;
    let now = chrono::Utc::now().to_rfc3339();
    db.insert_page(
        "page_1",
        "T",
        Some("s"),
        "body",
        None,
        None,
        &["mem_a"],
        &now,
    )
    .await
    .unwrap();
    let p = db.get_page("page_1").await.unwrap().unwrap();
    assert_eq!(p.review_status, "confirmed");
}

use origin_types::requests::CreateConceptRequest;

#[tokio::test]
async fn distilled_zero_source_page_rejected() {
    let (db, dir) = make_db().await;
    let req = CreateConceptRequest {
        title: "T".into(),
        content: "body".into(),
        summary: Some("s".into()),
        entity_id: None,
        space: None,
        source_memory_ids: vec![],
        creation_kind: Some("distilled".into()),
    };
    let r = origin_core::post_write::create_page(&db, req, "test", Some(dir.path())).await;
    assert!(matches!(
        r,
        Err(origin_core::error::OriginError::Validation(_))
    ));
}

#[tokio::test]
async fn authored_zero_source_page_accepted_unconfirmed() {
    let (db, dir) = make_db().await;
    let req = CreateConceptRequest {
        title: "Authored".into(),
        content: "hand written body".into(),
        summary: Some("sum".into()),
        entity_id: None,
        space: None,
        source_memory_ids: vec![],
        creation_kind: Some("authored".into()),
    };
    let wr = origin_core::post_write::create_page(&db, req, "test", Some(dir.path()))
        .await
        .unwrap();
    let p = db.get_page(&wr.id).await.unwrap().unwrap();
    assert_eq!(p.creation_kind, "authored");
    assert_eq!(p.review_status, "unconfirmed");
    assert!(
        db.get_page_evidence(&wr.id).await.unwrap().is_empty(),
        "authored page has zero evidence"
    );
}

#[tokio::test]
async fn garbage_creation_kind_rejected() {
    let (db, dir) = make_db().await;
    let req = CreateConceptRequest {
        title: "T".into(),
        content: "body".into(),
        summary: None,
        entity_id: None,
        space: None,
        source_memory_ids: vec![],
        creation_kind: Some("garbage".into()),
    };
    let r = origin_core::post_write::create_page(&db, req, "test", Some(dir.path())).await;
    assert!(
        matches!(r, Err(origin_core::error::OriginError::Validation(_))),
        "bad kind must be a 422-class Validation error, not a DB CHECK 500"
    );
}

#[tokio::test]
async fn scoped_matcher_excludes_user_edited_when_disallowed() {
    let (db, _d) = make_db().await;
    seed_memory(&db, "mem_a", "rust async runtime tokio").await;
    let now = chrono::Utc::now().to_rfc3339();
    db.insert_page(
        "page_ue",
        "Rust",
        Some("rust async tokio"),
        "body",
        None,
        Some("work"),
        &["mem_a"],
        &now,
    )
    .await
    .unwrap();
    // Mark user_edited via the manual-edit path (sets user_edited=1, keeps review_status=confirmed).
    db.update_page_content("page_ue", "edited body", &["mem_a"], "manual_edit")
        .await
        .unwrap();
    let emb = db
        .generate_embeddings(&["rust async tokio".to_string()])
        .unwrap()
        .remove(0);

    // CONTROL: with allow_user_edited=true, the page IS returned (proves it matches by embedding + workspace + confirmed).
    let allowed = db
        .find_matching_page_scoped(None, &emb, 0.5, Some("work"), true)
        .await
        .unwrap();
    assert_eq!(allowed.as_ref().map(|p| p.id.as_str()), Some("page_ue"),
        "control: a confirmed in-workspace page must match when user_edited is allowed (else threshold/embedding is the problem, fix the test seed)");

    // TEST: with allow_user_edited=false, the SAME page is REFUSED.
    let refused = db
        .find_matching_page_scoped(None, &emb, 0.5, Some("work"), false)
        .await
        .unwrap();
    assert!(
        refused.map(|p| p.id != "page_ue").unwrap_or(true),
        "must not return a user_edited page when disallowed"
    );
}

#[tokio::test]
async fn source_less_page_embeds_title_plus_content() {
    let (db, _d) = make_db().await;
    let now = chrono::Utc::now().to_rfc3339();
    // Source-less confirmed authored page: OFF-TOPIC title ("Cooking Recipes"),
    // Rust-specific body, NO summary.
    // Pre-Task-11 (title-only embed): "Cooking Recipes" embeds far from the Rust
    // query — cosine < 0.5 → find_matching_page_scoped returns None.
    // Post-Task-11 (title+content embed): the Rust-dense body pulls the vector
    // close to the query — cosine > 0.5 → returns the page.
    db.insert_page_with_kind(
        "p_notes", "Cooking Recipes", None,
        "Detailed notes about Rust ownership, borrowing, and the lifetime system used in async code.",
        None, None, &[], &now, "authored", "confirmed",
    ).await.unwrap();
    let q = db
        .generate_embeddings(&["Rust ownership borrowing lifetimes".to_string()])
        .unwrap()
        .remove(0);
    let m = db
        .find_matching_page_scoped(None, &q, 0.5, None, true)
        .await
        .unwrap();
    assert_eq!(m.as_ref().map(|p| p.id.as_str()), Some("p_notes"),
        "source-less page must embed its content so a content-related query matches above 0.5 (title alone would not)");
}

#[tokio::test]
async fn scoped_matcher_never_crosses_workspace() {
    let (db, _d) = make_db().await;
    seed_memory(&db, "mem_w", "kubernetes deployment rollout strategy").await;
    let now = chrono::Utc::now().to_rfc3339();
    // A confirmed page in workspace "personal".
    db.insert_page(
        "page_personal",
        "K8s",
        Some("kubernetes deployment rollout strategy"),
        "Kubernetes rollout strategies including blue-green and canary deployments.",
        None,
        Some("personal"),
        &["mem_w"],
        &now,
    )
    .await
    .unwrap();
    let q = db
        .generate_embeddings(&["kubernetes rollout strategy".to_string()])
        .unwrap()
        .remove(0);

    // Scoped to "work": the high-similarity "personal" page must NOT leak.
    let leaked = db
        .find_matching_page_scoped(None, &q, 0.5, Some("work"), true)
        .await
        .unwrap();
    assert!(
        leaked.is_none(),
        "SECURITY: scoped matcher leaked a cross-workspace page"
    );

    // CONTROL — scoped to "personal": it SHOULD match (proves the None above is a
    // workspace exclusion, not a threshold miss).
    let same = db
        .find_matching_page_scoped(None, &q, 0.5, Some("personal"), true)
        .await
        .unwrap();
    assert_eq!(
        same.as_ref().map(|p| p.id.as_str()),
        Some("page_personal"),
        "control: the page must match within its own workspace"
    );
}
