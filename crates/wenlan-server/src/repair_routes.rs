// SPDX-License-Identifier: Apache-2.0
//! Thin HTTP framing for approval-gated lint repair artifacts.

use crate::{
    error::ServerError,
    route_registry::{post, TrackedRouter},
    state::SharedState,
};
use axum::{extract::State, Json};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use wenlan_core::repair::RepairArtifactStore;
use wenlan_types::repair::{
    ApplyRepairRequest, PrepareRepairRequest, RepairApplyReceipt, RepairManifest,
    RepairVerificationReceipt, VerifyRepairRequest,
};

pub(crate) fn register(router: TrackedRouter<SharedState>) -> TrackedRouter<SharedState> {
    router
        .route("/api/repairs/prepare", post(handle_prepare))
        .route("/api/repairs/apply", post(handle_apply))
        .route("/api/repairs/verify", post(handle_verify))
}

async fn repair_context(
    state: &SharedState,
) -> Result<
    (
        std::sync::Arc<wenlan_core::db::MemoryDB>,
        RepairArtifactStore,
        Option<std::path::PathBuf>,
        crate::maintenance_coordinator::MaintenanceCoordinator,
    ),
    ServerError,
> {
    let state = state.read().await;
    let db = state.db.clone().ok_or(ServerError::DbNotInitialized)?;
    let root = state
        .repair_root
        .clone()
        .ok_or_else(|| ServerError::Internal("repair artifact root not configured".to_string()))?;
    let page_root = state
        .lint_config
        .page_root()
        .map(std::path::Path::to_path_buf);
    Ok((
        db,
        RepairArtifactStore::new(root),
        page_root,
        state.maintenance_coordinator.clone(),
    ))
}

fn now_epoch_seconds() -> Result<i64, ServerError> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| ServerError::Internal(format!("system clock before epoch: {error}")))?
        .as_secs();
    i64::try_from(seconds)
        .map_err(|_| ServerError::Internal("system clock outside repair range".to_string()))
}

async fn handle_prepare(
    State(state): State<SharedState>,
    Json(request): Json<PrepareRepairRequest>,
) -> Result<Json<RepairManifest>, ServerError> {
    let (db, store, page_root, _) = repair_context(&state).await?;
    wenlan_core::repair::prepare_memory_reclassification_with_pages(
        &db,
        &store,
        request,
        page_root.as_deref(),
        now_epoch_seconds()?,
    )
    .await
    .map(Json)
    .map_err(ServerError::from)
}

async fn handle_apply(
    State(state): State<SharedState>,
    Json(request): Json<ApplyRepairRequest>,
) -> Result<Json<RepairApplyReceipt>, ServerError> {
    let manifest_id = request.manifest_id().to_string();
    let (db, store, _, coordinator) = repair_context(&state).await?;
    let repair_fence = coordinator
        .acquire_repair(&manifest_id, Duration::from_secs(30))
        .await
        .map_err(|error| {
            ServerError::from(wenlan_core::error::WenlanError::Conflict(error.to_string()))
        })?;
    match wenlan_core::repair::apply_repair(&db, &store, request, now_epoch_seconds()?).await {
        Ok(receipt) => {
            repair_fence.retain_until_verification().map_err(|error| {
                ServerError::from(wenlan_core::error::WenlanError::Conflict(error.to_string()))
            })?;
            Ok(Json(receipt))
        }
        Err(apply_error) => {
            // Core commits the canonical mutation before publishing the apply
            // receipt. If publication fails, the durable pending/final
            // artifact is the crash-recovery signal: retain the fence even
            // though this HTTP attempt returns an error. Pre-commit failures
            // abort and remove the pending artifact, so their provisional
            // fence still drops normally.
            let durable_apply = store
                .pending_verification_manifest_ids()
                .map(|ids| ids.iter().any(|id| id == &manifest_id));
            match durable_apply {
                Ok(true) => repair_fence.retain_until_verification().map_err(|error| {
                    ServerError::from(wenlan_core::error::WenlanError::Conflict(error.to_string()))
                })?,
                Ok(false) => repair_fence
                    .release_after_precommit_failure()
                    .map_err(|error| {
                        ServerError::from(wenlan_core::error::WenlanError::Conflict(
                            error.to_string(),
                        ))
                    })?,
                Err(scan_error) => {
                    // Unreadable durable state is itself fail-closed: keep the
                    // fence and surface the scan error instead of resuming
                    // background writers under uncertainty.
                    repair_fence.retain_until_verification().map_err(|error| {
                        ServerError::from(wenlan_core::error::WenlanError::Conflict(
                            error.to_string(),
                        ))
                    })?;
                    return Err(ServerError::from(scan_error));
                }
            }
            Err(ServerError::from(apply_error))
        }
    }
}

async fn handle_verify(
    State(state): State<SharedState>,
    Json(request): Json<VerifyRepairRequest>,
) -> Result<Json<RepairVerificationReceipt>, ServerError> {
    let manifest_id = request.manifest_id().to_string();
    let (db, store, page_root, coordinator) = repair_context(&state).await?;
    let repair_fence = match coordinator.acquire_repair_verification(&manifest_id) {
        Ok(repair_fence) => Some(repair_fence),
        Err(error) => {
            if error == crate::maintenance_coordinator::MaintenanceFenceError::Expired
                && store
                    .has_completed_verification(&manifest_id)
                    .map_err(ServerError::from)?
            {
                None
            } else {
                return Err(ServerError::from(
                    wenlan_core::error::WenlanError::Conflict(error.to_string()),
                ));
            }
        }
    };
    let result = wenlan_core::repair::record_repair_verification(
        &db,
        &store,
        request,
        page_root.as_deref(),
        now_epoch_seconds()?,
    )
    .await;
    match result {
        Ok(receipt) => {
            if let Some(repair_fence) = repair_fence {
                repair_fence.release_after_verification().map_err(|error| {
                    ServerError::from(wenlan_core::error::WenlanError::Conflict(error.to_string()))
                })?;
            }
            Ok(Json(receipt))
        }
        Err(error) => Err(ServerError::from(error)),
    }
}
