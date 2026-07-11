use super::*;

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
#[should_panic(expected = "nested router registration")]
fn nested_router_cannot_bypass_classification() {
    let nested: Router<()> = Router::new().route("/", axum::routing::get(|| async { "leak" }));
    let _ = TrackedRouter::<()>::new().nest("/api/nested", nested);
}

#[test]
#[should_panic(expected = "merged router registration")]
fn merged_router_cannot_bypass_classification() {
    let merged: Router<()> =
        Router::new().route("/api/merged", axum::routing::get(|| async { "leak" }));
    let _ = TrackedRouter::<()>::new().merge(merged);
}

#[test]
#[should_panic(expected = "route service registration")]
fn route_service_cannot_bypass_classification() {
    let service = tower::service_fn(|_request: Request| async {
        Ok::<_, Infallible>(axum::response::Response::new(axum::body::Body::empty()))
    });
    let _ = TrackedRouter::<()>::new().route_service("/api/service", service);
}

#[test]
#[should_panic]
fn duplicate_registration_fails_loud() {
    let router = TrackedRouter::<()>::new()
        .route("/api/profile", super::get(|| async { "first" }))
        .route("/api/profile", super::get(|| async { "second" }));
    let _ = router.finish();
}

#[test]
#[should_panic(expected = "after inventory finalization")]
fn registration_after_finish_fails_loud() {
    let mut router = TrackedRouter::<()>::new();
    for row in routes::sensitive_read_routes() {
        router = match row.method {
            Method::Get => router.route(row.path, super::get(|| async { "read" })),
            Method::Post => router.route(row.path, super::post(|| async { "read" })),
        };
    }
    let finalized = router.finish();
    let post_finish: TrackedMethodRouter<()> = super::get(|| async { "leak" });
    let _ = finalized.route("/api/post-finish", post_finish);
}
