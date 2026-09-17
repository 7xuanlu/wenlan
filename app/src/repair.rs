// SPDX-License-Identifier: AGPL-3.0-only
//! Tauri commands for the daemon's typed lint and repair-control plane.
//!
//! These commands deliberately contain no repair logic. They snapshot the
//! shared HTTP client, drop the app-state read guard, and forward the shared
//! request/response contracts to the daemon.

use std::sync::Arc;

use sha2::{Digest as _, Sha256};
use tokio::sync::RwLock;
use wenlan_types::lint::{LintAgentSubmission, LintReport, LintRequestQuery};
use wenlan_types::repair::{
    ApplyRepairRequest, PrepareRepairRequest, RepairApplyReceipt, RepairManifest,
    RepairVerificationReceipt, VerifyRepairRequest,
};
use wenlan_types::repair_current::PrepareCurrentRepairRequest;
use wenlan_types::repair_plan::{
    RepairPlanEntriesPage, RepairPlanEntriesRequest, RepairPlanRequest, RepairPlanSummary,
};
use wenlan_types::repair_recovery::RepairRecovery;

use crate::state::AppState;

type State = Arc<RwLock<AppState>>;

/// Validate a manifest's daemon-issued digest after the frontend has sent it
/// back through JSON. Deserializing into the shared contract first restores
/// Rust's canonical number representation; hashing the typed unsigned draft
/// therefore avoids using a JavaScript `JSON.stringify` result as a digest
/// authority.
#[tauri::command]
pub fn repair_validate_manifest(manifest: RepairManifest) -> Result<bool, String> {
    let canonical = manifest
        .canonical_unsigned_bytes()
        .map_err(|error| format!("serialize repair manifest for validation: {error}"))?;
    let digest = Sha256::digest(&canonical);
    let digest = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(digest == manifest.manifest_digest().as_str())
}

#[tauri::command]
pub async fn repair_lint(
    state: tauri::State<'_, State>,
    query: LintRequestQuery,
) -> Result<LintReport, String> {
    let client = { state.read().await.client.clone() };
    client.lint(query).await
}

#[tauri::command]
pub async fn repair_lint_submit(
    state: tauri::State<'_, State>,
    query: LintRequestQuery,
    submission: LintAgentSubmission,
) -> Result<LintReport, String> {
    let client = { state.read().await.client.clone() };
    client.lint_submit(query, submission).await
}

#[tauri::command]
pub async fn repair_prepare(
    state: tauri::State<'_, State>,
    request: PrepareRepairRequest,
) -> Result<RepairManifest, String> {
    let client = { state.read().await.client.clone() };
    client.prepare_repair(request).await
}

#[tauri::command]
pub async fn repair_prepare_current(
    state: tauri::State<'_, State>,
    request: PrepareCurrentRepairRequest,
) -> Result<RepairManifest, String> {
    let client = { state.read().await.client.clone() };
    client.prepare_current_repair(request).await
}

#[tauri::command]
pub async fn repair_recovery(
    state: tauri::State<'_, State>,
    review_id: String,
) -> Result<Option<RepairRecovery>, String> {
    let client = { state.read().await.client.clone() };
    client.repair_recovery(&review_id).await
}

#[tauri::command]
pub async fn repair_apply(
    state: tauri::State<'_, State>,
    request: ApplyRepairRequest,
) -> Result<RepairApplyReceipt, String> {
    let client = { state.read().await.client.clone() };
    client.apply_repair(request).await
}

#[tauri::command]
pub async fn repair_verify(
    state: tauri::State<'_, State>,
    request: VerifyRepairRequest,
) -> Result<RepairVerificationReceipt, String> {
    let client = { state.read().await.client.clone() };
    client.verify_repair(request).await
}

#[tauri::command]
pub async fn repair_plan(
    state: tauri::State<'_, State>,
    request: RepairPlanRequest,
) -> Result<RepairPlanSummary, String> {
    let client = { state.read().await.client.clone() };
    client.repair_plan(request).await
}

#[tauri::command]
pub async fn repair_plan_entries(
    state: tauri::State<'_, State>,
    request: RepairPlanEntriesRequest,
) -> Result<RepairPlanEntriesPage, String> {
    let client = { state.read().await.client.clone() };
    client.repair_plan_entries(request).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use wenlan_types::lint::{
        LintDbSnapshotMode, LintDbSnapshotReceipt, LintDigest, LintEvidenceRef, LintGateEffect,
        LintPageSnapshotMode, LintPageSnapshotReceipt, LintProducerReceipt, LintScope,
        LintSemanticAction, LintSemanticFinding, LintSemanticProviderRoute, LintSemanticReasonCode,
        LintSnapshotReceipts,
    };
    use wenlan_types::repair::{
        RepairAllowedEffects, RepairCheckBaseline, RepairDigest, RepairExpectedState,
        RepairLintScope, RepairManifestDraft, RepairMutation, RepairPostAssertions,
        RepairRollbackArtifact, RepairScope, RepairSource, RepairTarget, RepairWriter,
    };

    fn digest(bytes: &[u8]) -> RepairDigest {
        let digest = Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        RepairDigest::parse(&digest).unwrap()
    }

    fn snapshots(seed: u64) -> LintSnapshotReceipts {
        LintSnapshotReceipts::new(
            LintDbSnapshotReceipt::new(
                LintDbSnapshotMode::TransactionalReadOnly,
                LintDigest::from_u64(seed),
                Some(LintDigest::from_u64(seed)),
            ),
            LintPageSnapshotReceipt::new(
                LintPageSnapshotMode::BestEffort,
                LintDigest::from_u64(seed + 1),
                Some(LintDigest::from_u64(seed + 1)),
            ),
        )
    }

    fn semantic_manifest(confidence_basis_points: u16) -> RepairManifest {
        let evidence_id = LintDigest::from_u64(42);
        let finding = LintSemanticFinding::try_new(
            wenlan_types::lint::LintOpaqueId::from_sorted_position(0).unwrap(),
            LintSemanticAction::ReclassifyMemory,
            LintSemanticReasonCode::ClassificationMismatch,
            confidence_basis_points,
            LintSemanticProviderRoute::CallingAgent,
            vec![evidence_id.clone()],
            vec![],
        )
        .unwrap();
        let source = RepairSource::try_new(
            RepairLintScope::global(),
            LintScope::global(),
            finding,
            snapshots(1),
            snapshots(3),
            LintProducerReceipt::new(None),
            LintProducerReceipt::new(None),
            LintDigest::from_u64(5),
        )
        .unwrap();
        let target = RepairTarget::memory(
            "mem_target".into(),
            RepairScope::registered("work".into()).unwrap(),
        )
        .unwrap();
        let general_baseline = vec![RepairCheckBaseline::try_new_current(
            "memories.structural.integrity".into(),
            wenlan_types::lint::LintOutcome::Pass,
            LintGateEffect::Actionable,
            0,
            vec![],
        )
        .unwrap()];
        let deep_baseline = vec![RepairCheckBaseline::try_new_current(
            "memories.semantic.classification".into(),
            wenlan_types::lint::LintOutcome::Finding,
            LintGateEffect::Actionable,
            1,
            vec![LintEvidenceRef::ReasonCode {
                reason_code: wenlan_types::lint::LintReasonCode::SemanticAgentAdjudicationRequired,
            }],
        )
        .unwrap()];
        let assertions =
            RepairPostAssertions::try_new(evidence_id, general_baseline, deep_baseline, vec![])
                .unwrap();
        let draft = RepairManifestDraft::try_new(
            "repair_550e8400-e29b-41d4-a716-446655440000".into(),
            1_721_000_000,
            source,
            target.clone(),
            RepairExpectedState::try_new(
                None,
                RepairDigest::parse(
                    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                )
                .unwrap(),
            )
            .unwrap(),
            RepairWriter::ReclassifyMemory,
            RepairMutation::try_reclassify(Some("fact"), "decision").unwrap(),
            RepairAllowedEffects::memory_type(target),
            RepairRollbackArtifact::try_new(
                "rollback-v1.json".into(),
                RepairDigest::parse(
                    "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                )
                .unwrap(),
            )
            .unwrap(),
            assertions,
        )
        .unwrap();
        let manifest_digest = digest(&draft.canonical_bytes().unwrap());
        RepairManifest::try_new(draft, manifest_digest).unwrap()
    }

    #[test]
    fn repair_manifest_validation_accepts_typed_semantic_manifest_after_json_roundtrip() {
        // The current wire contract stores semantic confidence as integer
        // basis points. Typed deserialization restores the Rust contract
        // before canonical bytes are produced.
        let manifest = semantic_manifest(10_000);
        let value = serde_json::to_value(&manifest).unwrap();
        assert_eq!(
            value["source"]["finding"]["confidence_basis_points"],
            10_000
        );
        let roundtripped: RepairManifest = serde_json::from_value(value).unwrap();

        assert!(repair_validate_manifest(roundtripped).unwrap());
    }

    #[test]
    fn repair_manifest_validation_rejects_changed_mutation_and_digest() {
        let manifest = semantic_manifest(10_000);
        let mut changed_mutation = serde_json::to_value(&manifest).unwrap();
        changed_mutation["mutation"]["after_memory_type"] = serde_json::json!("lesson");
        let changed_mutation: RepairManifest = serde_json::from_value(changed_mutation).unwrap();
        assert!(!repair_validate_manifest(changed_mutation).unwrap());

        let mut changed_digest = serde_json::to_value(&manifest).unwrap();
        changed_digest["manifest_digest"] =
            serde_json::json!("0000000000000000000000000000000000000000000000000000000000000000");
        let changed_digest: RepairManifest = serde_json::from_value(changed_digest).unwrap();
        assert!(!repair_validate_manifest(changed_digest).unwrap());
    }

    #[test]
    fn repair_manifest_validation_keeps_schema_rejection_at_typed_boundary() {
        let manifest = semantic_manifest(10_000);
        let mut value = serde_json::to_value(manifest).unwrap();
        value["manifest_schema_version"] = serde_json::json!(1);

        assert!(serde_json::from_value::<RepairManifest>(value).is_err());
    }
}
