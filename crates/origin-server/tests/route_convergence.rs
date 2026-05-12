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
use origin_core::db::MemoryDB;
use origin_core::events::NoopEmitter;
use origin_server::router::build_router;
use origin_server::state::ServerState;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

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
