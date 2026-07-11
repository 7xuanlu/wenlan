// SPDX-License-Identifier: Apache-2.0
//! E2E: doc-path `/api/search` honors the `space` filter.
//!
//! Activated in PR-C. Previously the parameter was accepted but discarded in
//! `MemoryDB::search` (the `memories` table has a `space` column but the WHERE
//! clause did not reference it). PR-C wires the filter into both the vector and
//! FTS branches.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde::Deserialize;
use tower::ServiceExt;
use wenlan_core::sources::RawDocument;
use wenlan_types::memory::SearchResult;

#[derive(Debug, Deserialize)]
struct SearchResponse {
    results: Vec<SearchResult>,
}

async fn post_search(router: &common::AppRouter, body: serde_json::Value) -> SearchResponse {
    let resp = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/search")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "/api/search must return 200");
    let bytes = axum::body::to_bytes(resp.into_body(), 256 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).expect("response body must be valid SearchResponse JSON")
}

async fn seed_doc(
    db: &std::sync::Arc<wenlan_core::db::MemoryDB>,
    source_id: &str,
    content: &str,
    space: &str,
) {
    let doc = RawDocument {
        source: "local_files".to_string(),
        source_id: source_id.to_string(),
        title: format!("doc-{source_id}"),
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
}

async fn register_test_spaces(db: &std::sync::Arc<wenlan_core::db::MemoryDB>) {
    db.create_space("alpha", None, false).await.unwrap();
    db.create_space("beta", None, false).await.unwrap();
}

#[tokio::test]
async fn doc_search_filters_by_space_alpha() {
    let (router, _tmp, db) = common::test_app().await;

    register_test_spaces(&db).await;
    seed_doc(&db, "doc_alpha_001", "unique_alpha_content", "alpha").await;
    seed_doc(&db, "doc_beta_001", "unique_beta_content", "beta").await;

    let response = post_search(
        &router,
        serde_json::json!({
            "query": "content",
            "limit": 10,
            "source_filter": "local_files",
            "space": "alpha",
        }),
    )
    .await;

    let source_ids: Vec<&str> = response
        .results
        .iter()
        .map(|r| r.source_id.as_str())
        .collect();
    assert!(
        source_ids.contains(&"doc_alpha_001"),
        "alpha-tagged doc must appear when space=alpha; got: {source_ids:?}"
    );
    assert!(
        !source_ids.contains(&"doc_beta_001"),
        "beta-tagged doc must NOT appear when space=alpha; got: {source_ids:?}"
    );
}

#[tokio::test]
async fn doc_search_filters_by_space_beta() {
    let (router, _tmp, db) = common::test_app().await;

    register_test_spaces(&db).await;
    seed_doc(&db, "doc_alpha_001", "unique_alpha_content", "alpha").await;
    seed_doc(&db, "doc_beta_001", "unique_beta_content", "beta").await;

    let response = post_search(
        &router,
        serde_json::json!({
            "query": "content",
            "limit": 10,
            "source_filter": "local_files",
            "space": "beta",
        }),
    )
    .await;

    let source_ids: Vec<&str> = response
        .results
        .iter()
        .map(|r| r.source_id.as_str())
        .collect();
    assert!(
        source_ids.contains(&"doc_beta_001"),
        "beta-tagged doc must appear when space=beta; got: {source_ids:?}"
    );
    assert!(
        !source_ids.contains(&"doc_alpha_001"),
        "alpha-tagged doc must NOT appear when space=beta; got: {source_ids:?}"
    );
}

#[tokio::test]
async fn doc_search_no_space_returns_all() {
    let (router, _tmp, db) = common::test_app().await;

    seed_doc(&db, "doc_alpha_001", "unique_alpha_content", "alpha").await;
    seed_doc(&db, "doc_beta_001", "unique_beta_content", "beta").await;

    let response = post_search(
        &router,
        serde_json::json!({
            "query": "content",
            "limit": 10,
            "source_filter": "local_files",
        }),
    )
    .await;

    let source_ids: Vec<&str> = response
        .results
        .iter()
        .map(|r| r.source_id.as_str())
        .collect();
    assert!(
        source_ids.contains(&"doc_alpha_001"),
        "alpha doc must appear when space is unset; got: {source_ids:?}"
    );
    assert!(
        source_ids.contains(&"doc_beta_001"),
        "beta doc must appear when space is unset; got: {source_ids:?}"
    );
}
