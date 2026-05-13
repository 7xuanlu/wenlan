// SPDX-License-Identifier: Apache-2.0
//! Integration tests for /api/refinery/queue endpoints.
//!
//! Drives the axum Router via tower::ServiceExt::oneshot — no TCP socket.
//! Tests: list filtering, typed payload decode, empty queue,
//! reject happy + default agent + 404 + 422.

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use origin_core::db::MemoryDB;
use origin_core::events::NoopEmitter;
use origin_server::router::build_router;
use origin_server::state::ServerState;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

async fn test_app() -> (axum::Router, tempfile::TempDir, Arc<MemoryDB>) {
    let dir = tempfile::tempdir().unwrap();
    let db = MemoryDB::new(dir.path(), Arc::new(NoopEmitter))
        .await
        .unwrap();
    let db_arc = Arc::new(db);
    let state = ServerState {
        db: Some(db_arc.clone()),
        ..ServerState::default()
    };
    let router = build_router(Arc::new(RwLock::new(state)));
    (router, dir, db_arc)
}

/// Insert a proposal and immediately transition it to `awaiting_review` so the
/// list route (which filters on that status) can surface it.
async fn seed_awaiting(
    db: &Arc<MemoryDB>,
    id: &str,
    action: &str,
    payload: Option<&str>,
    confidence: f64,
) {
    db.insert_refinement_proposal(
        id,
        action,
        &["src_a".to_string(), "src_b".to_string()],
        payload,
        confidence,
    )
    .await
    .unwrap();
    db.resolve_refinement(id, "awaiting_review").await.unwrap();
}

async fn read_body_json(resp: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn list_queue_returns_only_awaiting_review() {
    let (app, _tmp, db) = test_app().await;
    seed_awaiting(&db, "ref_a", "entity_merge", None, 0.86).await;
    seed_awaiting(&db, "ref_b", "entity_merge", None, 0.87).await;
    // ref_c: insert then immediately dismiss — should not appear
    db.insert_refinement_proposal("ref_c", "entity_merge", &["src_a".to_string()], None, 0.88)
        .await
        .unwrap();
    db.resolve_refinement("ref_c", "dismissed").await.unwrap();

    let req = Request::builder()
        .method(Method::GET)
        .uri("/api/refinery/queue")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = read_body_json(resp).await;
    let proposals = body["proposals"].as_array().unwrap();
    assert_eq!(proposals.len(), 2, "dismissed proposal must not appear");
}

#[tokio::test]
async fn list_queue_filters_by_action() {
    let (app, _tmp, db) = test_app().await;
    seed_awaiting(&db, "r1", "entity_merge", None, 0.86).await;
    seed_awaiting(&db, "r2", "relation_conflict", None, 0.7).await;
    seed_awaiting(&db, "r3", "suggest_entity", Some("\"X\""), 0.9).await;

    let req = Request::builder()
        .method(Method::GET)
        .uri("/api/refinery/queue?action=entity_merge")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = read_body_json(resp).await;
    let proposals = body["proposals"].as_array().unwrap();
    assert_eq!(proposals.len(), 1);
    assert_eq!(proposals[0]["action"], "entity_merge");
}

#[tokio::test]
async fn list_queue_decodes_typed_payload() {
    let (app, _tmp, db) = test_app().await;
    let payload = r#"{"existing_id":"e1","new_id":"e2","similarity":0.86}"#;
    seed_awaiting(&db, "ref_typed", "entity_merge", Some(payload), 0.86).await;

    let req = Request::builder()
        .method(Method::GET)
        .uri("/api/refinery/queue")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = read_body_json(resp).await;
    // RefinementPayload is a serde internally-tagged enum with tag "action",
    // so the serialized object carries `"action": "entity_merge"` alongside the fields.
    let payload_val = &body["proposals"][0]["payload"];
    assert_eq!(payload_val["action"], "entity_merge");
    assert_eq!(payload_val["existing_id"], "e1");
    assert_eq!(payload_val["new_id"], "e2");
}

#[tokio::test]
async fn list_queue_empty_returns_200() {
    let (app, _tmp, _db) = test_app().await;

    let req = Request::builder()
        .method(Method::GET)
        .uri("/api/refinery/queue")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = read_body_json(resp).await;
    assert_eq!(body["proposals"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn reject_marks_dismissed_and_logs_activity() {
    let (app, _tmp, db) = test_app().await;
    // Reject works from 'pending' status directly (resolve_proposal only guards terminal states)
    db.insert_refinement_proposal(
        "ref_rej_1",
        "detect_contradiction",
        &["src_a".to_string()],
        None,
        0.8,
    )
    .await
    .unwrap();

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/refinery/queue/ref_rej_1/reject")
        .header("x-agent-name", "claude-code")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify queue row dismissed
    let pending = db.get_pending_refinements().await.unwrap();
    assert!(!pending.iter().any(|p| p.id == "ref_rej_1"));

    // Verify activity row logged with x-agent-name value
    let acts = db
        .list_agent_activity(10, Some("claude-code"), None)
        .await
        .unwrap_or_default();
    assert!(
        acts.iter().any(|a| a.action == "refinement_resolve"),
        "activity row should be logged with x-agent-name value"
    );
}

#[tokio::test]
async fn reject_default_agent_when_header_missing() {
    let (app, _tmp, db) = test_app().await;
    db.insert_refinement_proposal(
        "ref_no_agent",
        "entity_merge",
        &["src_a".to_string()],
        None,
        0.86,
    )
    .await
    .unwrap();

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/refinery/queue/ref_no_agent/reject")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify queue row dismissed regardless of which agent name was used
    let pending = db.get_pending_refinements().await.unwrap();
    assert!(!pending.iter().any(|p| p.id == "ref_no_agent"));

    // extract_agent_name defaults to "unknown" when header is absent
    let acts = db
        .list_agent_activity(10, Some("unknown"), None)
        .await
        .unwrap_or_default();
    assert!(
        acts.iter().any(|a| a.action == "refinement_resolve"),
        "default-agent path should log activity under 'unknown'"
    );
}

#[tokio::test]
async fn reject_unknown_id_returns_404() {
    let (app, _tmp, _db) = test_app().await;

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/refinery/queue/ref_does_not_exist/reject")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn reject_already_terminal_returns_422() {
    let (app, _tmp, db) = test_app().await;
    db.insert_refinement_proposal(
        "ref_done",
        "entity_merge",
        &["src_a".to_string()],
        None,
        0.86,
    )
    .await
    .unwrap();
    db.resolve_refinement("ref_done", "dismissed")
        .await
        .unwrap();

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/refinery/queue/ref_done/reject")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}
