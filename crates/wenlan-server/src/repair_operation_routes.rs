// SPDX-License-Identifier: Apache-2.0
//! Exact-manifest status and cancellation; neither endpoint applies a repair.

use axum::{extract::State, Json};
use wenlan_core::repair::RepairArtifactStore;
use wenlan_types::{repair::ApplyRepairRequest, repair_operation::RepairOperationStatus};

use crate::{
    error::ServerError,
    repair_routes::{now_epoch_seconds, validate_manifest_scope_binding},
    route_registry::{post, TrackedRouter},
    space_header::SpaceHeader,
    state::SharedState,
};

pub(crate) fn register(router: TrackedRouter<SharedState>) -> TrackedRouter<SharedState> {
    router
        .route("/api/repairs/status", post(handle_status))
        .route("/api/repairs/cancel", post(handle_cancel))
}

async fn bound_store(
    state: &SharedState,
    header_space: Option<&str>,
    request: &ApplyRepairRequest,
) -> Result<RepairArtifactStore, ServerError> {
    let root = state
        .read()
        .await
        .repair_root
        .clone()
        .ok_or_else(|| ServerError::Internal("repair artifact root not configured".into()))?;
    let store = RepairArtifactStore::new(root);
    validate_manifest_scope_binding(
        &store,
        header_space,
        request.manifest_id(),
        request.approved_manifest_digest(),
    )?;
    Ok(store)
}

async fn handle_status(
    State(state): State<SharedState>,
    SpaceHeader(space): SpaceHeader,
    Json(request): Json<ApplyRepairRequest>,
) -> Result<Json<RepairOperationStatus>, ServerError> {
    bound_store(&state, space.as_deref(), &request)
        .await?
        .repair_operation_status(&request)
        .map(Json)
        .map_err(ServerError::from)
}

async fn handle_cancel(
    State(state): State<SharedState>,
    SpaceHeader(space): SpaceHeader,
    Json(request): Json<ApplyRepairRequest>,
) -> Result<Json<RepairOperationStatus>, ServerError> {
    bound_store(&state, space.as_deref(), &request)
        .await?
        .cancel_prepared_repair(&request, now_epoch_seconds()?)
        .map(Json)
        .map_err(ServerError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ServerState;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use wenlan_types::repair_operation::RepairOperationState;

    fn fixture() -> (tempfile::TempDir, SharedState, ApplyRepairRequest) {
        let root = tempfile::tempdir().unwrap();
        let id = "repair_550e8400-e29b-41d4-a716-446655440000";
        let dir = root.path().join(id);
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(
            dir.join("manifest.json"),
            include_bytes!("../../wenlan-types/testdata/repair/v1/manifest.json"),
        )
        .unwrap();
        let manifest = RepairArtifactStore::new(root.path().to_path_buf())
            .load_manifest(id)
            .unwrap();
        let request = ApplyRepairRequest::try_new(
            id.into(),
            manifest.manifest_digest().clone(),
            format!(
                "apply repair {} {}",
                id,
                manifest.manifest_digest().as_str()
            ),
        )
        .unwrap();
        let state = Arc::new(RwLock::new(ServerState {
            repair_root: Some(root.path().to_path_buf()),
            ..ServerState::default()
        }));
        (root, state, request)
    }

    #[tokio::test]
    #[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
    async fn status_cancel_work_without_database_and_preserve_runtime() {
        let (root, state, request) = fixture();
        let before = state.read().await.runtime_instance_id.clone();
        let status = handle_status(
            State(state.clone()),
            SpaceHeader(None),
            Json(request.clone()),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(status.state, RepairOperationState::Prepared);
        assert!(!root
            .path()
            .join(request.manifest_id())
            .join("cancelled.json")
            .exists());
        let cancelled = handle_cancel(
            State(state.clone()),
            SpaceHeader(None),
            Json(request.clone()),
        )
        .await
        .unwrap()
        .0;
        assert!(matches!(
            cancelled.state,
            RepairOperationState::Cancelled { .. }
        ));
        let retry = handle_cancel(
            State(state.clone()),
            SpaceHeader(None),
            Json(request.clone()),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(retry, cancelled);
        assert_eq!(
            handle_status(
                State(state.clone()),
                SpaceHeader(None),
                Json(request.clone())
            )
            .await
            .unwrap()
            .0,
            cancelled
        );
        assert_eq!(state.read().await.runtime_instance_id, before);
        assert!(!root
            .path()
            .join(request.manifest_id())
            .join("apply-receipt.json")
            .exists());
    }

    #[tokio::test]
    #[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
    async fn foreign_scope_cannot_query_or_cancel() {
        let (root, state, request) = fixture();
        for cancel in [false, true] {
            let result = if cancel {
                handle_cancel(
                    State(state.clone()),
                    SpaceHeader(Some("other-space".into())),
                    Json(request.clone()),
                )
                .await
            } else {
                handle_status(
                    State(state.clone()),
                    SpaceHeader(Some("other-space".into())),
                    Json(request.clone()),
                )
                .await
            };
            assert!(matches!(result, Err(ServerError::ValidationError(_))));
        }
        assert!(!root
            .path()
            .join(request.manifest_id())
            .join("cancelled.json")
            .exists());
    }
}
