// SPDX-License-Identifier: Apache-2.0
//! Durable relation preparation and the canonical transaction writer boundary.

use super::relation_snapshot::{RelationCaptureContext, RelationReader};
use super::*;
use wenlan_types::lint::{LintSemanticCheckId, LintSemanticFinding};
use wenlan_types::repair::{RepairRollbackPayloadV3, RepairRollbackV3};
use wenlan_types::repair_plan::RepairPlanRequest;
use wenlan_types::repair_relation::{
    EntityRelationRepairChoice, EntityRelationRepairSelection, RepairRelationMutation,
    RepairRelationSnapshot,
};

#[cfg(test)]
#[path = "relation_tests.rs"]
mod tests;

/// Candidate findings carry the complete record evidence set. A shared
/// endpoint alone must not make another relation look like this target.
pub(super) fn finding_matches_subject(
    selected: &LintSemanticFinding,
    from_entity: &str,
    to_entity: &str,
    predicate: &str,
    observed: &LintSemanticFinding,
) -> bool {
    let same_evidence = selected
        .evidence_ids()
        .iter()
        .all(|id| observed.evidence_ids().contains(id));
    // After Add, an ExistingLink judgment uses predicate-bound keys instead
    // of the MissingLink entity/memory keys. Retire can make that transition
    // in reverse. Both describe the same repaired relationship; advisory
    // findings are not caught by the generic new-actionable gate.
    let plain_pair =
        [from_entity, to_entity].map(|id| crate::lint::semantic_record_digest("entity", id));
    let predicate_pair = [
        crate::lint::semantic_record_key_digest(&format!(
            "relation-entity:{from_entity}:{predicate}:from"
        )),
        crate::lint::semantic_record_key_digest(&format!(
            "relation-entity:{to_entity}:{predicate}:to"
        )),
    ];
    same_evidence
        || [plain_pair, predicate_pair]
            .iter()
            .any(|pair| pair.iter().all(|id| observed.evidence_ids().contains(id)))
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn prepare(
    db: &MemoryDB,
    store: &RepairArtifactStore,
    lint_scope: RepairLintScope,
    selection: &EntityRelationRepairSelection,
    expected_finding: Option<&LintSemanticFinding>,
    general: &LintReport,
    deep: Option<&LintReport>,
    page_root: Option<&Path>,
    now_epoch: i64,
) -> Result<RepairManifest, WenlanError> {
    ensure_repair_artifacts_supported()?;
    if now_epoch <= 0 {
        return Err(WenlanError::Validation("invalid_repair_prepared_at".into()));
    }
    let deep = deep.ok_or_else(|| WenlanError::Validation("repair_deep_report_missing".into()))?;
    let request =
        RepairPlanRequest::try_new(lint_scope.clone(), general.clone(), Some(deep.clone()))
            .map_err(|error| WenlanError::Validation(error.to_string()))?;
    let snapshot = db.open_lint_snapshot().await.map_err(snapshot_error)?;
    let resolved = crate::repair_plan::relation::resolve_entity_relation_on_snapshot(
        &snapshot, &request, selection,
    )
    .await?;
    if expected_finding.is_some_and(|finding| finding != &resolved.selected_finding) {
        return Err(WenlanError::Conflict(
            "repair_current_finding_missing".into(),
        ));
    }
    let manifest_id = format!("repair_{}", Uuid::new_v4());
    let context = RelationCaptureContext {
        manifest_id: &manifest_id,
        review_id: resolved.review_binding.review_id(),
        from_entity: &resolved.from_entity,
        to_entity: &resolved.to_entity,
        owner_ids: resolved.review_binding.owner_ids(),
        canonical_relation_type: Some(&resolved.canonical_relation_type),
        vocabulary_promotion: resolved.vocabulary_promotion.as_deref(),
    };
    let before = relation_snapshot::capture(&RelationReader::Snapshot(&snapshot), &context).await?;
    let before_receipt = relation_snapshot::receipt(&before)?;
    let snapshot_receipt = snapshot.finish().await.map_err(snapshot_error)?;
    validate_report_source_receipts(&[general, deep], snapshot_receipt)?;
    validate_current_page_receipts(general, Some(deep), page_root).await?;
    let change = match &selection.choice {
        EntityRelationRepairChoice::Add {
            relation_type,
            source_memory_id,
            ..
        } => RepairRelationMutation::Add {
            requested_relation_type: relation_type.clone(),
            canonical_relation_type: resolved.canonical_relation_type.clone(),
            source_memory_id: source_memory_id.clone(),
            confidence_basis_points: resolved.selected_finding.confidence_basis_points(),
            retire_relation_ids: resolved.retire_relation_ids.clone(),
            vocabulary_promotion: resolved.vocabulary_promotion.clone(),
        },
        EntityRelationRepairChoice::Retire { .. } => RepairRelationMutation::Retire,
    };
    let scope = match lint_scope {
        RepairLintScope::Global => RepairScope::global(),
        RepairLintScope::Uncategorized => RepairScope::uncategorized(),
        RepairLintScope::Registered { ref space } => RepairScope::registered(space.clone())
            .map_err(|e| WenlanError::Validation(e.to_string()))?,
    };
    let target = RepairTarget::entity_relation(
        resolved.target_relation_id,
        resolved.from_entity,
        resolved.to_entity,
        resolved.review_binding.owner_ids().to_vec(),
        scope,
    )
    .map_err(|e| WenlanError::Validation(e.to_string()))?;
    let agent_work_digest = deep
        .agent_work()
        .ok_or_else(|| WenlanError::Validation("repair_agent_work_missing".into()))?
        .work_digest()
        .clone();
    let source = RepairSource::try_new_entity_relation(
        lint_scope,
        deep.scope().clone(),
        resolved.selected_finding.clone(),
        general.snapshots().clone(),
        deep.snapshots().clone(),
        general.producer_receipt().clone(),
        deep.producer_receipt().clone(),
        agent_work_digest,
    )
    .and_then(|source| source.try_with_review_binding(resolved.review_binding))
    .map_err(|e| WenlanError::Validation(e.to_string()))?;
    let rollback =
        RepairRollbackV3::try_new(RepairRollbackPayloadV3::EntityRelation { snapshot: before })
            .map_err(|e| WenlanError::Validation(e.to_string()))?;
    let rollback_bytes = serde_json::to_vec(&rollback)?;
    if rollback_bytes.len() as u64 > REPAIR_ROLLBACK_ARTIFACT_MAX_BYTES {
        return Err(WenlanError::Validation(
            "repair_relation_snapshot_too_large".into(),
        ));
    }
    let rollback_artifact = RepairRollbackArtifact::entity_relation(
        "rollback-v3.json".into(),
        repair_digest(&rollback_bytes),
    )
    .map_err(|e| WenlanError::Validation(e.to_string()))?;
    let evidence = resolved
        .selected_finding
        .evidence_ids()
        .first()
        .ok_or_else(|| WenlanError::Validation("repair_relation_evidence_missing".into()))?
        .clone();
    let post_assertions = RepairPostAssertions::try_new_for_check(
        LintSemanticCheckId::EntityRelations.as_str().to_string(),
        evidence,
        repair_check_baseline(general)?,
        repair_check_baseline(deep)?,
        vec![LintSemanticCheckId::EntityRelations.as_str().to_string()],
        vec![],
    )
    .map_err(|e| WenlanError::Validation(e.to_string()))?;
    let draft = RepairManifestDraft::try_new(
        manifest_id,
        now_epoch,
        source,
        target.clone(),
        RepairExpectedState::try_new(None, before_receipt)
            .map_err(|e| WenlanError::Validation(e.to_string()))?,
        RepairWriter::EntityRelation,
        RepairMutation::entity_relation(change)
            .map_err(|e| WenlanError::Validation(e.to_string()))?,
        RepairAllowedEffects::entity_relation(target),
        rollback_artifact,
        post_assertions,
    )
    .map_err(|e| WenlanError::Validation(e.to_string()))?;
    let digest = repair_digest(&draft.canonical_bytes()?);
    let manifest = RepairManifest::try_new(draft, digest)
        .map_err(|e| WenlanError::Validation(e.to_string()))?;
    store.persist_prepared(&manifest, &rollback_bytes)?;
    Ok(manifest)
}

pub(crate) fn load_rollback(
    store: &RepairArtifactStore,
    manifest: &RepairManifest,
) -> Result<RepairRelationSnapshot, WenlanError> {
    if manifest.writer() != RepairWriter::EntityRelation
        || manifest.rollback().format_version() != 3
    {
        return Err(WenlanError::Validation(
            "repair_rollback_writer_mismatch".into(),
        ));
    }
    let path = store
        .manifest_dir(manifest.manifest_id())?
        .join(manifest.rollback().relative_path());
    let bytes = read_bounded_file(&path, REPAIR_ROLLBACK_ARTIFACT_MAX_BYTES)?;
    if repair_digest(&bytes) != *manifest.rollback().digest() {
        return Err(WenlanError::Validation(
            "repair_rollback_digest_mismatch".into(),
        ));
    }
    let StoredRepairRollbackArtifact::V3(rollback) =
        StoredRepairRollbackArtifact::from_slice(&bytes)?
    else {
        return Err(WenlanError::Validation("repair_rollback_mismatch".into()));
    };
    let RepairRollbackPayloadV3::EntityRelation { snapshot } = rollback.payload();
    if relation_snapshot::receipt(snapshot)? != *manifest.expected_state().canonical_receipt() {
        return Err(WenlanError::Validation(
            "repair_rollback_target_mismatch".into(),
        ));
    }
    Ok(snapshot.clone())
}

pub(crate) async fn apply(
    db: &MemoryDB,
    store: &RepairArtifactStore,
    manifest: &RepairManifest,
    now_epoch: i64,
) -> Result<RepairApplyReceipt, WenlanError> {
    let rollback = load_rollback(store, manifest)?;
    if let Some(receipt) = recover(db, store, manifest).await? {
        return Ok(receipt);
    }
    let mut pending = store.begin_apply_receipt(manifest.manifest_id())?;
    let mut prepared = None;
    let before_commit = |proof: &crate::post_write::RepairWriteProof| {
        let draft = RepairApplyReceiptDraft::try_new(
            manifest.manifest_id().to_string(),
            manifest.manifest_digest().clone(),
            now_epoch,
            proof.before_target_receipt().clone(),
            proof.after_target_receipt().clone(),
            proof.non_target_before().clone(),
            proof.non_target_after().clone(),
            proof.post_apply_db_digest().clone(),
            manifest.allowed_effects().clone(),
            manifest.writer(),
        )
        .map_err(|e| WenlanError::Validation(e.to_string()))?;
        let digest = repair_digest(&draft.canonical_bytes()?);
        let receipt = RepairApplyReceipt::from_draft(draft, digest);
        pending.prepare(&receipt)?;
        prepared = Some(receipt);
        Ok(())
    };
    if let Err(error) = db
        .relation_repair_cas(manifest, &rollback, before_commit)
        .await
    {
        if should_retain_pending_apply_receipt(&error) {
            pending.retain()?;
        } else {
            pending.abort();
        }
        return Err(error);
    }
    let receipt = prepared
        .ok_or_else(|| WenlanError::VectorDb("repair_receipt_not_prepared_before_commit".into()))?;
    pending.publish()?;
    Ok(receipt)
}

async fn recover(
    db: &MemoryDB,
    store: &RepairArtifactStore,
    manifest: &RepairManifest,
) -> Result<Option<RepairApplyReceipt>, WenlanError> {
    let directory = store.manifest_dir(manifest.manifest_id())?;
    let final_path = directory.join(APPLY_RECEIPT_FILE);
    let pending_path = directory.join(APPLY_RECEIPT_PENDING_FILE);
    if final_path.exists() {
        let receipt = store.load_apply_receipt(manifest)?;
        if pending_path.exists() {
            fs::remove_file(pending_path)?;
            sync_dir(&directory)?;
        }
        return Ok(Some(receipt));
    }
    if !pending_path.exists() {
        return Ok(None);
    }
    let bytes = read_bounded_file(&pending_path, REPAIR_ROLLBACK_ARTIFACT_MAX_BYTES)?;
    let parsed = StoredRepairApplyReceipt::from_slice(&bytes)
        .ok()
        .and_then(|receipt| verify_stored_apply_receipt(receipt, manifest).ok());
    let current = db.capture_relation_repair_state(manifest).await?;
    let context = crate::db::repair_relation_cas::capture_context(manifest)?;
    if let Some(receipt) = parsed {
        if relation_snapshot::applied_receipt(&current, &context)?
            == *receipt.after_target_receipt()
        {
            publish_no_replace(&pending_path, &final_path, "repair_already_applied")?;
            return Ok(Some(receipt));
        }
    }
    if relation_snapshot::receipt(&current)? != *manifest.expected_state().canonical_receipt() {
        return Err(WenlanError::Conflict(
            "repair_apply_recovery_required".into(),
        ));
    }
    fs::remove_file(&pending_path)?;
    sync_dir(&directory)?;
    Ok(None)
}
