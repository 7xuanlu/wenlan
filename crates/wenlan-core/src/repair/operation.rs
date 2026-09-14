// SPDX-License-Identifier: Apache-2.0
//! Frozen per-manifest repair operation status and cancellation.
//!
//! These methods never apply a repair. The existing approval phrase is only
//! exact-manifest binding here, never an apply action: every entry point
//! loads the canonical manifest and compares
//! [`ApplyRepairRequest::approved_manifest_digest`] before returning any
//! state. A caller timing out or closing UI is never proof of cancellation;
//! only an authenticated [`CancelMarker`] cancels, and only a manifest with
//! no possibly-committed or in-flight apply artifacts can publish one.
//!
//! Future client operation-ID prepare lookup is separate work and is not
//! implemented here.

use super::{
    ensure_repair_artifacts_supported, publish_no_replace, read_bounded_file, repair_digest,
    sync_dir, write_private_file, RepairArtifactStore, APPLY_RECEIPT_FILE,
    APPLY_RECEIPT_PENDING_FILE, REVIEW_COMPLETION_MARKER_FILE,
    STALE_PAGE_PROJECTION_APPLY_JOURNAL_FILE, STALE_PAGE_PROJECTION_APPLY_JOURNAL_PENDING_FILE,
    VERIFICATION_RECEIPT_FILE,
};
use crate::error::WenlanError;
use serde::{Deserialize, Serialize};
use std::fs;
use uuid::Uuid;
use wenlan_types::repair::{ApplyRepairRequest, RepairDigest, RepairManifest};
use wenlan_types::repair_operation::{RepairOperationState, RepairOperationStatus};

const CANCELLATION_FILE: &str = "cancelled.json";
const CANCELLATION_MARKER_SCHEMA_VERSION: u16 = 1;
const CANCELLATION_MARKER_MAX_BYTES: u64 = 64 * 1024;

/// Immutable cancellation marker bound to one exact manifest. Published with
/// no-replace atomic publication, so the first writer wins and every later
/// reader observes the same timestamp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CancelMarker {
    marker_schema_version: u16,
    manifest_id: String,
    manifest_digest: RepairDigest,
    cancelled_at: i64,
    marker_digest: RepairDigest,
}

#[derive(Serialize)]
struct CancelMarkerDraft<'a> {
    marker_schema_version: u16,
    manifest_id: &'a str,
    manifest_digest: &'a RepairDigest,
    cancelled_at: i64,
}

fn cancel_marker_mismatch() -> WenlanError {
    WenlanError::Validation("repair_operation_cancel_invalid".to_string())
}

fn operation_conflict() -> WenlanError {
    WenlanError::Conflict("repair_operation_conflict".to_string())
}

impl CancelMarker {
    fn new(manifest: &RepairManifest, cancelled_at: i64) -> Result<Self, WenlanError> {
        if cancelled_at <= 0 {
            return Err(WenlanError::Validation(
                "invalid_repair_cancelled_at".to_string(),
            ));
        }
        let draft = CancelMarkerDraft {
            marker_schema_version: CANCELLATION_MARKER_SCHEMA_VERSION,
            manifest_id: manifest.manifest_id(),
            manifest_digest: manifest.manifest_digest(),
            cancelled_at,
        };
        let marker_digest = repair_digest(&serde_json::to_vec(&draft)?);
        Ok(Self {
            marker_schema_version: draft.marker_schema_version,
            manifest_id: draft.manifest_id.to_string(),
            manifest_digest: draft.manifest_digest.clone(),
            cancelled_at,
            marker_digest,
        })
    }

    fn verify(self, manifest: &RepairManifest) -> Result<i64, WenlanError> {
        let draft = CancelMarkerDraft {
            marker_schema_version: self.marker_schema_version,
            manifest_id: &self.manifest_id,
            manifest_digest: &self.manifest_digest,
            cancelled_at: self.cancelled_at,
        };
        if self.marker_schema_version != CANCELLATION_MARKER_SCHEMA_VERSION
            || self.manifest_id != manifest.manifest_id()
            || self.manifest_digest != *manifest.manifest_digest()
            || self.cancelled_at <= 0
            || repair_digest(&serde_json::to_vec(&draft)?) != self.marker_digest
        {
            return Err(cancel_marker_mismatch());
        }
        Ok(self.cancelled_at)
    }
}

fn artifact_present(path: &std::path::Path) -> Result<bool, WenlanError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn operation_lock_contended(error: &WenlanError) -> bool {
    matches!(
        error,
        WenlanError::Conflict(message) if message == "repair_operation_in_progress"
    )
}

fn prepared_status(manifest: &RepairManifest) -> RepairOperationStatus {
    RepairOperationStatus {
        manifest_id: manifest.manifest_id().to_string(),
        manifest_digest: manifest.manifest_digest().clone(),
        state: RepairOperationState::Prepared,
    }
}

fn in_progress_status(manifest: &RepairManifest) -> RepairOperationStatus {
    RepairOperationStatus {
        manifest_id: manifest.manifest_id().to_string(),
        manifest_digest: manifest.manifest_digest().clone(),
        state: RepairOperationState::InProgress,
    }
}

fn cancelled_status(manifest: &RepairManifest, cancelled_at: i64) -> RepairOperationStatus {
    RepairOperationStatus {
        manifest_id: manifest.manifest_id().to_string(),
        manifest_digest: manifest.manifest_digest().clone(),
        state: RepairOperationState::Cancelled { cancelled_at },
    }
}

enum LockedOperation {
    Verified {
        apply_receipt: Box<wenlan_types::repair::RepairApplyReceipt>,
        verification_receipt: Box<wenlan_types::repair::RepairVerificationReceipt>,
    },
    AppliedUnverified {
        apply_receipt: Box<wenlan_types::repair::RepairApplyReceipt>,
    },
    Indeterminate,
    Prepared,
    Cancelled {
        cancelled_at: i64,
    },
}

impl LockedOperation {
    fn into_status(self, manifest: &RepairManifest) -> RepairOperationStatus {
        let state = match self {
            Self::Verified {
                apply_receipt,
                verification_receipt,
            } => RepairOperationState::Verified {
                apply_receipt,
                verification_receipt,
            },
            Self::AppliedUnverified { apply_receipt } => {
                RepairOperationState::AppliedUnverified { apply_receipt }
            }
            Self::Indeterminate => RepairOperationState::Indeterminate,
            Self::Prepared => return prepared_status(manifest),
            Self::Cancelled { cancelled_at } => return cancelled_status(manifest, cancelled_at),
        };
        RepairOperationStatus {
            manifest_id: manifest.manifest_id().to_string(),
            manifest_digest: manifest.manifest_digest().clone(),
            state,
        }
    }
}

impl RepairArtifactStore {
    fn cancel_marker_path(
        &self,
        manifest: &RepairManifest,
    ) -> Result<std::path::PathBuf, WenlanError> {
        Ok(self
            .manifest_dir(manifest.manifest_id())?
            .join(CANCELLATION_FILE))
    }

    fn cancel_marker_present(&self, manifest: &RepairManifest) -> Result<bool, WenlanError> {
        artifact_present(&self.cancel_marker_path(manifest)?)
    }

    /// Read and fully validate the cancellation marker. A missing marker is
    /// `Ok(None)`; a corrupt or mis-bound marker fails closed.
    fn read_cancel_marker(&self, manifest: &RepairManifest) -> Result<Option<i64>, WenlanError> {
        let path = self.cancel_marker_path(manifest)?;
        if !artifact_present(&path)? {
            return Ok(None);
        }
        let marker = serde_json::from_slice::<CancelMarker>(&read_bounded_file(
            &path,
            CANCELLATION_MARKER_MAX_BYTES,
        )?)
        .map_err(|_| cancel_marker_mismatch())?;
        marker.verify(manifest).map(Some)
    }

    fn reject_cancel_marker_with_apply_artifacts(
        &self,
        manifest: &RepairManifest,
    ) -> Result<(), WenlanError> {
        if self.cancel_marker_present(manifest)? {
            return Err(operation_conflict());
        }
        Ok(())
    }

    /// Classify the operation while the caller holds the per-manifest
    /// operation lock. Terminal verification is authenticated first via the
    /// canonical accessor (which also enforces the retained
    /// verification-plus-post-COMMIT marker semantics), then the applied
    /// receipt, then incomplete apply pending/journal artifacts. A
    /// cancellation marker coexisting with any apply artifact is
    /// corruption/conflict, never cancelled success.
    fn locked_operation(&self, manifest: &RepairManifest) -> Result<LockedOperation, WenlanError> {
        let manifest_dir = self.manifest_dir(manifest.manifest_id())?;
        let apply_exists = artifact_present(&manifest_dir.join(APPLY_RECEIPT_FILE))?;
        if apply_exists {
            self.reject_cancel_marker_with_apply_artifacts(manifest)?;
            if let Some(verification_receipt) =
                self.completed_verification_receipt(manifest.manifest_id())?
            {
                let apply_receipt = Box::new(self.load_apply_receipt(manifest)?);
                return Ok(LockedOperation::Verified {
                    apply_receipt,
                    verification_receipt: Box::new(verification_receipt),
                });
            }
            let apply_receipt = Box::new(self.load_apply_receipt(manifest)?);
            return Ok(LockedOperation::AppliedUnverified { apply_receipt });
        }
        for artifact in [
            APPLY_RECEIPT_PENDING_FILE,
            STALE_PAGE_PROJECTION_APPLY_JOURNAL_FILE,
            STALE_PAGE_PROJECTION_APPLY_JOURNAL_PENDING_FILE,
            VERIFICATION_RECEIPT_FILE,
            REVIEW_COMPLETION_MARKER_FILE,
        ] {
            if artifact_present(&manifest_dir.join(artifact))? {
                self.reject_cancel_marker_with_apply_artifacts(manifest)?;
                return Ok(LockedOperation::Indeterminate);
            }
        }
        match self.read_cancel_marker(manifest)? {
            Some(cancelled_at) => Ok(LockedOperation::Cancelled { cancelled_at }),
            None => Ok(LockedOperation::Prepared),
        }
    }

    fn publish_cancel_marker(
        &self,
        manifest: &RepairManifest,
        marker: &CancelMarker,
    ) -> Result<(), WenlanError> {
        let manifest_dir = self.manifest_dir(manifest.manifest_id())?;
        let final_path = manifest_dir.join(CANCELLATION_FILE);
        if artifact_present(&final_path)? {
            // A concurrent canceller won publication. Validate the winner
            // before accepting idempotent completion; never rewrite it.
            if self.read_cancel_marker(manifest)?.is_some() {
                return Ok(());
            }
            return Err(cancel_marker_mismatch());
        }
        let temp_path = manifest_dir.join(format!(".{CANCELLATION_FILE}.tmp-{}", Uuid::new_v4()));
        let result = (|| {
            write_private_file(&temp_path, &serde_json::to_vec_pretty(marker)?)?;
            match publish_no_replace(&temp_path, &final_path, "repair_operation_cancel_exists") {
                Ok(()) => Ok(()),
                Err(WenlanError::Conflict(message))
                    if message == "repair_operation_cancel_exists" =>
                {
                    if self.read_cancel_marker(manifest)?.is_some() {
                        Ok(())
                    } else {
                        Err(cancel_marker_mismatch())
                    }
                }
                Err(error) => Err(error),
            }
        })();
        if result.is_err() && temp_path.exists() {
            let _ = fs::remove_file(&temp_path);
            let _ = sync_dir(&manifest_dir);
        }
        result
    }

    fn authenticated_manifest(
        &self,
        request: &ApplyRepairRequest,
    ) -> Result<RepairManifest, WenlanError> {
        let manifest = self.load_manifest(request.manifest_id())?;
        if manifest.manifest_digest() != request.approved_manifest_digest() {
            return Err(WenlanError::Conflict(
                "repair_approval_mismatch".to_string(),
            ));
        }
        Ok(manifest)
    }

    /// Durable status of one exact prepared repair. Takes the same
    /// per-manifest OS file lock as apply; lock contention reports
    /// `InProgress` instead of an error. Never applies the repair.
    pub fn repair_operation_status(
        &self,
        request: &ApplyRepairRequest,
    ) -> Result<RepairOperationStatus, WenlanError> {
        ensure_repair_artifacts_supported()?;
        let manifest = self.authenticated_manifest(request)?;
        let _operation_lock = match self.lock_manifest_operation(manifest.manifest_id()) {
            Ok(lock) => lock,
            Err(error) if operation_lock_contended(&error) => {
                return Ok(in_progress_status(&manifest));
            }
            Err(error) => return Err(error),
        };
        Ok(self.locked_operation(&manifest)?.into_status(&manifest))
    }

    /// Cancel a prepared repair that has no possibly-committed or in-flight
    /// apply. Applied, verified, and indeterminate operations are returned as
    /// their current status without mutating any file; lock contention
    /// reports `InProgress`. Cancelling an already-cancelled manifest is
    /// idempotent and returns the original timestamp without rewriting the
    /// marker.
    pub fn cancel_prepared_repair(
        &self,
        request: &ApplyRepairRequest,
        now_epoch: i64,
    ) -> Result<RepairOperationStatus, WenlanError> {
        ensure_repair_artifacts_supported()?;
        if now_epoch <= 0 {
            return Err(WenlanError::Validation(
                "invalid_repair_cancelled_at".to_string(),
            ));
        }
        let manifest = self.authenticated_manifest(request)?;
        let _operation_lock = match self.lock_manifest_operation(manifest.manifest_id()) {
            Ok(lock) => lock,
            Err(error) if operation_lock_contended(&error) => {
                return Ok(in_progress_status(&manifest));
            }
            Err(error) => return Err(error),
        };
        match self.locked_operation(&manifest)? {
            LockedOperation::Prepared => {
                let marker = CancelMarker::new(&manifest, now_epoch)?;
                self.publish_cancel_marker(&manifest, &marker)?;
                let cancelled_at = self
                    .read_cancel_marker(&manifest)?
                    .ok_or_else(cancel_marker_mismatch)?;
                Ok(cancelled_status(&manifest, cancelled_at))
            }
            operation => Ok(operation.into_status(&manifest)),
        }
    }

    /// Fail closed when a validated cancellation marker exists. The caller
    /// already holds the manifest operation lock; this runs before all apply
    /// writer branches and canonical writes. Daemon coordinator leases and
    /// queue/canonical DB state are intentionally untouched here.
    pub(super) fn ensure_not_cancelled(
        &self,
        manifest: &RepairManifest,
    ) -> Result<(), WenlanError> {
        if self.read_cancel_marker(manifest)?.is_some() {
            return Err(WenlanError::Conflict(
                "repair_operation_cancelled".to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fs2::FileExt as _;
    use std::fs::OpenOptions;

    const V1_MANIFEST_ID: &str = "repair_550e8400-e29b-41d4-a716-446655440000";
    const V1_MANIFEST_JSON: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../wenlan-types/testdata/repair/v1/manifest.json"
    ));
    const V1_APPLY_RECEIPT_JSON: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../wenlan-types/testdata/repair/v1/apply-receipt.json"
    ));
    const V1_VERIFICATION_RECEIPT_JSON: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../wenlan-types/testdata/repair/v1/verification-receipt.json"
    ));

    fn seeded_store() -> (tempfile::TempDir, RepairArtifactStore) {
        let root = tempfile::tempdir().unwrap();
        let store = RepairArtifactStore::new(root.path().to_path_buf());
        let dir = store.manifest_dir(V1_MANIFEST_ID).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("manifest.json"), V1_MANIFEST_JSON).unwrap();
        (root, store)
    }

    fn exact_apply(manifest: &RepairManifest) -> ApplyRepairRequest {
        ApplyRepairRequest::try_new(
            manifest.manifest_id().to_string(),
            manifest.manifest_digest().clone(),
            format!(
                "apply repair {} {}",
                manifest.manifest_id(),
                manifest.manifest_digest().as_str()
            ),
        )
        .unwrap()
    }

    fn wrong_digest_apply(manifest: &RepairManifest) -> ApplyRepairRequest {
        let wrong =
            RepairDigest::parse("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
                .unwrap();
        ApplyRepairRequest::try_new(
            manifest.manifest_id().to_string(),
            wrong.clone(),
            format!("apply repair {} {}", manifest.manifest_id(), wrong.as_str()),
        )
        .unwrap()
    }

    #[test]
    #[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
    fn prepared_cancel_roundtrip_and_restart_idempotent() {
        let (root, store) = seeded_store();
        let manifest = store.load_manifest(V1_MANIFEST_ID).unwrap();
        let request = exact_apply(&manifest);

        assert_eq!(
            store.repair_operation_status(&request).unwrap(),
            prepared_status(&manifest)
        );
        let cancelled = store
            .cancel_prepared_repair(&request, 1_721_000_100)
            .unwrap();
        assert_eq!(cancelled, cancelled_status(&manifest, 1_721_000_100));

        let marker_bytes = std::fs::read(store.cancel_marker_path(&manifest).unwrap()).unwrap();
        assert_eq!(
            store.repair_operation_status(&request).unwrap(),
            cancelled_status(&manifest, 1_721_000_100)
        );
        // A second cancellation is idempotent: same timestamp, never rewritten.
        assert_eq!(
            store
                .cancel_prepared_repair(&request, 1_721_000_200)
                .unwrap(),
            cancelled_status(&manifest, 1_721_000_100)
        );
        assert_eq!(
            std::fs::read(store.cancel_marker_path(&manifest).unwrap()).unwrap(),
            marker_bytes
        );

        // A restarted process observes the same cancellation.
        let restarted = RepairArtifactStore::new(root.path().to_path_buf());
        assert_eq!(
            restarted.repair_operation_status(&request).unwrap(),
            cancelled_status(&manifest, 1_721_000_100)
        );
        assert!(matches!(
            restarted.ensure_not_cancelled(&manifest),
            Err(WenlanError::Conflict(message)) if message == "repair_operation_cancelled"
        ));
    }

    #[test]
    #[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
    fn digest_mismatch_and_marker_corruption_fail_closed() {
        let (_root, store) = seeded_store();
        let manifest = store.load_manifest(V1_MANIFEST_ID).unwrap();
        let mismatched = wrong_digest_apply(&manifest);

        for result in [
            store.repair_operation_status(&mismatched),
            store.cancel_prepared_repair(&mismatched, 1_721_000_100),
        ] {
            assert!(matches!(
                result,
                Err(WenlanError::Conflict(message)) if message == "repair_approval_mismatch"
            ));
        }

        // An undecodable marker fails closed on every read path.
        std::fs::write(
            store.cancel_marker_path(&manifest).unwrap(),
            b"not a cancellation marker",
        )
        .unwrap();
        let request = exact_apply(&manifest);
        for result in [
            store.repair_operation_status(&request),
            store.cancel_prepared_repair(&request, 1_721_000_100),
        ] {
            assert!(matches!(
                result,
                Err(WenlanError::Validation(message))
                    if message == "repair_operation_cancel_invalid"
            ));
        }
        assert!(matches!(
            store.ensure_not_cancelled(&manifest),
            Err(WenlanError::Validation(message))
                if message == "repair_operation_cancel_invalid"
        ));

        // A marker rebound to another digest fails checksum/binding validation.
        let marker = CancelMarker::new(&manifest, 1_721_000_100).unwrap();
        let mut value = serde_json::to_value(&marker).unwrap();
        value["manifest_digest"] =
            serde_json::json!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
        std::fs::write(
            store.cancel_marker_path(&manifest).unwrap(),
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            store.repair_operation_status(&request),
            Err(WenlanError::Validation(message))
                if message == "repair_operation_cancel_invalid"
        ));
    }

    #[test]
    #[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
    fn lock_contention_reports_in_progress_without_mutation() {
        let (_root, store) = seeded_store();
        let manifest = store.load_manifest(V1_MANIFEST_ID).unwrap();
        let request = exact_apply(&manifest);
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(
                store
                    .manifest_dir(V1_MANIFEST_ID)
                    .unwrap()
                    .join(".operation.lock"),
            )
            .unwrap();
        lock_file.lock_exclusive().unwrap();

        assert_eq!(
            store.repair_operation_status(&request).unwrap(),
            in_progress_status(&manifest)
        );
        assert_eq!(
            store
                .cancel_prepared_repair(&request, 1_721_000_100)
                .unwrap(),
            in_progress_status(&manifest)
        );
        assert!(
            !store.cancel_marker_path(&manifest).unwrap().exists(),
            "contended cancel must not publish a marker"
        );
    }

    #[test]
    #[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
    fn pending_and_journal_apply_is_indeterminate_and_not_cancellable() {
        let (_root, store) = seeded_store();
        let manifest = store.load_manifest(V1_MANIFEST_ID).unwrap();
        let request = exact_apply(&manifest);
        let manifest_dir = store.manifest_dir(V1_MANIFEST_ID).unwrap();

        std::fs::write(
            manifest_dir.join(APPLY_RECEIPT_PENDING_FILE),
            b"partial pre-commit receipt",
        )
        .unwrap();
        assert_eq!(
            store.repair_operation_status(&request).unwrap(),
            LockedOperation::Indeterminate.into_status(&manifest)
        );
        assert_eq!(
            store
                .cancel_prepared_repair(&request, 1_721_000_100)
                .unwrap(),
            LockedOperation::Indeterminate.into_status(&manifest)
        );
        assert!(!store.cancel_marker_path(&manifest).unwrap().exists());
        std::fs::remove_file(manifest_dir.join(APPLY_RECEIPT_PENDING_FILE)).unwrap();

        std::fs::write(
            manifest_dir.join(STALE_PAGE_PROJECTION_APPLY_JOURNAL_PENDING_FILE),
            b"journal",
        )
        .unwrap();
        assert_eq!(
            store.repair_operation_status(&request).unwrap(),
            LockedOperation::Indeterminate.into_status(&manifest)
        );
        assert_eq!(
            store
                .cancel_prepared_repair(&request, 1_721_000_100)
                .unwrap(),
            LockedOperation::Indeterminate.into_status(&manifest)
        );
        assert!(!store.cancel_marker_path(&manifest).unwrap().exists());
    }

    #[test]
    #[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
    fn orphan_or_malformed_apply_artifacts_cannot_be_cancelled() {
        for name in [
            VERIFICATION_RECEIPT_FILE,
            REVIEW_COMPLETION_MARKER_FILE,
            APPLY_RECEIPT_PENDING_FILE,
        ] {
            let (_root, store) = seeded_store();
            let manifest = store.load_manifest(V1_MANIFEST_ID).unwrap();
            let request = exact_apply(&manifest);
            // Even an unexpected directory at an artifact path is not absence.
            fs::create_dir(store.manifest_dir(V1_MANIFEST_ID).unwrap().join(name)).unwrap();
            assert!(matches!(
                store
                    .cancel_prepared_repair(&request, 1_721_000_100)
                    .unwrap()
                    .state,
                RepairOperationState::Indeterminate
            ));
            assert!(!store.cancel_marker_present(&manifest).unwrap());
        }
    }

    #[test]
    #[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
    fn applied_and_verified_receipts_report_terminal_state() {
        let (_root, store) = seeded_store();
        let manifest = store.load_manifest(V1_MANIFEST_ID).unwrap();
        let request = exact_apply(&manifest);
        let manifest_dir = store.manifest_dir(V1_MANIFEST_ID).unwrap();

        std::fs::write(manifest_dir.join(APPLY_RECEIPT_FILE), V1_APPLY_RECEIPT_JSON).unwrap();
        let apply_receipt = Box::new(store.load_apply_receipt(&manifest).unwrap());
        assert_eq!(
            store.repair_operation_status(&request).unwrap(),
            RepairOperationStatus {
                manifest_id: manifest.manifest_id().to_string(),
                manifest_digest: manifest.manifest_digest().clone(),
                state: RepairOperationState::AppliedUnverified {
                    apply_receipt: apply_receipt.clone(),
                },
            }
        );
        // A committed apply cannot be cancelled; the status is returned unchanged.
        assert_eq!(
            store
                .cancel_prepared_repair(&request, 1_721_000_100)
                .unwrap()
                .state,
            RepairOperationState::AppliedUnverified { apply_receipt }
        );
        assert!(!store.cancel_marker_path(&manifest).unwrap().exists());

        std::fs::write(
            manifest_dir.join("verification-receipt.json"),
            V1_VERIFICATION_RECEIPT_JSON,
        )
        .unwrap();
        let terminal = store
            .completed_verification_receipt(V1_MANIFEST_ID)
            .unwrap();
        assert!(
            terminal.is_some(),
            "canonical accessor must authenticate the terminal receipt"
        );
        let status = store.repair_operation_status(&request).unwrap();
        match status.state {
            RepairOperationState::Verified {
                apply_receipt,
                verification_receipt,
            } => {
                assert_eq!(*verification_receipt, terminal.unwrap());
                assert_eq!(*apply_receipt, store.load_apply_receipt(&manifest).unwrap());
            }
            other => panic!("expected verified terminal state, got {other:?}"),
        }
    }

    #[test]
    #[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
    fn cancel_guard_and_marker_conflict_fail_closed() {
        let (_root, store) = seeded_store();
        let manifest = store.load_manifest(V1_MANIFEST_ID).unwrap();

        assert!(store.ensure_not_cancelled(&manifest).is_ok());

        let request = exact_apply(&manifest);
        store
            .cancel_prepared_repair(&request, 1_721_000_100)
            .unwrap();
        assert!(matches!(
            store.ensure_not_cancelled(&manifest),
            Err(WenlanError::Conflict(message)) if message == "repair_operation_cancelled"
        ));

        // A marker coexisting with apply artifacts is conflict, never success.
        let manifest_dir = store.manifest_dir(V1_MANIFEST_ID).unwrap();
        std::fs::write(manifest_dir.join(APPLY_RECEIPT_FILE), V1_APPLY_RECEIPT_JSON).unwrap();
        for result in [
            store.repair_operation_status(&request),
            store.cancel_prepared_repair(&request, 1_721_000_200),
        ] {
            assert!(matches!(
                result,
                Err(WenlanError::Conflict(message)) if message == "repair_operation_conflict"
            ));
        }
    }
}
