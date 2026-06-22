// SPDX-License-Identifier: Apache-2.0
//! E2E: GET /api/pages?space=alpha filters pages by space.
//!
//! Inserts one page tagged `space=alpha` and one tagged `space=beta` directly
//! via `MemoryDB::insert_page`, then queries the list endpoint with
//! `?space=alpha` and asserts only the alpha page is returned.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use wenlan_types::pages::Page;
use serde::Deserialize;
use tower::ServiceExt;

#[derive(Debug, Deserialize)]
struct ListPagesResponse {
    pages: Vec<Page>,
}

/// GET `/api/pages` with an optional space query param.
async fn list_pages(router: &axum::Router, space: Option<&str>) -> ListPagesResponse {
    let uri = match space {
        Some(s) => format!("/api/pages?space={}", s),
        None => "/api/pages".to_string(),
    };
    let resp = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "GET {} must return 200", uri);
    let bytes = axum::body::to_bytes(resp.into_body(), 256 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).expect("response body must be valid ListPagesResponse JSON")
}

#[tokio::test]
async fn list_pages_filters_by_space() {
    let (router, _tmp, db) = common::test_app().await;

    let now = chrono::Utc::now().to_rfc3339();

    // Insert two pages — one per space.
    db.insert_page(
        "page_alpha_001",
        "Alpha Page Title",
        None,
        "Content about alpha space topics.",
        None,
        Some("alpha"),
        &[],
        &now,
    )
    .await
    .expect("insert alpha page must succeed");

    db.insert_page(
        "page_beta_001",
        "Beta Page Title",
        None,
        "Content about beta space topics.",
        None,
        Some("beta"),
        &[],
        &now,
    )
    .await
    .expect("insert beta page must succeed");

    // Query with space=alpha — only the alpha page must come back.
    let response = list_pages(&router, Some("alpha")).await;

    let titles: Vec<&str> = response.pages.iter().map(|p| p.title.as_str()).collect();

    assert!(
        titles.iter().any(|t| t.contains("Alpha")),
        "alpha page must appear in space=alpha filtered list; got: {:?}",
        titles
    );
    assert!(
        !titles.iter().any(|t| t.contains("Beta")),
        "beta page must not appear in space=alpha filtered list; got: {:?}",
        titles
    );

    // Sanity: unfiltered list must return both.
    let all = list_pages(&router, None).await;
    assert_eq!(
        all.pages.len(),
        2,
        "unfiltered list must return both pages; got: {:?}",
        all.pages
            .iter()
            .map(|p| p.title.as_str())
            .collect::<Vec<_>>()
    );
}
