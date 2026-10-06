// SPDX-License-Identifier: Apache-2.0
//! Knowledge directory inspection endpoints.

use crate::error::ServerError;
use crate::route_registry::{get, TrackedRouter};
use crate::state::SharedState;
use axum::response::Json;
use wenlan_types::responses::{KnowledgeCountResponse, KnowledgePathResponse};

pub(crate) fn register(router: TrackedRouter<SharedState>) -> TrackedRouter<SharedState> {
    router
        .route("/api/knowledge/path", get(handle_get_knowledge_path))
        .route("/api/knowledge/count", get(handle_get_knowledge_count))
        .route(
            "/api/knowledge/folders",
            get(handle_list_knowledge_folders).post(handle_create_knowledge_folder),
        )
}

/// GET /api/knowledge/path
pub async fn handle_get_knowledge_path() -> Result<Json<KnowledgePathResponse>, ServerError> {
    let cfg = wenlan_core::config::load_config();
    let path = cfg.knowledge_path_or_default();
    Ok(Json(KnowledgePathResponse {
        path: path.to_string_lossy().to_string(),
    }))
}

/// GET /api/knowledge/count
pub async fn handle_get_knowledge_count() -> Result<Json<KnowledgeCountResponse>, ServerError> {
    let cfg = wenlan_core::config::load_config();
    let path = cfg.knowledge_path_or_default();
    if !path.exists() {
        return Ok(Json(KnowledgeCountResponse { count: 0 }));
    }
    let count = std::fs::read_dir(&path)
        .map_err(|e| ServerError::Internal(e.to_string()))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .and_then(|s| s.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("md"))
                .unwrap_or(false)
        })
        .count();
    Ok(Json(KnowledgeCountResponse {
        count: count as u64,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn get_knowledge_count_returns_ok() {
        let result = handle_get_knowledge_count().await;
        assert!(result.is_ok());
    }

    #[test]
    fn count_md_files_in_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.md"), "x").unwrap();
        std::fs::write(dir.path().join("b.md"), "y").unwrap();
        std::fs::write(dir.path().join("c.txt"), "z").unwrap();

        let count = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(|ext| ext.eq_ignore_ascii_case("md"))
                    .unwrap_or(false)
            })
            .count();
        assert_eq!(count, 2);
    }
}

/// Global browsing lists real directories. Scoped browsing derives only authorized ancestors.
pub async fn handle_list_knowledge_folders(
    axum::extract::State(state): axum::extract::State<SharedState>,
    crate::space_header::SpaceHeader(header_space): crate::space_header::SpaceHeader,
    view: crate::truth_guard::TruthView,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<wenlan_types::responses::KnowledgeFoldersResponse>, ServerError> {
    use wenlan_types::responses::{KnowledgeFolderEntry, KnowledgeFoldersResponse};
    let (db, root) = {
        let s = state.read().await;
        (
            s.db.clone().ok_or(ServerError::DbNotInitialized)?,
            s.lint_config.page_root().map(std::path::Path::to_path_buf),
        )
    };
    let scope = crate::read_scope::effective_read_scope(
        &db,
        params.get("space").map(String::as_str),
        header_space.as_deref(),
    )
    .await?;
    let Some(root) = root else {
        return Ok(Json(KnowledgeFoldersResponse {
            folders: vec![],
            truncated: false,
        }));
    };
    let folders = if scope == wenlan_core::read_scope::ReadScope::Global {
        tokio::task::spawn_blocking(move || {
            wenlan_core::export::knowledge::list_knowledge_folders(&root)
        })
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))??
        .into_iter()
        .map(|f| KnowledgeFolderEntry {
            path: f.path,
            parent_path: f.parent_path,
            name: f.name,
        })
        .collect()
    } else {
        let pages = db.list_pages_scoped("active", i64::MAX, 0, &scope).await?;
        let pages = wenlan_core::truth_adapter::filter_pages(&db, &view.grant, pages).await?;
        let ids: Vec<String> = pages
            .into_iter()
            .filter(|p| p.kind != "entity" && p.kind != "overview")
            .map(|p| p.id)
            .collect();
        let paths = tokio::task::spawn_blocking(move || {
            let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
            wenlan_core::export::knowledge::KnowledgeWriter::new(root, &db)
                .live_page_filenames(&ids)
        })
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;
        let mut folders = std::collections::BTreeMap::new();
        for path in paths.values() {
            let components: Vec<&str> = path.split('/').collect();
            for depth in 1..components.len() {
                let path = components[..depth].join("/");
                folders.insert(
                    path.clone(),
                    KnowledgeFolderEntry {
                        path,
                        parent_path: components[..depth - 1].join("/"),
                        name: components[depth - 1].into(),
                    },
                );
            }
        }
        folders.into_values().collect()
    };
    Ok(Json(KnowledgeFoldersResponse {
        folders,
        truncated: false,
    }))
}

pub async fn handle_create_knowledge_folder(
    axum::extract::State(state): axum::extract::State<SharedState>,
    crate::space_header::SpaceHeader(header_space): crate::space_header::SpaceHeader,
    Json(req): Json<wenlan_types::requests::CreateKnowledgeFolderRequest>,
) -> Result<Json<wenlan_types::responses::CreateKnowledgeFolderResponse>, ServerError> {
    let (db, root) = {
        let s = state.read().await;
        (
            s.db.clone().ok_or(ServerError::DbNotInitialized)?,
            s.lint_config.page_root().map(std::path::Path::to_path_buf),
        )
    };
    let scope = crate::read_scope::effective_read_scope(&db, None, header_space.as_deref()).await?;
    if scope != wenlan_core::read_scope::ReadScope::Global {
        return Err(ServerError::ValidationError(
            "Scoped callers cannot create unowned empty Wiki folders".into(),
        ));
    }
    let root = root
        .ok_or_else(|| ServerError::ValidationError("Wiki projection is not configured".into()))?;
    let projection_write = db.begin_page_projection_write();
    let path = tokio::task::spawn_blocking(move || {
        let _projection_write = projection_write;
        wenlan_core::export::knowledge::create_knowledge_folder(&root, &req.parent_path, &req.name)
    })
    .await
    .map_err(|e| ServerError::Internal(e.to_string()))??;
    Ok(Json(
        wenlan_types::responses::CreateKnowledgeFolderResponse { path },
    ))
}

#[cfg(test)]
mod folder_route_tests {
    use super::*;
    use crate::space_header::SpaceHeader;
    use crate::state::{LintServerConfig, ServerState};
    use crate::truth_guard::TruthView;
    use axum::extract::{Query, State};
    use std::sync::Arc;
    use tokio::sync::RwLock;

    #[tokio::test]
    async fn folder_http_routes_round_trip_and_reject_scoped_creation() {
        use axum::{
            body::Body,
            http::{Request, StatusCode},
        };
        use tower::ServiceExt;
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(
            wenlan_core::db::MemoryDB::new(tmp.path(), Arc::new(wenlan_core::events::NoopEmitter))
                .await
                .unwrap(),
        );
        db.create_space("work", None, false).await.unwrap();
        let root = tmp.path().join("pages");
        std::fs::create_dir(&root).unwrap();
        let state = Arc::new(RwLock::new(ServerState {
            db: Some(db),
            lint_config: LintServerConfig::new(vec![], Some(root.clone())),
            ..Default::default()
        }));
        let response = crate::router::build_router(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/knowledge/folders")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"parent_path":"","name":"Writing"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let response = crate::router::build_router(state.clone())
            .oneshot(
                Request::builder()
                    .uri("/api/knowledge/folders")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 65536)
            .await
            .unwrap();
        let response: wenlan_types::responses::KnowledgeFoldersResponse =
            serde_json::from_slice(&bytes).unwrap();
        assert_eq!(response.folders[0].path, "Writing");
        assert!(!response.truncated);
        let response = crate::router::build_router(state)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/knowledge/folders")
                    .header("content-type", "application/json")
                    .header("x-wenlan-space", "work")
                    .body(Body::from(r#"{"parent_path":"","name":"Forbidden"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert!(!root.join("Forbidden").exists());
    }

    #[tokio::test]
    async fn scoped_folder_listing_returns_only_authorized_live_ancestors() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(
            wenlan_core::db::MemoryDB::new(tmp.path(), Arc::new(wenlan_core::events::NoopEmitter))
                .await
                .unwrap(),
        );
        db.create_space("work", None, false).await.unwrap();
        db.create_space("private", None, false).await.unwrap();
        let root = tmp.path().join("pages");
        std::fs::create_dir_all(root.join("Visible/Nested")).unwrap();
        std::fs::create_dir_all(root.join("Secret")).unwrap();
        std::fs::create_dir_all(root.join("Empty")).unwrap();
        for (id, space, folder) in [
            (
                "page_00000000-0000-4000-8000-000000000fb1",
                "work",
                "Visible/Nested",
            ),
            (
                "page_00000000-0000-4000-8000-000000000fb2",
                "private",
                "Secret",
            ),
        ] {
            let draft = db
                .create_page_draft_with_id_in_registered_space_and_folder(
                    id,
                    id,
                    "Body",
                    Some(space),
                    folder,
                )
                .await
                .unwrap();
            let wenlan_core::pages::PageDraftPublishOutcome::Published(page) =
                db.publish_page_draft(id, draft.version).await.unwrap()
            else {
                panic!("publish")
            };
            wenlan_core::export::knowledge::KnowledgeProjectionWrite::new(root.clone(), &db)
                .write_page_gated(&db, &page)
                .await
                .unwrap();
        }
        // A live old projection must not reveal its folder after a failed verdict.
        let hidden_id = "page_00000000-0000-4000-8000-000000000fb3";
        std::fs::create_dir(root.join("HiddenTruth")).unwrap();
        let draft = db
            .create_page_draft_with_id_in_registered_space_and_folder(
                hidden_id,
                "Hidden",
                "Body",
                Some("work"),
                "HiddenTruth",
            )
            .await
            .unwrap();
        let wenlan_core::pages::PageDraftPublishOutcome::Published(hidden) = db
            .publish_page_draft(hidden_id, draft.version)
            .await
            .unwrap()
        else {
            panic!("publish")
        };
        wenlan_core::export::knowledge::KnowledgeProjectionWrite::new(root.clone(), &db)
            .write_page_gated(&db, &hidden)
            .await
            .unwrap();
        let fixture = libsql::Builder::new_local(tmp.path().join("origin_memory.db"))
            .build()
            .await
            .unwrap();
        let conn = fixture.connect().unwrap();
        conn.execute("INSERT INTO page_truth_state(page_id,page_version,support_status,human_reviewed,updated_at,evaluated_at) VALUES(?1,?2,'provisional',0,1,1)",libsql::params![hidden_id,hidden.version]).await.unwrap();
        conn.execute("INSERT OR REPLACE INTO claim_derivation_markers(page_id,page_version,page_version_digest,extractor_version,inventory_count,created_at) VALUES(?1,?2,?3,?4,1,0)",libsql::params![hidden_id,hidden.version,wenlan_core::provenance::revision_content_digest(&hidden.content),wenlan_core::db::EXTRACTOR_VERSION]).await.unwrap();
        // Exercise enforcement explicitly; the production default promoter is advisory.
        db.set_app_metadata("claim_promoter_enforcement", "1")
            .await
            .unwrap();
        db.set_truth_cutover_generation(1).await.unwrap();
        let state = Arc::new(RwLock::new(ServerState {
            db: Some(db),
            lint_config: LintServerConfig::new(vec![], Some(root)),
            ..Default::default()
        }));
        let Json(response) = handle_list_knowledge_folders(
            State(state.clone()),
            SpaceHeader(Some("work".into())),
            TruthView::automatic(),
            Query(Default::default()),
        )
        .await
        .unwrap();
        assert_eq!(
            response
                .folders
                .iter()
                .map(|f| f.path.as_str())
                .collect::<Vec<_>>(),
            vec!["Visible", "Visible/Nested"]
        );
        assert!(!response.truncated);
        let Json(global) = handle_list_knowledge_folders(
            State(state.clone()),
            SpaceHeader(None),
            TruthView::automatic(),
            Query(Default::default()),
        )
        .await
        .unwrap();
        assert!(global.folders.iter().any(|f| f.path == "Empty"));
        assert!(global.folders.iter().any(|f| f.path == "Secret"));
        assert!(handle_create_knowledge_folder(
            State(state.clone()),
            SpaceHeader(Some("work".into())),
            Json(wenlan_types::requests::CreateKnowledgeFolderRequest {
                parent_path: "".into(),
                name: "Forbidden".into()
            })
        )
        .await
        .is_err());
        let Json(created) = handle_create_knowledge_folder(
            State(state),
            SpaceHeader(None),
            Json(wenlan_types::requests::CreateKnowledgeFolderRequest {
                parent_path: "Visible".into(),
                name: "New".into(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(created.path, "Visible/New");
    }
}
