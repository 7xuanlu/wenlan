use serde_json::json;
use wenlan_types::{
    CreatePageDraftRequest, PageDraftResponse, PageDraftVersionRequest, UpdatePageDraftRequest,
};

#[test]
fn create_requires_client_id() {
    assert!(serde_json::from_value::<CreatePageDraftRequest>(json!({
        "title": "Missing stable id"
    }))
    .is_err());
}

#[test]
fn create_preserves_partial_snapshot_defaults() {
    let title_first: CreatePageDraftRequest = serde_json::from_value(json!({
        "draft_id": "page_00000000-0000-4000-8000-000000000001",
        "title": "Working title"
    }))
    .unwrap();

    assert_eq!(title_first.content, "");
    assert_eq!(title_first.space, None);
    assert!(!title_first.space_was_provided());

    let content_first: CreatePageDraftRequest = serde_json::from_value(json!({
        "draft_id": "page_00000000-0000-4000-8000-000000000002",
        "content": "Opening paragraph"
    }))
    .unwrap();

    assert_eq!(content_first.title, "");
    assert_eq!(content_first.content, "Opening paragraph");
    assert!(!content_first.space_was_provided());
}

#[test]
fn create_round_trips_omitted_null_and_named_space_distinctly() {
    let omitted = json!({
        "draft_id": "page_00000000-0000-4000-8000-000000000001",
        "title": "Inherit request header",
        "content": ""
    });
    let omitted_request: CreatePageDraftRequest = serde_json::from_value(omitted.clone()).unwrap();

    assert!(!omitted_request.space_was_provided());
    assert_eq!(serde_json::to_value(omitted_request).unwrap(), omitted);

    let explicit_null = json!({
        "draft_id": "page_00000000-0000-4000-8000-000000000002",
        "title": "Unscoped",
        "content": "",
        "space": null
    });
    let explicit_null_request: CreatePageDraftRequest =
        serde_json::from_value(explicit_null.clone()).unwrap();

    assert!(explicit_null_request.space_was_provided());
    assert_eq!(
        serde_json::to_value(explicit_null_request).unwrap(),
        explicit_null
    );

    let named = json!({
        "draft_id": "page_00000000-0000-4000-8000-000000000003",
        "title": "",
        "content": "Opening paragraph",
        "space": "work"
    });
    let named_request: CreatePageDraftRequest = serde_json::from_value(named.clone()).unwrap();

    assert!(named_request.space_was_provided());
    assert_eq!(named_request.space.as_deref(), Some("work"));
    assert_eq!(serde_json::to_value(named_request).unwrap(), named);
}

#[test]
fn create_constructors_choose_header_inheritance_or_explicit_space() {
    let inherited = CreatePageDraftRequest::new_inheriting_header_space(
        "page_00000000-0000-4000-8000-000000000004".into(),
        "Inherited".into(),
        String::new(),
    );
    assert_eq!(
        serde_json::to_value(inherited).unwrap(),
        json!({
            "draft_id": "page_00000000-0000-4000-8000-000000000004",
            "title": "Inherited",
            "content": ""
        })
    );

    let unscoped = CreatePageDraftRequest::new(
        "page_00000000-0000-4000-8000-000000000005".into(),
        "Unscoped".into(),
        String::new(),
        None,
    );
    assert_eq!(
        serde_json::to_value(unscoped).unwrap(),
        json!({
            "draft_id": "page_00000000-0000-4000-8000-000000000005",
            "title": "Unscoped",
            "content": "",
            "space": null
        })
    );
}

#[test]
fn update_request_round_trips_exactly() {
    let update = json!({
        "expected_version": 4,
        "title": "Working title",
        "content": "Revised paragraph",
        "space": null
    });

    let parsed: UpdatePageDraftRequest = serde_json::from_value(update.clone()).unwrap();

    assert_eq!(serde_json::to_value(parsed).unwrap(), update);
}

#[test]
fn update_requires_a_complete_snapshot() {
    for incomplete in [
        json!({
            "expected_version": 4,
            "content": "Revised paragraph",
            "space": null
        }),
        json!({
            "expected_version": 4,
            "title": "Working title",
            "space": null
        }),
        json!({
            "expected_version": 4,
            "title": "Working title",
            "content": "Revised paragraph"
        }),
    ] {
        assert!(serde_json::from_value::<UpdatePageDraftRequest>(incomplete).is_err());
    }
}

#[test]
fn version_request_round_trips_exactly() {
    let version = json!({ "expected_version": 7 });

    let parsed: PageDraftVersionRequest = serde_json::from_value(version.clone()).unwrap();

    assert_eq!(serde_json::to_value(parsed).unwrap(), version);
}

#[test]
fn response_envelope_deserializes_an_existing_page() {
    let payload = json!({
        "page": {
            "id": "page_draft_1",
            "title": "Working title",
            "summary": null,
            "content": "Opening paragraph",
            "entity_id": null,
            "space": null,
            "source_memory_ids": [],
            "version": 3,
            "status": "draft",
            "created_at": "2026-07-16T00:00:00Z",
            "last_compiled": "2026-07-16T00:00:00Z",
            "last_modified": "2026-07-16T00:00:00Z",
            "sources_updated_count": 0,
            "stale_reason": null,
            "user_edited": true,
            "workspace": null,
            "creation_kind": "authored",
            "review_status": "unconfirmed"
        }
    });

    let response: PageDraftResponse = serde_json::from_value(payload).unwrap();

    assert_eq!(response.page.id, "page_draft_1");
    assert_eq!(response.page.status, "draft");
}
