// SPDX-License-Identifier: Apache-2.0
//! Thin HTTP framing for approval-gated lint repair artifacts.

use crate::{
    error::ServerError,
    route_registry::{post, TrackedRouter},
    state::SharedState,
};
use axum::{extract::State, Json};
use std::time::{SystemTime, UNIX_EPOCH};
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
    Ok((db, RepairArtifactStore::new(root), page_root))
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
    let (db, store, _) = repair_context(&state).await?;
    wenlan_core::repair::prepare_memory_reclassification(&db, &store, request, now_epoch_seconds()?)
        .await
        .map(Json)
        .map_err(ServerError::from)
}

async fn handle_apply(
    State(state): State<SharedState>,
    Json(request): Json<ApplyRepairRequest>,
) -> Result<Json<RepairApplyReceipt>, ServerError> {
    let (db, store, _) = repair_context(&state).await?;
    wenlan_core::repair::apply_repair(&db, &store, request, now_epoch_seconds()?)
        .await
        .map(Json)
        .map_err(ServerError::from)
}

async fn handle_verify(
    State(state): State<SharedState>,
    Json(request): Json<VerifyRepairRequest>,
) -> Result<Json<RepairVerificationReceipt>, ServerError> {
    let (db, store, page_root) = repair_context(&state).await?;
    wenlan_core::repair::record_repair_verification(
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
