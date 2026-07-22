// SPDX-License-Identifier: Apache-2.0
//! Thin HTTP framing for approval-gated lint repair artifacts.

use crate::{
    error::ServerError,
    route_registry::{post, TrackedRouter},
    space_header::SpaceHeader,
    state::SharedState,
};
use axum::{extract::State, Json};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use wenlan_core::repair::RepairArtifactStore;
use wenlan_types::repair::{
    ApplyRepairRequest, PrepareRepairRequest, RepairApplyReceipt, RepairDigest, RepairLintScope,
    RepairManifest, RepairVerificationReceipt, VerifyRepairRequest,
};
use wenlan_types::repair_plan::{
    PrepareRepairPlanResponse, RepairPlanEntriesPage, RepairPlanEntriesRequest, RepairPlanRequest,
    RepairPlanSummary,
};

const REPAIR_HANDOFF_TTL: Duration = Duration::from_secs(120);

pub(crate) fn register(router: TrackedRouter<SharedState>) -> TrackedRouter<SharedState> {
    register_execution(
        router
            .route("/api/repairs/plan", post(handle_plan))
            .route("/api/repairs/plan/entries", post(handle_plan_entries))
            .route("/api/repairs/prepare", post(handle_prepare)),
    )
}

pub(crate) fn register_execution(router: TrackedRouter<SharedState>) -> TrackedRouter<SharedState> {
    router
        .route("/api/repairs/apply", post(handle_apply))
        .route("/api/repairs/verify", post(handle_verify))
}

async fn handle_plan(
    State(state): State<SharedState>,
    SpaceHeader(header_space): SpaceHeader,
    Json(request): Json<RepairPlanRequest>,
) -> Result<Json<RepairPlanSummary>, ServerError> {
    validate_repair_scope_binding(
        header_space.as_deref(),
        request.scope(),
        request.general_report().scope(),
        request.deep_report().map(|report| report.scope()),
    )?;
    let (db, store, page_root, _) = repair_context(&state).await?;
    let plan = wenlan_core::repair_plan::prepare_repair_plan(
        &db,
        &store,
        request,
        page_root.as_deref(),
        now_epoch_seconds()?,
    )
    .await?;
    let artifact_path = store
        .plan_path(plan.plan_id())?
        .to_string_lossy()
        .into_owned();
    PrepareRepairPlanResponse::try_new(plan, artifact_path)
        .and_then(|response| response.compact_summary())
        .map(Json)
        .map_err(|error| {
            ServerError::from(wenlan_core::error::WenlanError::Validation(
                error.to_string(),
            ))
        })
}

pub(crate) fn validate_repair_scope_binding(
    header_space: Option<&str>,
    lint_scope: &RepairLintScope,
    general_report_scope: &wenlan_types::lint::LintScope,
    deep_report_scope: Option<&wenlan_types::lint::LintScope>,
) -> Result<(), ServerError> {
    validate_repair_scope_header(header_space, lint_scope)?;
    if header_space.is_none() {
        return Ok(());
    }
    let reports_are_registered = general_report_scope.kind()
        == wenlan_types::lint::LintScopeKind::Registered
        && deep_report_scope
            .is_none_or(|scope| scope.kind() == wenlan_types::lint::LintScopeKind::Registered);
    if reports_are_registered {
        Ok(())
    } else {
        Err(ServerError::ValidationError(
            "repair scope must exactly match X-Wenlan-Space".to_string(),
        ))
    }
}

fn validate_repair_scope_header(
    header_space: Option<&str>,
    lint_scope: &RepairLintScope,
) -> Result<(), ServerError> {
    let Some(header_space) = header_space else {
        return Ok(());
    };
    if matches!(
        lint_scope,
        RepairLintScope::Registered { space } if space == header_space
    ) {
        Ok(())
    } else {
        Err(ServerError::ValidationError(
            "repair scope must exactly match X-Wenlan-Space".to_string(),
        ))
    }
}

fn validate_manifest_scope_binding(
    store: &RepairArtifactStore,
    header_space: Option<&str>,
    manifest_id: &str,
    expected_digest: &RepairDigest,
) -> Result<(), ServerError> {
    let manifest = store
        .load_manifest(manifest_id)
        .map_err(ServerError::from)?;
    if manifest.manifest_digest() != expected_digest {
        return Err(ServerError::ValidationError(
            "repair manifest digest mismatch".to_string(),
        ));
    }
    validate_repair_scope_header(header_space, manifest.source().lint_scope())
}

async fn handle_plan_entries(
    State(state): State<SharedState>,
    SpaceHeader(header_space): SpaceHeader,
    Json(request): Json<RepairPlanEntriesRequest>,
) -> Result<Json<RepairPlanEntriesPage>, ServerError> {
    let (_, store, _, _) = repair_context(&state).await?;
    let page = store
        .load_plan_entries_page(&request)
        .map_err(ServerError::from)?;
    validate_repair_scope_header(header_space.as_deref(), page.scope())?;
    Ok(Json(page))
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
    SpaceHeader(header_space): SpaceHeader,
    Json(request): Json<PrepareRepairRequest>,
) -> Result<Json<RepairManifest>, ServerError> {
    validate_repair_scope_binding(
        header_space.as_deref(),
        request.lint_scope(),
        request.general_report().scope(),
        request.deep_report().map(|report| report.scope()),
    )?;
    let (db, store, page_root, _) = repair_context(&state).await?;
    // This is the single choice dispatcher. Title intent is absent from the
    // repair-only router, and core additionally fails closed when the DB has no
    // embedder; only core-computed canonical bytes can enter the manifest.
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
    SpaceHeader(header_space): SpaceHeader,
    Json(request): Json<ApplyRepairRequest>,
) -> Result<Json<RepairApplyReceipt>, ServerError> {
    let manifest_id = request.manifest_id().to_string();
    let (db, store, page_root, coordinator) = repair_context(&state).await?;
    validate_manifest_scope_binding(
        &store,
        header_space.as_deref(),
        request.manifest_id(),
        request.approved_manifest_digest(),
    )?;
    let mut repair_fence = coordinator
        .acquire_approved_repair(&request, Duration::from_secs(30))
        .await
        .map_err(|error| {
            ServerError::from(wenlan_core::error::WenlanError::Conflict(error.to_string()))
        })?;
    repair_fence.retain_on_uncertain_drop();
    match wenlan_core::repair::apply_repair_with_pages(
        &db,
        &store,
        request,
        page_root.as_deref(),
        now_epoch_seconds()?,
    )
    .await
    {
        Ok(receipt) => {
            if store
                .has_completed_verification(&manifest_id)
                .map_err(ServerError::from)?
            {
                repair_fence
                    .release_after_precommit_failure()
                    .map_err(|error| {
                        ServerError::from(wenlan_core::error::WenlanError::Conflict(
                            error.to_string(),
                        ))
                    })?;
            } else {
                repair_fence.retain_until_verification().map_err(|error| {
                    ServerError::from(wenlan_core::error::WenlanError::Conflict(error.to_string()))
                })?;
            }
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
    SpaceHeader(header_space): SpaceHeader,
    Json(request): Json<VerifyRepairRequest>,
) -> Result<Json<RepairVerificationReceipt>, ServerError> {
    let manifest_id = request.manifest_id().to_string();
    let (db, store, page_root, coordinator) = repair_context(&state).await?;
    validate_manifest_scope_binding(
        &store,
        header_space.as_deref(),
        request.manifest_id(),
        request.manifest_digest(),
    )?;
    let next_apply = request.next_apply().cloned();
    if let Some(next_apply) = next_apply.as_ref() {
        validate_manifest_scope_binding(
            &store,
            header_space.as_deref(),
            next_apply.manifest_id(),
            next_apply.approved_manifest_digest(),
        )?;
        validate_handoff(&store, &manifest_id, next_apply)?;
    }
    let already_verified = store
        .has_completed_verification(&manifest_id)
        .map_err(ServerError::from)?;
    let repair_fence = match coordinator.acquire_repair_verification(&manifest_id) {
        Ok(repair_fence) => Some(repair_fence),
        Err(error) => {
            let matching_retry = already_verified
                && next_apply
                    .as_ref()
                    .is_none_or(|next| coordinator.matches_handoff(&manifest_id, next));
            if matches!(
                error,
                crate::maintenance_coordinator::MaintenanceFenceError::Expired
                    | crate::maintenance_coordinator::MaintenanceFenceError::Conflict
            ) && matching_retry
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
                match next_apply {
                    Some(next_apply) => {
                        repair_fence.handoff_after_verification(next_apply, REPAIR_HANDOFF_TTL)
                    }
                    None => repair_fence.release_after_verification(),
                }
                .map_err(|error| {
                    ServerError::from(wenlan_core::error::WenlanError::Conflict(error.to_string()))
                })?;
            }
            Ok(Json(receipt))
        }
        Err(error) => Err(ServerError::from(error)),
    }
}

fn validate_handoff(
    store: &RepairArtifactStore,
    current_manifest_id: &str,
    next_apply: &ApplyRepairRequest,
) -> Result<(), ServerError> {
    let current = store
        .load_manifest(current_manifest_id)
        .map_err(ServerError::from)?;
    let next = store
        .load_manifest(next_apply.manifest_id())
        .map_err(ServerError::from)?;
    if next.manifest_digest() != next_apply.approved_manifest_digest()
        || current.source().lint_scope() != next.source().lint_scope()
    {
        return Err(ServerError::Conflict(
            "repair_handoff_manifest_mismatch".to_string(),
        ));
    }
    Ok(())
}
