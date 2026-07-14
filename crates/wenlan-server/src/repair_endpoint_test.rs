// SPDX-License-Identifier: Apache-2.0
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

#[tokio::test]
async fn repair_routes_are_distinct_typed_posts() {
    let state = Arc::new(RwLock::new(crate::state::ServerState::default()));
    for path in [
        "/api/repairs/prepare",
        "/api/repairs/apply",
        "/api/repairs/verify",
    ] {
        let response = crate::router::build_router(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(path)
                    .header("Content-Type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{path}"
        );
    }
}
