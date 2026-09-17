// SPDX-License-Identifier: Apache-2.0
//! Build a repair manifest from daemon-fresh lint output.
//!
//! The HTTP seam deliberately accepts only the small, owner-selected intent
//! needed by each existing repair writer.  It derives the classification
//! finding from the fresh deep report, then delegates to the established
//! preparation path so its snapshot, queue binding, and artifact checks stay
//! authoritative.

use crate::{
    db::MemoryDB,
    error::WenlanError,
    repair::{prepare_memory_reclassification_with_pages, RepairArtifactStore},
};
use std::path::Path;
use wenlan_types::{
    lint::{LintEvidenceRef, LintOutcome, LintReport},
    repair::{
        PrepareRepairRequest, RepairChoice, RepairContractError, RepairManifest,
        REPAIR_CLASSIFICATION_CHECK_ID,
    },
    repair_current::{CurrentRepairChoice, PrepareCurrentRepairRequest},
};

/// Prepare an approved repair using reports produced while the caller held
/// the daemon's analysis fence.  The returned manifest is still checked by the
/// existing writer-specific preparation path; this helper only converts the
/// compact current request into that established request shape.
pub async fn prepare_current_repair_with_pages(
    db: &MemoryDB,
    store: &RepairArtifactStore,
    request: PrepareCurrentRepairRequest,
    general_report: LintReport,
    deep_report: Option<LintReport>,
    page_root: Option<&Path>,
    now_epoch: i64,
) -> Result<RepairManifest, WenlanError> {
    let (choice, review_id) = match request.choice() {
        CurrentRepairChoice::ReclassifyMemory {
            review_id,
            memory_id,
            after_memory_type,
        } => {
            let deep = deep_report
                .as_ref()
                .ok_or_else(|| WenlanError::Validation("repair_deep_report_missing".to_string()))?;
            let expected_evidence = crate::lint::semantic_record_digest("memory", memory_id);
            let classification_check = deep
                .checks()
                .iter()
                .find(|check| check.check_id() == REPAIR_CLASSIFICATION_CHECK_ID)
                .ok_or_else(|| {
                    WenlanError::Conflict("repair_current_check_unavailable".to_string())
                })?;
            if !matches!(
                classification_check.outcome(),
                LintOutcome::Pass | LintOutcome::Finding
            ) {
                return Err(WenlanError::Conflict(
                    "repair_current_check_unavailable".to_string(),
                ));
            }
            let selected_finding = classification_check
                .evidence()
                .iter()
                .find_map(|evidence| match evidence {
                    LintEvidenceRef::SemanticFinding { finding }
                        if finding.evidence_ids().contains(&expected_evidence) =>
                    {
                        Some(finding.clone())
                    }
                    _ => None,
                })
                .ok_or_else(|| {
                    WenlanError::Conflict("repair_current_finding_missing".to_string())
                })?;
            let choice =
                RepairChoice::reclassify_memory(selected_finding, after_memory_type.clone())
                    .map_err(invalid_prepare_request)?;
            (choice, review_id.clone())
        }
        CurrentRepairChoice::RenamePageTitle {
            review_id,
            page_id,
            before_title,
            after_title,
        } => {
            let choice = RepairChoice::rename_page_title(
                review_id.clone(),
                page_id.clone(),
                before_title.clone(),
                after_title.clone(),
            )
            .map_err(invalid_prepare_request)?;
            (choice, review_id.clone())
        }
        CurrentRepairChoice::CompleteEntityExtraction {
            review_id,
            memory_id,
            entity_ids,
        } => {
            let choice = RepairChoice::complete_entity_extraction(
                review_id.clone(),
                memory_id.clone(),
                entity_ids.clone(),
            )
            .map_err(invalid_prepare_request)?;
            (choice, review_id.clone())
        }
    };

    let repair_request = PrepareRepairRequest::try_new_with_choice(
        request.lint_scope().clone(),
        general_report,
        deep_report,
        choice,
    )
    .map_err(invalid_prepare_request)?;
    let manifest =
        prepare_memory_reclassification_with_pages(db, store, repair_request, page_root, now_epoch)
            .await?;
    if manifest
        .source()
        .review_binding()
        .is_none_or(|binding| binding.review_id() != review_id)
    {
        return Err(WenlanError::Conflict(
            "repair_current_review_binding_mismatch".to_string(),
        ));
    }
    Ok(manifest)
}

fn invalid_prepare_request(_: RepairContractError) -> WenlanError {
    WenlanError::Validation("invalid_prepare_current_repair_request".to_string())
}
