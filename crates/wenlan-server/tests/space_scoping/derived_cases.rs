// SPDX-License-Identifier: Apache-2.0

use super::fixture::ScopeFixture;
use axum::body::{to_bytes, Body};
use axum::http::{Method as HttpMethod, Response, StatusCode};
use serde_json::Value;
use wenlan_server::sensitive_read_routes::{
    route, Method, ScopeBinding, SelectionGate, SelectorPrecedence, UnknownScopePolicy,
};

const ROUTES: &[(Method, &str)] = &[
    (Method::Get, "/api/home-stats"),
    (Method::Get, "/api/retrievals/recent"),
    (Method::Get, "/api/activities"),
    (Method::Get, "/api/tags"),
];

async fn json_body(response: Response<Body>) -> (StatusCode, Value) {
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

pub async fn unknown_selectors_are_rejected() {
    let fixture = ScopeFixture::new().await;
    for uri in [
        "/api/home-stats",
        "/api/retrievals/recent?limit=1",
        "/api/activities?limit=1",
        "/api/tags",
    ] {
        let response = fixture
            .send(HttpMethod::GET, uri, None, Some("missing-space"))
            .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY, "{uri}");
    }
}

pub async fn projections_exclude_cross_scope_and_orphan_owners() {
    let fixture = ScopeFixture::new().await;
    fixture
        .seed_record(
            "memory",
            "work-event",
            Some("work"),
            "fact",
            10,
            None,
            false,
        )
        .await;
    fixture
        .seed_record(
            "memory",
            "personal-event",
            Some("personal"),
            "fact",
            20,
            None,
            false,
        )
        .await;
    fixture
        .seed_record(
            "file",
            "work-file-event",
            Some("work"),
            "fact",
            15,
            None,
            false,
        )
        .await;
    fixture
        .db
        .set_document_tags("memory", "work-event", vec!["work-tag".to_string()])
        .await
        .unwrap();
    fixture
        .db
        .set_document_tags("memory", "personal-event", vec!["personal-tag".to_string()])
        .await
        .unwrap();
    fixture
        .db
        .set_document_tags("memory", "missing-owner", vec!["orphan-tag".to_string()])
        .await
        .unwrap();
    let work_page = fixture.seed_page("work tagged page", "work").await;
    let personal_page = fixture.seed_page("personal tagged page", "personal").await;
    fixture
        .db
        .set_document_tags("page", &work_page, vec!["work-page-tag".to_string()])
        .await
        .unwrap();
    fixture
        .db
        .set_document_tags(
            "page",
            &personal_page,
            vec!["personal-page-tag".to_string()],
        )
        .await
        .unwrap();

    fixture
        .db
        .log_agent_activity(
            "codex",
            "read",
            &["work-event".to_string()],
            Some("work-query"),
            "work-detail",
        )
        .await
        .unwrap();
    fixture
        .db
        .log_agent_activity(
            "codex",
            "read",
            &["work-file-event".to_string()],
            Some("work-file-query"),
            "work-file-detail",
        )
        .await
        .unwrap();
    fixture
        .db
        .log_agent_activity(
            "codex",
            "read",
            &["personal-event".to_string()],
            Some("personal-query"),
            "personal-detail-secret",
        )
        .await
        .unwrap();
    fixture
        .db
        .log_agent_activity(
            "codex",
            "read",
            &["missing-owner".to_string()],
            Some("missing-owner-query"),
            "missing-owner-detail-secret",
        )
        .await
        .unwrap();
    fixture
        .db
        .log_agent_activity(
            "codex",
            "read",
            &["work-event".to_string(), "personal-event".to_string()],
            Some("mixed-query"),
            "mixed-detail-secret",
        )
        .await
        .unwrap();
    fixture
        .db
        .log_agent_activity(
            "codex",
            "read",
            &[],
            Some("empty-query"),
            "empty-detail-secret",
        )
        .await
        .unwrap();

    let (status, home) = json_body(
        fixture
            .send(HttpMethod::GET, "/api/home-stats", None, Some("work"))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{home}");
    assert_eq!(home["total"], 1, "{home}");
    assert_eq!(home["confirmed"], 1, "{home}");
    assert!(!home.to_string().contains("personal-event"), "{home}");

    for uri in [
        "/api/retrievals/recent?limit=100",
        "/api/activities?limit=100",
    ] {
        let (status, body) =
            json_body(fixture.send(HttpMethod::GET, uri, None, Some("work")).await).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
        let serialized = body.to_string();
        assert!(serialized.contains("work-query"), "{uri}: {serialized}");
        assert!(
            serialized.contains("work-file-query"),
            "{uri}: {serialized}"
        );
        assert!(
            !serialized.contains("personal-query"),
            "{uri}: {serialized}"
        );
        assert!(
            !serialized.contains("mixed-detail-secret"),
            "{uri}: {serialized}"
        );
        assert!(
            !serialized.contains("empty-detail-secret"),
            "{uri}: {serialized}"
        );
        assert!(
            !serialized.contains("missing-owner-detail-secret"),
            "{uri}: {serialized}"
        );
    }

    let (status, tags) = json_body(
        fixture
            .send(HttpMethod::GET, "/api/tags", None, Some("work"))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{tags}");
    let serialized = tags.to_string();
    assert!(serialized.contains("work-tag"), "{serialized}");
    assert!(serialized.contains("work-page-tag"), "{serialized}");
    assert!(!serialized.contains("personal-tag"), "{serialized}");
    assert!(!serialized.contains("personal-page-tag"), "{serialized}");
    assert!(!serialized.contains("orphan-tag"), "{serialized}");

    let (status, global_home) = json_body(
        fixture
            .send(HttpMethod::GET, "/api/home-stats", None, None)
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{global_home}");
    assert_eq!(global_home["total"], 2, "{global_home}");

    let (status, global_tags) =
        json_body(fixture.send(HttpMethod::GET, "/api/tags", None, None).await).await;
    assert_eq!(status, StatusCode::OK, "{global_tags}");
    let serialized = global_tags.to_string();
    assert!(serialized.contains("personal-tag"), "{serialized}");
    assert!(serialized.contains("personal-page-tag"), "{serialized}");
    assert!(serialized.contains("orphan-tag"), "{serialized}");

    let (status, global_activity) = json_body(
        fixture
            .send(HttpMethod::GET, "/api/activities?limit=100", None, None)
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{global_activity}");
    let serialized = global_activity.to_string();
    assert!(serialized.contains("mixed-detail-secret"), "{serialized}");
    assert!(serialized.contains("empty-detail-secret"), "{serialized}");
}

pub async fn event_scope_is_applied_before_limit() {
    let fixture = ScopeFixture::new().await;
    fixture
        .seed_record("memory", "work-old", Some("work"), "fact", 10, None, false)
        .await;
    fixture
        .seed_record(
            "memory",
            "personal-new",
            Some("personal"),
            "fact",
            20,
            None,
            false,
        )
        .await;
    fixture
        .db
        .log_agent_activity(
            "codex",
            "search",
            &["work-old".to_string()],
            Some("work-limit-query"),
            "work-limit-detail",
        )
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(1_050)).await;
    fixture
        .db
        .log_agent_activity(
            "codex",
            "search",
            &["personal-new".to_string()],
            Some("personal-limit-query"),
            "personal-limit-detail",
        )
        .await
        .unwrap();

    for uri in ["/api/retrievals/recent?limit=1", "/api/activities?limit=1"] {
        let (status, body) =
            json_body(fixture.send(HttpMethod::GET, uri, None, Some("work")).await).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
        let serialized = body.to_string();
        assert!(
            serialized.contains("work-limit-query"),
            "{uri}: {serialized}"
        );
        assert!(
            !serialized.contains("personal-limit-query"),
            "{uri}: {serialized}"
        );
    }
}

pub fn registry_matches_completed_contracts() {
    assert_eq!(ROUTES.len(), 4);
    for (method, path) in ROUTES {
        let actual = route(*method, path).expect("cataloged derived route");
        assert_eq!(
            actual.selector_precedence,
            SelectorPrecedence::HeaderOnly,
            "{path}"
        );
        assert_eq!(actual.scope_binding, ScopeBinding::MemorySpace, "{path}");
        assert_eq!(
            actual.selection_gate,
            SelectionGate::NotApplicable,
            "{path}"
        );
        assert_eq!(actual.unknown_scope, UnknownScopePolicy::Rejected, "{path}");
        assert!(!actual.scope_contract_violation(), "{path}");
    }
}
