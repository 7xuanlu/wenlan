// SPDX-License-Identifier: Apache-2.0
//! Integration tests for read-only curation routes.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use wenlan_types::responses::{OrphanLinksResponse, PendingRevisionItem};

mod common;
use common::{insert_memory, insert_page_with_orphan_link, test_app};

async fn body_as_json<T: serde::de::DeserializeOwned>(response: axum::http::Response<Body>) -> T {
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).expect("response body is valid JSON of expected type")
}

#[tokio::test]
async fn list_pending_revisions_returns_target_id_not_revision_id() {
    let (app, _tmp, db) = test_app().await;

    // Target memory (not pending, no supersedes)
    insert_memory(
        &db,
        "mem_target",
        "Original",
        "memory",
        None,
        None,
        false,
        1_000,
    )
    .await;
    // Revision row pending, supersedes target
    insert_memory(
        &db,
        "mem_rev",
        "Revised",
        "memory",
        Some("agent-1"),
        Some("mem_target"),
        true,
        2_000,
    )
    .await;

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/memory/pending-revisions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let items: Vec<PendingRevisionItem> = body_as_json(resp).await;
    assert_eq!(items.len(), 1, "got {items:?}");
    assert_eq!(
        items[0].target_source_id, "mem_target",
        "regression: list must return target id, not revision row id (adversarial C1)"
    );
    assert_eq!(items[0].revision_source_id, "mem_rev");
    assert_eq!(items[0].revision_content, "Revised");
}

#[tokio::test]
async fn list_pending_revisions_round_trips_through_accept() {
    let (app, _tmp, db) = test_app().await;
    insert_memory(&db, "mem_t", "Orig", "memory", None, None, false, 1).await;
    insert_memory(&db, "mem_r", "Rev", "memory", None, Some("mem_t"), true, 2).await;

    let items: Vec<PendingRevisionItem> = body_as_json(
        app.oneshot(
            Request::builder()
                .uri("/api/memory/pending-revisions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap(),
    )
    .await;

    let target = &items[0].target_source_id;
    // Call existing accept primitive. Proves the id from list is action-compatible.
    db.accept_pending_revision(target)
        .await
        .expect("accept must succeed with target_source_id from list");
}

#[tokio::test]
async fn list_pending_revisions_orders_newest_first() {
    let (app, _tmp, db) = test_app().await;
    insert_memory(&db, "mem_t1", "t", "memory", None, None, false, 1).await;
    insert_memory(&db, "mem_t2", "t", "memory", None, None, false, 2).await;
    insert_memory(
        &db,
        "mem_r1",
        "old",
        "memory",
        None,
        Some("mem_t1"),
        true,
        100,
    )
    .await;
    insert_memory(
        &db,
        "mem_r2",
        "new",
        "memory",
        None,
        Some("mem_t2"),
        true,
        200,
    )
    .await;

    let items: Vec<PendingRevisionItem> = body_as_json(
        app.oneshot(
            Request::builder()
                .uri("/api/memory/pending-revisions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(items[0].revision_source_id, "mem_r2");
    assert_eq!(items[1].revision_source_id, "mem_r1");
}

#[tokio::test]
async fn list_pending_revisions_clamps_limit() {
    let (app, _tmp, db) = test_app().await;
    for i in 0..3 {
        insert_memory(
            &db,
            &format!("mem_t{i}"),
            "t",
            "memory",
            None,
            None,
            false,
            i,
        )
        .await;
        insert_memory(
            &db,
            &format!("mem_r{i}"),
            "r",
            "memory",
            None,
            Some(&format!("mem_t{i}")),
            true,
            100 + i,
        )
        .await;
    }
    let items: Vec<PendingRevisionItem> = body_as_json(
        app.oneshot(
            Request::builder()
                .uri("/api/memory/pending-revisions?limit=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(items.len(), 2);
}

#[tokio::test]
async fn list_pending_revisions_empty_on_clean_db() {
    let (app, _tmp, _db) = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/memory/pending-revisions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let items: Vec<PendingRevisionItem> = body_as_json(resp).await;
    assert!(items.is_empty());
}

#[tokio::test]
async fn orphan_links_returns_typed_envelope() {
    let (app, _tmp, db) = test_app().await;

    // Two pages both referencing "Missing" — an orphan because no page with
    // that title exists. With min_count=2 both must appear for the label to
    // surface; this exercises the threshold and the typed envelope shape.
    insert_page_with_orphan_link(&db, "page1", "Source page", "Missing").await;
    insert_page_with_orphan_link(&db, "page2", "Other source", "Missing").await;

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/pages/orphan-links?min_count=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body: OrphanLinksResponse = body_as_json(resp).await;

    assert_eq!(body.min_count, 2, "min_count echo must match query param");
    assert_eq!(
        body.orphan_labels.len(),
        1,
        "exactly one orphan label expected (got {:?})",
        body.orphan_labels
    );
    let entry = &body.orphan_labels[0];
    assert_eq!(entry.label, "Missing", "label must be the wikilink text");
    assert_eq!(
        entry.count, 2,
        "count must be the number of distinct source pages"
    );
}

#[tokio::test]
async fn get_page_exposes_pending_rebuild_for_stale_survivor_without_lane() {
    let (app, _tmp, db) = test_app().await;
    let now = chrono::Utc::now().to_rfc3339();
    db.insert_page(
        "page_pending_rebuild",
        "Pending rebuild",
        None,
        "Evidence has already moved, prose has not.",
        None,
        None,
        &["mem_a", "mem_b"],
        &now,
    )
    .await
    .unwrap();
    db.set_page_stale("page_pending_rebuild", "source_updated")
        .await
        .unwrap();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/pages/page_pending_rebuild")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = body_as_json(resp).await;
    let page = &body["page"];
    assert_eq!(page["stale_reason"], "source_updated");
    assert_eq!(
        page["pending_rebuild"], "evidence updated, prose rebuild pending",
        "GET page must expose the merged-but-unrebuilt state even before a compile lane runs"
    );
}
