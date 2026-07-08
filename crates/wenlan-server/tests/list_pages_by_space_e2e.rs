// SPDX-License-Identifier: Apache-2.0
mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde::Deserialize;
use tower::ServiceExt;
use wenlan_types::pages::Page;

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

    db.create_space("alpha", None, false)
        .await
        .expect("alpha space must be registered");
    db.create_space("beta", None, false)
        .await
        .expect("beta space must be registered");

    common::create_page_fixture(
        &db,
        "Alpha Page Title",
        "Content about alpha space topics.",
        Some("alpha"),
        &[],
        "authored",
    )
    .await;

    common::create_page_fixture(
        &db,
        "Beta Page Title",
        "Content about beta space topics.",
        Some("beta"),
        &[],
        "authored",
    )
    .await;

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

#[tokio::test]
async fn list_pages_unregistered_query_space_falls_back_to_unscoped() {
    let (router, _tmp, db) = common::test_app().await;

    common::create_page_fixture(
        &db,
        "Unscoped Fallback Page",
        "Content that should remain visible when an unregistered page space is ignored.",
        None,
        &[],
        "authored",
    )
    .await;

    let response = list_pages(&router, Some("ghost-pages-space")).await;
    let titles: Vec<&str> = response.pages.iter().map(|p| p.title.as_str()).collect();

    assert!(
        titles.contains(&"Unscoped Fallback Page"),
        "unregistered page query spaces must not filter out unscoped page listings; got: {titles:?}"
    );
    assert!(
        db.get_space("ghost-pages-space").await.unwrap().is_none(),
        "read-only page filters must not auto-create an unregistered spaces row"
    );
}

#[tokio::test]
async fn list_pages_empty_query_space_falls_back_to_unscoped() {
    let (router, _tmp, db) = common::test_app().await;

    common::create_page_fixture(
        &db,
        "Empty Space Fallback Page",
        "Content that should remain visible when the page space query is empty.",
        None,
        &[],
        "authored",
    )
    .await;

    let response = list_pages(&router, Some("")).await;
    let titles: Vec<&str> = response.pages.iter().map(|p| p.title.as_str()).collect();

    assert!(
        titles.contains(&"Empty Space Fallback Page"),
        "empty page query spaces must not filter out unscoped page listings; got: {titles:?}"
    );
}
