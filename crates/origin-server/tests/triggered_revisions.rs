// SPDX-License-Identifier: Apache-2.0
//! Integration tests for the topic-match-protected -> triggered_revisions fix.
//!
//! Verifies:
//!   1. Storing a memory against a protected target auto-sets `supersedes` so
//!      `list_pending_revisions` surfaces the row (the bug: supersedes=NULL
//!      caused it to be invisible).
//!   2. The `/api/memory/store` response includes `triggered_revisions` with
//!      the matched target id.
//!
//! These tests use `test_app_no_gate()` so the quality-gate novelty filter
//! does not reject content that is intentionally similar to an existing
//! protected memory. The topic-match + pending-revision path is exercised
//! with the gate out of the way.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use origin_types::responses::{PendingRevisionItem, StoreMemoryResponse};
use tower::ServiceExt;

async fn body_as_json<T: serde::de::DeserializeOwned>(response: axum::http::Response<Body>) -> T {
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).expect("response body is valid JSON of expected type")
}

/// Store a memory via the HTTP API and return the response.
///
/// `domain` + `memory_type` are included in the request body so that
/// topic-match uses the "exact domain+type" threshold (0.70) rather than
/// the stricter semantic-only threshold (0.90). This matters for test
/// content that sits at ~0.77 cosine similarity.
async fn store_memory_via_http(
    router: &axum::Router,
    content: &str,
    memory_type: &str,
    domain: &str,
) -> axum::http::Response<Body> {
    let body = serde_json::json!({
        "content": content,
        "memory_type": memory_type,
        "domain": domain,
    });
    router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/memory/store")
                .header("content-type", "application/json")
                .header("x-agent-name", "test-agent")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap()
}

/// List pending revisions via the HTTP API.
async fn list_pending_revisions(router: &axum::Router) -> Vec<PendingRevisionItem> {
    let resp = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/memory/pending-revisions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "pending-revisions endpoint must return 200"
    );
    body_as_json(resp).await
}

#[tokio::test]
async fn topic_match_protected_auto_sets_supersedes_and_surfaces_in_pending_revisions() {
    let (router, _tmp, db) = common::test_app_no_gate().await;

    // Seed the target memory via HTTP with explicit domain + memory_type.
    // Using the same domain/type for both seed and capture means topic-match
    // uses threshold_exact=0.70 rather than threshold_none=0.90 (semantic
    // only). The TDD-vs-BDD content sits at ~0.77 cosine similarity, which
    // falls above 0.70 but below 0.90 — exact-match threshold needed here.
    let seed_resp = store_memory_via_http(
        &router,
        "I prefer TDD because it catches regressions early.",
        "preference",
        "engineering",
    )
    .await;
    assert_eq!(
        seed_resp.status(),
        StatusCode::OK,
        "seed store must succeed"
    );
    let seed: StoreMemoryResponse = body_as_json(seed_resp).await;
    let protected_id = seed.source_id.clone();

    // Mark as confirmed (= protected) so is_memory_protected returns true.
    db.confirm_memory(&protected_id)
        .await
        .expect("confirm_memory must succeed");

    // Store a contradicting capture via HTTP with matching domain + type.
    let resp = store_memory_via_http(
        &router,
        "I prefer BDD over TDD because specs stay closer to requirements.",
        "preference",
        "engineering",
    )
    .await;
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "contradicting capture must succeed"
    );
    let store_resp: StoreMemoryResponse = body_as_json(resp).await;

    // Verify triggered_revisions is populated in the store response.
    assert_eq!(
        store_resp.triggered_revisions,
        vec![protected_id.clone()],
        "triggered_revisions must contain the matched protected target id"
    );

    // Verify the new memory surfaces in list_pending_revisions (the bug: without
    // auto-set of supersedes, list_pending_revisions filtered by
    // `supersedes IS NOT NULL` and returned nothing).
    let pending = list_pending_revisions(&router).await;
    assert!(
        !pending.is_empty(),
        "pending revisions must be non-empty after topic-match-protected capture"
    );
    assert_eq!(
        pending[0].target_source_id, protected_id,
        "pending revision must reference the protected target"
    );
    assert_eq!(
        pending[0].revision_source_id, store_resp.source_id,
        "pending revision_source_id must match the newly stored memory"
    );
}

#[tokio::test]
async fn non_protected_topic_match_does_not_set_triggered_revisions() {
    let (router, _tmp, _db) = common::test_app_no_gate().await;

    // Seed a memory that is NOT protected (stability = 'new', not confirmed).
    // No confirm call follows, so is_memory_protected returns false.
    let seed_resp = store_memory_via_http(
        &router,
        "I prefer TDD because it catches regressions early.",
        "preference",
        "engineering",
    )
    .await;
    assert_eq!(
        seed_resp.status(),
        StatusCode::OK,
        "seed store must succeed"
    );

    let resp = store_memory_via_http(
        &router,
        "I prefer BDD over TDD because specs stay closer to requirements.",
        "preference",
        "engineering",
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let store_resp: StoreMemoryResponse = body_as_json(resp).await;

    // Non-protected match must NOT populate triggered_revisions.
    assert!(
        store_resp.triggered_revisions.is_empty(),
        "triggered_revisions must be empty when matched memory is not protected"
    );

    // And pending-revisions must be empty too.
    let pending = list_pending_revisions(&router).await;
    assert!(
        pending.is_empty(),
        "no pending revisions expected for non-protected topic match"
    );
}
