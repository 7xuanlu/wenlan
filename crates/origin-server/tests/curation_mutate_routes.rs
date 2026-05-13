// SPDX-License-Identifier: Apache-2.0
//! HTTP integration tests for Spec C-2 curation mutate routes.
//! Boots the real Axum router with a temp-dir MemoryDB and asserts:
//!   - typed responses match the wire-type structs
//!   - missing ids return HTTP 404 (Shape A/B/C)
//!   - re-call returns HTTP 404 (row consumed after first approve/dismiss)
//!   - x-agent-name header threads into agent_activity rows

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use origin_types::{EntitySuggestionApproveResponse, EntitySuggestionDismissResponse};
use tower::ServiceExt;

async fn body_as_json<T: serde::de::DeserializeOwned>(response: axum::http::Response<Body>) -> T {
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).expect("response body is valid JSON of expected type")
}

/// Seed a `suggest_entity` row via the public `insert_refinement_proposal` API.
async fn seed_entity_suggestion(
    db: &std::sync::Arc<origin_core::db::MemoryDB>,
    id: &str,
    entity_name: &str,
) {
    db.insert_refinement_proposal(id, "suggest_entity", &[], Some(entity_name), 0.9)
        .await
        .expect("seed_entity_suggestion must succeed");
    // insert_refinement_proposal uses INSERT OR REPLACE with default status='pending'.
    // get_pending_refinements queries both 'pending' and 'awaiting_review', so this works.
}

#[tokio::test]
async fn approve_entity_suggestion_creates_entity_and_returns_typed_response() {
    let (router, _tmp, db) = common::test_app().await;
    seed_entity_suggestion(&db, "ref_a1", "Acme Corp").await;

    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/memory/entity-suggestions/ref_a1/approve")
                .header("x-agent-name", "test-agent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let parsed: EntitySuggestionApproveResponse = body_as_json(response).await;
    assert_eq!(parsed.suggestion_id, "ref_a1");
    assert_eq!(parsed.entity_name, "Acme Corp");
    assert!(parsed.wrote);
}

#[tokio::test]
async fn approve_entity_suggestion_returns_404_on_missing_id() {
    let (router, _tmp, _db) = common::test_app().await;

    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/memory/entity-suggestions/ref_missing/approve")
                .header("x-agent-name", "test-agent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn approve_entity_suggestion_threads_x_agent_name_into_activity_log() {
    let (router, _tmp, db) = common::test_app().await;
    seed_entity_suggestion(&db, "ref_attr_1", "AttrCorp").await;

    router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/memory/entity-suggestions/ref_attr_1/approve")
                .header("x-agent-name", "claude-code-test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // canonicalize_agent_id("claude-code-test") = "claude-code-test" (already canonical)
    let count = common::count_activity_for_action_and_agent(
        &db,
        "entity_suggestion_approve",
        "claude-code-test",
    )
    .await;
    assert_eq!(
        count, 1,
        "should log exactly one approve activity row attributed to claude-code-test"
    );
}

#[tokio::test]
async fn approve_entity_suggestion_returns_404_on_re_call() {
    let (router, _tmp, db) = common::test_app().await;
    seed_entity_suggestion(&db, "ref_recall_1", "RecallCo").await;

    // First call: should succeed
    let first = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/memory/entity-suggestions/ref_recall_1/approve")
                .header("x-agent-name", "test-agent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);

    // Second call: should 404 (proposal is resolved, no longer in awaiting_review/pending)
    let second = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/memory/entity-suggestions/ref_recall_1/approve")
                .header("x-agent-name", "test-agent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::NOT_FOUND);
}

// ── dismiss_entity_suggestion ────────────────────────────────────────────────

#[tokio::test]
async fn dismiss_entity_suggestion_succeeds_and_returns_typed_response() {
    let (router, _tmp, db) = common::test_app().await;
    seed_entity_suggestion(&db, "ref_dd1", "X").await;

    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/memory/entity-suggestions/ref_dd1/dismiss")
                .header("x-agent-name", "test-agent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let parsed: EntitySuggestionDismissResponse = body_as_json(response).await;
    assert_eq!(parsed.suggestion_id, "ref_dd1");
    assert!(parsed.wrote);
}

#[tokio::test]
async fn dismiss_entity_suggestion_returns_404_on_missing_id() {
    let (router, _tmp, _db) = common::test_app().await;

    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/memory/entity-suggestions/ref_missing/dismiss")
                .header("x-agent-name", "test-agent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn dismiss_entity_suggestion_returns_404_on_re_call() {
    let (router, _tmp, db) = common::test_app().await;
    seed_entity_suggestion(&db, "ref_dd2", "Y").await;

    let first = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/memory/entity-suggestions/ref_dd2/dismiss")
                .header("x-agent-name", "test-agent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);

    let second = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/memory/entity-suggestions/ref_dd2/dismiss")
                .header("x-agent-name", "test-agent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn dismiss_entity_suggestion_threads_x_agent_name() {
    let (router, _tmp, db) = common::test_app().await;
    seed_entity_suggestion(&db, "ref_dd3", "Z").await;

    router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/memory/entity-suggestions/ref_dd3/dismiss")
                .header("x-agent-name", "cursor-test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let count = common::count_activity_for_action_and_agent(
        &db,
        "entity_suggestion_dismiss",
        "cursor-test",
    )
    .await;
    assert_eq!(count, 1);
}
