// SPDX-License-Identifier: Apache-2.0
//! Integration tests: X-Origin-Space header fallback in POST /api/memory/store.
//!
//! - When body omits `space`, the header value is used.
//! - When body supplies `space`, the body wins regardless of the header.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use origin_types::responses::StoreMemoryResponse;
use tower::ServiceExt;

async fn body_as_json<T: serde::de::DeserializeOwned>(response: axum::http::Response<Body>) -> T {
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).expect("response body is valid JSON of expected type")
}

#[tokio::test]
async fn header_used_when_body_omits_space() {
    let (router, _tmp, db) = common::test_app().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/memory/store")
        .header("Content-Type", "application/json")
        .header("X-Origin-Space", "career")
        .body(Body::from(
            serde_json::json!({
                "content": "header fallback test memory content",
                "memory_type": "fact"
            })
            .to_string(),
        ))
        .unwrap();

    let res = router.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK, "store must return 200");

    let stored: StoreMemoryResponse = body_as_json(res).await;
    let space = db
        .get_memory_space(&stored.source_id)
        .await
        .expect("get_memory_space must not fail");
    assert_eq!(
        space.as_deref(),
        Some("career"),
        "stored memory must have space=career from header, got: {space:?}"
    );
}

#[tokio::test]
async fn body_space_wins_over_header() {
    let (router, _tmp, db) = common::test_app().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/memory/store")
        .header("Content-Type", "application/json")
        .header("X-Origin-Space", "career")
        .body(Body::from(
            serde_json::json!({
                "content": "body space wins test memory content here",
                "memory_type": "fact",
                "space": "health"
            })
            .to_string(),
        ))
        .unwrap();

    let res = router.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK, "store must return 200");

    let stored: StoreMemoryResponse = body_as_json(res).await;
    let space = db
        .get_memory_space(&stored.source_id)
        .await
        .expect("get_memory_space must not fail");
    assert_eq!(
        space.as_deref(),
        Some("health"),
        "stored memory must have space=health from body (not career from header), got: {space:?}"
    );
}
