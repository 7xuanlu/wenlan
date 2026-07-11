use axum::body::to_bytes;
use axum::http::{Method, StatusCode};
use std::path::PathBuf;
use tower::ServiceExt;
use wenlan_types::lint::{LintOutcome, LintReport, LintScopeKind};
use wenlan_types::sources::{Source, SourceType, SyncStatus};

#[path = "lint_endpoint_test/support.rs"]
mod support;
use support::{request, Fixture};

async fn report(fixture: &Fixture, uri: &str) -> (StatusCode, String, Option<LintReport>) {
    let response = fixture
        .app
        .clone()
        .oneshot(request(Method::GET, uri))
        .await
        .expect("lint response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("lint body");
    let body = String::from_utf8(bytes.to_vec()).expect("utf8 body");
    let decoded = serde_json::from_str(&body).ok();
    (status, body, decoded)
}

#[tokio::test]
async fn lint_global_complete_response_uses_shared_remote_safe_report() {
    let fixture = Fixture::new(Vec::new(), None).await;

    let (status, body, decoded) = report(&fixture, "/api/lint").await;

    let decoded = decoded.expect("shared LintReport");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(decoded.scope().kind(), LintScopeKind::Global);
    assert!(decoded.complete());
    assert_eq!(decoded.totals().incomplete(), 0);
    assert!(!body.contains(fixture.root.path().to_string_lossy().as_ref()));
    assert!(!body.contains("knowledge_path"));
}

#[tokio::test]
async fn lint_finding_response_stays_complete_and_typed() {
    let source = Source {
        id: String::new(),
        source_type: SourceType::Directory,
        path: PathBuf::new(),
        status: SyncStatus::Active,
        last_sync: None,
        file_count: 0,
        memory_count: 0,
        last_sync_errors: 0,
        last_sync_error_detail: None,
    };
    let fixture = Fixture::new(vec![source], None).await;

    let (status, _, decoded) = report(&fixture, "/api/lint").await;

    let decoded = decoded.expect("shared LintReport");
    assert_eq!(status, StatusCode::OK);
    assert!(decoded.complete());
    assert!(decoded.totals().findings() > 0);
    assert!(decoded.checks().iter().any(|check| {
        check.check_id() == "operations.source_configuration"
            && check.outcome() == LintOutcome::Finding
    }));
}

#[tokio::test]
async fn lint_check_failure_stays_inside_incomplete_report() {
    let missing = PathBuf::from("/definitely/missing/task-16-page-root");
    let fixture = Fixture::new(Vec::new(), Some(missing)).await;

    let (status, _, decoded) = report(&fixture, "/api/lint").await;

    let decoded = decoded.expect("shared LintReport");
    assert_eq!(status, StatusCode::OK);
    assert!(!decoded.complete());
    assert!(decoded.totals().incomplete() > 0);
}

#[tokio::test]
async fn lint_registered_scope_is_applied_by_core() {
    let fixture = Fixture::new(Vec::new(), None).await;
    fixture
        .db
        .create_space("work", None, false)
        .await
        .expect("space");

    let (status, _, decoded) = report(&fixture, "/api/lint?space=work").await;

    let decoded = decoded.expect("shared LintReport");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(decoded.scope().kind(), LintScopeKind::Registered);
    assert!(decoded.scope().opaque_scope_ref().is_some());
}

#[tokio::test]
async fn lint_uncategorized_scope_is_applied_by_core() {
    let fixture = Fixture::new(Vec::new(), None).await;

    let (status, _, decoded) = report(&fixture, "/api/lint?space=uncategorized").await;

    let decoded = decoded.expect("shared LintReport");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(decoded.scope().kind(), LintScopeKind::Uncategorized);
    assert!(decoded.scope().opaque_scope_ref().is_none());
}

#[tokio::test]
async fn lint_unknown_scope_fails_closed_before_later_stages() {
    let missing = PathBuf::from("/definitely/missing/task-16-must-not-scan");
    let fixture = Fixture::new(Vec::new(), Some(missing)).await;

    let response = fixture
        .app
        .clone()
        .oneshot(request(Method::GET, "/api/lint?space=missing"))
        .await
        .expect("invalid scope response");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("invalid scope body");

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).expect("typed error"),
        serde_json::json!({"error": "invalid_scope"})
    );
}

#[tokio::test]
async fn lint_route_rejects_unsupported_method_and_wiki_route_is_absent() {
    let fixture = Fixture::new(Vec::new(), None).await;

    let put = fixture
        .app
        .clone()
        .oneshot(request(Method::PUT, "/api/lint"))
        .await
        .expect("method response");
    let wiki = fixture
        .app
        .clone()
        .oneshot(request(Method::GET, "/api/wiki/check"))
        .await
        .expect("wiki response");

    assert_eq!(put.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(wiki.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn lint_endpoint_does_not_mutate_database_or_page_tree() {
    let fixture = Fixture::new(Vec::new(), None).await;
    let before = fixture.fingerprint().await;

    let (status, _, _) = report(&fixture, "/api/lint").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(fixture.fingerprint().await, before);
}
