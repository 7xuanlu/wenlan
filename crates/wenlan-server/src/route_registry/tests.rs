use super::*;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[test]
#[should_panic(expected = "unclassified router path")]
fn helper_registration_cannot_bypass_classification() {
    fn helper(router: TrackedRouter) -> TrackedRouter {
        router.route("/api/memory/dynamic-leak", super::get(|| async { "leak" }))
    }
    let _ = helper(TrackedRouter::new());
}

#[test]
#[should_panic(expected = "unclassified router path")]
fn unknown_route_cannot_bypass_classification() {
    let _ = TrackedRouter::<()>::new().route("/api/unknown-read", super::get(|| async { "leak" }));
}

#[test]
#[should_panic(expected = "unclassified router path")]
fn wrong_method_cannot_satisfy_a_sensitive_registration() {
    let _ = TrackedRouter::<()>::new().route("/api/agents", super::post(|| async { "leak" }));
}

#[test]
#[should_panic]
fn duplicate_registration_fails_loud() {
    let router = TrackedRouter::<()>::new()
        .route("/api/profile", super::get(|| async { "first" }))
        .route("/api/profile", super::get(|| async { "second" }));
    let _ = router.finish();
}

#[tokio::test]
async fn finalized_inventory_yields_sealed_http_service() {
    let mut router = TrackedRouter::<()>::new();
    for row in routes::sensitive_read_routes() {
        router = match row.method {
            Method::Get => router.route(row.path, super::get(|| async { "read" })),
            Method::Post => router.route(row.path, super::post(|| async { "read" })),
        };
    }
    let app: AppRouter = router.finish().with_state(());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/profile")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
}
