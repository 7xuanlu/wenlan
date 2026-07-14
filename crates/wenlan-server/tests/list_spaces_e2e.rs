// SPDX-License-Identifier: Apache-2.0
//! E2E acceptance: GET /api/spaces returns a populated Vec<Space>.
//!
//! Two tests:
//!   - `list_spaces_returns_seeded_spaces`: seed 2 memories into space=alpha and
//!     1 into space=beta (with explicit create_space calls), then assert the
//!     endpoint returns both spaces with correct memory_count values.
//!   - `list_spaces_empty_db_returns_empty_array`: empty DB, assert 200 + `[]`.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use wenlan_core::sources::RawDocument;
use wenlan_types::Space;

async fn body_as_json<T: serde::de::DeserializeOwned>(response: axum::http::Response<Body>) -> T {
    let bytes = axum::body::to_bytes(response.into_body(), 256 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).expect("response body is valid JSON of expected type")
}

async fn get_spaces(router: &common::AppRouter) -> (StatusCode, Vec<Space>) {
    let resp = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/spaces")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let spaces: Vec<Space> = body_as_json(resp).await;
    (status, spaces)
}

#[tokio::test]
async fn list_spaces_returns_seeded_spaces() {
    let (router, _tmp, db) = common::test_app().await;

    db.create_space("alpha", None, false)
        .await
        .expect("create_space alpha must succeed");
    db.create_space("beta", None, false)
        .await
        .expect("create_space beta must succeed");

    for (source_id, content, space) in [
        ("mem_alpha_1", "Alpha content one", "alpha"),
        ("mem_alpha_2", "Alpha content two", "alpha"),
        ("mem_beta_1", "Beta content one", "beta"),
    ] {
        let doc = RawDocument {
            source: "memory".to_string(),
            source_id: source_id.to_string(),
            title: format!("title-{}", source_id),
            content: content.to_string(),
            space: Some(space.to_string()),
            last_modified: chrono::Utc::now().timestamp(),
            confirmed: None,
            stability: Some("new".to_string()),
            pending_revision: false,
            supersede_mode: "hide".to_string(),
            enrichment_status: "raw".to_string(),
            ..RawDocument::default()
        };
        db.upsert_documents(vec![doc])
            .await
            .unwrap_or_else(|e| panic!("upsert_documents failed for {source_id}: {e}"));
        db.confirm_memory(source_id)
            .await
            .unwrap_or_else(|e| panic!("confirm_memory failed for {source_id}: {e}"));
    }

    let (status, spaces) = get_spaces(&router).await;
    assert_eq!(status, StatusCode::OK, "GET /api/spaces must return 200");

    let names: Vec<&str> = spaces.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&"alpha"),
        "response must contain space 'alpha', got: {names:?}"
    );
    assert!(
        names.contains(&"beta"),
        "response must contain space 'beta', got: {names:?}"
    );

    let alpha = spaces.iter().find(|s| s.name == "alpha").unwrap();
    let beta = spaces.iter().find(|s| s.name == "beta").unwrap();

    assert_eq!(
        alpha.memory_count, 2,
        "alpha must have memory_count=2, got {}",
        alpha.memory_count
    );
    assert_eq!(
        beta.memory_count, 1,
        "beta must have memory_count=1, got {}",
        beta.memory_count
    );
}

#[tokio::test]
async fn list_spaces_empty_db_returns_empty_array() {
    let (router, _tmp, _db) = common::test_app().await;

    let (status, spaces) = get_spaces(&router).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "GET /api/spaces must return 200 on empty DB"
    );
    assert!(
        spaces.is_empty(),
        "empty DB must return [], got: {spaces:?}"
    );
}
