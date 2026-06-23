// SPDX-License-Identifier: Apache-2.0
//! Integration tests for the 4 converged write-path routes.
//!
//! Drives the axum Router via tower::ServiceExt::oneshot — no TCP socket.
//! Tests that the HTTP layer (route registration, JSON extraction, error
//! mapping) stays in sync with the origin-types wire shapes and
//! post_write capability signatures.
//!
//! create_page integration is deferred: it requires a memory seeded via
//! /api/memory/store (FastEmbed) before the hallucination guard can run,
//! which adds significant setup cost. The post_write::create_page unit
//! tests in origin-core already cover the logic; the HTTP shim is smoke-
//! tested via the manual smoke test. See task notes for rationale.

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;
use wenlan_core::db::MemoryDB;
use wenlan_core::events::NoopEmitter;
use wenlan_server::router::build_router;
use wenlan_server::state::ServerState;

async fn test_app() -> (axum::Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db = MemoryDB::new(dir.path(), Arc::new(NoopEmitter))
        .await
        .unwrap();
    let state = ServerState {
        db: Some(Arc::new(db)),
        ..ServerState::default()
    };
    let router = build_router(Arc::new(RwLock::new(state)));
    (router, dir)
}

async fn json_post(
    app: &axum::Router,
    path: &str,
    agent: Option<&str>,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(a) = agent {
        builder = builder.header("x-agent-name", a);
    }
    let req = builder
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let val: serde_json::Value = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
    };
    (status, val)
}

// ── handle_create_entity ────────────────────────────────────────────────────

#[tokio::test]
async fn create_entity_happy_path() {
    let (app, _dir) = test_app().await;
    let (status, body) = json_post(
        &app,
        "/api/memory/entities",
        Some("test-agent"),
        serde_json::json!({"name": "TestEntity", "entity_type": "project"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert!(
        body["id"].as_str().is_some(),
        "expected id in response, got: {body}"
    );
}

#[tokio::test]
async fn create_entity_empty_name_returns_422() {
    let (app, _dir) = test_app().await;
    let (status, _body) = json_post(
        &app,
        "/api/memory/entities",
        None,
        serde_json::json!({"name": "", "entity_type": "person"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

// ── handle_create_relation ──────────────────────────────────────────────────

#[tokio::test]
async fn create_relation_happy_path() {
    let (app, _dir) = test_app().await;

    let (_, e1) = json_post(
        &app,
        "/api/memory/entities",
        Some("test"),
        serde_json::json!({"name": "Alice", "entity_type": "person"}),
    )
    .await;
    let (_, e2) = json_post(
        &app,
        "/api/memory/entities",
        Some("test"),
        serde_json::json!({"name": "Bob", "entity_type": "person"}),
    )
    .await;
    let from = e1["id"].as_str().expect("e1 missing id");
    let to = e2["id"].as_str().expect("e2 missing id");

    let (status, body) = json_post(
        &app,
        "/api/memory/relations",
        Some("test"),
        serde_json::json!({"from_entity": from, "to_entity": to, "relation_type": "knows"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert!(
        body["id"].as_str().is_some(),
        "expected id in response, got: {body}"
    );
}

#[tokio::test]
async fn create_relation_missing_entities_returns_422() {
    let (app, _dir) = test_app().await;
    let (status, _body) = json_post(
        &app,
        "/api/memory/relations",
        None,
        serde_json::json!({
            "from_entity": "no-such-id",
            "to_entity": "also-missing",
            "relation_type": "knows"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

// ── handle_add_observation ──────────────────────────────────────────────────

#[tokio::test]
async fn add_observation_happy_path() {
    let (app, _dir) = test_app().await;

    let (_, e) = json_post(
        &app,
        "/api/memory/entities",
        Some("test"),
        serde_json::json!({"name": "Subject", "entity_type": "person"}),
    )
    .await;
    let entity_id = e["id"].as_str().expect("entity missing id");

    let (status, body) = json_post(
        &app,
        "/api/memory/observations",
        Some("test"),
        serde_json::json!({"entity_id": entity_id, "content": "Subject likes Rust"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert!(
        body["id"].as_str().is_some(),
        "expected id in response, got: {body}"
    );
}

#[tokio::test]
async fn add_observation_short_content_returns_422() {
    let (app, _dir) = test_app().await;

    let (_, e) = json_post(
        &app,
        "/api/memory/entities",
        Some("test"),
        serde_json::json!({"name": "S", "entity_type": "person"}),
    )
    .await;
    let entity_id = e["id"].as_str().expect("entity missing id");

    let (status, _body) = json_post(
        &app,
        "/api/memory/observations",
        None,
        serde_json::json!({"entity_id": entity_id, "content": "hi"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

// ── Revision history endpoints ───────────────────────────────────────────────

/// Helper: return (app, Arc<MemoryDB>, TempDir) for tests that need direct DB
/// access alongside the HTTP router.
async fn test_app_with_db() -> (axum::Router, Arc<MemoryDB>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(
        MemoryDB::new(dir.path(), Arc::new(NoopEmitter))
            .await
            .unwrap(),
    );
    let state = ServerState {
        db: Some(Arc::clone(&db)),
        ..ServerState::default()
    };
    let router = build_router(Arc::new(RwLock::new(state)));
    (router, db, dir)
}

async fn json_get(app: &axum::Router, path: &str) -> (StatusCode, serde_json::Value) {
    let req = Request::builder()
        .method(Method::GET)
        .uri(path)
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let val: serde_json::Value = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
    };
    (status, val)
}

/// Non-existent source_id returns 200 with an empty entries array.
/// walk_supersede_chain returns [] for unknown ids; the handler wraps that.
#[tokio::test]
async fn memory_revisions_unknown_id_returns_empty_chain() {
    let (app, _dir) = test_app().await;
    let (status, body) = json_get(&app, "/api/memory/nonexistent_mem_id/revisions").await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(
        body["current_source_id"].as_str(),
        Some("nonexistent_mem_id"),
        "envelope current_source_id mismatch: {body}"
    );
    assert_eq!(
        body["chain_depth"].as_i64(),
        Some(0),
        "chain_depth should be 0 for unknown id: {body}"
    );
    assert!(
        body["entries"]
            .as_array()
            .map(|a| a.is_empty())
            .unwrap_or(false),
        "entries should be empty for unknown id: {body}"
    );
}

/// Non-existent page id returns 404.
#[tokio::test]
async fn page_revisions_unknown_id_returns_404() {
    let (app, _dir) = test_app().await;
    let (status, _body) = json_get(&app, "/api/pages/nonexistent_page_id/revisions").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// Known page returns 200 with correct envelope and empty entries list
/// (newly inserted pages have changelog = '[]').
#[tokio::test]
async fn page_revisions_known_page_returns_envelope() {
    let (app, db, _dir) = test_app_with_db().await;

    let page_id = "page_rev_test_001";
    db.insert_page(
        page_id,
        "Test Revision Page",
        Some("A page for revision testing"),
        "Full content of the test page for revision surfacing.",
        None,
        None,
        &[],
        "2026-01-01T00:00:00Z",
    )
    .await
    .unwrap();

    let (status, body) = json_get(&app, &format!("/api/pages/{page_id}/revisions")).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(
        body["page_id"].as_str(),
        Some(page_id),
        "envelope page_id mismatch: {body}"
    );
    assert_eq!(
        body["current_version"].as_i64(),
        Some(1),
        "newly inserted page should have version=1: {body}"
    );
    assert_eq!(
        body["user_edited"].as_bool(),
        Some(false),
        "newly inserted page should not be user_edited: {body}"
    );
    assert!(
        body["entries"]
            .as_array()
            .map(|a| a.is_empty())
            .unwrap_or(false),
        "fresh page should have empty changelog entries: {body}"
    );
}

// ── POST /api/memory/list with confirmed filter ──────────────────────────────

#[tokio::test]
async fn list_memories_confirmed_false_filters_unconfirmed() {
    use wenlan_core::sources::RawDocument;

    let (app, db, _dir) = test_app_with_db().await;

    // Insert one confirmed and one unconfirmed memory directly via DB.
    let doc_confirmed = RawDocument {
        source: "memory".to_string(),
        source_id: "mem_conf_001".to_string(),
        title: "Confirmed memory".to_string(),
        content: "Some confirmed content about widgets.".to_string(),
        last_modified: 1_000_000,
        ..Default::default()
    };
    let doc_unconfirmed = RawDocument {
        source: "memory".to_string(),
        source_id: "mem_unconf_001".to_string(),
        title: "Unconfirmed memory".to_string(),
        content: "Some unconfirmed content about gears.".to_string(),
        last_modified: 2_000_000,
        ..Default::default()
    };
    db.upsert_documents(vec![doc_confirmed, doc_unconfirmed])
        .await
        .unwrap();
    // Flip the confirmed memory to confirmed=true via the public API
    db.confirm_memory("mem_conf_001").await.unwrap();

    // POST /api/memory/list with confirmed=false
    let (status, body) = json_post(
        &app,
        "/api/memory/list",
        None,
        serde_json::json!({"confirmed": false, "limit": 50}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");

    let memories = body["memories"].as_array().expect("memories array");
    let ids: Vec<&str> = memories
        .iter()
        .filter_map(|m| m["source_id"].as_str())
        .collect();
    assert!(
        ids.contains(&"mem_unconf_001"),
        "unconfirmed memory must appear; got: {ids:?}"
    );
    assert!(
        !ids.contains(&"mem_conf_001"),
        "confirmed memory must not appear; got: {ids:?}"
    );

    // created_at must be present and non-null
    let unconf = memories
        .iter()
        .find(|m| m["source_id"].as_str() == Some("mem_unconf_001"))
        .expect("unconfirmed memory in response");
    assert!(
        unconf["created_at"].is_number(),
        "created_at must be a number; got: {unconf}"
    );
}

// ── POST /api/memory/:id/update-page demotes review_status ──────────────────

#[tokio::test]
async fn manual_page_edit_route_demotes_review_status_to_unconfirmed() {
    let (app, db, _dir) = test_app_with_db().await;
    let now = chrono::Utc::now().to_rfc3339();
    // Seed a confirmed distilled page directly.
    db.insert_page(
        "page_demote",
        "T",
        Some("s"),
        "original body",
        None,
        None,
        &[],
        &now,
    )
    .await
    .unwrap();
    assert_eq!(
        db.get_page("page_demote")
            .await
            .unwrap()
            .unwrap()
            .review_status,
        "confirmed",
        "freshly seeded distilled page starts confirmed"
    );
    // Manual edit via the HTTP route.
    let (status, _body) = json_post(
        &app,
        "/api/memory/page_demote/update-page",
        Some("tester"),
        serde_json::json!({ "content": "manually edited body" }),
    )
    .await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "update route should 200"
    );
    // The trust boundary must cover this bypass route.
    assert_eq!(
        db.get_page("page_demote")
            .await
            .unwrap()
            .unwrap()
            .review_status,
        "unconfirmed",
        "manual edit must demote the page to unconfirmed"
    );
}
