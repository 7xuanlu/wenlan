// SPDX-License-Identifier: Apache-2.0
//! Durable identity and recovery for preparation before the client knows a manifest ID.
use crate::{
    error::ServerError,
    repair_routes::{now_epoch_seconds, validate_repair_scope_header},
    route_registry::{post, TrackedRouter},
    space_header::SpaceHeader,
    state::SharedState,
};
use axum::{extract::State, Json};
use wenlan_core::repair::{prepare_operation::BeginPrepareOperation, RepairArtifactStore};
use wenlan_types::repair_prepare_operation::{
    RepairPrepareOperationRequest, RepairPrepareOperationStatus,
};

pub(crate) fn register_prepare(router: TrackedRouter<SharedState>) -> TrackedRouter<SharedState> {
    router.route(
        "/api/repairs/prepare-operation",
        post(handle_prepare_operation),
    )
}
pub(crate) fn register_control(router: TrackedRouter<SharedState>) -> TrackedRouter<SharedState> {
    router
        .route(
            "/api/repairs/prepare-operation/status",
            post(handle_prepare_status),
        )
        .route(
            "/api/repairs/prepare-operation/cancel",
            post(handle_prepare_cancel),
        )
}
async fn bound_store(
    state: &SharedState,
    space: Option<&str>,
    request: &RepairPrepareOperationRequest,
) -> Result<RepairArtifactStore, ServerError> {
    request.validate().map_err(ServerError::ValidationError)?;
    validate_repair_scope_header(space, request.request.lint_scope())?;
    let root = state
        .read()
        .await
        .repair_root
        .clone()
        .ok_or_else(|| ServerError::Internal("repair artifact root not configured".into()))?;
    Ok(RepairArtifactStore::new(root))
}
async fn handle_prepare_operation(
    State(state): State<SharedState>,
    SpaceHeader(space): SpaceHeader,
    Json(request): Json<RepairPrepareOperationRequest>,
) -> Result<Json<RepairPrepareOperationStatus>, ServerError> {
    let store = bound_store(&state, space.as_deref(), &request).await?;
    match store.begin_prepare_operation(&request)? {
        BeginPrepareOperation::Existing(status) => Ok(Json(status)),
        BeginPrepareOperation::Run(bound) => {
            let include_deep = matches!(
                request.request.choice(),
                wenlan_types::repair_current::CurrentRepairChoice::ReclassifyMemory { .. }
                    | wenlan_types::repair_current::CurrentRepairChoice::EntityRelation { .. }
            );
            let mut fresh = crate::lint_routes::fresh_repair_reports(
                state,
                request.request.lint_scope(),
                include_deep,
            )
            .await?;
            if fresh.store.root() != bound.root() {
                return Err(ServerError::Conflict("repair_artifact_root_changed".into()));
            }
            wenlan_core::repair::current::prepare_current_repair_with_pages(
                &fresh.db,
                &bound,
                request.request.clone(),
                fresh.general,
                fresh.deep.take(),
                fresh.page_root.as_deref(),
                now_epoch_seconds()?,
            )
            .await?;
            // Release the prepare lock before querying its durable result.
            drop(bound);
            store
                .prepare_operation_status(&request)
                .map(Json)
                .map_err(ServerError::from)
        }
    }
}
async fn handle_prepare_status(
    State(state): State<SharedState>,
    SpaceHeader(space): SpaceHeader,
    Json(request): Json<RepairPrepareOperationRequest>,
) -> Result<Json<RepairPrepareOperationStatus>, ServerError> {
    bound_store(&state, space.as_deref(), &request)
        .await?
        .prepare_operation_status(&request)
        .map(Json)
        .map_err(ServerError::from)
}
async fn handle_prepare_cancel(
    State(state): State<SharedState>,
    SpaceHeader(space): SpaceHeader,
    Json(request): Json<RepairPrepareOperationRequest>,
) -> Result<Json<RepairPrepareOperationStatus>, ServerError> {
    bound_store(&state, space.as_deref(), &request)
        .await?
        .cancel_prepare_operation(&request, now_epoch_seconds()?)
        .map(Json)
        .map_err(ServerError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ServerState;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use wenlan_types::{
        repair::RepairLintScope,
        repair_current::{CurrentRepairChoice, PrepareCurrentRepairRequest},
        repair_prepare_operation::RepairPrepareOperationState,
    };

    fn request() -> RepairPrepareOperationRequest {
        RepairPrepareOperationRequest {
            operation_id: "a50e8400-e29b-41d4-a716-446655440000".into(),
            request: PrepareCurrentRepairRequest {
                lint_scope: RepairLintScope::global(),
                choice: CurrentRepairChoice::rename_page_title(
                    "review-1".into(),
                    "page-1".into(),
                    "Before".into(),
                    "After".into(),
                )
                .unwrap(),
            },
        }
    }

    #[tokio::test]
    #[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
    async fn cancelling_before_arrival_prevents_delayed_prepare_without_a_database() {
        let root = tempfile::tempdir().unwrap();
        let state = Arc::new(RwLock::new(ServerState {
            repair_root: Some(root.path().into()),
            ..ServerState::default()
        }));
        let request = request();
        let initial = handle_prepare_status(
            State(state.clone()),
            SpaceHeader(None),
            Json(request.clone()),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(initial.state, RepairPrepareOperationState::NotStarted);
        let cancelled = handle_prepare_cancel(
            State(state.clone()),
            SpaceHeader(None),
            Json(request.clone()),
        )
        .await
        .unwrap()
        .0;
        assert!(matches!(
            cancelled.state,
            RepairPrepareOperationState::Cancelled { .. }
        ));
        // If begin accidentally executes, absence of a DB makes this fail.
        assert_eq!(
            handle_prepare_operation(
                State(state.clone()),
                SpaceHeader(None),
                Json(request.clone())
            )
            .await
            .unwrap()
            .0,
            cancelled
        );
        assert_eq!(
            handle_prepare_status(
                State(state.clone()),
                SpaceHeader(None),
                Json(request.clone())
            )
            .await
            .unwrap()
            .0,
            cancelled
        );
        // A foreign Space cannot use the same operation ID even after cancellation.
        assert!(matches!(
            handle_prepare_cancel(
                State(state),
                SpaceHeader(Some("other".into())),
                Json(request)
            )
            .await,
            Err(ServerError::ValidationError(_))
        ));
    }
    #[tokio::test]
    #[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
    async fn interrupted_prepare_is_looked_up_without_repeating_work() {
        let root = tempfile::tempdir().unwrap();
        let state = Arc::new(RwLock::new(ServerState {
            repair_root: Some(root.path().into()),
            ..ServerState::default()
        }));
        let request = request();
        // Failure happens after durable admission but before a manifest exists.
        assert!(matches!(
            handle_prepare_operation(
                State(state.clone()),
                SpaceHeader(None),
                Json(request.clone())
            )
            .await,
            Err(ServerError::DbNotInitialized)
        ));
        let status = handle_prepare_status(
            State(state.clone()),
            SpaceHeader(None),
            Json(request.clone()),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(status.state, RepairPrepareOperationState::Interrupted);
        // Repeating the identical request returns its state instead of invoking preparation again.
        assert_eq!(
            handle_prepare_operation(
                State(state.clone()),
                SpaceHeader(None),
                Json(request.clone())
            )
            .await
            .unwrap()
            .0,
            status
        );
        assert!(matches!(
            handle_prepare_cancel(State(state), SpaceHeader(None), Json(request))
                .await
                .unwrap()
                .0
                .state,
            RepairPrepareOperationState::Cancelled { .. }
        ));
    }
}
