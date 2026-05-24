// SPDX-License-Identifier: Apache-2.0
//! Integration tests: X-Origin-Space header fallback in space-aware POST handlers.
//!
//! - When body omits `space`, the header value is used.
//! - When body supplies `space`, the body wins regardless of the header.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use origin_types::responses::{CreateEntityResponse, CreatePageResponse, StoreMemoryResponse};
use tower::ServiceExt;

async fn body_as_json<T: serde::de::DeserializeOwned>(response: axum::http::Response<Body>) -> T {
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).expect("response body is valid JSON of expected type")
}

// ===== /api/memory/store (handle_store_memory) — Task 2, already done =====

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

// ===== /api/memory/search (handle_search_memory) =====

#[tokio::test]
async fn search_memory_header_fallback_returns_200() {
    let (router, _tmp, _db) = common::test_app().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/memory/search")
        .header("Content-Type", "application/json")
        .header("X-Origin-Space", "career")
        .body(Body::from(
            serde_json::json!({
                "query": "test query"
            })
            .to_string(),
        ))
        .unwrap();

    let res = router.oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "search_memory with header space must return 200"
    );
}

// ===== /api/memory/list (handle_list_memories) =====

#[tokio::test]
async fn list_memories_header_fallback_returns_200() {
    let (router, _tmp, _db) = common::test_app().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/memory/list")
        .header("Content-Type", "application/json")
        .header("X-Origin-Space", "health")
        .body(Body::from(serde_json::json!({}).to_string()))
        .unwrap();

    let res = router.oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "list_memories with header space must return 200"
    );
}

// ===== /api/search (handle_search) =====

#[tokio::test]
async fn search_header_fallback_returns_200() {
    let (router, _tmp, _db) = common::test_app().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/search")
        .header("Content-Type", "application/json")
        .header("X-Origin-Space", "work")
        .body(Body::from(
            serde_json::json!({
                "query": "test query"
            })
            .to_string(),
        ))
        .unwrap();

    let res = router.oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "search with header space must return 200"
    );
}

// ===== /api/chat-context (handle_chat_context) =====

#[tokio::test]
async fn chat_context_header_fallback_returns_200() {
    let (router, _tmp, _db) = common::test_app().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/chat-context")
        .header("Content-Type", "application/json")
        .header("X-Origin-Space", "personal")
        .body(Body::from(
            serde_json::json!({
                "query": "what do I like?"
            })
            .to_string(),
        ))
        .unwrap();

    let res = router.oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "chat_context with header space must return 200"
    );
}

// ===== /api/memory/entities (handle_list_entities) =====

#[tokio::test]
async fn list_entities_header_fallback_returns_200() {
    let (router, _tmp, _db) = common::test_app().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/memory/entities/list")
        .header("Content-Type", "application/json")
        .header("X-Origin-Space", "work")
        .body(Body::from(serde_json::json!({}).to_string()))
        .unwrap();

    let res = router.oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "list_entities with header space must return 200"
    );
}

// ===== POST /api/memory/entities (handle_create_entity) =====

#[tokio::test]
async fn create_entity_uses_header_when_body_omits_space() {
    let (router, _tmp, db) = common::test_app().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/memory/entities")
        .header("Content-Type", "application/json")
        .header("X-Origin-Space", "career")
        .body(Body::from(
            serde_json::json!({
                "name": "test_ent_space_header",
                "entity_type": "person"
            })
            .to_string(),
        ))
        .unwrap();

    let res = router.oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "create_entity must return 200"
    );

    let created: CreateEntityResponse = body_as_json(res).await;
    let detail = db
        .get_entity_detail(&created.id)
        .await
        .expect("get_entity_detail must not fail");
    assert_eq!(
        detail.entity.space.as_deref(),
        Some("career"),
        "created entity must have space=career from header, got: {:?}",
        detail.entity.space
    );
}

// ===== POST /api/pages (handle_create_page) =====

#[tokio::test]
async fn create_page_uses_header_when_body_omits_space() {
    let (router, _tmp, db) = common::test_app_no_gate().await;

    // Seed a source memory via HTTP so we have a valid source_id to cite.
    let content = "Rust is a systems programming language with memory safety guarantees";
    let store_res = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/memory/store")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "content": content,
                        "memory_type": "fact"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        store_res.status(),
        StatusCode::OK,
        "seed memory store must return 200"
    );
    let stored: StoreMemoryResponse = body_as_json(store_res).await;
    let source_id = stored.source_id;

    // Create a page citing that memory, omitting `space` in body — header must fill it.
    let page_content = "Rust provides memory safety through ownership and borrowing.";
    let create_res = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/pages")
                .header("Content-Type", "application/json")
                .header("X-Origin-Space", "career")
                .body(Body::from(
                    serde_json::json!({
                        "title": "Rust Systems Language",
                        "content": page_content,
                        "source_memory_ids": [source_id]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        create_res.status(),
        StatusCode::OK,
        "create_page must return 200"
    );

    let created: CreatePageResponse = body_as_json(create_res).await;
    let page = db
        .get_page(&created.id)
        .await
        .expect("get_page must not fail")
        .expect("page must exist after creation");
    assert_eq!(
        page.space.as_deref(),
        Some("career"),
        "created page must have space=career from header, got: {:?}",
        page.space
    );
}
