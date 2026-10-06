// SPDX-License-Identifier: Apache-2.0
use crate::error::ServerError;
use crate::route_registry::{delete, get, post, put, TrackedRouter};
use crate::state::{ServerState, SharedState};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue},
    response::Json,
};
use std::sync::Arc;
use tokio::sync::RwLock;

pub(crate) fn register(router: TrackedRouter<SharedState>) -> TrackedRouter<SharedState> {
    router
        .route("/api/indexed-files", get(handle_list_indexed_files))
        .route("/api/chunks/{source_id}", get(handle_get_chunks))
        .route(
            "/api/webpage-chunks/{source_id}",
            get(handle_get_webpage_chunks),
        )
        .route("/api/chunks/{id}/update", put(handle_update_chunk))
        .route(
            "/api/chunks/time-range",
            delete(handle_delete_by_time_range),
        )
        .route("/api/chunks/delete-bulk", post(handle_delete_bulk))
}

// =====================================================================
// Batch 2 — Indexed files / chunks
// =====================================================================

#[derive(Debug, Default, serde::Deserialize)]
pub struct SourceReadQuery {
    space: Option<String>,
}

// Query values support canonical Unicode names. A conflicting selector must
// not override the scope bound by a present header.
fn source_read_selector<'a>(
    query: Option<&'a str>,
    header: Option<&str>,
) -> Result<Option<&'a str>, ServerError> {
    let Some(query) = query else {
        return Ok(None);
    };
    let query = query.trim();
    if query.is_empty() {
        return Err(ServerError::ValidationError(
            "Space query must not be blank".into(),
        ));
    }
    if header.is_some_and(|header| header.trim() != query) {
        return Err(ServerError::ValidationError(
            "Space query conflicts with Space header".into(),
        ));
    }
    Ok(Some(query))
}

// RFC 3986 unreserved encoding matches the app's scope acknowledgement check.
// A paired client rejects responses from older daemons that ignore the query.
fn source_scope_headers(selector: Option<&str>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if let Some(selector) = selector {
        let encoded: String = selector
            .bytes()
            .flat_map(|byte| match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                    vec![byte as char]
                }
                _ => format!("%{byte:02X}").chars().collect(),
            })
            .collect();
        headers.insert(
            "x-wenlan-source-scope",
            HeaderValue::from_str(&encoded).expect("percent encoded scope is ASCII"),
        );
    }
    headers
}

/// GET /api/indexed-files
pub async fn handle_list_indexed_files(
    State(state): State<Arc<RwLock<ServerState>>>,
    crate::space_header::SpaceHeader(header_space): crate::space_header::SpaceHeader,
    Query(query): Query<SourceReadQuery>,
) -> Result<
    (
        HeaderMap,
        Json<wenlan_types::responses::IndexedFilesResponse>,
    ),
    ServerError,
> {
    let db = {
        let s = state.read().await;
        s.db.clone().ok_or(ServerError::DbNotInitialized)?
    };
    let selector = source_read_selector(query.space.as_deref(), header_space.as_deref())?;
    let scope =
        crate::read_scope::effective_read_scope(&db, selector, header_space.as_deref()).await?;
    let files = db
        .list_indexed_files_scoped(&scope)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;
    Ok((
        source_scope_headers(selector),
        Json(wenlan_types::responses::IndexedFilesResponse { files }),
    ))
}

/// GET /api/chunks/{source_id}
pub async fn handle_get_chunks(
    State(state): State<Arc<RwLock<ServerState>>>,
    crate::space_header::SpaceHeader(header_space): crate::space_header::SpaceHeader,
    Query(query): Query<SourceReadQuery>,
    Path(source_id): Path<String>,
) -> Result<(HeaderMap, Json<Vec<wenlan_core::db::MemoryDetail>>), ServerError> {
    let db = {
        let s = state.read().await;
        s.db.clone().ok_or(ServerError::DbNotInitialized)?
    };
    let selector = source_read_selector(query.space.as_deref(), header_space.as_deref())?;
    let scope =
        crate::read_scope::effective_read_scope(&db, selector, header_space.as_deref()).await?;
    let chunks = db
        .get_chunks_scoped(&source_id, &scope)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?
        .ok_or_else(|| ServerError::NotFound("memory not found".to_string()))?;
    Ok((source_scope_headers(selector), Json(chunks)))
}

/// GET /api/webpage-chunks/{source_id}. Exact kind prevents document collisions;
/// a dedicated path also fails closed against an older daemon.
pub async fn handle_get_webpage_chunks(
    State(state): State<Arc<RwLock<ServerState>>>,
    crate::space_header::SpaceHeader(header_space): crate::space_header::SpaceHeader,
    Query(query): Query<SourceReadQuery>,
    Path(source_id): Path<String>,
) -> Result<(HeaderMap, Json<Vec<wenlan_core::db::MemoryDetail>>), ServerError> {
    let db = {
        let s = state.read().await;
        s.db.clone().ok_or(ServerError::DbNotInitialized)?
    };
    let selector = source_read_selector(query.space.as_deref(), header_space.as_deref())?;
    let scope =
        crate::read_scope::effective_read_scope(&db, selector, header_space.as_deref()).await?;
    let chunks = db
        .get_webpage_chunks_scoped(&source_id, &scope)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?
        .ok_or_else(|| ServerError::NotFound("webpage not found".to_string()))?;
    Ok((source_scope_headers(selector), Json(chunks)))
}

/// PUT /api/chunks/{id}/update
pub async fn handle_update_chunk(
    State(state): State<Arc<RwLock<ServerState>>>,
    Path(id): Path<String>,
    Json(req): Json<wenlan_types::requests::UpdateChunkRequest>,
) -> Result<Json<wenlan_types::responses::SuccessResponse>, ServerError> {
    let db = {
        let s = state.read().await;
        s.db.clone().ok_or(ServerError::DbNotInitialized)?
    };
    db.update_memory(&id, &req.content)
        .await
        .map_err(ServerError::from)?;
    Ok(Json(wenlan_types::responses::SuccessResponse { ok: true }))
}

/// DELETE /api/chunks/time-range
pub async fn handle_delete_by_time_range(
    State(state): State<Arc<RwLock<ServerState>>>,
    Json(req): Json<wenlan_types::requests::DeleteByTimeRangeRequest>,
) -> Result<Json<wenlan_types::responses::DeleteCountResponse>, ServerError> {
    let db = {
        let s = state.read().await;
        s.db.clone().ok_or(ServerError::DbNotInitialized)?
    };
    let deleted = db
        .delete_by_time_range(req.start, req.end)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;
    Ok(Json(wenlan_types::responses::DeleteCountResponse {
        deleted,
    }))
}

/// POST /api/chunks/delete-bulk
pub async fn handle_delete_bulk(
    State(state): State<Arc<RwLock<ServerState>>>,
    Json(req): Json<wenlan_types::requests::BulkDeleteRequest>,
) -> Result<Json<wenlan_types::responses::DeleteCountResponse>, ServerError> {
    let db = {
        let s = state.read().await;
        s.db.clone().ok_or(ServerError::DbNotInitialized)?
    };
    let mut deleted = 0usize;
    for item in &req.items {
        if db
            .delete_by_source_id(&item.source, &item.source_id)
            .await
            .is_ok()
        {
            deleted += 1;
        }
    }
    Ok(Json(wenlan_types::responses::DeleteCountResponse {
        deleted,
    }))
}

#[cfg(test)]
mod webpage_chunk_tests {
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use tower::ServiceExt;

    #[tokio::test]
    async fn saved_webpage_route_is_exact_scoped_and_preserves_legacy_chunks() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(
            wenlan_core::db::MemoryDB::new(tmp.path(), Arc::new(wenlan_core::events::NoopEmitter))
                .await
                .unwrap(),
        );
        for space in ["work", "personal"] {
            db.create_space(space, None, false).await.unwrap();
        }
        let url = "https://example.com/post?q=a&lang=zh#part";
        for (source, id, content) in [
            ("memory", "shared", "Legacy memory body"),
            ("file", "file-only", "Legacy file body"),
            ("webpage", "web-original", "Saved web excerpt"),
            ("webpage", url, "URL saved excerpt"),
        ] {
            db.upsert_documents(vec![wenlan_core::sources::RawDocument {
                source: source.into(),
                source_id: id.into(),
                title: id.into(),
                content: content.into(),
                space: Some("work".into()),
                ..Default::default()
            }])
            .await
            .unwrap();
        }
        // Rebinding preserves independently minted row IDs while reproducing a
        // legacy cross-kind source_id collision.
        db.rebind_source_id("webpage", "web-original", "shared")
            .await
            .unwrap();
        let state = Arc::new(RwLock::new(crate::state::ServerState {
            db: Some(db),
            ..Default::default()
        }));
        let app = crate::router::build_router(state);
        for (path, space, status, expected) in [
            (
                "/api/webpage-chunks/shared",
                "work",
                StatusCode::OK,
                Some("Saved web excerpt"),
            ),
            (
                "/api/webpage-chunks/shared",
                "personal",
                StatusCode::NOT_FOUND,
                None,
            ),
            (
                "/api/webpage-chunks/missing",
                "work",
                StatusCode::NOT_FOUND,
                None,
            ),
            (
                "/api/webpage-chunks/file-only",
                "work",
                StatusCode::NOT_FOUND,
                None,
            ),
            (
                "/api/webpage-chunks/shared",
                "unknown",
                StatusCode::UNPROCESSABLE_ENTITY,
                None,
            ),
            (
                "/api/chunks/shared",
                "work",
                StatusCode::OK,
                Some("Legacy memory body"),
            ),
            (
                "/api/chunks/file-only",
                "work",
                StatusCode::OK,
                Some("Legacy file body"),
            ),
            (
                "/api/webpage-chunks/https%3A%2F%2Fexample.com%2Fpost%3Fq%3Da%26lang%3Dzh%23part",
                "work",
                StatusCode::OK,
                Some("URL saved excerpt"),
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(path)
                        .header("x-wenlan-space", space)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), status, "{path} in {space}");
            if let Some(content) = expected {
                let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
                let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(value[0]["content"], content);
            }
        }
    }
}

#[cfg(test)]
mod source_scope_query_tests {
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use tower::ServiceExt;

    #[tokio::test]
    async fn source_query_scopes_before_aggregation_and_chunk_selection() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(
            wenlan_core::db::MemoryDB::new(tmp.path(), Arc::new(wenlan_core::events::NoopEmitter))
                .await
                .unwrap(),
        );
        for space in ["專案 & R+D", "work"] {
            db.create_space(space, None, false).await.unwrap();
        }
        // Preserve rows from an existing document whose chunks belong to
        // different Spaces; a global aggregate followed by UI filtering leaks.
        let fixture = libsql::Builder::new_local(tmp.path().join("origin_memory.db"))
            .build()
            .await
            .unwrap();
        let conn = fixture.connect().unwrap();
        for (id, source, title, space) in [
            ("project-memory", "memory", "Project memory", "專案 & R+D"),
            ("work-memory", "memory", "Work memory", "work"),
            ("project-web", "webpage", "Project web", "專案 & R+D"),
            ("work-web", "webpage", "Work web", "work"),
        ] {
            conn.execute(
                "INSERT INTO memories (id,content,source,source_id,title,chunk_index,last_modified,chunk_type,space)
                 VALUES (?1,?1,?2,'shared',?3,0,1,'text',?4)",
                libsql::params![id,source,title,space],
            ).await.unwrap();
        }
        drop(conn);
        let state = Arc::new(RwLock::new(crate::state::ServerState {
            db: Some(db),
            ..Default::default()
        }));
        let app = crate::router::build_router(state);
        for path in [
            "/api/indexed-files",
            "/api/chunks/shared",
            "/api/webpage-chunks/shared",
        ] {
            for (query, header, status, expected_owner) in [
                (
                    Some("space=%E5%B0%88%E6%A1%88+%26+R%2BD"),
                    None,
                    StatusCode::OK,
                    Some("project"),
                ),
                (Some("space=work"), None, StatusCode::OK, Some("work")),
                (None, Some("work"), StatusCode::OK, Some("work")),
                (
                    Some("space=%20work%20"),
                    Some(" work "),
                    StatusCode::OK,
                    Some("work"),
                ),
                (None, None, StatusCode::OK, None),
                (Some("space="), None, StatusCode::UNPROCESSABLE_ENTITY, None),
                (
                    Some("space=+%20"),
                    Some("work"),
                    StatusCode::UNPROCESSABLE_ENTITY,
                    None,
                ),
                (
                    Some("space=missing"),
                    None,
                    StatusCode::UNPROCESSABLE_ENTITY,
                    None,
                ),
                (
                    Some("space=%E5%B0%88%E6%A1%88+%26+R%2BD"),
                    Some("work"),
                    StatusCode::UNPROCESSABLE_ENTITY,
                    None,
                ),
                (
                    Some("space=work"),
                    Some("missing"),
                    StatusCode::UNPROCESSABLE_ENTITY,
                    None,
                ),
            ] {
                let uri = query.map_or_else(|| path.to_string(), |query| format!("{path}?{query}"));
                let mut request = Request::builder().uri(&uri);
                if let Some(header) = header {
                    request = request.header("x-wenlan-space", header);
                }
                let response = app
                    .clone()
                    .oneshot(request.body(Body::empty()).unwrap())
                    .await
                    .unwrap();
                assert_eq!(response.status(), status, "{uri} header {header:?}");
                let expected_ack = if status == StatusCode::OK && query.is_some() {
                    match expected_owner {
                        Some("project") => Some("%E5%B0%88%E6%A1%88%20%26%20R%2BD"),
                        Some("work") => Some("work"),
                        _ => None,
                    }
                } else {
                    None
                };
                assert_eq!(
                    response
                        .headers()
                        .get("x-wenlan-source-scope")
                        .and_then(|value| value.to_str().ok()),
                    expected_ack,
                    "{uri}"
                );
                let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
                let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
                if status != StatusCode::OK {
                    assert!(!String::from_utf8_lossy(&body).contains("Project memory"));
                    assert!(!String::from_utf8_lossy(&body).contains("Work memory"));
                    continue;
                }
                if path == "/api/indexed-files" {
                    let files = value["files"].as_array().unwrap();
                    assert_eq!(files.len(), 2);
                    for file in files {
                        let count = if expected_owner.is_some() { 1 } else { 2 };
                        assert_eq!(file["chunk_count"], count, "{uri}");
                        if let Some(owner) = expected_owner {
                            let title_prefix = if owner == "project" {
                                "Project"
                            } else {
                                "Work"
                            };
                            assert!(
                                file["title"].as_str().unwrap().starts_with(title_prefix),
                                "{value}"
                            );
                            let name = if owner == "project" {
                                "專案 & R+D"
                            } else {
                                "work"
                            };
                            assert_eq!(file["space"], name);
                        }
                    }
                } else {
                    let chunks = value.as_array().unwrap();
                    assert_eq!(chunks.len(), if expected_owner.is_some() { 1 } else { 2 });
                    let kind = if path.contains("webpage") {
                        "web"
                    } else {
                        "memory"
                    };
                    if let Some(owner) = expected_owner {
                        assert_eq!(chunks[0]["content"], format!("{owner}-{kind}"));
                    }
                }
            }
        }
    }
}
