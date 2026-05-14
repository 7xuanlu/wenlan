// SPDX-License-Identifier: Apache-2.0
//! E2E: doc-path `/api/search` currently ignores the space filter.
//!
//! This is documented behavior pending PR-C, which will add a `space` column
//! to the documents/chunks tables and wire the doc-path filter to honor it.
//! Until then, sending `space=alpha` with `source_filter != "memory"` returns
//! all documents regardless of the filter value — the space parameter is
//! explicitly discarded in `MemoryDB::search` (see `origin-core/src/db.rs`,
//! the `TODO(PR-C)` comment).
//!
//! This test pins the current no-op behavior so a future PR-C author knows
//! exactly where to update the assertion.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use origin_types::memory::SearchResult;
use serde::Deserialize;
use tower::ServiceExt;

#[derive(Debug, Deserialize)]
struct SearchResponse {
    results: Vec<SearchResult>,
}

/// POST `/api/search` with a given body and return the deserialized response.
async fn post_search(router: &axum::Router, body: serde_json::Value) -> SearchResponse {
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

/// POST `/api/ingest/text` to insert a document with the given source and content.
async fn ingest_doc(router: &axum::Router, source_id: &str, source: &str, content: &str) {
    let body = serde_json::json!({
        "source": source,
        "source_id": source_id,
        "title": format!("doc-{source_id}"),
        "content": content,
    });
    let resp = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/ingest/text")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "/api/ingest/text must return 200 for source_id={source_id}"
    );
}

#[tokio::test]
async fn doc_search_space_filter_is_noop_until_pr_c() {
    let (router, _tmp, _db) = common::test_app().await;

    // Ingest two documents under a non-"memory" source.
    // Neither gets a space tag — ingest/text does not accept a space field
    // (the handler hard-codes `space: None`). The chunks table also lacks a
    // `space` column, so the space filter has nowhere to operate even if it
    // were wired through.
    ingest_doc(
        &router,
        "doc_alpha_001",
        "local_files",
        "unique_alpha_content",
    )
    .await;
    ingest_doc(
        &router,
        "doc_beta_001",
        "local_files",
        "unique_beta_content",
    )
    .await;

    // Search with source_filter="local_files" (not "memory") and space="alpha".
    // The code path hits `MemoryDB::search`, which explicitly discards `space`
    // (see TODO(PR-C) at origin-core/src/db.rs). Both documents must appear.
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

    // Both docs must appear because the filter is a no-op.
    //
    // IF THIS ASSERTION STARTS FAILING: you have likely wired the space filter
    // through to the doc path (PR-C work). Update this test to assert true
    // filter behavior: with space=alpha only the alpha-tagged doc should appear,
    // and the beta-tagged doc must not.
    assert!(
        response.results.len() >= 2,
        "doc-path space filter is a no-op until PR-C — both docs must still return, \
         got {} result(s): {:?}",
        response.results.len(),
        response
            .results
            .iter()
            .map(|r| r.source_id.as_str())
            .collect::<Vec<_>>()
    );

    // Confirm both documents are actually present in the results.
    let source_ids: Vec<&str> = response
        .results
        .iter()
        .map(|r| r.source_id.as_str())
        .collect();
    assert!(
        source_ids.contains(&"doc_alpha_001"),
        "doc_alpha_001 must be in results regardless of space filter; got: {:?}",
        source_ids
    );
    assert!(
        source_ids.contains(&"doc_beta_001"),
        "doc_beta_001 must be in results regardless of space filter (proves filter is no-op); \
         got: {:?}",
        source_ids
    );
}
