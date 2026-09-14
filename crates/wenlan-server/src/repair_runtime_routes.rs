// SPDX-License-Identifier: Apache-2.0
//! Only the exact completed cold-recovery repair may retire its daemon.

use axum::{extract::State, Json};
use wenlan_core::repair::RepairArtifactStore;
use wenlan_types::repair_runtime::{RepairRuntimeStatus, ResumeRepairRuntimeRequest};

use crate::{
    error::ServerError,
    repair_routes::validate_manifest_scope_binding,
    route_registry::{get, post, TrackedRouter},
    space_header::SpaceHeader,
    state::{ServerState, SharedState},
};

pub(crate) fn register(router: TrackedRouter<SharedState>) -> TrackedRouter<SharedState> {
    router
        .route("/api/repairs/runtime", get(handle_runtime))
        .route("/api/repairs/runtime/resume", post(handle_resume))
}

fn runtime_status(state: &ServerState) -> RepairRuntimeStatus {
    RepairRuntimeStatus {
        instance_id: state.runtime_instance_id.clone(),
        pid: std::process::id(),
        repair_only: state.optional_runtime_workers_suspended,
        shutdown_requested: state.shutdown.is_requested(),
    }
}

async fn handle_runtime(State(state): State<SharedState>) -> Json<RepairRuntimeStatus> {
    let state = state.read().await;
    Json(runtime_status(&state))
}

async fn handle_resume(
    State(state): State<SharedState>,
    SpaceHeader(header_space): SpaceHeader,
    Json(request): Json<ResumeRepairRuntimeRequest>,
) -> Result<Json<RepairRuntimeStatus>, ServerError> {
    let (mut status, root, coordinator, shutdown) = {
        let state = state.read().await;
        (
            runtime_status(&state),
            state.repair_root.clone(),
            state.maintenance_coordinator.clone(),
            state.shutdown.clone(),
        )
    };
    if status.instance_id != request.instance_id {
        return Err(conflict("repair_runtime_instance_changed"));
    }
    if status.shutdown_requested {
        return Err(conflict("repair_runtime_shutting_down"));
    }
    // A normal daemon is never retired by this operation. Native retries must
    // check normal Activity themselves before reporting successful resumption.
    if !status.repair_only {
        return Ok(Json(status));
    }

    // Seal admission first, then inspect durable artifacts. Failed validation
    // drops the provisional seal; no lock is held across an async operation.
    let seal = coordinator
        .begin_runtime_resumption(&request.apply)
        .map_err(|error| conflict(&error.to_string()))?;
    let store =
        RepairArtifactStore::new(root.ok_or_else(|| conflict("repair_runtime_root_unavailable"))?);
    validate_manifest_scope_binding(
        &store,
        header_space.as_deref(),
        request.apply.manifest_id(),
        request.apply.approved_manifest_digest(),
    )?;
    let receipt = store
        .completed_verification_receipt(request.apply.manifest_id())?
        .ok_or_else(|| conflict("repair_runtime_verification_incomplete"))?;
    if receipt.manifest_digest() != request.apply.approved_manifest_digest()
        || receipt.receipt_digest() != &request.verification_receipt_digest
    {
        return Err(conflict("repair_runtime_receipt_mismatch"));
    }
    if !store.pending_verification_manifest_ids()?.is_empty() {
        return Err(conflict("repair_runtime_pending_repairs"));
    }
    if shutdown.is_requested() {
        return Err(conflict("repair_runtime_shutting_down"));
    }
    seal.commit();
    // Axum drains the current response after this sticky shutdown request.
    // This process only stops itself; the native owner decides how to restart
    // its exact child or registered service, preserving launch provenance.
    shutdown.request();
    status.shutdown_requested = true;
    Ok(Json(status))
}

fn conflict(code: &str) -> ServerError {
    ServerError::Conflict(code.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use tower::ServiceExt;
    use wenlan_types::repair::{ApplyRepairRequest, RepairDigest};

    fn request(instance_id: &str) -> ResumeRepairRuntimeRequest {
        ResumeRepairRuntimeRequest {
            instance_id: instance_id.into(),
            apply: ApplyRepairRequest::try_new(
                "repair_550e8400-e29b-41d4-a716-446655440000".into(),
                RepairDigest::parse(&"ab".repeat(32)).unwrap(),
                format!(
                    "apply repair repair_550e8400-e29b-41d4-a716-446655440000 {}",
                    "ab".repeat(32)
                ),
            )
            .unwrap(),
            verification_receipt_digest: RepairDigest::parse(&"cd".repeat(32)).unwrap(),
        }
    }

    #[tokio::test]
    #[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
    async fn authenticated_terminal_receipt_retires_only_the_bound_instance() {
        use wenlan_types::repair::RepairVerificationReceipt;
        let root = tempfile::tempdir().unwrap();
        let manifest_bytes = include_bytes!("../../wenlan-types/testdata/repair/v1/manifest.json");
        let receipt_bytes =
            include_bytes!("../../wenlan-types/testdata/repair/v1/verification-receipt.json");
        let receipt: RepairVerificationReceipt = serde_json::from_slice(receipt_bytes).unwrap();
        let dir = root.path().join(receipt.manifest_id());
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("manifest.json"), manifest_bytes).unwrap();
        std::fs::write(
            dir.join("apply-receipt.json"),
            include_bytes!("../../wenlan-types/testdata/repair/v1/apply-receipt.json"),
        )
        .unwrap();
        std::fs::write(dir.join("verification-receipt.json"), receipt_bytes).unwrap();
        // Legacy artifacts are authenticated and upgraded by the canonical
        // store loader, not deserialized as a current wire manifest.
        let manifest = RepairArtifactStore::new(root.path().to_path_buf())
            .load_manifest(receipt.manifest_id())
            .unwrap();
        let mut server = ServerState::default();
        server.optional_runtime_workers_suspended = true;
        server.repair_root = Some(root.path().to_path_buf());
        let request = ResumeRepairRuntimeRequest {
            instance_id: server.runtime_instance_id.clone(),
            apply: ApplyRepairRequest::try_new(
                manifest.manifest_id().into(),
                manifest.manifest_digest().clone(),
                format!(
                    "apply repair {} {}",
                    manifest.manifest_id(),
                    manifest.manifest_digest().as_str()
                ),
            )
            .unwrap(),
            verification_receipt_digest: receipt.receipt_digest().clone(),
        };
        let coordinator = server.maintenance_coordinator.clone();
        coordinator
            .rearm_approved_repair(request.apply.clone())
            .unwrap();
        coordinator.finish_recovery();
        coordinator
            .acquire_repair_verification(manifest.manifest_id())
            .unwrap()
            .release_after_verification()
            .unwrap();
        let state = Arc::new(RwLock::new(server));
        let mut wrong = request.clone();
        wrong.verification_receipt_digest = RepairDigest::parse(&"ff".repeat(32)).unwrap();
        let error = handle_resume(State(state.clone()), SpaceHeader(None), Json(wrong))
            .await
            .unwrap_err();
        assert!(
            matches!(error, ServerError::Conflict(code) if code == "repair_runtime_receipt_mismatch")
        );
        assert!(!state.read().await.shutdown.is_requested());

        let status = handle_resume(
            State(state.clone()),
            SpaceHeader(None),
            Json(request.clone()),
        )
        .await
        .unwrap()
        .0;
        assert!(status.shutdown_requested);
        assert_eq!(status.instance_id, request.instance_id);
        assert!(state.read().await.shutdown.is_requested());
        assert!(coordinator.try_begin_background().is_none());
        assert!(coordinator
            .begin_runtime_resumption(&request.apply)
            .is_err());
        assert_eq!(
            std::fs::read(dir.join("verification-receipt.json")).unwrap(),
            receipt_bytes
        );
    }

    #[tokio::test]
    async fn identity_endpoint_is_available_in_both_routers_without_reading_data() {
        let state = Arc::new(RwLock::new(ServerState::default()));
        let expected = state.read().await.runtime_instance_id.clone();
        for router in [
            crate::router::build_router(state.clone()),
            crate::router::build_repair_router(state.clone()),
        ] {
            let response = router
                .oneshot(
                    Request::get("/api/repairs/runtime")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), axum::http::StatusCode::OK);
            let body = axum::body::to_bytes(response.into_body(), 4096)
                .await
                .unwrap();
            let status: RepairRuntimeStatus = serde_json::from_slice(&body).unwrap();
            assert_eq!(status.instance_id, expected);
            assert_eq!(status.pid, std::process::id());
            assert!(!status.shutdown_requested);
        }
        assert_ne!(expected, ServerState::default().runtime_instance_id);
    }

    #[tokio::test]
    async fn wrong_instance_cannot_shutdown_even_a_normal_daemon() {
        let state = Arc::new(RwLock::new(ServerState::default()));
        let error = handle_resume(
            State(state.clone()),
            SpaceHeader(None),
            Json(request("old")),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(error, ServerError::Conflict(code) if code == "repair_runtime_instance_changed")
        );
        assert!(!state.read().await.shutdown.is_requested());
    }

    #[tokio::test]
    async fn normal_runtime_is_idempotent_and_never_requests_shutdown() {
        let state = Arc::new(RwLock::new(ServerState::default()));
        let request = request(&state.read().await.runtime_instance_id);
        let status = handle_resume(State(state.clone()), SpaceHeader(None), Json(request))
            .await
            .unwrap()
            .0;
        assert!(!status.repair_only);
        assert!(!status.shutdown_requested);
        assert!(!state.read().await.shutdown.is_requested());
    }

    #[tokio::test]
    async fn external_shutdown_wins_before_resumption_admission() {
        let mut server = ServerState::default();
        server.optional_runtime_workers_suspended = true;
        server.shutdown.request();
        let request = request(&server.runtime_instance_id);
        let state = Arc::new(RwLock::new(server));
        let error = handle_resume(State(state), SpaceHeader(None), Json(request))
            .await
            .unwrap_err();
        assert!(
            matches!(error, ServerError::Conflict(code) if code == "repair_runtime_shutting_down")
        );
    }

    #[tokio::test]
    async fn missing_durable_artifacts_cannot_retire_runtime_and_release_provisional_seal() {
        let root = tempfile::tempdir().unwrap();
        let mut server = ServerState::default();
        server.optional_runtime_workers_suspended = true;
        server.repair_root = Some(root.path().to_path_buf());
        let request = request(&server.runtime_instance_id);
        let coordinator = server.maintenance_coordinator.clone();
        coordinator
            .rearm_approved_repair(request.apply.clone())
            .unwrap();
        coordinator.finish_recovery();
        coordinator
            .acquire_repair_verification(request.apply.manifest_id())
            .unwrap()
            .release_after_verification()
            .unwrap();
        let state = Arc::new(RwLock::new(server));
        assert!(handle_resume(
            State(state.clone()),
            SpaceHeader(None),
            Json(request.clone())
        )
        .await
        .is_err());
        assert!(!state.read().await.shutdown.is_requested());
        assert!(coordinator.begin_runtime_resumption(&request.apply).is_ok());
    }
}
