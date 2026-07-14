// SPDX-License-Identifier: Apache-2.0

use super::fixture::ScopeFixture;
use axum::body::{to_bytes, Body};
use axum::http::{Method as HttpMethod, Response, StatusCode};
use serde_json::Value;
use wenlan_server::sensitive_read_routes::{
    route, Method, ScopeBinding, SelectionGate, SelectorPrecedence, UnknownScopePolicy,
};

const ROUTES: &[(Method, &str, SelectionGate)] = &[
    (Method::Get, "/api/briefing", SelectionGate::NotApplicable),
    (
        Method::Get,
        "/api/snapshots/{id}/captures",
        SelectionGate::ParentCollectionFiltered,
    ),
    (
        Method::Get,
        "/api/snapshots/{id}/captures-with-content",
        SelectionGate::ParentCollectionFiltered,
    ),
];

async fn body_bytes(response: Response<Body>) -> (StatusCode, Vec<u8>) {
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, bytes.to_vec())
}

async fn json_body(response: Response<Body>) -> (StatusCode, Value) {
    let (status, bytes) = body_bytes(response).await;
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn seed_snapshot_fixture(fixture: &ScopeFixture) {
    fixture
        .seed_record(
            "focus_capture",
            "work-capture",
            Some("work"),
            "fact",
            10,
            None,
            false,
        )
        .await;
    fixture
        .seed_record(
            "focus_capture",
            "personal-capture",
            Some("personal"),
            "fact",
            20,
            None,
            false,
        )
        .await;
    fixture
        .seed_record(
            "focus_capture",
            "personal-only-capture",
            Some("personal"),
            "fact",
            30,
            None,
            false,
        )
        .await;
    fixture
        .db
        .insert_capture_ref(
            "work-capture",
            "activity-mixed",
            Some("snapshot-mixed"),
            "WorkApp",
            "Work Window",
            10,
            "focus",
        )
        .await
        .unwrap();
    fixture
        .db
        .insert_capture_ref(
            "orphan-capture",
            "activity-orphan",
            Some("snapshot-orphan"),
            "OrphanApp",
            "Orphan Window",
            40,
            "focus",
        )
        .await
        .unwrap();
    fixture
        .db
        .insert_capture_ref(
            "personal-capture",
            "activity-mixed",
            Some("snapshot-mixed"),
            "PersonalApp",
            "Personal Window",
            20,
            "focus",
        )
        .await
        .unwrap();
    fixture
        .db
        .insert_capture_ref(
            "personal-only-capture",
            "activity-personal",
            Some("snapshot-personal"),
            "PersonalApp",
            "Personal Only Window",
            30,
            "focus",
        )
        .await
        .unwrap();
}

pub async fn unknown_selectors_are_rejected() {
    let fixture = ScopeFixture::new().await;
    for uri in [
        "/api/briefing",
        "/api/snapshots/snapshot/captures",
        "/api/snapshots/snapshot/captures-with-content",
    ] {
        let response = fixture
            .send(HttpMethod::GET, uri, None, Some("missing-space"))
            .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY, "{uri}");
    }
}

pub async fn scoped_briefing_does_not_read_or_write_global_cache() {
    let fixture = ScopeFixture::new().await;
    let now = chrono::Utc::now().timestamp();
    fixture
        .seed_record(
            "memory",
            "work-brief",
            Some("work"),
            "fact",
            now - 1,
            None,
            false,
        )
        .await;
    fixture
        .seed_record(
            "memory",
            "personal-brief-secret",
            Some("personal"),
            "fact",
            now,
            None,
            false,
        )
        .await;
    fixture
        .db
        .upsert_briefing_cache("personal-cache-secret", 99)
        .await
        .unwrap();
    let before = fixture.db.get_cached_briefing().await.unwrap();

    let (status, body) = json_body(
        fixture
            .send(HttpMethod::GET, "/api/briefing", None, Some("work"))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let serialized = body.to_string();
    assert!(serialized.contains("work-brief"), "{serialized}");
    assert!(
        !serialized.contains("personal-brief-secret"),
        "{serialized}"
    );
    assert!(
        !serialized.contains("personal-cache-secret"),
        "{serialized}"
    );

    let after = fixture.db.get_cached_briefing().await.unwrap();
    assert_eq!(
        after, before,
        "selected briefing must not mutate Global cache"
    );
}

pub async fn snapshot_parent_collections_are_scoped() {
    let fixture = ScopeFixture::new().await;
    seed_snapshot_fixture(&fixture).await;

    for uri in [
        "/api/snapshots/snapshot-mixed/captures",
        "/api/snapshots/snapshot-mixed/captures-with-content",
    ] {
        let (status, body) =
            json_body(fixture.send(HttpMethod::GET, uri, None, Some("work")).await).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
        let serialized = body.to_string();
        assert!(serialized.contains("work-capture"), "{uri}: {serialized}");
        assert!(
            !serialized.contains("personal-capture"),
            "{uri}: {serialized}"
        );
        assert!(
            !serialized.contains("Personal Window"),
            "{uri}: {serialized}"
        );
    }

    for suffix in ["captures", "captures-with-content"] {
        let mismatch = body_bytes(
            fixture
                .send(
                    HttpMethod::GET,
                    &format!("/api/snapshots/snapshot-personal/{suffix}"),
                    None,
                    Some("work"),
                )
                .await,
        )
        .await;
        let missing = body_bytes(
            fixture
                .send(
                    HttpMethod::GET,
                    &format!("/api/snapshots/snapshot-missing/{suffix}"),
                    None,
                    Some("work"),
                )
                .await,
        )
        .await;
        assert_eq!(mismatch.0, StatusCode::NOT_FOUND, "{suffix}");
        assert_eq!(missing.0, StatusCode::NOT_FOUND, "{suffix}");
        assert_eq!(mismatch.1, missing.1, "{suffix}");

        let (status, global_mixed) = json_body(
            fixture
                .send(
                    HttpMethod::GET,
                    &format!("/api/snapshots/snapshot-mixed/{suffix}"),
                    None,
                    None,
                )
                .await,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{suffix}: {global_mixed}");
        let serialized = global_mixed.to_string();
        assert!(
            serialized.contains("work-capture"),
            "{suffix}: {serialized}"
        );
        assert!(
            serialized.contains("personal-capture"),
            "{suffix}: {serialized}"
        );

        let (status, global_personal) = json_body(
            fixture
                .send(
                    HttpMethod::GET,
                    &format!("/api/snapshots/snapshot-personal/{suffix}"),
                    None,
                    None,
                )
                .await,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{suffix}: {global_personal}");
        assert!(global_personal
            .to_string()
            .contains("personal-only-capture"));
    }

    let (status, global_orphan) = json_body(
        fixture
            .send(
                HttpMethod::GET,
                "/api/snapshots/snapshot-orphan/captures",
                None,
                None,
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{global_orphan}");
    assert!(global_orphan.to_string().contains("orphan-capture"));

    let missing_content = fixture
        .send(
            HttpMethod::GET,
            "/api/snapshots/snapshot-orphan/captures-with-content",
            None,
            None,
        )
        .await;
    assert_eq!(
        missing_content.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "required capture content must fail loud"
    );
}

pub fn registry_matches_completed_contracts() {
    assert_eq!(ROUTES.len(), 3);
    for (method, path, gate) in ROUTES {
        let actual = route(*method, path).expect("cataloged parent route");
        assert_eq!(
            actual.selector_precedence,
            SelectorPrecedence::HeaderOnly,
            "{path}"
        );
        assert_eq!(actual.scope_binding, ScopeBinding::MemorySpace, "{path}");
        assert_eq!(actual.selection_gate, *gate, "{path}");
        assert_eq!(actual.unknown_scope, UnknownScopePolicy::Rejected, "{path}");
        assert!(!actual.scope_contract_violation(), "{path}");
    }
}
