// SPDX-License-Identifier: Apache-2.0
//! Completion of the exact lint Review Item that produced a repair manifest.
//!
//! Review completion is deliberately a small, receipt-backed operation. It
//! does not look up a new finding or infer ownership from the target: the
//! manifest's immutable binding and the authenticated verification receipt are
//! the authority, while this module only validates the queue row and performs
//! its compare-and-set transition on a caller-owned transaction connection.

use crate::error::WenlanError;
use serde::Deserialize;
use wenlan_types::repair::{RepairManifest, RepairVerificationReceipt};

const REVIEW_COMPLETION_CONFLICT: &str = "repair_review_completion_conflict";

#[derive(Debug, Deserialize)]
struct VerificationReceiptTimestamp {
    verified_at: i64,
}

fn completion_conflict() -> WenlanError {
    WenlanError::Conflict(REVIEW_COMPLETION_CONFLICT.to_string())
}

fn receipt_verified_at(receipt: &RepairVerificationReceipt) -> Result<i64, WenlanError> {
    let canonical = receipt
        .canonical_unsigned_bytes()
        .map_err(|_| WenlanError::Validation("repair_verification_receipt_mismatch".to_string()))?;
    let timestamp = serde_json::from_slice::<VerificationReceiptTimestamp>(&canonical)
        .map_err(|_| WenlanError::Validation("repair_verification_receipt_mismatch".to_string()))?;
    if timestamp.verified_at <= 0 {
        return Err(WenlanError::Validation(
            "repair_verification_receipt_mismatch".to_string(),
        ));
    }
    Ok(timestamp.verified_at)
}

/// Validate and resolve the Review Item bound by `manifest` using the already
/// persisted `receipt`. The caller must have started its transaction and must
/// commit it together with the verification receipt's SQL side effects.
pub(crate) async fn finalize_lint_repair_review_on_connection(
    connection: &libsql::Connection,
    manifest: &RepairManifest,
    receipt: &RepairVerificationReceipt,
) -> Result<(), WenlanError> {
    let Some(binding) = manifest.source().review_binding() else {
        return Ok(());
    };
    let verified_at = receipt_verified_at(receipt)?;
    let mut rows = connection
        .query(
            "SELECT action,source_ids,payload,status
               FROM refinement_queue
              WHERE id=?1
              LIMIT 2",
            libsql::params![binding.review_id()],
        )
        .await
        .map_err(|error| WenlanError::VectorDb(format!("repair review completion read: {error}")))?;
    let Some(row) = rows.next().await.map_err(|error| {
        WenlanError::VectorDb(format!("repair review completion read row: {error}"))
    })? else {
        return Err(completion_conflict());
    };

    // Read all fields before probing for an unexpected duplicate. libSQL rows
    // may reuse their backing row buffer after `next()`.
    let action = row.get::<String>(0).map_err(|error| {
        WenlanError::VectorDb(format!("repair review completion action: {error}"))
    })?;
    let source_ids_json = row.get::<String>(1).map_err(|error| {
        WenlanError::VectorDb(format!("repair review completion source_ids: {error}"))
    })?;
    let payload = row.get::<Option<String>>(2).map_err(|error| {
        WenlanError::VectorDb(format!("repair review completion payload: {error}"))
    })?;
    let status = row.get::<String>(3).map_err(|error| {
        WenlanError::VectorDb(format!("repair review completion status: {error}"))
    })?;
    if rows.next().await.map_err(|error| {
        WenlanError::VectorDb(format!("repair review completion duplicate read: {error}"))
    })?.is_some() {
        return Err(completion_conflict());
    }

    if action != "lint_repair_review" {
        return Err(completion_conflict());
    }
    let source_ids = serde_json::from_str::<Vec<String>>(&source_ids_json)
        .map_err(|_| completion_conflict())?;
    if source_ids != binding.owner_ids() {
        return Err(completion_conflict());
    }
    let payload = payload.ok_or_else(completion_conflict)?;
    let decoded = crate::db::validate_lint_review_contract(
        binding.review_id(),
        &source_ids,
        &payload,
    )
    .map_err(|_| completion_conflict())?;
    let wenlan_types::RefinementPayload::LintRepairReview {
        check_id,
        occurrence_digest,
        owner_binding_digest,
        ..
    } = decoded
    else {
        return Err(completion_conflict());
    };
    if check_id != manifest.source().check_id()
        || occurrence_digest != *binding.occurrence_digest()
        || owner_binding_digest
            != crate::repair::lint_review_owner_binding_digest(&occurrence_digest, &source_ids)
                .map_err(|_| completion_conflict())?
    {
        return Err(completion_conflict());
    }

    match status.as_str() {
        "awaiting_review" => {
            let changed = connection
                .execute(
                    "UPDATE refinement_queue
                        SET status='resolved', resolved_at=datetime(?1,'unixepoch')
                      WHERE id=?2 AND status='awaiting_review'",
                    libsql::params![verified_at, binding.review_id()],
                )
                .await
                .map_err(|error| {
                    WenlanError::VectorDb(format!("repair review completion resolve: {error}"))
                })?;
            if changed != 1 {
                return Err(completion_conflict());
            }
            Ok(())
        }
        "resolved" => Ok(()),
        _ => Err(completion_conflict()),
    }
}
