// SPDX-License-Identifier: Apache-2.0
//! Durable preparation and CAS application of workflow-approved repairs.
//!
//! The exact approval phrase binds cooperating agent workflows to one manifest;
//! it is not an authentication boundary against malicious local processes.

use crate::{
    db::MemoryDB,
    error::WenlanError,
    lint::{
        context::ExecutionGate,
        pages::fs::{scan_page_root_controlled, PageScanControl},
        snapshot::{LintReadSnapshot, SnapshotError, SnapshotReceipt},
    },
};
use fs2::FileExt as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fmt::Write as _,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    str::FromStr,
};
use uuid::Uuid;
use wenlan_types::{
    lint::{
        LintDigest, LintEvidenceRef, LintGateEffect, LintOutcome, LintProfile, LintReport,
        LintScope, LintScopeKind, LintSemanticAction,
    },
    repair::{
        ApplyRepairRequest, PrepareRepairRequest, RepairAllowedEffects, RepairApplyReceipt,
        RepairApplyReceiptDraft, RepairCheckBaseline, RepairContractError, RepairDigest,
        RepairExpectedState, RepairLintScope, RepairManifest, RepairManifestDraft, RepairMutation,
        RepairPostAssertions, RepairRollbackArtifact, RepairScope, RepairSource, RepairTarget,
        RepairVerificationReceipt, RepairVerificationReceiptDraft, RepairWriter,
        StoredRepairApplyReceipt, StoredRepairManifest, StoredRepairRollbackArtifact,
        StoredRepairVerificationReceipt, VerifyRepairRequest, REPAIR_CLASSIFICATION_CHECK_ID,
        REPAIR_ROLLBACK_FORMAT_VERSION,
    },
    MemoryType,
};

const MANIFEST_FILE: &str = "manifest.json";
const ROLLBACK_FILE: &str = "rollback-v1.json";
const APPLY_RECEIPT_FILE: &str = "apply-receipt.json";
const APPLY_RECEIPT_PENDING_FILE: &str = ".apply-receipt.json.pending";
const VERIFICATION_RECEIPT_FILE: &str = "verification-receipt.json";
const OPERATION_LOCK_FILE: &str = ".operation.lock";

#[derive(Debug, Clone)]
pub struct RepairArtifactStore {
    root: PathBuf,
}

impl RepairArtifactStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn manifest_dir(&self, manifest_id: &str) -> Result<PathBuf, WenlanError> {
        if !safe_manifest_id(manifest_id) {
            return Err(WenlanError::Validation(
                "invalid_repair_manifest_id".to_string(),
            ));
        }
        Ok(self.root.join(manifest_id))
    }

    fn read_stored_manifest(&self, manifest_id: &str) -> Result<StoredRepairManifest, WenlanError> {
        let path = self.manifest_dir(manifest_id)?.join(MANIFEST_FILE);
        let manifest = StoredRepairManifest::from_slice(&fs::read(path)?)?;
        if manifest.manifest_id() != manifest_id {
            return Err(WenlanError::Validation(
                "repair_manifest_id_mismatch".to_string(),
            ));
        }
        Ok(manifest)
    }

    pub fn load_manifest(&self, manifest_id: &str) -> Result<RepairManifest, WenlanError> {
        let manifest = self.read_stored_manifest(manifest_id)?;
        manifest
            .verify_and_try_into_current(|canonical, expected| {
                repair_digest(canonical).as_str() == expected.as_str()
            })
            .map_err(|error| match error {
                RepairContractError::InvalidDigest => {
                    WenlanError::Validation("repair_manifest_digest_mismatch".to_string())
                }
                _ => WenlanError::Validation(error.to_string()),
            })
    }

    fn persist_prepared(
        &self,
        manifest: &RepairManifest,
        rollback_bytes: &[u8],
    ) -> Result<(), WenlanError> {
        ensure_private_dir(&self.root)?;
        let final_dir = self.manifest_dir(manifest.manifest_id())?;
        if final_dir.exists() {
            return Err(WenlanError::Conflict("repair_manifest_exists".to_string()));
        }
        let temp_dir = self.root.join(format!(
            ".{}.tmp-{}",
            manifest.manifest_id(),
            Uuid::new_v4()
        ));
        fs::create_dir(&temp_dir)?;
        set_private_dir_permissions(&temp_dir)?;
        let result = (|| {
            write_private_file(&temp_dir.join(ROLLBACK_FILE), rollback_bytes)?;
            write_private_file(
                &temp_dir.join(MANIFEST_FILE),
                &serde_json::to_vec_pretty(manifest)?,
            )?;
            sync_dir(&temp_dir)?;
            fs::rename(&temp_dir, &final_dir)?;
            sync_dir(&self.root)?;
            Ok::<(), WenlanError>(())
        })();
        if result.is_err() && temp_dir.exists() {
            let _ = fs::remove_dir_all(&temp_dir);
        }
        result
    }

    fn load_rollback(
        &self,
        manifest: &RepairManifest,
    ) -> Result<(StoredRollbackArtifact, Vec<u8>), WenlanError> {
        let path = self
            .manifest_dir(manifest.manifest_id())?
            .join(manifest.rollback().relative_path());
        let bytes = fs::read(path)?;
        if repair_digest(&bytes) != *manifest.rollback().digest() {
            return Err(WenlanError::Validation(
                "repair_rollback_digest_mismatch".to_string(),
            ));
        }
        let stored = StoredRepairRollbackArtifact::from_slice(&bytes)?;
        let rollback = stored.as_v1();
        if rollback.format_version() != REPAIR_ROLLBACK_FORMAT_VERSION
            || rollback.table() != "memories"
            || rollback.source_id() != manifest.target().memory_source_id()
            || rollback.rows().is_empty()
        {
            return Err(WenlanError::Validation(
                "repair_rollback_mismatch".to_string(),
            ));
        }
        Ok((
            StoredRollbackArtifact {
                format_version: rollback.format_version(),
                table: rollback.table().to_string(),
                source_id: rollback.source_id().to_string(),
                columns: rollback.columns().to_vec(),
                rows: rollback.rows().to_vec(),
            },
            bytes,
        ))
    }

    fn lock_manifest_operation(
        &self,
        manifest_id: &str,
    ) -> Result<ManifestOperationLock, WenlanError> {
        let path = self.manifest_dir(manifest_id)?.join(OPERATION_LOCK_FILE);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;
        set_private_file_permissions(&path)?;
        file.try_lock_exclusive().map_err(|error| {
            if error.kind() == std::io::ErrorKind::WouldBlock {
                WenlanError::Conflict("repair_operation_in_progress".to_string())
            } else {
                WenlanError::Io(error)
            }
        })?;
        Ok(ManifestOperationLock { _file: file })
    }

    fn begin_apply_receipt(&self, manifest_id: &str) -> Result<PendingApplyReceipt, WenlanError> {
        let manifest_dir = self.manifest_dir(manifest_id)?;
        let final_path = manifest_dir.join(APPLY_RECEIPT_FILE);
        if final_path.exists() {
            return Err(WenlanError::Conflict("repair_already_applied".to_string()));
        }
        let pending_path = manifest_dir.join(APPLY_RECEIPT_PENDING_FILE);
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pending_path)
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    WenlanError::Conflict("repair_apply_in_progress".to_string())
                } else {
                    WenlanError::Io(error)
                }
            })?;
        set_private_file_permissions(&pending_path)?;
        Ok(PendingApplyReceipt {
            file: Some(file),
            pending_path,
            final_path,
        })
    }

    fn load_apply_receipt(
        &self,
        manifest: &RepairManifest,
    ) -> Result<RepairApplyReceipt, WenlanError> {
        let path = self
            .manifest_dir(manifest.manifest_id())?
            .join(APPLY_RECEIPT_FILE);
        let receipt = StoredRepairApplyReceipt::from_slice(&fs::read(path)?)?;
        verify_stored_apply_receipt(receipt, manifest)
    }

    fn persist_verification_receipt(
        &self,
        receipt: &RepairVerificationReceipt,
    ) -> Result<(), WenlanError> {
        let manifest_dir = self.manifest_dir(receipt.manifest_id())?;
        let final_path = manifest_dir.join(VERIFICATION_RECEIPT_FILE);
        if final_path.exists() {
            return Err(WenlanError::Conflict("repair_already_verified".to_string()));
        }
        let temp_path = manifest_dir.join(format!(
            ".{VERIFICATION_RECEIPT_FILE}.tmp-{}",
            Uuid::new_v4()
        ));
        let result = (|| {
            write_private_file(&temp_path, &serde_json::to_vec_pretty(receipt)?)?;
            publish_no_replace(&temp_path, &final_path, "repair_already_verified")?;
            Ok::<(), WenlanError>(())
        })();
        if result.is_err() && temp_path.exists() {
            let _ = fs::remove_file(temp_path);
        }
        result
    }

    fn load_verification_receipt(
        &self,
        manifest: &RepairManifest,
        apply_receipt: &RepairApplyReceipt,
    ) -> Result<Option<RepairVerificationReceipt>, WenlanError> {
        let path = self
            .manifest_dir(manifest.manifest_id())?
            .join(VERIFICATION_RECEIPT_FILE);
        if !path.exists() {
            return Ok(None);
        }
        let receipt = StoredRepairVerificationReceipt::from_slice(&fs::read(path)?)?;
        verify_stored_verification_receipt(receipt, manifest, apply_receipt).map(Some)
    }

    /// Return every durably applied repair that still needs verification.
    ///
    /// Startup uses this to restore the daemon-owned writer fence after a
    /// process restart. Corrupt or mismatched durable artifacts fail closed
    /// instead of allowing background maintenance to resume.
    pub fn pending_verification_manifest_ids(&self) -> Result<Vec<String>, WenlanError> {
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(WenlanError::Io(error)),
        };
        let mut pending = Vec::new();
        for entry in entries {
            let entry = entry?;
            if !entry.file_type()?.is_dir() || !entry.path().join(MANIFEST_FILE).is_file() {
                continue;
            }
            let manifest_id = entry.file_name().into_string().map_err(|_| {
                WenlanError::Validation("invalid_repair_manifest_directory".to_string())
            })?;
            let manifest = self.load_manifest(&manifest_id)?;
            if entry.path().join(APPLY_RECEIPT_PENDING_FILE).is_file() {
                pending.push(manifest_id);
                continue;
            }
            let apply_path = entry.path().join(APPLY_RECEIPT_FILE);
            if !apply_path.is_file() {
                continue;
            }
            let apply_receipt = self.load_apply_receipt(&manifest)?;
            if self
                .load_verification_receipt(&manifest, &apply_receipt)?
                .is_none()
            {
                pending.push(manifest_id);
            }
        }
        pending.sort();
        Ok(pending)
    }

    /// Validate and report whether a repair already has its durable terminal
    /// receipt. This keeps a lost HTTP verify response safely retryable after
    /// the in-memory writer fence has been released.
    pub fn has_completed_verification(&self, manifest_id: &str) -> Result<bool, WenlanError> {
        let manifest = self.load_manifest(manifest_id)?;
        let apply_path = self.manifest_dir(manifest_id)?.join(APPLY_RECEIPT_FILE);
        if !apply_path.is_file() {
            return Ok(false);
        }
        let apply_receipt = self.load_apply_receipt(&manifest)?;
        Ok(self
            .load_verification_receipt(&manifest, &apply_receipt)?
            .is_some())
    }
}

fn verify_stored_apply_receipt(
    receipt: StoredRepairApplyReceipt,
    manifest: &RepairManifest,
) -> Result<RepairApplyReceipt, WenlanError> {
    if receipt.manifest_id() != manifest.manifest_id()
        || receipt.manifest_digest().as_str() != manifest.manifest_digest().as_str()
    {
        return Err(WenlanError::Validation(
            "repair_apply_receipt_mismatch".to_string(),
        ));
    }
    receipt
        .verify_and_try_into_current(|canonical, expected| {
            repair_digest(canonical).as_str() == expected.as_str()
        })
        .map_err(|_| WenlanError::Validation("repair_apply_receipt_mismatch".to_string()))
}

fn verify_stored_verification_receipt(
    receipt: StoredRepairVerificationReceipt,
    manifest: &RepairManifest,
    apply_receipt: &RepairApplyReceipt,
) -> Result<RepairVerificationReceipt, WenlanError> {
    if receipt.manifest_id() != manifest.manifest_id()
        || receipt.manifest_digest().as_str() != manifest.manifest_digest().as_str()
        || receipt.apply_receipt_digest().as_str() != apply_receipt.receipt_digest().as_str()
    {
        return Err(WenlanError::Validation(
            "repair_verification_receipt_mismatch".to_string(),
        ));
    }
    receipt
        .verify_and_try_into_current(|canonical, expected| {
            repair_digest(canonical).as_str() == expected.as_str()
        })
        .map_err(|_| WenlanError::Validation("repair_verification_receipt_mismatch".to_string()))
}

struct ManifestOperationLock {
    _file: File,
}

struct PendingApplyReceipt {
    file: Option<File>,
    pending_path: PathBuf,
    final_path: PathBuf,
}

impl PendingApplyReceipt {
    fn prepare(&mut self, receipt: &RepairApplyReceipt) -> Result<(), WenlanError> {
        let bytes = serde_json::to_vec_pretty(receipt)?;
        let mut file = self.file.take().expect("pending apply receipt file");
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        if let Some(parent) = self.pending_path.parent() {
            sync_dir(parent)?;
        }
        Ok(())
    }

    fn publish(self) -> Result<(), WenlanError> {
        publish_no_replace(
            &self.pending_path,
            &self.final_path,
            "repair_already_applied",
        )
    }

    fn abort(mut self) {
        drop(self.file.take());
        let _ = fs::remove_file(&self.pending_path);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredRollbackArtifact {
    format_version: u16,
    table: String,
    source_id: String,
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
}

struct ResolvedTarget {
    source_id: String,
    memory_type: Option<String>,
    space: Option<String>,
    version: Option<i64>,
    evidence_id: LintDigest,
}

pub fn semantic_record_digest(kind: &str, durable_id: &str) -> LintDigest {
    crate::lint::semantic_record_digest(kind, durable_id)
}

pub async fn prepare_memory_reclassification(
    db: &MemoryDB,
    store: &RepairArtifactStore,
    request: PrepareRepairRequest,
    now_epoch: i64,
) -> Result<RepairManifest, WenlanError> {
    prepare_memory_reclassification_with_pages(db, store, request, None, now_epoch).await
}

pub async fn prepare_memory_reclassification_with_pages(
    db: &MemoryDB,
    store: &RepairArtifactStore,
    request: PrepareRepairRequest,
    page_root: Option<&Path>,
    now_epoch: i64,
) -> Result<RepairManifest, WenlanError> {
    ensure_repair_artifacts_supported()?;
    if now_epoch <= 0 {
        return Err(WenlanError::Validation(
            "invalid_repair_prepared_at".to_string(),
        ));
    }
    validate_selected_finding(&request)?;

    let snapshot = db.open_lint_snapshot().await.map_err(snapshot_error)?;
    validate_durable_scope(
        &snapshot,
        request.lint_scope(),
        request.deep_report().scope(),
    )
    .await?;
    let target = resolve_target(&snapshot, &request).await?;
    let rollback = capture_rollback(&snapshot, &target.source_id).await?;
    let rollback_bytes = serde_json::to_vec_pretty(&rollback)?;
    let target_receipt = target_receipt(&rollback)?;
    let snapshot_receipt = snapshot.finish().await.map_err(snapshot_error)?;
    validate_source_receipts(&request, snapshot_receipt)?;
    validate_current_page_receipts(request.general_report(), request.deep_report(), page_root)
        .await
        .map_err(|error| match error {
            WenlanError::Conflict(_) => {
                WenlanError::Conflict("repair_source_reports_stale".to_string())
            }
            other => other,
        })?;
    if request.general_report().producer_receipt() != request.deep_report().producer_receipt() {
        return Err(WenlanError::Conflict(
            "repair_source_producers_mismatch".to_string(),
        ));
    }

    let after_memory_type = request.after_memory_type().to_string();
    let mutation =
        RepairMutation::try_reclassify(target.memory_type.as_deref(), after_memory_type.as_str())
            .map_err(|error| WenlanError::Validation(error.to_string()))?;
    let target_scope = match target.space {
        Some(space) => RepairScope::registered(space),
        None => Ok(RepairScope::uncategorized()),
    }
    .map_err(|error| WenlanError::Validation(error.to_string()))?;
    let repair_target = RepairTarget::memory(target.source_id, target_scope)
        .map_err(|error| WenlanError::Validation(error.to_string()))?;
    let general = request.general_report();
    let deep = request.deep_report();
    let agent_work_digest = deep
        .agent_work()
        .ok_or_else(|| WenlanError::Validation("repair_agent_work_missing".to_string()))?
        .work_digest()
        .clone();
    let source = RepairSource::try_new(
        request.lint_scope().clone(),
        deep.scope().clone(),
        request.selected_finding().clone(),
        general.snapshots().clone(),
        deep.snapshots().clone(),
        general.producer_receipt().clone(),
        deep.producer_receipt().clone(),
        agent_work_digest,
    )
    .map_err(|error| WenlanError::Validation(error.to_string()))?;
    let allowed_effects = RepairAllowedEffects::memory_type(repair_target.clone());
    let rollback_contract =
        RepairRollbackArtifact::try_new(ROLLBACK_FILE.to_string(), repair_digest(&rollback_bytes))
            .map_err(|error| WenlanError::Validation(error.to_string()))?;
    let post_assertions = RepairPostAssertions::try_new(
        target.evidence_id,
        repair_check_baseline(general)?,
        repair_check_baseline(deep)?,
        vec![],
    )
    .map_err(|error| WenlanError::Validation(error.to_string()))?;
    let manifest_id = format!("repair_{}", Uuid::new_v4());
    let draft = RepairManifestDraft::try_new(
        manifest_id,
        now_epoch,
        source,
        repair_target,
        RepairExpectedState::try_new(target.version, target_receipt)
            .map_err(|error| WenlanError::Validation(error.to_string()))?,
        RepairWriter::ReclassifyMemory,
        mutation,
        allowed_effects,
        rollback_contract,
        post_assertions,
    )
    .map_err(|error| WenlanError::Validation(error.to_string()))?;
    let manifest_digest = repair_digest(&draft.canonical_bytes()?);
    let manifest = RepairManifest::try_new(draft, manifest_digest)
        .map_err(|error| WenlanError::Validation(error.to_string()))?;
    store.persist_prepared(&manifest, &rollback_bytes)?;
    Ok(manifest)
}

fn repair_check_baseline(
    report: &wenlan_types::lint::LintReport,
) -> Result<Vec<RepairCheckBaseline>, WenlanError> {
    report
        .checks()
        .iter()
        .map(|check| {
            RepairCheckBaseline::try_new(
                check.check_id().to_string(),
                check.outcome(),
                check.gate_effect(),
                check.evidence().to_vec(),
            )
            .map_err(|error| WenlanError::Validation(error.to_string()))
        })
        .collect()
}

pub async fn apply_repair(
    db: &MemoryDB,
    store: &RepairArtifactStore,
    request: ApplyRepairRequest,
    now_epoch: i64,
) -> Result<RepairApplyReceipt, WenlanError> {
    ensure_repair_artifacts_supported()?;
    if now_epoch <= 0 {
        return Err(WenlanError::Validation(
            "invalid_repair_applied_at".to_string(),
        ));
    }
    let manifest = store.load_manifest(request.manifest_id())?;
    if manifest.manifest_digest() != request.approved_manifest_digest() {
        return Err(WenlanError::Conflict(
            "repair_approval_mismatch".to_string(),
        ));
    }
    let _operation_lock = store.lock_manifest_operation(manifest.manifest_id())?;
    if let Some(receipt) = recover_apply_receipt(db, store, &manifest).await? {
        return Ok(receipt);
    }
    let (rollback, _) = store.load_rollback(&manifest)?;
    if target_receipt(&rollback)? != *manifest.expected_state().canonical_receipt() {
        return Err(WenlanError::Validation(
            "repair_rollback_target_mismatch".to_string(),
        ));
    }
    let mut pending = store.begin_apply_receipt(manifest.manifest_id())?;
    let after_memory_type = MemoryType::from_str(manifest.mutation().after_memory_type())
        .map_err(WenlanError::Validation)?;
    let mut prepared_receipt = None;
    let proof = match crate::post_write::reclassify_memory_cas(
        db,
        manifest.target().memory_source_id(),
        manifest.expected_state().canonical_receipt(),
        manifest.target().scope().space(),
        after_memory_type,
        |proof| {
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
            .map_err(|error| WenlanError::Validation(error.to_string()))?;
            let receipt_digest = repair_digest(&draft.canonical_bytes()?);
            let receipt = RepairApplyReceipt::from_draft(draft, receipt_digest);
            pending.prepare(&receipt)?;
            prepared_receipt = Some(receipt);
            Ok(())
        },
    )
    .await
    {
        Ok(proof) => proof,
        Err(error) => {
            pending.abort();
            return Err(error);
        }
    };
    let receipt = prepared_receipt.ok_or_else(|| {
        WenlanError::VectorDb("repair_receipt_not_prepared_before_commit".to_string())
    })?;
    debug_assert_eq!(proof.after_target_receipt(), receipt.after_target_receipt());
    pending.publish()?;
    Ok(receipt)
}

async fn recover_apply_receipt(
    db: &MemoryDB,
    store: &RepairArtifactStore,
    manifest: &RepairManifest,
) -> Result<Option<RepairApplyReceipt>, WenlanError> {
    let manifest_dir = store.manifest_dir(manifest.manifest_id())?;
    let final_path = manifest_dir.join(APPLY_RECEIPT_FILE);
    if final_path.exists() {
        let receipt = store.load_apply_receipt(manifest)?;
        let pending_path = manifest_dir.join(APPLY_RECEIPT_PENDING_FILE);
        if pending_path.exists() {
            fs::remove_file(pending_path)?;
            sync_dir(&manifest_dir)?;
        }
        return Ok(Some(receipt));
    }
    let pending_path = manifest_dir.join(APPLY_RECEIPT_PENDING_FILE);
    if !pending_path.exists() {
        return Ok(None);
    }
    let pending = fs::read(&pending_path)?;
    let parsed = StoredRepairApplyReceipt::from_slice(&pending)
        .ok()
        .and_then(|receipt| verify_stored_apply_receipt(receipt, manifest).ok());
    if let Some(receipt) = parsed {
        let (target_now, _) = target_receipt_current(db, manifest).await?;
        if target_now == *receipt.after_target_receipt() {
            publish_no_replace(&pending_path, &final_path, "repair_already_applied")?;
            return Ok(Some(receipt));
        }
        if target_now != *manifest.expected_state().canonical_receipt() {
            return Err(WenlanError::Conflict(
                "repair_apply_recovery_required".to_string(),
            ));
        }
    } else {
        let (target_now, _) = target_receipt_current(db, manifest).await?;
        if target_now != *manifest.expected_state().canonical_receipt() {
            return Err(WenlanError::Conflict(
                "repair_apply_recovery_required".to_string(),
            ));
        }
    }
    fs::remove_file(&pending_path)?;
    sync_dir(&manifest_dir)?;
    Ok(None)
}

async fn target_receipt_current(
    db: &MemoryDB,
    manifest: &RepairManifest,
) -> Result<(RepairDigest, u64), WenlanError> {
    let connection = db.conn.lock().await;
    validate_target_space_on_connection(
        &connection,
        manifest.target().memory_source_id(),
        manifest.target().scope().space(),
    )
    .await?;
    target_receipt_on_connection(&connection, manifest.target().memory_source_id()).await
}

pub async fn record_repair_verification(
    db: &MemoryDB,
    store: &RepairArtifactStore,
    request: VerifyRepairRequest,
    page_root: Option<&Path>,
    now_epoch: i64,
) -> Result<RepairVerificationReceipt, WenlanError> {
    ensure_repair_artifacts_supported()?;
    if now_epoch <= 0 {
        return Err(WenlanError::Validation(
            "invalid_repair_verified_at".to_string(),
        ));
    }
    let manifest = store.load_manifest(request.manifest_id())?;
    if manifest.manifest_digest() != request.manifest_digest() {
        return Err(WenlanError::Conflict(
            "repair_verification_manifest_mismatch".to_string(),
        ));
    }
    let _operation_lock = store.lock_manifest_operation(manifest.manifest_id())?;
    let apply_receipt = store.load_apply_receipt(&manifest)?;
    if apply_receipt.receipt_digest() != request.apply_receipt_digest() {
        return Err(WenlanError::Validation(
            "repair_apply_receipt_mismatch".to_string(),
        ));
    }
    if apply_receipt.post_apply_db_digest().is_none() {
        if let Some(receipt) = store.load_verification_receipt(&manifest, &apply_receipt)? {
            return Ok(receipt);
        }
        return Err(WenlanError::Validation(
            "repair_legacy_apply_receipt_unverifiable".to_string(),
        ));
    }
    if apply_receipt.actual_effects() != manifest.allowed_effects()
        || apply_receipt.writer() != manifest.writer()
        || apply_receipt.before_target_receipt() != manifest.expected_state().canonical_receipt()
    {
        return Err(WenlanError::Validation(
            "repair_apply_receipt_mismatch".to_string(),
        ));
    }
    if let Some(receipt) = store.load_verification_receipt(&manifest, &apply_receipt)? {
        return Ok(receipt);
    }
    let post_apply_db_digest = apply_receipt
        .post_apply_db_digest()
        .expect("legacy receipts returned above")
        .clone();
    validate_verification_reports(&manifest, request.general_report(), request.deep_report())?;
    validate_current_page_receipts(request.general_report(), request.deep_report(), page_root)
        .await?;
    let connection = db.conn.lock().await;
    connection
        .execute("BEGIN IMMEDIATE", ())
        .await
        .map_err(|error| WenlanError::VectorDb(format!("repair verify begin: {error}")))?;
    let result = async {
        validate_current_db_receipts(db, request.general_report(), request.deep_report()).await?;
        let current = database_content_digest(&connection).await?;
        if current != post_apply_db_digest {
            return Err(WenlanError::Conflict(
                "repair_non_target_state_changed".to_string(),
            ));
        }
        validate_target_space_on_connection(
            &connection,
            manifest.target().memory_source_id(),
            manifest.target().scope().space(),
        )
        .await?;
        let (target_now, _) =
            target_receipt_on_connection(&connection, manifest.target().memory_source_id()).await?;
        if target_now != *apply_receipt.after_target_receipt() {
            return Err(WenlanError::Conflict(
                "repair_verification_state_changed".to_string(),
            ));
        }
        let draft = RepairVerificationReceiptDraft::try_new(
            manifest.manifest_id().to_string(),
            manifest.manifest_digest().clone(),
            apply_receipt.receipt_digest().clone(),
            now_epoch,
            request.general_report().snapshots().clone(),
            request.deep_report().snapshots().clone(),
        )
        .map_err(|error| WenlanError::Validation(error.to_string()))?;
        let receipt_digest = repair_digest(&draft.canonical_bytes()?);
        let receipt = RepairVerificationReceipt::from_draft(draft, receipt_digest);
        store.persist_verification_receipt(&receipt)?;
        Ok(receipt)
    }
    .await;
    let receipt = match result {
        Ok(receipt) => receipt,
        Err(error) => {
            let _ = connection.execute("ROLLBACK", ()).await;
            return Err(error);
        }
    };
    connection
        .execute("COMMIT", ())
        .await
        .map_err(|error| WenlanError::VectorDb(format!("repair verify commit: {error}")))?;
    Ok(receipt)
}

fn validate_verification_reports(
    manifest: &RepairManifest,
    general: &LintReport,
    deep: &LintReport,
) -> Result<(), WenlanError> {
    if !manifest
        .source()
        .lint_scope()
        .matches_report_scope_kind(general.scope())
        || !manifest
            .source()
            .lint_scope()
            .matches_report_scope_kind(deep.scope())
        || general.scope() != manifest.source().report_scope()
        || deep.scope() != manifest.source().report_scope()
        || deep.agent_work().is_none()
        || general.producer_receipt() != deep.producer_receipt()
        || !stable_report_snapshot(general)
        || !stable_report_snapshot(deep)
    {
        return Err(WenlanError::Validation(
            "repair_verification_report_mismatch".to_string(),
        ));
    }
    let general_baseline = manifest.post_assertions().general_baseline();
    let deep_baseline = manifest.post_assertions().deep_baseline();
    if manifest
        .post_assertions()
        .verification_policy()
        .requires_whole_reports()
        && (!general.complete() || !deep.complete())
    {
        return Err(WenlanError::Validation(
            "repair_legacy_verification_not_clean".to_string(),
        ));
    }
    if general_baseline.is_empty() && deep_baseline.is_empty() {
        // The first durable v1 producer predated baseline binding. Preserve
        // those manifests without inventing unapproved baseline state: their
        // post-repair reports must instead be conservatively clean.
        if [general, deep].iter().any(|report| {
            report.totals().actionable_findings() != 0 || report.totals().incomplete() != 0
        }) {
            return Err(WenlanError::Validation(
                "repair_legacy_verification_not_clean".to_string(),
            ));
        }
    } else {
        validate_check_deltas(
            general_baseline,
            general,
            manifest.post_assertions().allowed_non_target_check_deltas(),
        )?;
        validate_check_deltas(
            deep_baseline,
            deep,
            manifest.post_assertions().allowed_non_target_check_deltas(),
        )?;
    }
    let target_survives = deep
        .checks()
        .iter()
        .find(|check| check.check_id() == REPAIR_CLASSIFICATION_CHECK_ID)
        .is_some_and(|check| {
            check.evidence().iter().any(|evidence| match evidence {
                LintEvidenceRef::SemanticFinding { finding } => finding
                    .evidence_ids()
                    .contains(manifest.post_assertions().target_evidence_id()),
                _ => false,
            })
        });
    if target_survives {
        return Err(WenlanError::Validation(
            "repair_target_assertion_failed".to_string(),
        ));
    }
    Ok(())
}

fn stable_report_snapshot(report: &LintReport) -> bool {
    report.snapshots().db().post_run_digest() == Some(report.snapshots().db().analysis_digest())
        && report.snapshots().pages().after_scan_digest()
            == Some(report.snapshots().pages().before_scan_digest())
}

async fn validate_current_page_receipts(
    general: &LintReport,
    deep: &LintReport,
    page_root: Option<&Path>,
) -> Result<(), WenlanError> {
    let general_page = current_page_digest(page_root, LintProfile::General).await?;
    let deep_page = current_page_digest(page_root, LintProfile::Deep).await?;
    for (report, current_page) in [(general, general_page), (deep, deep_page)] {
        if report.snapshots().pages().before_scan_digest() != &current_page
            || report.snapshots().pages().after_scan_digest() != Some(&current_page)
        {
            return Err(WenlanError::Conflict(
                "repair_verification_reports_stale".to_string(),
            ));
        }
    }
    Ok(())
}

async fn validate_current_db_receipts(
    db: &MemoryDB,
    general: &LintReport,
    deep: &LintReport,
) -> Result<(), WenlanError> {
    let snapshot = db.open_lint_snapshot().await.map_err(snapshot_error)?;
    let current = snapshot.finish().await.map_err(snapshot_error)?;
    if !current.is_consistent() {
        return Err(WenlanError::Conflict(
            "repair_verification_reports_stale".to_string(),
        ));
    }
    let current_db = lint_digest(current.analysis_receipt_digest().as_bytes());
    for report in [general, deep] {
        if report.snapshots().db().analysis_digest() != &current_db
            || report.snapshots().db().post_run_digest() != Some(&current_db)
        {
            return Err(WenlanError::Conflict(
                "repair_verification_reports_stale".to_string(),
            ));
        }
    }
    Ok(())
}

async fn current_page_digest(
    page_root: Option<&Path>,
    profile: LintProfile,
) -> Result<LintDigest, WenlanError> {
    let Some(root) = page_root else {
        return Ok(lint_digest([0; 32]));
    };
    let root = root.to_path_buf();
    let timeout = ExecutionGate::page_budget_for(profile);
    let control = PageScanControl::with_timeout(timeout);
    let worker_control = control.clone();
    let mut task = tokio::task::spawn_blocking(move || {
        let scan = scan_page_root_controlled(&root, profile == LintProfile::Deep, &worker_control)
            .map_err(page_snapshot_error)?;
        let before = scan.normalized_bytes();
        let after = scan
            .verify_unchanged_with_control(&root, &worker_control)
            .map_err(page_snapshot_error)?
            .after_normalized_bytes();
        if before != after {
            return Err(WenlanError::Conflict(
                "repair_verification_reports_stale".to_string(),
            ));
        }
        Ok(lint_digest(before))
    });
    match tokio::time::timeout(timeout, &mut task).await {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => Err(WenlanError::VectorDb(format!(
            "repair page snapshot task: {error}"
        ))),
        Err(_) => {
            control.cancel();
            let _ = task.await;
            Err(WenlanError::Conflict(
                "repair_verification_reports_stale: page snapshot deadline exceeded".to_string(),
            ))
        }
    }
}

fn validate_check_deltas(
    baseline: &[RepairCheckBaseline],
    report: &LintReport,
    allowed_check_deltas: &[String],
) -> Result<(), WenlanError> {
    if baseline.len() != report.checks().len() {
        return Err(WenlanError::Validation(
            "repair_verification_catalog_mismatch".to_string(),
        ));
    }
    for before in baseline {
        let Some(after) = report
            .checks()
            .iter()
            .find(|check| check.check_id() == before.check_id())
        else {
            return Err(WenlanError::Validation(
                "repair_verification_catalog_mismatch".to_string(),
            ));
        };
        if before.gate_effect() != after.gate_effect() {
            return Err(WenlanError::Validation(
                "repair_verification_catalog_mismatch".to_string(),
            ));
        }
        if allowed_check_deltas
            .binary_search_by(|value| value.as_str().cmp(after.check_id()))
            .is_ok()
        {
            continue;
        }
        let new_actionable = after.gate_effect() == LintGateEffect::Actionable
            && after.outcome() == LintOutcome::Finding
            && (before.outcome() != LintOutcome::Finding
                || after
                    .evidence()
                    .iter()
                    .any(|evidence| !before.evidence().contains(evidence)));
        if new_actionable {
            return Err(WenlanError::Validation(
                "repair_new_actionable_finding".to_string(),
            ));
        }
        let after_incomplete = matches!(
            after.outcome(),
            LintOutcome::NotRunPrerequisite
                | LintOutcome::InconsistentSnapshot
                | LintOutcome::FailedToRun
        );
        if after_incomplete
            && (before.outcome() != after.outcome() || before.evidence() != after.evidence())
        {
            return Err(WenlanError::Validation(
                "repair_new_incomplete_check".to_string(),
            ));
        }
    }
    Ok(())
}

pub(crate) async fn target_receipt_on_connection(
    connection: &libsql::Connection,
    source_id: &str,
) -> Result<(RepairDigest, u64), WenlanError> {
    let rollback = capture_rollback_on_connection(connection, source_id).await?;
    let row_count = u64::try_from(rollback.rows.len())
        .map_err(|_| WenlanError::Validation("repair_target_too_large".to_string()))?;
    Ok((target_receipt(&rollback)?, row_count))
}

pub(crate) async fn validate_target_space_on_connection(
    connection: &libsql::Connection,
    source_id: &str,
    expected_space: Option<&str>,
) -> Result<(), WenlanError> {
    let mut rows = connection
        .query(
            "SELECT space FROM memories
             WHERE source='memory' AND source_id=?1 ORDER BY chunk_index,id",
            libsql::params![source_id],
        )
        .await
        .map_err(database_error)?;
    let mut seen = 0_u64;
    while let Some(row) = rows.next().await.map_err(database_error)? {
        let actual: Option<String> = row.get(0).map_err(database_error)?;
        if actual.as_deref() != expected_space {
            return Err(WenlanError::Conflict("repair_target_stale".to_string()));
        }
        seen = seen.saturating_add(1);
    }
    if seen == 0 {
        return Err(WenlanError::Conflict("repair_target_stale".to_string()));
    }
    Ok(())
}

pub(crate) fn effect_guard_receipt(normalized_total_changes: u64) -> RepairDigest {
    let mut bytes = b"wenlan-repair-effect-guard-v1".to_vec();
    bytes.extend_from_slice(&normalized_total_changes.to_le_bytes());
    repair_digest(&bytes)
}

fn validate_selected_finding(request: &PrepareRepairRequest) -> Result<(), WenlanError> {
    if request.selected_finding().proposed_action() != LintSemanticAction::ReclassifyMemory
        || request.selected_finding().evidence_ids().len() != 1
        || !request.selected_finding().counterevidence_ids().is_empty()
    {
        return Err(WenlanError::Validation(
            "unsupported_repair_finding".to_string(),
        ));
    }
    let present = request.deep_report().checks().iter().any(|check| {
        check.check_id() == wenlan_types::repair::REPAIR_CLASSIFICATION_CHECK_ID
            && check.evidence().iter().any(|evidence| {
                matches!(evidence, LintEvidenceRef::SemanticFinding { finding } if finding == request.selected_finding())
            })
    });
    if !present {
        return Err(WenlanError::Validation(
            "repair_finding_not_in_deep_report".to_string(),
        ));
    }
    Ok(())
}

async fn validate_durable_scope(
    snapshot: &LintReadSnapshot<'_>,
    lint_scope: &RepairLintScope,
    report_scope: &LintScope,
) -> Result<(), WenlanError> {
    if !lint_scope.matches_report_scope_kind(report_scope) {
        return Err(WenlanError::Validation("repair_scope_mismatch".to_string()));
    }
    let RepairLintScope::Registered { space } = lint_scope else {
        return Ok(());
    };
    let mut rows = snapshot
        .query(
            "SELECT (SELECT COUNT(*) FROM spaces prior WHERE prior.name < current.name)
             FROM spaces current WHERE current.name=?1 LIMIT 1",
            libsql::params::Params::Positional(vec![libsql::Value::Text(space.clone())]),
        )
        .await
        .map_err(snapshot_error)?;
    let Some(row) = rows.next().await.map_err(snapshot_error)? else {
        return Err(WenlanError::Validation(
            "repair_scope_not_registered".to_string(),
        ));
    };
    let position = usize::try_from(row.get::<i64>(0).map_err(database_error)?)
        .map_err(|_| WenlanError::Validation("repair_scope_mismatch".to_string()))?;
    let expected = wenlan_types::lint::LintOpaqueId::from_sorted_position(position)
        .ok_or_else(|| WenlanError::Validation("repair_scope_mismatch".to_string()))?;
    if report_scope.kind() != LintScopeKind::Registered
        || report_scope.opaque_scope_ref() != Some(expected)
    {
        return Err(WenlanError::Validation("repair_scope_mismatch".to_string()));
    }
    Ok(())
}

async fn resolve_target(
    snapshot: &LintReadSnapshot<'_>,
    request: &PrepareRepairRequest,
) -> Result<ResolvedTarget, WenlanError> {
    let (scope_clause, params) = match request.lint_scope() {
        RepairLintScope::Global => (String::new(), libsql::params::Params::None),
        RepairLintScope::Registered { space } => (
            " AND m.space=?1".to_string(),
            libsql::params::Params::Positional(vec![libsql::Value::Text(space.clone())]),
        ),
        RepairLintScope::Uncategorized => (
            " AND m.space IS NULL".to_string(),
            libsql::params::Params::None,
        ),
    };
    let sql = format!(
        "SELECT m.source_id,m.memory_type,m.space,m.version
         FROM memories m
         WHERE m.source='memory' AND m.chunk_index=0 AND m.pending_revision=0
           AND COALESCE(m.is_recap,0)=0 AND m.supersede_mode!='evicted'{scope_clause}
         ORDER BY m.source_id,m.id"
    );
    let target_evidence = request.selected_finding().evidence_ids()[0].clone();
    let mut rows = snapshot.query(&sql, params).await.map_err(snapshot_error)?;
    let mut matches = Vec::new();
    while let Some(row) = rows.next().await.map_err(snapshot_error)? {
        let source_id: String = row.get(0).map_err(database_error)?;
        if semantic_record_digest("memory", &source_id) == target_evidence {
            matches.push(ResolvedTarget {
                source_id,
                memory_type: row.get(1).map_err(database_error)?,
                space: row.get(2).map_err(database_error)?,
                version: row.get(3).map_err(database_error)?,
                evidence_id: target_evidence.clone(),
            });
        }
    }
    if matches.len() != 1 {
        return Err(WenlanError::Validation(
            "repair_target_not_unique".to_string(),
        ));
    }
    Ok(matches.remove(0))
}

async fn capture_rollback(
    snapshot: &LintReadSnapshot<'_>,
    source_id: &str,
) -> Result<StoredRollbackArtifact, WenlanError> {
    let mut column_rows = snapshot
        .query("PRAGMA table_info(memories)", libsql::params::Params::None)
        .await
        .map_err(snapshot_error)?;
    let mut columns = Vec::new();
    while let Some(row) = column_rows.next().await.map_err(snapshot_error)? {
        columns.push(row.get::<String>(1).map_err(database_error)?);
    }
    drop(column_rows);
    if columns.is_empty() {
        return Err(WenlanError::Validation(
            "repair_target_schema_missing".to_string(),
        ));
    }
    let selected = columns
        .iter()
        .map(|column| format!("quote({})", quote_identifier(column)))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT {selected} FROM memories
         WHERE source='memory' AND source_id=?1 ORDER BY chunk_index,id"
    );
    let mut query_rows = snapshot
        .query(
            &sql,
            libsql::params::Params::Positional(vec![libsql::Value::Text(source_id.to_string())]),
        )
        .await
        .map_err(snapshot_error)?;
    let mut rows = Vec::new();
    while let Some(row) = query_rows.next().await.map_err(snapshot_error)? {
        let mut values = Vec::with_capacity(columns.len());
        for index in 0..columns.len() {
            let index = i32::try_from(index).map_err(|_| {
                WenlanError::Validation("repair_target_schema_too_wide".to_string())
            })?;
            values.push(row.get::<String>(index).map_err(database_error)?);
        }
        rows.push(values);
    }
    if rows.is_empty() {
        return Err(WenlanError::NotFound("repair_target_missing".to_string()));
    }
    Ok(StoredRollbackArtifact {
        format_version: REPAIR_ROLLBACK_FORMAT_VERSION,
        table: "memories".to_string(),
        source_id: source_id.to_string(),
        columns,
        rows,
    })
}

async fn capture_rollback_on_connection(
    connection: &libsql::Connection,
    source_id: &str,
) -> Result<StoredRollbackArtifact, WenlanError> {
    let mut column_rows = connection
        .query("PRAGMA table_info(memories)", ())
        .await
        .map_err(database_error)?;
    let mut columns = Vec::new();
    while let Some(row) = column_rows.next().await.map_err(database_error)? {
        columns.push(row.get::<String>(1).map_err(database_error)?);
    }
    drop(column_rows);
    if columns.is_empty() {
        return Err(WenlanError::Validation(
            "repair_target_schema_missing".to_string(),
        ));
    }
    let selected = columns
        .iter()
        .map(|column| format!("quote({})", quote_identifier(column)))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT {selected} FROM memories
         WHERE source='memory' AND source_id=?1 ORDER BY chunk_index,id"
    );
    let mut query_rows = connection
        .query(&sql, libsql::params![source_id])
        .await
        .map_err(database_error)?;
    let mut rows = Vec::new();
    while let Some(row) = query_rows.next().await.map_err(database_error)? {
        let mut values = Vec::with_capacity(columns.len());
        for index in 0..columns.len() {
            let index = i32::try_from(index).map_err(|_| {
                WenlanError::Validation("repair_target_schema_too_wide".to_string())
            })?;
            values.push(row.get::<String>(index).map_err(database_error)?);
        }
        rows.push(values);
    }
    if rows.is_empty() {
        return Err(WenlanError::Conflict("repair_target_stale".to_string()));
    }
    Ok(StoredRollbackArtifact {
        format_version: REPAIR_ROLLBACK_FORMAT_VERSION,
        table: "memories".to_string(),
        source_id: source_id.to_string(),
        columns,
        rows,
    })
}

fn target_receipt(rollback: &StoredRollbackArtifact) -> Result<RepairDigest, WenlanError> {
    let mut bytes = b"wenlan-repair-target-v1".to_vec();
    bytes.extend(serde_json::to_vec(rollback)?);
    Ok(repair_digest(&bytes))
}

fn validate_source_receipts(
    request: &PrepareRepairRequest,
    current: SnapshotReceipt,
) -> Result<(), WenlanError> {
    if !current.is_consistent() {
        return Err(WenlanError::Conflict(
            "repair_snapshot_inconsistent".to_string(),
        ));
    }
    let current = lint_digest(current.analysis_receipt_digest().as_bytes());
    for report in [request.general_report(), request.deep_report()] {
        let db = report.snapshots().db();
        if db.analysis_digest() != &current || db.post_run_digest() != Some(&current) {
            return Err(WenlanError::Conflict(
                "repair_source_reports_stale".to_string(),
            ));
        }
    }
    Ok(())
}

fn lint_digest(bytes: [u8; 32]) -> LintDigest {
    LintDigest::from_u64(u64::from_le_bytes(
        bytes[..8].try_into().expect("eight bytes"),
    ))
}

pub(crate) fn repair_digest(bytes: &[u8]) -> RepairDigest {
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}").expect("write to string");
    }
    RepairDigest::parse(&encoded).expect("sha256 is lowercase hex")
}

/// Logical digest of every ordinary SQLite table, streamed one row at a time.
/// This is intentionally separate from lint's cheap structural snapshot: a
/// repair verification must detect in-place updates that preserve schema and
/// row counts without loading the whole database into memory.
pub(crate) async fn database_content_digest(
    connection: &libsql::Connection,
) -> Result<RepairDigest, WenlanError> {
    let mut digest = Sha256::new();
    digest_len_bytes(&mut digest, b"wenlan-repair-db-content-v1")?;

    let mut schema_rows = connection
        .query(
            "SELECT type, name, COALESCE(sql, '')
             FROM sqlite_schema
             WHERE type IN ('index', 'table', 'trigger', 'view')
               AND name NOT LIKE 'sqlite_%'
             ORDER BY type, name",
            (),
        )
        .await
        .map_err(database_error)?;
    let mut ordinary_tables = Vec::new();
    while let Some(row) = schema_rows.next().await.map_err(database_error)? {
        let object_type = row.get::<String>(0).map_err(database_error)?;
        let name = row.get::<String>(1).map_err(database_error)?;
        let sql = row.get::<String>(2).map_err(database_error)?;
        digest_len_bytes(&mut digest, object_type.as_bytes())?;
        digest_len_bytes(&mut digest, name.as_bytes())?;
        digest_len_bytes(&mut digest, sql.as_bytes())?;
        if object_type == "table"
            && !sql
                .trim_start()
                .to_ascii_lowercase()
                .starts_with("create virtual table")
        {
            ordinary_tables.push((name, sql));
        }
    }
    drop(schema_rows);

    for (table, create_sql) in ordinary_tables {
        digest_len_bytes(&mut digest, table.as_bytes())?;
        let order_by = if create_sql.to_ascii_lowercase().contains("without rowid") {
            without_rowid_order(connection, &table).await?
        } else {
            "_rowid_".to_string()
        };
        let query = format!(
            "SELECT * FROM {} ORDER BY {order_by}",
            quote_identifier(&table)
        );
        let mut rows = connection.query(&query, ()).await.map_err(database_error)?;
        let column_count = rows.column_count();
        digest.update(i64::from(column_count).to_le_bytes());
        while let Some(row) = rows.next().await.map_err(database_error)? {
            digest.update(b"row");
            for column in 0..column_count {
                digest_sql_value(&mut digest, row.get_value(column).map_err(database_error)?)?;
            }
        }
    }

    Ok(repair_digest(&digest.finalize()))
}

async fn without_rowid_order(
    connection: &libsql::Connection,
    table: &str,
) -> Result<String, WenlanError> {
    let query = format!("PRAGMA table_info({})", quote_identifier(table));
    let mut rows = connection.query(&query, ()).await.map_err(database_error)?;
    let mut primary_key = Vec::new();
    while let Some(row) = rows.next().await.map_err(database_error)? {
        let name = row.get::<String>(1).map_err(database_error)?;
        let position = row.get::<i64>(5).map_err(database_error)?;
        if position > 0 {
            primary_key.push((position, name));
        }
    }
    primary_key.sort_by_key(|(position, _)| *position);
    if primary_key.is_empty() {
        return Err(WenlanError::VectorDb(format!(
            "repair db digest: WITHOUT ROWID table {table} has no primary key"
        )));
    }
    Ok(primary_key
        .into_iter()
        .map(|(_, name)| quote_identifier(&name))
        .collect::<Vec<_>>()
        .join(","))
}

fn digest_sql_value(digest: &mut Sha256, value: libsql::Value) -> Result<(), WenlanError> {
    match value {
        libsql::Value::Null => digest.update(b"n"),
        libsql::Value::Integer(value) => {
            digest.update(b"i");
            digest.update(value.to_le_bytes());
        }
        libsql::Value::Real(value) => {
            digest.update(b"r");
            digest.update(value.to_bits().to_le_bytes());
        }
        libsql::Value::Text(value) => {
            digest.update(b"t");
            digest_len_bytes(digest, value.as_bytes())?;
        }
        libsql::Value::Blob(value) => {
            digest.update(b"b");
            digest_len_bytes(digest, &value)?;
        }
    }
    Ok(())
}

fn digest_len_bytes(digest: &mut Sha256, value: &[u8]) -> Result<(), WenlanError> {
    let length = u64::try_from(value.len())
        .map_err(|_| WenlanError::Validation("repair_db_digest_value_too_large".to_string()))?;
    digest.update(length.to_le_bytes());
    digest.update(value);
    Ok(())
}

fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn safe_manifest_id(value: &str) -> bool {
    value.starts_with("repair_")
        && Path::new(value)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
        && Path::new(value).components().count() == 1
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), WenlanError> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    set_private_file_permissions(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn publish_no_replace(
    pending_path: &Path,
    final_path: &Path,
    exists_error: &str,
) -> Result<(), WenlanError> {
    publish_no_replace_with_hook(pending_path, final_path, exists_error, || {})
}

fn publish_no_replace_with_hook<F>(
    pending_path: &Path,
    final_path: &Path,
    exists_error: &str,
    after_final_sync: F,
) -> Result<(), WenlanError>
where
    F: FnOnce(),
{
    fs::hard_link(pending_path, final_path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            WenlanError::Conflict(exists_error.to_string())
        } else {
            WenlanError::Io(error)
        }
    })?;
    if let Some(parent) = final_path.parent() {
        sync_dir(parent)?;
    }
    after_final_sync();
    fs::remove_file(pending_path)?;
    if let Some(parent) = final_path.parent() {
        sync_dir(parent)?;
    }
    Ok(())
}

#[cfg(unix)]
fn ensure_repair_artifacts_supported() -> Result<(), WenlanError> {
    Ok(())
}

#[cfg(not(unix))]
fn ensure_repair_artifacts_supported() -> Result<(), WenlanError> {
    Err(WenlanError::Validation(
        "lint_repair_unsupported_platform".to_string(),
    ))
}

fn ensure_private_dir(path: &Path) -> Result<(), WenlanError> {
    fs::create_dir_all(path)?;
    set_private_dir_permissions(path)
}

#[cfg(unix)]
fn set_private_dir_permissions(path: &Path) -> Result<(), WenlanError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_dir_permissions(_path: &Path) -> Result<(), WenlanError> {
    Ok(())
}

#[cfg(unix)]
fn set_private_file_permissions(path: &Path) -> Result<(), WenlanError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_file_permissions(_path: &Path) -> Result<(), WenlanError> {
    Ok(())
}

#[cfg(unix)]
fn sync_dir(path: &Path) -> Result<(), WenlanError> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_dir(_path: &Path) -> Result<(), WenlanError> {
    Ok(())
}

fn snapshot_error(error: SnapshotError) -> WenlanError {
    WenlanError::VectorDb(format!("repair snapshot: {error}"))
}

fn page_snapshot_error(error: crate::lint::pages::fs::PageFsError) -> WenlanError {
    WenlanError::Conflict(format!("repair_verification_reports_stale: {error}"))
}

fn database_error(error: libsql::Error) -> WenlanError {
    WenlanError::VectorDb(format!("repair database: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db::{tests::test_db, MemoryDB},
        lint::{
            context::{CancellationToken, LintClock},
            runner::LintRunner,
            snapshot::LintReadSnapshot,
        },
    };
    use wenlan_types::{
        lint::{
            LintAgentSubmission, LintAgentVerdict, LintEvidenceRef, LintProfile, LintQuery,
            LintSemanticCheckId, LintSemanticDecision,
        },
        repair::{
            ApplyRepairRequest, PrepareRepairRequest, RepairDigest, RepairLintScope,
            VerifyRepairRequest,
        },
        MemoryType,
    };

    #[test]
    fn receipt_publication_keeps_pending_link_through_final_directory_sync() {
        let root = tempfile::tempdir().unwrap();
        let pending = root.path().join("pending.json");
        let final_path = root.path().join("final.json");
        std::fs::write(&pending, b"receipt").unwrap();
        let mut observed = false;

        publish_no_replace_with_hook(&pending, &final_path, "already_exists", || {
            assert!(pending.is_file());
            assert!(final_path.is_file());
            observed = true;
        })
        .unwrap();

        assert!(observed);
        assert!(!pending.exists());
        assert_eq!(std::fs::read(final_path).unwrap(), b"receipt");
    }

    async fn fixture() -> (MemoryDB, tempfile::TempDir) {
        let (db, dir) = test_db().await;
        db.conn
            .lock()
            .await
            .execute_batch(
                "INSERT INTO spaces (id,name,created_at,updated_at)
                 VALUES ('space-personal','personal',1,1),('space-work','work',1,1);
                 INSERT INTO memories
                     (id,content,source,source_id,title,chunk_index,last_modified,chunk_type,
                      pending_revision,is_recap,supersede_mode,memory_type,space)
                 VALUES ('row-target','Target decision','memory','mem_target','target',0,10,
                         'text',0,0,'hide',NULL,'work'),
                        ('row-target-2','Target detail','memory','mem_target','target',1,10,
                         'text',0,0,'hide',NULL,'work'),
                        ('row-other','Other fact','memory','mem_other','other',0,11,
                         'text',0,0,'hide','fact','personal');",
            )
            .await
            .unwrap();
        (db, dir)
    }

    async fn request(db: &MemoryDB) -> PrepareRepairRequest {
        request_for_scope(db, RepairLintScope::global(), None).await
    }

    async fn request_for_scope(
        db: &MemoryDB,
        lint_scope: RepairLintScope,
        space: Option<&str>,
    ) -> PrepareRepairRequest {
        let general = LintRunner::new(LintClock::fixed(), CancellationToken::new())
            .run(
                db,
                &LintQuery::new(Some(LintProfile::General), space.map(str::to_string)),
                None,
                false,
            )
            .await
            .unwrap();
        let prepared = LintRunner::new(LintClock::fixed(), CancellationToken::new())
            .with_semantic_agent_assist()
            .run(
                db,
                &LintQuery::new(Some(LintProfile::Deep), space.map(str::to_string)),
                None,
                false,
            )
            .await
            .unwrap();
        let work = prepared.agent_work().expect("agent work");
        let verdicts = work
            .candidates()
            .iter()
            .map(|candidate| {
                let finding = candidate.check_id() == LintSemanticCheckId::MemoryClassification;
                LintAgentVerdict::try_new(
                    candidate.reference(),
                    if finding {
                        LintSemanticDecision::Finding
                    } else {
                        LintSemanticDecision::Pass
                    },
                    None,
                    candidate.reason_code(),
                    9_000,
                    vec![],
                )
                .unwrap()
            })
            .collect();
        let submission =
            LintAgentSubmission::try_new(work.work_digest().clone(), verdicts).unwrap();
        let deep = LintRunner::new(LintClock::fixed(), CancellationToken::new())
            .with_semantic_agent_submission(submission)
            .run(
                db,
                &LintQuery::new(Some(LintProfile::Deep), space.map(str::to_string)),
                None,
                false,
            )
            .await
            .unwrap();
        let deep = complete_deep_fixture(deep);
        let finding = deep
            .checks()
            .iter()
            .find(|check| check.check_id() == LintSemanticCheckId::MemoryClassification.as_str())
            .and_then(|check| {
                check.evidence().iter().find_map(|evidence| match evidence {
                    LintEvidenceRef::SemanticFinding { finding } => Some(finding.clone()),
                    _ => None,
                })
            })
            .expect("classification finding");
        assert!(general.complete(), "general totals: {:?}", general.totals());
        assert!(
            deep.complete(),
            "deep totals: {:?}; incomplete: {:?}",
            deep.totals(),
            deep.checks()
                .iter()
                .filter(|check| !matches!(
                    check.outcome(),
                    wenlan_types::lint::LintOutcome::Pass
                        | wenlan_types::lint::LintOutcome::Finding
                ))
                .map(|check| (check.check_id(), check.outcome()))
                .collect::<Vec<_>>()
        );
        assert!(deep.agent_work().is_some(), "final Deep lost agent work");
        PrepareRepairRequest::try_new(lint_scope, general, deep, finding, MemoryType::Decision)
            .unwrap()
    }

    fn complete_deep_fixture(
        report: wenlan_types::lint::LintReport,
    ) -> wenlan_types::lint::LintReport {
        let mut value = serde_json::to_value(report).unwrap();
        let checks = value["checks"].as_array_mut().unwrap();
        for check in checks.iter_mut() {
            let complete = matches!(check["outcome"].as_str(), Some("pass" | "finding"));
            if !complete {
                check["outcome"] = serde_json::json!("pass");
                check["severity"] = serde_json::json!("info");
                check["applicability"] = serde_json::json!("expected_empty");
                check["precondition"] = serde_json::json!("expected_empty");
            }
        }
        let passed = checks
            .iter()
            .filter(|check| check["outcome"] == "pass")
            .count();
        let findings = checks
            .iter()
            .filter(|check| check["outcome"] == "finding")
            .count();
        let actionable_findings = checks
            .iter()
            .filter(|check| check["outcome"] == "finding" && check["gate_effect"] == "actionable")
            .count();
        value["totals"] = serde_json::json!({
            "checks": checks.len(),
            "passed": passed,
            "findings": findings,
            "actionable_findings": actionable_findings,
            "advisory_findings": findings - actionable_findings,
            "incomplete": 0
        });
        value["complete"] = serde_json::json!(true);
        serde_json::from_value(value).unwrap()
    }

    fn fail_deep_check(
        report: wenlan_types::lint::LintReport,
        classification: bool,
    ) -> wenlan_types::lint::LintReport {
        let mut value = serde_json::to_value(report).unwrap();
        let checks = value["checks"].as_array_mut().unwrap();
        let check = checks
            .iter_mut()
            .find(|check| {
                (check["check_id"] == REPAIR_CLASSIFICATION_CHECK_ID) == classification
                    && matches!(check["outcome"].as_str(), Some("pass" | "finding"))
            })
            .expect("eligible Deep check");
        check["outcome"] = serde_json::json!("failed_to_run");
        check["severity"] = serde_json::json!("error");
        check["applicability"] = serde_json::json!("applicable");
        check["precondition"] = serde_json::json!("ready");
        let passed = checks
            .iter()
            .filter(|check| check["outcome"] == "pass")
            .count();
        let findings = checks
            .iter()
            .filter(|check| check["outcome"] == "finding")
            .count();
        let actionable_findings = checks
            .iter()
            .filter(|check| check["outcome"] == "finding" && check["gate_effect"] == "actionable")
            .count();
        let incomplete = checks.len() - passed - findings;
        value["totals"] = serde_json::json!({
            "checks": checks.len(),
            "passed": passed,
            "findings": findings,
            "actionable_findings": actionable_findings,
            "advisory_findings": findings - actionable_findings,
            "incomplete": incomplete
        });
        value["complete"] = serde_json::json!(false);
        serde_json::from_value(value).unwrap()
    }

    fn with_incomplete_deep(
        request: PrepareRepairRequest,
        classification: bool,
    ) -> Result<PrepareRepairRequest, serde_json::Error> {
        let mut value = serde_json::to_value(request).unwrap();
        let deep: wenlan_types::lint::LintReport =
            serde_json::from_value(value["deep_report"].take()).unwrap();
        value["deep_report"] = serde_json::to_value(fail_deep_check(deep, classification)).unwrap();
        serde_json::from_value(value)
    }

    async fn fingerprint(db: &MemoryDB) -> [u8; 32] {
        let snapshot = LintReadSnapshot::open(&db._db).await.unwrap();
        let fingerprint = snapshot.analysis_digest().unwrap().as_bytes();
        snapshot.finish().await.unwrap();
        fingerprint
    }

    #[tokio::test]
    async fn prepare_resolves_one_hashed_memory_without_mutating_store() {
        let (db, _db_dir) = fixture().await;
        let repair_root = tempfile::tempdir().unwrap();
        let request = request(&db).await;
        let before = fingerprint(&db).await;

        let manifest = prepare_memory_reclassification(
            &db,
            &RepairArtifactStore::new(repair_root.path().to_path_buf()),
            request,
            1_721_000_000,
        )
        .await
        .unwrap();

        assert_eq!(manifest.target().memory_source_id(), "mem_target");
        assert_eq!(before, fingerprint(&db).await);
        let manifest_dir = repair_root.path().join(manifest.manifest_id());
        assert!(manifest_dir.join("manifest.json").is_file());
        assert!(manifest_dir.join("rollback-v1.json").is_file());
        assert!(matches!(
            RepairArtifactStore::new(repair_root.path().to_path_buf())
                .read_stored_manifest(manifest.manifest_id())
                .unwrap(),
            wenlan_types::repair::StoredRepairManifest::V2(_)
        ));
        assert_eq!(
            RepairArtifactStore::new(repair_root.path().to_path_buf())
                .load_manifest(manifest.manifest_id())
                .unwrap(),
            manifest
        );
    }

    #[tokio::test]
    async fn prepare_accepts_complete_classification_with_unrelated_incomplete_deep_check() {
        let (db, _db_dir) = fixture().await;
        let repair_root = tempfile::tempdir().unwrap();
        let request = with_incomplete_deep(request(&db).await, false)
            .expect("unrelated Deep incompleteness remains a valid repair request");

        let manifest = prepare_memory_reclassification(
            &db,
            &RepairArtifactStore::new(repair_root.path().to_path_buf()),
            request,
            1_721_000_000,
        )
        .await
        .expect("classification repair remains applicable");

        assert_eq!(manifest.target().memory_source_id(), "mem_target");
    }

    #[tokio::test]
    async fn prepare_rejects_incomplete_classification_check() {
        let (db, _db_dir) = fixture().await;

        assert!(with_incomplete_deep(request(&db).await, true).is_err());
    }

    #[test]
    fn artifact_store_loads_the_first_durable_v1_manifest_shape() {
        let repair_root = tempfile::tempdir().unwrap();
        let manifest_id = "repair_550e8400-e29b-41d4-a716-446655440000";
        let manifest_dir = repair_root.path().join(manifest_id);
        std::fs::create_dir(&manifest_dir).unwrap();
        std::fs::write(
            manifest_dir.join(MANIFEST_FILE),
            include_bytes!("../../wenlan-types/testdata/repair/v1/manifest-pre-baseline.json"),
        )
        .unwrap();

        let manifest = RepairArtifactStore::new(repair_root.path().to_path_buf())
            .load_manifest(manifest_id)
            .expect("the first daemon-persisted v1 shape remains loadable");

        assert!(manifest.post_assertions().general_baseline().is_empty());
        assert!(manifest.post_assertions().deep_baseline().is_empty());
    }

    #[tokio::test]
    async fn pre_baseline_v1_verification_requires_conservatively_clean_reports() {
        let repair_root = tempfile::tempdir().unwrap();
        let manifest_id = "repair_550e8400-e29b-41d4-a716-446655440000";
        let manifest_dir = repair_root.path().join(manifest_id);
        std::fs::create_dir(&manifest_dir).unwrap();
        std::fs::write(
            manifest_dir.join(MANIFEST_FILE),
            include_bytes!("../../wenlan-types/testdata/repair/v1/manifest-pre-baseline.json"),
        )
        .unwrap();
        let manifest = RepairArtifactStore::new(repair_root.path().to_path_buf())
            .load_manifest(manifest_id)
            .unwrap();
        let (db, _db_dir) = fixture().await;
        let before = request(&db).await;

        assert!(matches!(
            validate_verification_reports(
                &manifest,
                before.general_report(),
                before.deep_report()
            ),
            Err(WenlanError::Validation(code)) if code == "repair_legacy_verification_not_clean"
        ));
    }

    #[tokio::test]
    async fn unverified_v1_apply_receipt_cannot_claim_v2_non_target_proof() {
        let repair_root = tempfile::tempdir().unwrap();
        let manifest_id = "repair_550e8400-e29b-41d4-a716-446655440000";
        let manifest_dir = repair_root.path().join(manifest_id);
        std::fs::create_dir(&manifest_dir).unwrap();
        std::fs::write(
            manifest_dir.join(MANIFEST_FILE),
            include_bytes!("../../wenlan-types/testdata/repair/v1/manifest.json"),
        )
        .unwrap();
        std::fs::write(
            manifest_dir.join(APPLY_RECEIPT_FILE),
            include_bytes!("../../wenlan-types/testdata/repair/v1/apply-receipt.json"),
        )
        .unwrap();
        let store = RepairArtifactStore::new(repair_root.path().to_path_buf());
        let manifest = store.load_manifest(manifest_id).unwrap();
        let apply_receipt = store.load_apply_receipt(&manifest).unwrap();
        assert!(apply_receipt.post_apply_db_digest().is_none());
        let (db, _db_dir) = fixture().await;
        let (general, deep) = verification_reports(&db).await;

        let result = record_repair_verification(
            &db,
            &store,
            VerifyRepairRequest::try_new(
                manifest_id.to_string(),
                manifest.manifest_digest().clone(),
                apply_receipt.receipt_digest().clone(),
                general,
                deep,
            )
            .unwrap(),
            None,
            1_721_000_002,
        )
        .await;

        assert!(matches!(
            result,
            Err(WenlanError::Validation(message))
                if message == "repair_legacy_apply_receipt_unverifiable"
        ));
    }

    #[tokio::test]
    async fn prepare_rejects_stale_lint_receipt_without_writing_artifacts() {
        let (db, _db_dir) = fixture().await;
        let repair_root = tempfile::tempdir().unwrap();
        let request = request(&db).await;
        db.conn
            .lock()
            .await
            .execute(
                "UPDATE memories SET title='changed' WHERE source_id='mem_other'",
                (),
            )
            .await
            .unwrap();

        let result = prepare_memory_reclassification(
            &db,
            &RepairArtifactStore::new(repair_root.path().to_path_buf()),
            request,
            1_721_000_000,
        )
        .await;

        assert!(result.is_err());
        assert_eq!(std::fs::read_dir(repair_root.path()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn prepare_rejects_reports_from_different_runtime_producers() {
        let (db, _db_dir) = fixture().await;
        let repair_root = tempfile::tempdir().unwrap();
        let mut value = serde_json::to_value(request(&db).await).unwrap();
        value["deep_report"]["producer_receipt"]["runtime_commit"] =
            serde_json::json!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        let mixed: PrepareRepairRequest = serde_json::from_value(value).unwrap();

        let result = prepare_memory_reclassification(
            &db,
            &RepairArtifactStore::new(repair_root.path().to_path_buf()),
            mixed,
            1_721_000_000,
        )
        .await;

        assert!(matches!(
            result,
            Err(WenlanError::Conflict(message))
                if message == "repair_source_producers_mismatch"
        ));
        assert_eq!(std::fs::read_dir(repair_root.path()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn prepare_rejects_reports_from_a_different_page_projection() {
        let (db, _db_dir) = fixture().await;
        let repair_root = tempfile::tempdir().unwrap();
        let page_root = tempfile::tempdir().unwrap();
        std::fs::write(page_root.path().join("changed.md"), "# changed").unwrap();

        let result = prepare_memory_reclassification_with_pages(
            &db,
            &RepairArtifactStore::new(repair_root.path().to_path_buf()),
            request(&db).await,
            Some(page_root.path()),
            1_721_000_000,
        )
        .await;

        assert!(matches!(
            result,
            Err(WenlanError::Conflict(message)) if message == "repair_source_reports_stale"
        ));
        assert_eq!(std::fs::read_dir(repair_root.path()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn prepare_rejects_registered_label_bound_to_another_opaque_scope() {
        let (db, _db_dir) = fixture().await;
        let repair_root = tempfile::tempdir().unwrap();
        let request = request_for_scope(
            &db,
            RepairLintScope::registered("work".to_string()).unwrap(),
            Some("work"),
        )
        .await;
        let mut value = serde_json::to_value(request).unwrap();
        value["lint_scope"]["space"] = serde_json::json!("personal");
        let rebound: PrepareRepairRequest = serde_json::from_value(value).unwrap();

        let result = prepare_memory_reclassification(
            &db,
            &RepairArtifactStore::new(repair_root.path().to_path_buf()),
            rebound,
            1_721_000_000,
        )
        .await;

        assert!(matches!(
            result,
            Err(WenlanError::Validation(message)) if message == "repair_scope_mismatch"
        ));
        assert_eq!(std::fs::read_dir(repair_root.path()).unwrap().count(), 0);
    }

    async fn prepared_fixture() -> (
        MemoryDB,
        tempfile::TempDir,
        tempfile::TempDir,
        RepairManifest,
    ) {
        let (db, db_dir) = fixture().await;
        let repair_root = tempfile::tempdir().unwrap();
        let manifest = prepare_memory_reclassification(
            &db,
            &RepairArtifactStore::new(repair_root.path().to_path_buf()),
            request(&db).await,
            1_721_000_000,
        )
        .await
        .unwrap();
        (db, db_dir, repair_root, manifest)
    }

    fn exact_apply(manifest: &RepairManifest) -> ApplyRepairRequest {
        let approval = format!(
            "apply repair {} {}",
            manifest.manifest_id(),
            manifest.manifest_digest().as_str()
        );
        ApplyRepairRequest::try_new(
            manifest.manifest_id().to_string(),
            manifest.manifest_digest().clone(),
            approval,
        )
        .unwrap()
    }

    async fn target_memory_types(db: &MemoryDB) -> Vec<Option<String>> {
        let conn = db.conn.lock().await;
        let mut rows = conn
            .query(
                "SELECT memory_type FROM memories
                 WHERE source='memory' AND source_id='mem_target'
                 ORDER BY chunk_index,id",
                (),
            )
            .await
            .unwrap();
        let mut output = Vec::new();
        while let Some(row) = rows.next().await.unwrap() {
            output.push(row.get(0).unwrap());
        }
        output
    }

    async fn verification_reports(
        db: &MemoryDB,
    ) -> (
        wenlan_types::lint::LintReport,
        wenlan_types::lint::LintReport,
    ) {
        verification_reports_at(db, None).await
    }

    async fn verification_reports_at(
        db: &MemoryDB,
        page_root: Option<&Path>,
    ) -> (
        wenlan_types::lint::LintReport,
        wenlan_types::lint::LintReport,
    ) {
        let general = LintRunner::new(LintClock::fixed(), CancellationToken::new())
            .run(
                db,
                &LintQuery::new(Some(LintProfile::General), None),
                page_root,
                page_root.is_some(),
            )
            .await
            .unwrap();
        let prepared = LintRunner::new(LintClock::fixed(), CancellationToken::new())
            .with_semantic_agent_assist()
            .run(
                db,
                &LintQuery::new(Some(LintProfile::Deep), None),
                page_root,
                page_root.is_some(),
            )
            .await
            .unwrap();
        let work = prepared.agent_work().expect("agent work");
        let verdicts = work
            .candidates()
            .iter()
            .map(|candidate| {
                LintAgentVerdict::try_new(
                    candidate.reference(),
                    LintSemanticDecision::Pass,
                    None,
                    candidate.reason_code(),
                    9_000,
                    vec![],
                )
                .unwrap()
            })
            .collect();
        let submission =
            LintAgentSubmission::try_new(work.work_digest().clone(), verdicts).unwrap();
        let deep = LintRunner::new(LintClock::fixed(), CancellationToken::new())
            .with_semantic_agent_submission(submission)
            .run(
                db,
                &LintQuery::new(Some(LintProfile::Deep), None),
                page_root,
                page_root.is_some(),
            )
            .await
            .unwrap();
        (general, complete_deep_fixture(deep))
    }

    fn exact_verify(
        manifest: &RepairManifest,
        apply_receipt: &RepairApplyReceipt,
        general: wenlan_types::lint::LintReport,
        deep: wenlan_types::lint::LintReport,
    ) -> VerifyRepairRequest {
        VerifyRepairRequest::try_new(
            manifest.manifest_id().to_string(),
            manifest.manifest_digest().clone(),
            apply_receipt.receipt_digest().clone(),
            general,
            deep,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn wrong_approval_or_stale_target_performs_zero_writes() {
        let (db, _db_dir, repair_root, manifest) = prepared_fixture().await;
        let store = RepairArtifactStore::new(repair_root.path().to_path_buf());
        let before = fingerprint(&db).await;
        let wrong_digest =
            RepairDigest::parse("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
                .unwrap();
        let wrong = ApplyRepairRequest::try_new(
            manifest.manifest_id().to_string(),
            wrong_digest.clone(),
            format!(
                "apply repair {} {}",
                manifest.manifest_id(),
                wrong_digest.as_str()
            ),
        )
        .unwrap();

        assert!(apply_repair(&db, &store, wrong, 1_721_000_001)
            .await
            .is_err());
        assert_eq!(before, fingerprint(&db).await);

        db.conn
            .lock()
            .await
            .execute(
                "UPDATE memories SET title='stale' WHERE id='row-target'",
                (),
            )
            .await
            .unwrap();
        let stale_before = fingerprint(&db).await;
        assert!(matches!(
            apply_repair(&db, &store, exact_apply(&manifest), 1_721_000_001).await,
            Err(WenlanError::Conflict(message)) if message == "repair_target_stale"
        ));
        assert_eq!(stale_before, fingerprint(&db).await);
        assert!(!store
            .manifest_dir(manifest.manifest_id())
            .unwrap()
            .join("apply-receipt.json")
            .exists());
    }

    #[tokio::test]
    async fn successful_apply_changes_only_declared_owner_closure() {
        let (db, _db_dir, repair_root, manifest) = prepared_fixture().await;
        let store = RepairArtifactStore::new(repair_root.path().to_path_buf());

        let receipt = apply_repair(&db, &store, exact_apply(&manifest), 1_721_000_001)
            .await
            .unwrap();

        assert_eq!(
            target_memory_types(&db).await,
            vec![Some("decision".to_string()), Some("decision".to_string())]
        );
        let conn = db.conn.lock().await;
        let mut rows = conn
            .query("SELECT memory_type FROM memories WHERE id='row-other'", ())
            .await
            .unwrap();
        let other: Option<String> = rows.next().await.unwrap().unwrap().get(0).unwrap();
        drop(rows);
        drop(conn);
        assert_eq!(other.as_deref(), Some("fact"));
        assert_eq!(receipt.manifest_digest(), manifest.manifest_digest());
        assert!(store
            .manifest_dir(manifest.manifest_id())
            .unwrap()
            .join("apply-receipt.json")
            .is_file());
    }

    #[tokio::test]
    async fn apply_recovers_committed_receipt_after_unrelated_background_write() {
        let (db, _db_dir, repair_root, manifest) = prepared_fixture().await;
        let store = RepairArtifactStore::new(repair_root.path().to_path_buf());
        let receipt = apply_repair(&db, &store, exact_apply(&manifest), 1_721_000_001)
            .await
            .unwrap();
        let manifest_dir = store.manifest_dir(manifest.manifest_id()).unwrap();
        std::fs::rename(
            manifest_dir.join(APPLY_RECEIPT_FILE),
            manifest_dir.join(APPLY_RECEIPT_PENDING_FILE),
        )
        .unwrap();
        db.conn
            .lock()
            .await
            .execute(
                "UPDATE memories SET title='background' WHERE id='row-other'",
                (),
            )
            .await
            .unwrap();

        let recovered = apply_repair(&db, &store, exact_apply(&manifest), 1_721_000_001)
            .await
            .unwrap();

        assert_eq!(recovered, receipt);
        assert!(manifest_dir.join(APPLY_RECEIPT_FILE).is_file());
        assert!(!manifest_dir.join(APPLY_RECEIPT_PENDING_FILE).exists());
    }

    #[tokio::test]
    async fn apply_refuses_while_manifest_operation_lock_is_held() {
        use fs2::FileExt as _;

        let (db, _db_dir, repair_root, manifest) = prepared_fixture().await;
        let store = RepairArtifactStore::new(repair_root.path().to_path_buf());
        let lock_path = store
            .manifest_dir(manifest.manifest_id())
            .unwrap()
            .join(".operation.lock");
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)
            .unwrap();
        lock.lock_exclusive().unwrap();

        let result = apply_repair(&db, &store, exact_apply(&manifest), 1_721_000_001).await;

        assert!(matches!(
            result,
            Err(WenlanError::Conflict(message)) if message == "repair_operation_in_progress"
        ));
        assert_eq!(target_memory_types(&db).await, vec![None, None]);
    }

    #[tokio::test]
    async fn apply_discards_precommit_partial_receipt_and_retries() {
        let (db, _db_dir, repair_root, manifest) = prepared_fixture().await;
        let store = RepairArtifactStore::new(repair_root.path().to_path_buf());
        let manifest_dir = store.manifest_dir(manifest.manifest_id()).unwrap();
        std::fs::write(manifest_dir.join(APPLY_RECEIPT_PENDING_FILE), b"partial").unwrap();

        let receipt = apply_repair(&db, &store, exact_apply(&manifest), 1_721_000_001)
            .await
            .unwrap();

        assert_eq!(receipt.manifest_id(), manifest.manifest_id());
        assert!(manifest_dir.join(APPLY_RECEIPT_FILE).is_file());
        assert!(!manifest_dir.join(APPLY_RECEIPT_PENDING_FILE).exists());
    }

    #[tokio::test]
    async fn effect_escape_rolls_back_target_and_trigger_side_effect() {
        let (db, _db_dir) = fixture().await;
        db.conn
            .lock()
            .await
            .execute_batch(
                "CREATE TABLE repair_escape (value TEXT NOT NULL);
                 CREATE TRIGGER repair_escape_trigger AFTER UPDATE OF memory_type ON memories
                 WHEN NEW.source_id='mem_target'
                 BEGIN INSERT INTO repair_escape(value) VALUES ('escaped'); END;",
            )
            .await
            .unwrap();
        let repair_root = tempfile::tempdir().unwrap();
        let store = RepairArtifactStore::new(repair_root.path().to_path_buf());
        let manifest =
            prepare_memory_reclassification(&db, &store, request(&db).await, 1_721_000_000)
                .await
                .unwrap();
        let before = fingerprint(&db).await;

        let result = apply_repair(&db, &store, exact_apply(&manifest), 1_721_000_001).await;

        assert!(matches!(
            result,
            Err(WenlanError::VectorDb(message)) if message.contains("repair_effect_escape")
        ));
        assert_eq!(before, fingerprint(&db).await);
        assert_eq!(target_memory_types(&db).await, vec![None, None]);
    }

    #[tokio::test]
    async fn verification_records_receipt_only_after_post_lint_and_effect_proof() {
        let (db, _db_dir, repair_root, manifest) = prepared_fixture().await;
        let store = RepairArtifactStore::new(repair_root.path().to_path_buf());
        let apply_receipt = apply_repair(&db, &store, exact_apply(&manifest), 1_721_000_001)
            .await
            .unwrap();
        let (general, deep) = verification_reports(&db).await;

        let receipt = record_repair_verification(
            &db,
            &store,
            exact_verify(&manifest, &apply_receipt, general, deep),
            None,
            1_721_000_002,
        )
        .await
        .unwrap();

        assert_eq!(receipt.manifest_id(), manifest.manifest_id());
        assert!(store
            .manifest_dir(manifest.manifest_id())
            .unwrap()
            .join("verification-receipt.json")
            .is_file());
    }

    #[tokio::test]
    async fn post_repair_general_and_deep_must_share_one_producer() {
        let (db, _db_dir, _repair_root, manifest) = prepared_fixture().await;
        let (general, deep) = verification_reports(&db).await;
        let mut deep_value = serde_json::to_value(deep).unwrap();
        let replacement_commit = if general
            .producer_receipt()
            .runtime_commit()
            .is_some_and(|commit| commit.as_str() == "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        {
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        } else {
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        };
        deep_value["producer_receipt"]["runtime_commit"] = serde_json::json!(replacement_commit);
        let mixed_deep: wenlan_types::lint::LintReport =
            serde_json::from_value(deep_value).unwrap();

        assert!(
            VerifyRepairRequest::try_new(
                manifest.manifest_id().to_string(),
                manifest.manifest_digest().clone(),
                RepairDigest::parse(
                    "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                )
                .unwrap(),
                general.clone(),
                mixed_deep.clone(),
            )
            .is_err()
        );
        assert!(matches!(
            validate_verification_reports(&manifest, &general, &mixed_deep),
            Err(WenlanError::Validation(message))
                if message == "repair_verification_report_mismatch"
        ));
    }

    #[tokio::test]
    async fn pending_verification_manifest_ids_tracks_apply_until_verify() {
        let (db, _db_dir, repair_root, manifest) = prepared_fixture().await;
        let store = RepairArtifactStore::new(repair_root.path().to_path_buf());
        assert!(store
            .pending_verification_manifest_ids()
            .unwrap()
            .is_empty());

        let apply_receipt = apply_repair(&db, &store, exact_apply(&manifest), 1_721_000_001)
            .await
            .unwrap();
        assert_eq!(
            store.pending_verification_manifest_ids().unwrap(),
            vec![manifest.manifest_id().to_string()]
        );

        let (general, deep) = verification_reports(&db).await;
        record_repair_verification(
            &db,
            &store,
            VerifyRepairRequest::try_new(
                manifest.manifest_id().to_string(),
                manifest.manifest_digest().clone(),
                apply_receipt.receipt_digest().clone(),
                general,
                deep,
            )
            .unwrap(),
            None,
            1_721_000_002,
        )
        .await
        .unwrap();
        assert!(store
            .pending_verification_manifest_ids()
            .unwrap()
            .is_empty());
        assert!(store
            .has_completed_verification(manifest.manifest_id())
            .unwrap());
    }

    #[tokio::test]
    async fn verification_retry_returns_the_existing_receipt() {
        let (db, _db_dir, repair_root, manifest) = prepared_fixture().await;
        let store = RepairArtifactStore::new(repair_root.path().to_path_buf());
        let apply_receipt = apply_repair(&db, &store, exact_apply(&manifest), 1_721_000_001)
            .await
            .unwrap();
        let (general, deep) = verification_reports(&db).await;
        let request = exact_verify(&manifest, &apply_receipt, general, deep);

        let first = record_repair_verification(&db, &store, request.clone(), None, 1_721_000_002)
            .await
            .unwrap();
        let retried = record_repair_verification(&db, &store, request, None, 1_721_000_003)
            .await
            .unwrap();

        assert_eq!(retried, first);
    }

    #[tokio::test]
    async fn verification_refuses_while_manifest_operation_lock_is_held() {
        use fs2::FileExt as _;

        let (db, _db_dir, repair_root, manifest) = prepared_fixture().await;
        let store = RepairArtifactStore::new(repair_root.path().to_path_buf());
        let apply_receipt = apply_repair(&db, &store, exact_apply(&manifest), 1_721_000_001)
            .await
            .unwrap();
        let (general, deep) = verification_reports(&db).await;
        let lock_path = store
            .manifest_dir(manifest.manifest_id())
            .unwrap()
            .join(OPERATION_LOCK_FILE);
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .open(lock_path)
            .unwrap();
        lock.lock_exclusive().unwrap();

        let result = record_repair_verification(
            &db,
            &store,
            exact_verify(&manifest, &apply_receipt, general, deep),
            None,
            1_721_000_002,
        )
        .await;

        assert!(matches!(
            result,
            Err(WenlanError::Conflict(message)) if message == "repair_operation_in_progress"
        ));
        assert!(!store
            .manifest_dir(manifest.manifest_id())
            .unwrap()
            .join(VERIFICATION_RECEIPT_FILE)
            .exists());
    }

    #[tokio::test]
    async fn verification_rejects_target_evidence_that_survives_post_lint() {
        let (db, _db_dir, repair_root, manifest) = prepared_fixture().await;
        let store = RepairArtifactStore::new(repair_root.path().to_path_buf());
        let apply_receipt = apply_repair(&db, &store, exact_apply(&manifest), 1_721_000_001)
            .await
            .unwrap();
        let (general, deep) = verification_reports(&db).await;
        let mut value = serde_json::to_value(deep).unwrap();
        let checks = value["checks"].as_array_mut().unwrap();
        let target = checks
            .iter_mut()
            .find(|check| check["check_id"] == REPAIR_CLASSIFICATION_CHECK_ID)
            .unwrap();
        target["outcome"] = serde_json::json!("finding");
        target["severity"] = serde_json::json!("warning");
        target["applicability"] = serde_json::json!("applicable");
        target["precondition"] = serde_json::json!("ready");
        target["evidence"] = serde_json::json!([{
            "kind": "semantic_finding",
            "finding": manifest.source().finding()
        }]);
        let passed = checks
            .iter()
            .filter(|check| check["outcome"] == "pass")
            .count();
        let findings = checks
            .iter()
            .filter(|check| check["outcome"] == "finding")
            .count();
        let actionable_findings = checks
            .iter()
            .filter(|check| check["outcome"] == "finding" && check["gate_effect"] == "actionable")
            .count();
        value["totals"] = serde_json::json!({
            "checks": checks.len(),
            "passed": passed,
            "findings": findings,
            "actionable_findings": actionable_findings,
            "advisory_findings": findings - actionable_findings,
            "incomplete": 0
        });
        let deep = serde_json::from_value(value).unwrap();

        let result = record_repair_verification(
            &db,
            &store,
            exact_verify(&manifest, &apply_receipt, general, deep),
            None,
            1_721_000_002,
        )
        .await;

        assert!(matches!(
            result,
            Err(WenlanError::Validation(message)) if message == "repair_target_assertion_failed"
        ));
    }

    #[tokio::test]
    async fn verification_rejects_unrelated_write_after_apply_even_when_reports_are_fresh() {
        let (db, _db_dir, repair_root, manifest) = prepared_fixture().await;
        let store = RepairArtifactStore::new(repair_root.path().to_path_buf());
        let apply_receipt = apply_repair(&db, &store, exact_apply(&manifest), 1_721_000_001)
            .await
            .unwrap();
        db.conn
            .lock()
            .await
            .execute(
                "UPDATE memories SET title='changed' WHERE id='row-other'",
                (),
            )
            .await
            .unwrap();
        let (general, deep) = verification_reports(&db).await;

        let result = record_repair_verification(
            &db,
            &store,
            exact_verify(&manifest, &apply_receipt, general, deep),
            None,
            1_721_000_002,
        )
        .await;

        assert!(matches!(
            result,
            Err(WenlanError::Conflict(message)) if message == "repair_non_target_state_changed"
        ));
    }

    #[tokio::test]
    async fn verification_rejects_in_place_metadata_update_that_preserves_row_counts() {
        let (db, _db_dir, repair_root, manifest) = prepared_fixture().await;
        let store = RepairArtifactStore::new(repair_root.path().to_path_buf());
        db.set_app_metadata("repair_digest_probe", "before")
            .await
            .unwrap();
        let apply_receipt = apply_repair(&db, &store, exact_apply(&manifest), 1_721_000_001)
            .await
            .unwrap();
        db.set_app_metadata("repair_digest_probe", "after")
            .await
            .unwrap();
        let (general, deep) = verification_reports(&db).await;

        let result = record_repair_verification(
            &db,
            &store,
            exact_verify(&manifest, &apply_receipt, general, deep),
            None,
            1_721_000_002,
        )
        .await;

        assert!(matches!(
            result,
            Err(WenlanError::Conflict(message)) if message == "repair_non_target_state_changed"
        ));
    }

    #[tokio::test]
    async fn verification_rejects_reports_that_are_no_longer_current() {
        let (db, _db_dir, repair_root, manifest) = prepared_fixture().await;
        let store = RepairArtifactStore::new(repair_root.path().to_path_buf());
        let apply_receipt = apply_repair(&db, &store, exact_apply(&manifest), 1_721_000_001)
            .await
            .unwrap();
        let (general, deep) = verification_reports(&db).await;
        db.conn
            .lock()
            .await
            .execute("UPDATE memories SET title='later' WHERE id='row-other'", ())
            .await
            .unwrap();

        let result = record_repair_verification(
            &db,
            &store,
            exact_verify(&manifest, &apply_receipt, general, deep),
            None,
            1_721_000_002,
        )
        .await;

        assert!(matches!(
            result,
            Err(WenlanError::Conflict(message)) if message == "repair_verification_reports_stale"
        ));
    }

    #[tokio::test]
    async fn verification_rejects_reports_after_page_projection_changes() {
        let (db, _db_dir, repair_root, manifest) = prepared_fixture().await;
        let store = RepairArtifactStore::new(repair_root.path().to_path_buf());
        let apply_receipt = apply_repair(&db, &store, exact_apply(&manifest), 1_721_000_001)
            .await
            .unwrap();
        let page_root = tempfile::tempdir().unwrap();
        let (general, deep) = verification_reports_at(&db, Some(page_root.path())).await;
        std::fs::write(page_root.path().join("changed.md"), "# changed").unwrap();

        let result = record_repair_verification(
            &db,
            &store,
            exact_verify(&manifest, &apply_receipt, general, deep),
            Some(page_root.path()),
            1_721_000_002,
        )
        .await;

        assert!(matches!(
            result,
            Err(WenlanError::Conflict(message)) if message.starts_with("repair_verification_reports_stale")
        ));
    }

    #[tokio::test]
    async fn verification_rejects_new_actionable_check() {
        let (db, _db_dir, repair_root, manifest) = prepared_fixture().await;
        let store = RepairArtifactStore::new(repair_root.path().to_path_buf());
        let apply_receipt = apply_repair(&db, &store, exact_apply(&manifest), 1_721_000_001)
            .await
            .unwrap();
        let (general, deep) = verification_reports(&db).await;
        let mut value = serde_json::to_value(deep).unwrap();
        let checks = value["checks"].as_array_mut().unwrap();
        let injected = checks
            .iter_mut()
            .find(|check| {
                check["check_id"] != REPAIR_CLASSIFICATION_CHECK_ID
                    && check["outcome"] == "pass"
                    && check["gate_effect"] == "actionable"
            })
            .unwrap();
        injected["outcome"] = serde_json::json!("finding");
        injected["severity"] = serde_json::json!("warning");
        injected["applicability"] = serde_json::json!("applicable");
        injected["precondition"] = serde_json::json!("ready");
        let passed = checks
            .iter()
            .filter(|check| check["outcome"] == "pass")
            .count();
        let findings = checks
            .iter()
            .filter(|check| check["outcome"] == "finding")
            .count();
        let actionable_findings = checks
            .iter()
            .filter(|check| check["outcome"] == "finding" && check["gate_effect"] == "actionable")
            .count();
        value["totals"] = serde_json::json!({
            "checks": checks.len(),
            "passed": passed,
            "findings": findings,
            "actionable_findings": actionable_findings,
            "advisory_findings": findings - actionable_findings,
            "incomplete": 0
        });
        let deep = serde_json::from_value(value).unwrap();

        let result = record_repair_verification(
            &db,
            &store,
            exact_verify(&manifest, &apply_receipt, general, deep),
            None,
            1_721_000_002,
        )
        .await;

        assert!(matches!(
            result,
            Err(WenlanError::Validation(message)) if message == "repair_new_actionable_finding"
        ));
    }

    #[tokio::test]
    async fn verification_accepts_unchanged_unrelated_deep_incomplete_baseline() {
        let (db, _db_dir) = fixture().await;
        let repair_root = tempfile::tempdir().unwrap();
        let store = RepairArtifactStore::new(repair_root.path().to_path_buf());
        let request = with_incomplete_deep(request(&db).await, false).unwrap();
        let manifest = prepare_memory_reclassification(&db, &store, request, 1_721_000_000)
            .await
            .unwrap();
        let apply_receipt = apply_repair(&db, &store, exact_apply(&manifest), 1_721_000_001)
            .await
            .unwrap();
        let (general, deep) = verification_reports(&db).await;
        let deep = fail_deep_check(deep, false);

        let receipt = record_repair_verification(
            &db,
            &store,
            exact_verify(&manifest, &apply_receipt, general, deep),
            None,
            1_721_000_002,
        )
        .await
        .unwrap();

        assert_eq!(receipt.manifest_id(), manifest.manifest_id());
    }

    #[tokio::test]
    async fn verification_rejects_new_unrelated_deep_incomplete_check() {
        let (db, _db_dir, repair_root, manifest) = prepared_fixture().await;
        let store = RepairArtifactStore::new(repair_root.path().to_path_buf());
        let apply_receipt = apply_repair(&db, &store, exact_apply(&manifest), 1_721_000_001)
            .await
            .unwrap();
        let (general, deep) = verification_reports(&db).await;
        let deep = fail_deep_check(deep, false);

        let result = record_repair_verification(
            &db,
            &store,
            exact_verify(&manifest, &apply_receipt, general, deep),
            None,
            1_721_000_002,
        )
        .await;

        assert!(matches!(
            result,
            Err(WenlanError::Validation(message)) if message == "repair_new_incomplete_check"
        ));
    }

    #[test]
    fn semantic_digest_binds_kind_and_durable_id() {
        assert_eq!(
            semantic_record_digest("memory", "mem_target"),
            semantic_record_digest("memory", "mem_target")
        );
        assert_ne!(
            semantic_record_digest("memory", "mem_target"),
            semantic_record_digest("entity", "mem_target")
        );
    }
}
