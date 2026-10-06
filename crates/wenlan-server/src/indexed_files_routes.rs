// SPDX-License-Identifier: Apache-2.0
use crate::error::ServerError;
use crate::route_registry::{delete, get, post, put, TrackedRouter};
use crate::state::{ServerState, SharedState};
use axum::{
    extract::{Path, State},
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

/// GET /api/indexed-files
pub async fn handle_list_indexed_files(
    State(state): State<Arc<RwLock<ServerState>>>,
    crate::space_header::SpaceHeader(header_space): crate::space_header::SpaceHeader,
) -> Result<Json<wenlan_types::responses::IndexedFilesResponse>, ServerError> {
    let db = {
        let s = state.read().await;
        s.db.clone().ok_or(ServerError::DbNotInitialized)?
    };
    let scope = crate::read_scope::effective_read_scope(&db, None, header_space.as_deref()).await?;
    let files = db
        .list_indexed_files_scoped(&scope)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;
    Ok(Json(wenlan_types::responses::IndexedFilesResponse {
        files,
    }))
}

/// GET /api/chunks/{source_id}
pub async fn handle_get_chunks(
    State(state): State<Arc<RwLock<ServerState>>>,
    crate::space_header::SpaceHeader(header_space): crate::space_header::SpaceHeader,
    Path(source_id): Path<String>,
) -> Result<Json<Vec<wenlan_core::db::MemoryDetail>>, ServerError> {
    let db = {
        let s = state.read().await;
        s.db.clone().ok_or(ServerError::DbNotInitialized)?
    };
    let scope = crate::read_scope::effective_read_scope(&db, None, header_space.as_deref()).await?;
    let chunks = db
        .get_chunks_scoped(&source_id, &scope)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?
        .ok_or_else(|| ServerError::NotFound("memory not found".to_string()))?;
    Ok(Json(chunks))
}

/// GET /api/webpage-chunks/{source_id}. Exact kind prevents document collisions;
/// a dedicated path also fails closed against an older daemon.
pub async fn handle_get_webpage_chunks(
    State(state): State<Arc<RwLock<ServerState>>>,
    crate::space_header::SpaceHeader(header_space): crate::space_header::SpaceHeader,
    Path(source_id): Path<String>,
) -> Result<Json<Vec<wenlan_core::db::MemoryDetail>>, ServerError> {
    let db = {
        let s = state.read().await;
        s.db.clone().ok_or(ServerError::DbNotInitialized)?
    };
    let scope = crate::read_scope::effective_read_scope(&db, None, header_space.as_deref()).await?;
    let chunks = db
        .get_webpage_chunks_scoped(&source_id, &scope)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?
        .ok_or_else(|| ServerError::NotFound("webpage not found".to_string()))?;
    Ok(Json(chunks))
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
