// SPDX-License-Identifier: Apache-2.0
//! Durable client-operation recovery for repair preparation.
//!
//! A client picks a durable operation ID *before* the manifest ID is known,
//! so a lost prepare response can be recovered by re-presenting the exact
//! typed request. Every entry validates the request, digests the complete
//! typed payload (ID, scope and choice), and authenticates immutable,
//! versioned, checksummed records. One OS file lock per operation serializes
//! producers; contention never blocks and reports `InProgress`.
//!
//! A bound store retains the operation lock in a shared `Arc<File>`, so every
//! clone keeps lock ownership until the last clone drops. Preparation only
//! creates artifacts; it never touches the canonical DB, the queue, or daemon
//! coordinator leases. Cancellation can never falsely cancel an in-flight
//! task and never reports top-level `Cancelled` for a published manifest.

use super::{
    ensure_private_dir, ensure_repair_artifacts_supported, publish_no_replace, read_bounded_file,
    repair_digest, set_private_file_permissions, sync_dir, write_private_file, RepairArtifactStore,
    OPERATION_LOCK_FILE,
};
use crate::error::WenlanError;
use fs2::FileExt as _;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    sync::Arc,
};
use uuid::Uuid;
use wenlan_types::{
    repair::{ApplyRepairRequest, RepairDigest, RepairManifest},
    repair_operation::RepairOperationStatus,
    repair_prepare_operation::{
        RepairPrepareOperationRequest, RepairPrepareOperationState, RepairPrepareOperationStatus,
    },
};

const PREPARE_OPERATIONS_DIR: &str = ".prepare-operations";
const PREPARE_REQUEST_FILE: &str = "request.json";
const PREPARE_RESULT_FILE: &str = "result.json";
const PREPARE_CANCEL_FILE: &str = "cancelled.json";
const PREPARE_RECORD_MAX_BYTES: u64 = 64 * 1024;
const PREPARE_SCHEMA_VERSION: u16 = 1;

fn prepare_record_invalid() -> WenlanError {
    WenlanError::Validation("repair_prepare_operation_invalid".to_string())
}

fn prepare_operation_conflict() -> WenlanError {
    WenlanError::Conflict("repair_prepare_operation_conflict".to_string())
}

/// A cancellation marker coexisting with a published manifest is corruption,
/// never safely-cancelled success (mirrors the manifest-level marker rule).
fn prepare_manifest_conflict() -> WenlanError {
    WenlanError::Conflict("repair_operation_conflict".to_string())
}

/// Lock ownership retained by a bound store. Every clone of the store keeps
/// the same open file description alive, so the exclusive lock is held until
/// the last clone drops (or the producer process dies).
pub(super) struct PrepareBinding {
    operation_id: String,
    request_digest: RepairDigest,
    _operation_lock: Arc<File>,
}

impl Clone for PrepareBinding {
    fn clone(&self) -> Self {
        Self {
            operation_id: self.operation_id.clone(),
            request_digest: self.request_digest.clone(),
            _operation_lock: Arc::clone(&self._operation_lock),
        }
    }
}

impl std::fmt::Debug for PrepareBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrepareBinding")
            .field("operation_id", &self.operation_id)
            .field("request_digest", &self.request_digest)
            .finish_non_exhaustive()
    }
}

#[derive(Serialize)]
struct PrepareRequestDraft<'a> {
    schema_version: u16,
    operation_id: &'a str,
    request: &'a RepairPrepareOperationRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredPrepareRequest {
    schema_version: u16,
    operation_id: String,
    request: RepairPrepareOperationRequest,
    request_digest: RepairDigest,
}

#[derive(Serialize)]
struct PrepareResultDraft<'a> {
    schema_version: u16,
    operation_id: &'a str,
    request_digest: &'a RepairDigest,
    manifest_id: &'a str,
    manifest_digest: &'a RepairDigest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredPrepareResult {
    schema_version: u16,
    operation_id: String,
    request_digest: RepairDigest,
    manifest_id: String,
    manifest_digest: RepairDigest,
    pointer_digest: RepairDigest,
}

#[derive(Serialize)]
struct PrepareCancelDraft<'a> {
    schema_version: u16,
    operation_id: &'a str,
    request_digest: &'a RepairDigest,
    cancelled_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredPrepareCancel {
    schema_version: u16,
    operation_id: String,
    request_digest: RepairDigest,
    cancelled_at: i64,
    marker_digest: RepairDigest,
}

impl StoredPrepareRequest {
    fn new(
        operation_id: String,
        request: RepairPrepareOperationRequest,
    ) -> Result<Self, WenlanError> {
        let draft = PrepareRequestDraft {
            schema_version: PREPARE_SCHEMA_VERSION,
            operation_id: &operation_id,
            request: &request,
        };
        let request_digest = repair_digest(&serde_json::to_vec(&draft)?);
        Ok(Self {
            schema_version: PREPARE_SCHEMA_VERSION,
            operation_id,
            request,
            request_digest,
        })
    }

    fn verify(self, operation_id: &str) -> Result<Self, WenlanError> {
        let draft = PrepareRequestDraft {
            schema_version: self.schema_version,
            operation_id: &self.operation_id,
            request: &self.request,
        };
        if self.schema_version != PREPARE_SCHEMA_VERSION
            || self.operation_id != operation_id
            || repair_digest(&serde_json::to_vec(&draft)?) != self.request_digest
        {
            return Err(prepare_record_invalid());
        }
        Ok(self)
    }
}

impl StoredPrepareResult {
    fn new(binding: &PrepareBinding, manifest: &RepairManifest) -> Result<Self, WenlanError> {
        let draft = PrepareResultDraft {
            schema_version: PREPARE_SCHEMA_VERSION,
            operation_id: &binding.operation_id,
            request_digest: &binding.request_digest,
            manifest_id: manifest.manifest_id(),
            manifest_digest: manifest.manifest_digest(),
        };
        let pointer_digest = repair_digest(&serde_json::to_vec(&draft)?);
        Ok(Self {
            schema_version: PREPARE_SCHEMA_VERSION,
            operation_id: binding.operation_id.clone(),
            request_digest: binding.request_digest.clone(),
            manifest_id: manifest.manifest_id().to_string(),
            manifest_digest: manifest.manifest_digest().clone(),
            pointer_digest,
        })
    }

    fn verify(
        self,
        operation_id: &str,
        request_digest: &RepairDigest,
    ) -> Result<Self, WenlanError> {
        let draft = PrepareResultDraft {
            schema_version: self.schema_version,
            operation_id: &self.operation_id,
            request_digest: &self.request_digest,
            manifest_id: &self.manifest_id,
            manifest_digest: &self.manifest_digest,
        };
        if self.schema_version != PREPARE_SCHEMA_VERSION
            || self.operation_id != operation_id
            || self.request_digest != *request_digest
            || repair_digest(&serde_json::to_vec(&draft)?) != self.pointer_digest
        {
            return Err(prepare_record_invalid());
        }
        Ok(self)
    }
}

impl StoredPrepareCancel {
    fn new(
        operation_id: String,
        request_digest: RepairDigest,
        cancelled_at: i64,
    ) -> Result<Self, WenlanError> {
        if cancelled_at <= 0 {
            return Err(WenlanError::Validation(
                "invalid_repair_cancelled_at".to_string(),
            ));
        }
        let draft = PrepareCancelDraft {
            schema_version: PREPARE_SCHEMA_VERSION,
            operation_id: &operation_id,
            request_digest: &request_digest,
            cancelled_at,
        };
        let marker_digest = repair_digest(&serde_json::to_vec(&draft)?);
        Ok(Self {
            schema_version: PREPARE_SCHEMA_VERSION,
            operation_id,
            request_digest,
            cancelled_at,
            marker_digest,
        })
    }

    fn verify(self, operation_id: &str, request_digest: &RepairDigest) -> Result<i64, WenlanError> {
        let draft = PrepareCancelDraft {
            schema_version: self.schema_version,
            operation_id: &self.operation_id,
            request_digest: &self.request_digest,
            cancelled_at: self.cancelled_at,
        };
        if self.schema_version != PREPARE_SCHEMA_VERSION
            || self.operation_id != operation_id
            || self.request_digest != *request_digest
            || self.cancelled_at <= 0
            || repair_digest(&serde_json::to_vec(&draft)?) != self.marker_digest
        {
            return Err(prepare_record_invalid());
        }
        Ok(self.cancelled_at)
    }
}

fn artifact_present(path: &Path) -> Result<bool, WenlanError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn prepare_lock_contended(error: &WenlanError) -> bool {
    matches!(
        error,
        WenlanError::Conflict(message) if message == "repair_operation_in_progress"
    )
}

fn not_started(operation_id: &str) -> RepairPrepareOperationStatus {
    RepairPrepareOperationStatus {
        operation_id: operation_id.to_string(),
        state: RepairPrepareOperationState::NotStarted,
    }
}

fn prepare_in_progress(operation_id: &str) -> RepairPrepareOperationStatus {
    RepairPrepareOperationStatus {
        operation_id: operation_id.to_string(),
        state: RepairPrepareOperationState::InProgress,
    }
}

fn interrupted(operation_id: &str) -> RepairPrepareOperationStatus {
    RepairPrepareOperationStatus {
        operation_id: operation_id.to_string(),
        state: RepairPrepareOperationState::Interrupted,
    }
}

fn prepare_cancelled(operation_id: &str, cancelled_at: i64) -> RepairPrepareOperationStatus {
    RepairPrepareOperationStatus {
        operation_id: operation_id.to_string(),
        state: RepairPrepareOperationState::Cancelled { cancelled_at },
    }
}

fn ready(
    operation_id: &str,
    manifest: RepairManifest,
    operation: RepairOperationStatus,
) -> RepairPrepareOperationStatus {
    RepairPrepareOperationStatus {
        operation_id: operation_id.to_string(),
        state: RepairPrepareOperationState::Ready {
            manifest: Box::new(manifest),
            operation: Box::new(operation),
        },
    }
}

fn exact_apply_binding(manifest: &RepairManifest) -> Result<ApplyRepairRequest, WenlanError> {
    ApplyRepairRequest::try_new(
        manifest.manifest_id().to_string(),
        manifest.manifest_digest().clone(),
        format!(
            "apply repair {} {}",
            manifest.manifest_id(),
            manifest.manifest_digest().as_str()
        ),
    )
    .map_err(|error| WenlanError::Validation(error.to_string()))
}

enum ResolvedPrepare {
    Cancelled {
        cancelled_at: i64,
    },
    Ready {
        manifest: Box<RepairManifest>,
        operation: RepairOperationStatus,
    },
    Interrupted,
}

impl ResolvedPrepare {
    fn into_status(self, operation_id: &str) -> RepairPrepareOperationStatus {
        match self {
            Self::Cancelled { cancelled_at } => prepare_cancelled(operation_id, cancelled_at),
            Self::Ready {
                manifest,
                operation,
            } => ready(operation_id, *manifest, operation),
            Self::Interrupted => interrupted(operation_id),
        }
    }
}

/// What a producer does with a fresh operation: run preparation with this store.
#[derive(Debug, Clone)]
pub enum BeginPrepareOperation {
    Run(RepairArtifactStore),
    Existing(RepairPrepareOperationStatus),
}

impl RepairArtifactStore {
    fn prepare_op_dir(&self, operation_id: &str) -> Result<PathBuf, WenlanError> {
        // Defense in depth: the validated ID shape (lowercase hex + hyphens)
        // can never escape its directory.
        let id = operation_id.as_bytes();
        let shaped = id.len() == 36
            && id.iter().enumerate().all(|(i, b)| {
                if [8, 13, 18, 23].contains(&i) {
                    *b == b'-'
                } else {
                    b.is_ascii_digit() || (b'a'..=b'f').contains(b)
                }
            });
        if !shaped {
            return Err(WenlanError::Validation(
                "invalid_repair_prepare_operation_id".to_string(),
            ));
        }
        Ok(self.root().join(PREPARE_OPERATIONS_DIR).join(operation_id))
    }

    fn ensure_prepare_op_dir(&self, operation_id: &str) -> Result<PathBuf, WenlanError> {
        let operations_dir = self.root().join(PREPARE_OPERATIONS_DIR);
        ensure_private_dir(&operations_dir)?;
        let op_dir = self.prepare_op_dir(operation_id)?;
        ensure_private_dir(&op_dir)?;
        // Sync directory entries before admitting work or acknowledging a
        // cancellation. Syncing request.json's containing directory alone
        // does not make its own entry in .prepare-operations durable.
        sync_dir(&operations_dir)?;
        sync_dir(self.root())?;
        if let Some(parent) = self
            .root()
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
        {
            sync_dir(parent)?;
        }
        Ok(op_dir)
    }

    fn read_prepare_record<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, WenlanError> {
        if !artifact_present(path)? {
            return Ok(None);
        }
        let record =
            serde_json::from_slice::<T>(&read_bounded_file(path, PREPARE_RECORD_MAX_BYTES)?)
                .map_err(|_| prepare_record_invalid())?;
        Ok(Some(record))
    }

    fn publish_prepare_request(
        &self,
        op_dir: &Path,
        stored: &StoredPrepareRequest,
    ) -> Result<(), WenlanError> {
        let bytes = serde_json::to_vec_pretty(stored)?;
        if bytes.len() as u64 > PREPARE_RECORD_MAX_BYTES {
            return Err(prepare_record_invalid());
        }
        let temporary = op_dir.join(format!(".request.tmp-{}", Uuid::new_v4()));
        let result = (|| {
            write_private_file(&temporary, &bytes)?;
            publish_no_replace(
                &temporary,
                &op_dir.join(PREPARE_REQUEST_FILE),
                "repair_prepare_request_exists",
            )
        })();
        if result.is_err() && temporary.exists() {
            let _ = fs::remove_file(&temporary);
            let _ = sync_dir(op_dir);
        }
        result
    }

    fn read_prepare_request(
        &self,
        op_dir: &Path,
        operation_id: &str,
    ) -> Result<Option<StoredPrepareRequest>, WenlanError> {
        match Self::read_prepare_record::<StoredPrepareRequest>(&op_dir.join(PREPARE_REQUEST_FILE))?
        {
            Some(record) => Ok(Some(record.verify(operation_id)?)),
            None => Ok(None),
        }
    }

    fn read_prepare_result(
        &self,
        op_dir: &Path,
        operation_id: &str,
        request_digest: &RepairDigest,
    ) -> Result<Option<StoredPrepareResult>, WenlanError> {
        match Self::read_prepare_record::<StoredPrepareResult>(&op_dir.join(PREPARE_RESULT_FILE))? {
            Some(record) => Ok(Some(record.verify(operation_id, request_digest)?)),
            None => Ok(None),
        }
    }

    fn read_prepare_cancel(
        &self,
        op_dir: &Path,
        operation_id: &str,
        request_digest: &RepairDigest,
    ) -> Result<Option<i64>, WenlanError> {
        match Self::read_prepare_record::<StoredPrepareCancel>(&op_dir.join(PREPARE_CANCEL_FILE))? {
            Some(record) => Ok(Some(record.verify(operation_id, request_digest)?)),
            None => Ok(None),
        }
    }

    fn lock_prepare_operation(&self, op_dir: &Path) -> Result<File, WenlanError> {
        let path = op_dir.join(OPERATION_LOCK_FILE);
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
        Ok(file)
    }

    /// Publish the immutable result pointer for a bound store. No-op when
    /// unbound. Called from `persist_prepared` after staging/sync and before
    /// the manifest directory becomes visible, so no published manifest ever
    /// lacks a durable pointer. An existing pointer must bind the exact same
    /// manifest and request; it is never replaced.
    pub(super) fn bind_prepared_result(
        &self,
        manifest: &RepairManifest,
    ) -> Result<(), WenlanError> {
        let Some(binding) = self.prepare_binding.as_ref() else {
            return Ok(());
        };
        let op_dir = self.prepare_op_dir(&binding.operation_id)?;
        match self.read_prepare_request(&op_dir, &binding.operation_id)? {
            Some(stored) if stored.request_digest == binding.request_digest => {}
            _ => return Err(prepare_operation_conflict()),
        }
        if self
            .read_prepare_cancel(&op_dir, &binding.operation_id, &binding.request_digest)?
            .is_some()
        {
            return Err(prepare_operation_conflict());
        }
        let expected = StoredPrepareResult::new(binding, manifest)?;
        let final_path = op_dir.join(PREPARE_RESULT_FILE);
        if artifact_present(&final_path)? {
            return match self.read_prepare_result(
                &op_dir,
                &binding.operation_id,
                &binding.request_digest,
            )? {
                Some(existing) if existing == expected => Ok(()),
                _ => Err(prepare_operation_conflict()),
            };
        }
        let temp_path = op_dir.join(format!(".{PREPARE_RESULT_FILE}.tmp-{}", Uuid::new_v4()));
        let result = (|| {
            write_private_file(&temp_path, &serde_json::to_vec_pretty(&expected)?)?;
            match publish_no_replace(&temp_path, &final_path, "repair_prepare_result_exists") {
                Ok(()) => Ok(()),
                Err(WenlanError::Conflict(message))
                    if message == "repair_prepare_result_exists" =>
                {
                    match self.read_prepare_result(
                        &op_dir,
                        &binding.operation_id,
                        &binding.request_digest,
                    )? {
                        Some(existing) if existing == expected => Ok(()),
                        _ => Err(prepare_operation_conflict()),
                    }
                }
                Err(error) => Err(error),
            }
        })();
        if result.is_err() && temp_path.exists() {
            let _ = fs::remove_file(&temp_path);
            let _ = sync_dir(&op_dir);
        }
        result
    }

    /// Resolve a pointer to its canonical manifest. A missing manifest means
    /// publication has not occurred (`Ok(None)`); any corruption or digest
    /// mismatch fails closed. Manifest content is only ever loaded through
    /// the canonical loader.
    fn resolve_pointer_manifest(
        &self,
        pointer: &StoredPrepareResult,
    ) -> Result<Option<RepairManifest>, WenlanError> {
        if !artifact_present(&self.manifest_dir(&pointer.manifest_id)?)? {
            return Ok(None);
        }
        let manifest = self.load_manifest(&pointer.manifest_id)?;
        if manifest.manifest_id() != pointer.manifest_id
            || *manifest.manifest_digest() != pointer.manifest_digest
        {
            return Err(prepare_record_invalid());
        }
        Ok(Some(manifest))
    }

    /// Caller holds the per-operation lock. The stored request is already
    /// authenticated; result and cancel records are checksum-verified here
    /// before any status is derived from them.
    fn locked_prepare_status(
        &self,
        operation_id: &str,
        request_digest: &RepairDigest,
    ) -> Result<ResolvedPrepare, WenlanError> {
        let op_dir = self.prepare_op_dir(operation_id)?;
        let cancel = self.read_prepare_cancel(&op_dir, operation_id, request_digest)?;
        if let Some(pointer) = self.read_prepare_result(&op_dir, operation_id, request_digest)? {
            if let Some(manifest) = self.resolve_pointer_manifest(&pointer)? {
                if cancel.is_some() {
                    return Err(prepare_manifest_conflict());
                }
                let operation = self.repair_operation_status(&exact_apply_binding(&manifest)?)?;
                return Ok(ResolvedPrepare::Ready {
                    manifest: Box::new(manifest),
                    operation,
                });
            }
        }
        match cancel {
            Some(cancelled_at) => Ok(ResolvedPrepare::Cancelled { cancelled_at }),
            None => Ok(ResolvedPrepare::Interrupted),
        }
    }

    /// Publish the preparation cancellation marker without replacement. An
    /// existing marker wins unchanged (original timestamp, never rewritten).
    fn publish_prepare_cancel(
        &self,
        op_dir: &Path,
        operation_id: &str,
        request_digest: &RepairDigest,
        cancelled_at: i64,
    ) -> Result<i64, WenlanError> {
        let marker = StoredPrepareCancel::new(
            operation_id.to_string(),
            request_digest.clone(),
            cancelled_at,
        )?;
        let final_path = op_dir.join(PREPARE_CANCEL_FILE);
        if artifact_present(&final_path)? {
            return self
                .read_prepare_cancel(op_dir, operation_id, request_digest)?
                .ok_or_else(prepare_record_invalid);
        }
        let temp_path = op_dir.join(format!(".{PREPARE_CANCEL_FILE}.tmp-{}", Uuid::new_v4()));
        let result = (|| {
            write_private_file(&temp_path, &serde_json::to_vec_pretty(&marker)?)?;
            match publish_no_replace(&temp_path, &final_path, "repair_prepare_cancel_exists") {
                Ok(()) => Ok(cancelled_at),
                Err(WenlanError::Conflict(message))
                    if message == "repair_prepare_cancel_exists" =>
                {
                    self.read_prepare_cancel(op_dir, operation_id, request_digest)?
                        .ok_or_else(prepare_record_invalid)
                }
                Err(error) => Err(error),
            }
        })();
        if result.is_err() && temp_path.exists() {
            let _ = fs::remove_file(&temp_path);
            let _ = sync_dir(op_dir);
        }
        result
    }

    /// Start or recover a client prepare operation. A new ID persists the
    /// request and returns a bound store (with the operation lock) for the
    /// single producer run. An existing ID never re-runs: it returns the
    /// durable status instead. Lock contention reports `InProgress`.
    pub fn begin_prepare_operation(
        &self,
        request: &RepairPrepareOperationRequest,
    ) -> Result<BeginPrepareOperation, WenlanError> {
        request.validate().map_err(WenlanError::Validation)?;
        ensure_repair_artifacts_supported()?;
        let operation_id = request.operation_id.clone();
        let op_dir = self.ensure_prepare_op_dir(&operation_id)?;
        let lock = match self.lock_prepare_operation(&op_dir) {
            Ok(lock) => lock,
            Err(error) if prepare_lock_contended(&error) => {
                return Ok(BeginPrepareOperation::Existing(prepare_in_progress(
                    &operation_id,
                )));
            }
            Err(error) => return Err(error),
        };
        let request_digest = StoredPrepareRequest::new(operation_id.clone(), request.clone())?
            .request_digest
            .clone();
        match self.read_prepare_request(&op_dir, &operation_id)? {
            Some(stored) => {
                if stored.request_digest != request_digest {
                    return Err(prepare_operation_conflict());
                }
                let _held = lock;
                Ok(BeginPrepareOperation::Existing(
                    self.locked_prepare_status(&operation_id, &stored.request_digest)?
                        .into_status(&operation_id),
                ))
            }
            None => {
                if artifact_present(&op_dir.join(PREPARE_CANCEL_FILE))?
                    || artifact_present(&op_dir.join(PREPARE_RESULT_FILE))?
                {
                    return Err(prepare_record_invalid());
                }
                let stored = StoredPrepareRequest::new(operation_id.clone(), request.clone())?;
                debug_assert_eq!(stored.request_digest, request_digest);
                self.publish_prepare_request(&op_dir, &stored)?;
                let bound = RepairArtifactStore {
                    root: self.root().to_path_buf(),
                    prepare_binding: Some(PrepareBinding {
                        operation_id,
                        request_digest: stored.request_digest,
                        _operation_lock: Arc::new(lock),
                    }),
                };
                Ok(BeginPrepareOperation::Run(bound))
            }
        }
    }

    /// Durable status of one client prepare operation. Unknown IDs return
    /// `NotStarted`; lock contention reports `InProgress` without artifact
    /// data; anything else is fully authenticated first.
    pub fn prepare_operation_status(
        &self,
        request: &RepairPrepareOperationRequest,
    ) -> Result<RepairPrepareOperationStatus, WenlanError> {
        request.validate().map_err(WenlanError::Validation)?;
        ensure_repair_artifacts_supported()?;
        let operation_id = request.operation_id.clone();
        let op_dir = self.prepare_op_dir(&operation_id)?;
        if !artifact_present(&op_dir)? {
            return Ok(not_started(&operation_id));
        }
        let lock = match self.lock_prepare_operation(&op_dir) {
            Ok(lock) => lock,
            Err(error) if prepare_lock_contended(&error) => {
                return Ok(prepare_in_progress(&operation_id));
            }
            Err(error) => return Err(error),
        };
        let _held = lock;
        let request_digest =
            StoredPrepareRequest::new(operation_id.clone(), request.clone())?.request_digest;
        match self.read_prepare_request(&op_dir, &operation_id)? {
            None => {
                if artifact_present(&op_dir.join(PREPARE_CANCEL_FILE))?
                    || artifact_present(&op_dir.join(PREPARE_RESULT_FILE))?
                {
                    return Err(prepare_record_invalid());
                }
                Ok(not_started(&operation_id))
            }
            Some(stored) => {
                if stored.request_digest != request_digest {
                    return Err(prepare_operation_conflict());
                }
                Ok(self
                    .locked_prepare_status(&operation_id, &stored.request_digest)?
                    .into_status(&operation_id))
            }
        }
    }

    /// Cancel preparation. Cancelling before start persists the exact request
    /// plus the marker, so a delayed begin observes `Cancelled` and never
    /// runs. Cancelling an interrupted (unpublished) operation is safe because
    /// holding the operation lock proves no producer remains. Cancelling a
    /// published manifest delegates to the manifest-level cancellation and
    /// returns `Ready` with the real operation status; a marker coexisting
    /// with a published manifest is a conflict, never safe cancellation.
    pub fn cancel_prepare_operation(
        &self,
        request: &RepairPrepareOperationRequest,
        now: i64,
    ) -> Result<RepairPrepareOperationStatus, WenlanError> {
        request.validate().map_err(WenlanError::Validation)?;
        ensure_repair_artifacts_supported()?;
        if now <= 0 {
            return Err(WenlanError::Validation(
                "invalid_repair_cancelled_at".to_string(),
            ));
        }
        let operation_id = request.operation_id.clone();
        let op_dir = self.ensure_prepare_op_dir(&operation_id)?;
        let lock = match self.lock_prepare_operation(&op_dir) {
            Ok(lock) => lock,
            Err(error) if prepare_lock_contended(&error) => {
                return Ok(prepare_in_progress(&operation_id));
            }
            Err(error) => return Err(error),
        };
        let _held = lock;
        let request_digest =
            StoredPrepareRequest::new(operation_id.clone(), request.clone())?.request_digest;
        let stored_digest = match self.read_prepare_request(&op_dir, &operation_id)? {
            Some(stored) => {
                if stored.request_digest != request_digest {
                    return Err(prepare_operation_conflict());
                }
                stored.request_digest
            }
            None => {
                if artifact_present(&op_dir.join(PREPARE_CANCEL_FILE))?
                    || artifact_present(&op_dir.join(PREPARE_RESULT_FILE))?
                {
                    return Err(prepare_record_invalid());
                }
                let stored = StoredPrepareRequest::new(operation_id.clone(), request.clone())?;
                self.publish_prepare_request(&op_dir, &stored)?;
                stored.request_digest
            }
        };
        if let Some(pointer) = self.read_prepare_result(&op_dir, &operation_id, &stored_digest)? {
            if let Some(manifest) = self.resolve_pointer_manifest(&pointer)? {
                if self
                    .read_prepare_cancel(&op_dir, &operation_id, &stored_digest)?
                    .is_some()
                {
                    return Err(prepare_manifest_conflict());
                }
                // Prepare lock is held while taking the manifest lock; apply
                // never takes the prepare lock, so no cycle is possible.
                let operation =
                    self.cancel_prepared_repair(&exact_apply_binding(&manifest)?, now)?;
                return Ok(ready(&operation_id, manifest, operation));
            }
        }
        let cancelled_at =
            self.publish_prepare_cancel(&op_dir, &operation_id, &stored_digest, now)?;
        Ok(prepare_cancelled(&operation_id, cancelled_at))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wenlan_types::{
        repair::{RepairLintScope, RepairManifest},
        repair_current::{CurrentRepairChoice, PrepareCurrentRepairRequest},
        repair_operation::RepairOperationState,
    };

    const V1_MANIFEST_ID: &str = "repair_550e8400-e29b-41d4-a716-446655440000";
    const V1_MANIFEST_JSON: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../wenlan-types/testdata/repair/v1/manifest.json"
    ));
    // Publication flow never validates rollback content; the bytes below only
    // exercise staging/publication, never a real apply.

    fn op_request(operation_id: &str, review_id: &str) -> RepairPrepareOperationRequest {
        RepairPrepareOperationRequest {
            operation_id: operation_id.to_string(),
            request: PrepareCurrentRepairRequest {
                lint_scope: RepairLintScope::global(),
                choice: CurrentRepairChoice::rename_page_title(
                    review_id.to_string(),
                    "page-1".to_string(),
                    "Before".to_string(),
                    "After".to_string(),
                )
                .unwrap(),
            },
        }
    }

    /// Canonical manifest value loaded through `load_manifest`, never
    /// deserialized directly from the fixture.
    fn fixture_manifest(seed: &RepairArtifactStore) -> RepairManifest {
        let dir = seed.manifest_dir(V1_MANIFEST_ID).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("manifest.json"), V1_MANIFEST_JSON).unwrap();
        seed.load_manifest(V1_MANIFEST_ID).unwrap()
    }

    async fn current_fixture_manifest(seed: &RepairArtifactStore) -> (RepairManifest, Vec<u8>) {
        let (db, _db_dir) = super::super::tests::fixture().await;
        let request = super::super::tests::request(&db).await;
        let manifest =
            super::super::prepare_memory_reclassification(&db, seed, request, 1_721_000_000)
                .await
                .unwrap();
        let rollback = fs::read(
            seed.manifest_dir(manifest.manifest_id())
                .unwrap()
                .join(manifest.rollback().relative_path()),
        )
        .unwrap();
        (manifest, rollback)
    }

    fn begin_run(
        store: &RepairArtifactStore,
        request: &RepairPrepareOperationRequest,
    ) -> RepairArtifactStore {
        match store.begin_prepare_operation(request).unwrap() {
            BeginPrepareOperation::Run(bound) => bound,
            BeginPrepareOperation::Existing(status) => {
                panic!("expected fresh run, got {status:?}")
            }
        }
    }

    fn is_conflict(result: &Result<RepairPrepareOperationStatus, WenlanError>) -> bool {
        matches!(
            result,
            Err(WenlanError::Conflict(message)) if message == "repair_prepare_operation_conflict"
        )
    }

    #[tokio::test]
    #[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
    async fn begin_run_persist_recovers_ready_and_pointer_precedes_manifest() {
        let root = tempfile::tempdir().unwrap();
        let store = RepairArtifactStore::new(root.path().to_path_buf());
        let seed_root = tempfile::tempdir().unwrap();
        let (manifest, rollback) =
            current_fixture_manifest(&RepairArtifactStore::new(seed_root.path().to_path_buf()))
                .await;
        let request = op_request("11111111-1111-1111-1111-111111111111", "review-a");

        let bound = begin_run(&store, &request);
        let op_dir = store.prepare_op_dir(&request.operation_id).unwrap();
        let contended = store.prepare_operation_status(&request).unwrap();
        assert_eq!(contended.state, RepairPrepareOperationState::InProgress);
        bound
            .persist_prepared_with_hook(&manifest, &rollback, || {
                assert!(
                    op_dir.join(PREPARE_RESULT_FILE).is_file(),
                    "pointer must be durable before the manifest is published"
                );
                assert!(!store.manifest_dir(manifest.manifest_id())?.exists());
                Ok(())
            })
            .unwrap();
        drop(bound);

        let status = store.prepare_operation_status(&request).unwrap();
        let RepairPrepareOperationState::Ready {
            manifest: ready_manifest,
            operation,
        } = &status.state
        else {
            panic!("expected Ready, got {status:?}");
        };
        assert_eq!(ready_manifest.manifest_id(), manifest.manifest_id());
        assert_eq!(ready_manifest.manifest_digest(), manifest.manifest_digest());
        assert!(matches!(operation.state, RepairOperationState::Prepared));

        // A delayed begin with the same request recovers the response; it never re-runs.
        match store.begin_prepare_operation(&request).unwrap() {
            BeginPrepareOperation::Existing(existing) => {
                assert!(matches!(
                    existing.state,
                    RepairPrepareOperationState::Ready { .. }
                ));
            }
            BeginPrepareOperation::Run(_) => panic!("completed operation must never re-run"),
        }
    }

    #[test]
    #[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
    fn conflicting_payload_same_id_fails_closed() {
        let root = tempfile::tempdir().unwrap();
        let store = RepairArtifactStore::new(root.path().to_path_buf());
        let first = op_request("22222222-2222-2222-2222-222222222222", "review-a");
        let other = op_request("22222222-2222-2222-2222-222222222222", "review-b");

        drop(begin_run(&store, &first));
        assert!(matches!(
            store.begin_prepare_operation(&other),
            Err(WenlanError::Conflict(message)) if message == "repair_prepare_operation_conflict"
        ));
        assert!(is_conflict(&store.prepare_operation_status(&other)));
        assert!(is_conflict(
            &store.cancel_prepare_operation(&other, 1_721_000_100)
        ));
        // The original request still resolves to its own state.
        assert_eq!(
            store.prepare_operation_status(&first).unwrap().state,
            RepairPrepareOperationState::Interrupted
        );
    }

    #[test]
    #[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
    fn unknown_cancel_then_delayed_begin_never_runs() {
        let root = tempfile::tempdir().unwrap();
        let store = RepairArtifactStore::new(root.path().to_path_buf());
        let request = op_request("33333333-3333-3333-3333-333333333333", "review-a");

        let cancelled = store
            .cancel_prepare_operation(&request, 1_721_000_100)
            .unwrap();
        assert_eq!(
            cancelled.state,
            RepairPrepareOperationState::Cancelled {
                cancelled_at: 1_721_000_100
            }
        );
        match store.begin_prepare_operation(&request).unwrap() {
            BeginPrepareOperation::Existing(existing) => assert_eq!(existing, cancelled),
            BeginPrepareOperation::Run(_) => panic!("cancelled ID must never start"),
        }

        let marker_path = store
            .prepare_op_dir(&request.operation_id)
            .unwrap()
            .join(PREPARE_CANCEL_FILE);
        let marker_bytes = std::fs::read(&marker_path).unwrap();
        assert_eq!(
            store
                .cancel_prepare_operation(&request, 1_721_000_200)
                .unwrap(),
            cancelled,
            "restart cancellation is idempotent"
        );
        assert_eq!(std::fs::read(&marker_path).unwrap(), marker_bytes);

        let restarted = RepairArtifactStore::new(root.path().to_path_buf());
        assert_eq!(
            restarted.prepare_operation_status(&request).unwrap(),
            cancelled
        );
        assert!(matches!(
            restarted.begin_prepare_operation(&request).unwrap(),
            BeginPrepareOperation::Existing(existing) if existing == cancelled
        ));
    }

    #[test]
    #[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
    fn lock_contention_reports_in_progress_and_clone_keeps_lock() {
        use fs2::FileExt as _;
        use std::fs::OpenOptions;

        let root = tempfile::tempdir().unwrap();
        let store = RepairArtifactStore::new(root.path().to_path_buf());
        let request = op_request("44444444-4444-4444-4444-444444444444", "review-a");

        let bound = begin_run(&store, &request);
        let cloned = bound.clone();
        drop(bound);
        // The clone retains lock ownership, so a fresh handle observes contention.
        match store.begin_prepare_operation(&request).unwrap() {
            BeginPrepareOperation::Existing(existing) => {
                assert_eq!(existing.operation_id, request.operation_id);
                assert_eq!(existing.state, RepairPrepareOperationState::InProgress);
            }
            BeginPrepareOperation::Run(_) => panic!("locked operation must report contention"),
        }
        assert_eq!(
            store.prepare_operation_status(&request).unwrap().state,
            RepairPrepareOperationState::InProgress
        );
        let op_dir = store.prepare_op_dir(&request.operation_id).unwrap();
        assert!(
            !op_dir.join(PREPARE_RESULT_FILE).exists()
                && !op_dir.join(PREPARE_CANCEL_FILE).exists(),
            "contended paths must not mutate artifacts"
        );
        drop(cloned);

        // An externally held OS lock reports the same identity-only status.
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(op_dir.join(OPERATION_LOCK_FILE))
            .unwrap();
        lock_file.lock_exclusive().unwrap();
        assert_eq!(
            store.prepare_operation_status(&request).unwrap().state,
            RepairPrepareOperationState::InProgress
        );
        drop(lock_file);

        // Once no producer remains, the existing request is Interrupted, never re-run.
        match store.begin_prepare_operation(&request).unwrap() {
            BeginPrepareOperation::Existing(existing) => {
                assert_eq!(existing.state, RepairPrepareOperationState::Interrupted)
            }
            BeginPrepareOperation::Run(_) => panic!("existing request must not re-run"),
        }
    }

    #[test]
    #[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
    fn interrupted_unpublished_pointer_can_cancel() {
        let root = tempfile::tempdir().unwrap();
        let store = RepairArtifactStore::new(root.path().to_path_buf());
        let seed_root = tempfile::tempdir().unwrap();
        let manifest = fixture_manifest(&RepairArtifactStore::new(seed_root.path().to_path_buf()));
        let request = op_request("55555555-5555-5555-5555-555555555555", "review-a");

        let bound = begin_run(&store, &request);
        bound.bind_prepared_result(&manifest).unwrap();
        drop(bound);

        assert_eq!(
            store.prepare_operation_status(&request).unwrap().state,
            RepairPrepareOperationState::Interrupted,
            "pointer without publication is interrupted"
        );
        let cancelled = store
            .cancel_prepare_operation(&request, 1_721_000_100)
            .unwrap();
        assert_eq!(
            cancelled.state,
            RepairPrepareOperationState::Cancelled {
                cancelled_at: 1_721_000_100
            }
        );
        assert_eq!(store.prepare_operation_status(&request).unwrap(), cancelled);
        assert!(matches!(
            store.begin_prepare_operation(&request).unwrap(),
            BeginPrepareOperation::Existing(existing) if existing == cancelled
        ));
    }

    #[test]
    #[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
    fn corrupt_records_fail_closed() {
        let root = tempfile::tempdir().unwrap();
        let store = RepairArtifactStore::new(root.path().to_path_buf());
        let seed_root = tempfile::tempdir().unwrap();
        let manifest = fixture_manifest(&RepairArtifactStore::new(seed_root.path().to_path_buf()));

        let corrupted = op_request("66666666-6666-6666-6666-666666666666", "review-a");
        drop(begin_run(&store, &corrupted));
        let op_dir = store.prepare_op_dir(&corrupted.operation_id).unwrap();
        std::fs::write(op_dir.join(PREPARE_REQUEST_FILE), b"not a record").unwrap();
        assert!(matches!(
            store.begin_prepare_operation(&corrupted),
            Err(WenlanError::Validation(message)) if message == "repair_prepare_operation_invalid"
        ));
        assert!(matches!(
            store.prepare_operation_status(&corrupted),
            Err(WenlanError::Validation(message)) if message == "repair_prepare_operation_invalid"
        ));

        let bad_pointer = op_request("77777777-7777-7777-7777-777777777777", "review-a");
        let bound = begin_run(&store, &bad_pointer);
        bound.bind_prepared_result(&manifest).unwrap();
        drop(bound);
        let op_dir = store.prepare_op_dir(&bad_pointer.operation_id).unwrap();
        std::fs::write(op_dir.join(PREPARE_RESULT_FILE), b"{\"bogus\":true}").unwrap();
        assert!(matches!(
            store.prepare_operation_status(&bad_pointer),
            Err(WenlanError::Validation(message)) if message == "repair_prepare_operation_invalid"
        ));

        let bad_marker = op_request("88888888-8888-8888-8888-888888888888", "review-a");
        drop(begin_run(&store, &bad_marker));
        store
            .cancel_prepare_operation(&bad_marker, 1_721_000_100)
            .unwrap();
        let op_dir = store.prepare_op_dir(&bad_marker.operation_id).unwrap();
        std::fs::write(op_dir.join(PREPARE_CANCEL_FILE), b"not a marker").unwrap();
        assert!(matches!(
            store.prepare_operation_status(&bad_marker),
            Err(WenlanError::Validation(message)) if message == "repair_prepare_operation_invalid"
        ));
    }

    #[tokio::test]
    #[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
    async fn ready_cancellation_follows_real_manifest_status() {
        let root = tempfile::tempdir().unwrap();
        let store = RepairArtifactStore::new(root.path().to_path_buf());
        let seed_root = tempfile::tempdir().unwrap();
        let (manifest, rollback) =
            current_fixture_manifest(&RepairArtifactStore::new(seed_root.path().to_path_buf()))
                .await;
        let request = op_request("99999999-9999-9999-9999-999999999999", "review-a");

        let bound = begin_run(&store, &request);
        bound.persist_prepared(&manifest, &rollback).unwrap();
        drop(bound);

        let status = store
            .cancel_prepare_operation(&request, 1_721_000_100)
            .unwrap();
        let RepairPrepareOperationState::Ready {
            manifest: ready_manifest,
            operation,
        } = &status.state
        else {
            panic!("cancelling Ready must stay Ready, got {status:?}");
        };
        assert_eq!(ready_manifest.manifest_id(), manifest.manifest_id());
        assert!(
            matches!(
                operation.state,
                RepairOperationState::Cancelled {
                    cancelled_at: 1_721_000_100
                }
            ),
            "cancel follows the real manifest status, got {:?}",
            operation.state
        );

        // The manifest-level status agrees: no invented top-level shortcut.
        let apply = exact_apply_binding(&manifest).unwrap();
        assert!(matches!(
            store.repair_operation_status(&apply).unwrap().state,
            RepairOperationState::Cancelled {
                cancelled_at: 1_721_000_100
            }
        ));

        assert_eq!(
            store
                .cancel_prepare_operation(&request, 1_721_000_200)
                .unwrap(),
            status,
            "second cancel is idempotent"
        );
        assert!(matches!(
            store.prepare_operation_status(&request).unwrap().state,
            RepairPrepareOperationState::Ready { .. }
        ));
    }

    #[test]
    #[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
    fn invalid_operation_id_rejected_on_every_entry() {
        let root = tempfile::tempdir().unwrap();
        let store = RepairArtifactStore::new(root.path().to_path_buf());
        let mut request = op_request("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa", "review-a");
        request.operation_id = "not-a-uuid".to_string();

        for result in [
            store.begin_prepare_operation(&request).map(|_| ()),
            store.prepare_operation_status(&request).map(|_| ()),
            store.cancel_prepare_operation(&request, 1).map(|_| ()),
        ] {
            assert!(matches!(
                result,
                Err(WenlanError::Validation(message))
                    if message == "invalid_repair_prepare_operation_id"
            ));
        }
        assert!(
            !root
                .path()
                .join(PREPARE_OPERATIONS_DIR)
                .join("not-a-uuid")
                .exists(),
            "rejected IDs must not create directories"
        );
    }
}
