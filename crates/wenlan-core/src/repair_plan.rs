// SPDX-License-Identifier: Apache-2.0
//! Total lint-repair plan construction.

use crate::{error::WenlanError, lint::catalog::catalog_for_profile};
use sha2::{Digest as _, Sha256};
use std::collections::BTreeSet;
use uuid::Uuid;
use wenlan_types::{
    lint::{
        LintCheckResult, LintEvidenceRef, LintOutcome, LintProfile, LintSemanticAction,
        LintSemanticCheckId,
    },
    repair::RepairDigest,
    repair_plan::{
        RepairBlocked, RepairBlockedReasonCode, RepairFindingKind, RepairPlan, RepairPlanDraft,
        RepairPlanEntry, RepairPlanReportReceipt, RepairPlanRequest, RepairResolution,
        RepairSystemAction, RepairSystemActionKind,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SemanticActionRoute {
    ExactCandidate,
    Review,
    MutationCandidate,
}

pub(crate) const fn semantic_action_route(action: LintSemanticAction) -> SemanticActionRoute {
    match action {
        LintSemanticAction::ReclassifyMemory => SemanticActionRoute::ExactCandidate,
        LintSemanticAction::ReviewContradiction
        | LintSemanticAction::ReviewStaleness
        | LintSemanticAction::ReviewPageClaim
        | LintSemanticAction::ReviewRetrieval => SemanticActionRoute::Review,
        LintSemanticAction::SupersedeMemory
        | LintSemanticAction::AddMemoryEntityLink
        | LintSemanticAction::RemoveMemoryEntityLink
        | LintSemanticAction::AddEntityRelation
        | LintSemanticAction::RemoveEntityRelation
        | LintSemanticAction::AddPageEvidence
        | LintSemanticAction::RemovePageEvidence => SemanticActionRoute::MutationCandidate,
    }
}

fn is_complete(outcome: LintOutcome) -> bool {
    matches!(outcome, LintOutcome::Pass | LintOutcome::Finding)
}

fn is_semantic(check_id: &str) -> bool {
    LintSemanticCheckId::ALL
        .into_iter()
        .any(|semantic| semantic.as_str() == check_id)
}

fn digest_bytes(bytes: &[u8]) -> Result<RepairDigest, WenlanError> {
    let value = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    RepairDigest::parse(&value).map_err(|error| WenlanError::Validation(error.to_string()))
}

fn occurrence_digest(
    finding_kind: RepairFindingKind,
    profile: LintProfile,
    check: &LintCheckResult,
    evidence: Option<&LintEvidenceRef>,
) -> Result<RepairDigest, WenlanError> {
    digest_bytes(&serde_json::to_vec(&serde_json::json!({
        "finding_kind": finding_kind,
        "profile": profile,
        "check_id": check.check_id(),
        "outcome": check.outcome(),
        "evidence": evidence,
    }))?)
}

fn blocked(
    reason_code: RepairBlockedReasonCode,
    detail: String,
    next_action: String,
) -> Result<RepairResolution, WenlanError> {
    Ok(RepairResolution::Blocked {
        blocked: RepairBlocked::try_new(reason_code, detail, next_action)
            .map_err(|error| WenlanError::Validation(error.to_string()))?,
    })
}

fn deterministic_resolution(check: &LintCheckResult) -> Result<RepairResolution, WenlanError> {
    if !is_complete(check.outcome()) {
        return blocked(
            RepairBlockedReasonCode::SourceIncomplete,
            format!(
                "{} did not complete: {:?}",
                check.check_id(),
                check.outcome()
            ),
            "rerun lint for the same scope before preparing a repair".to_string(),
        );
    }
    match check.check_id() {
        "runtime.schema_contract" => Ok(RepairResolution::SystemAction {
            system_action: RepairSystemAction::try_new(
                RepairSystemActionKind::RunSchemaMigration,
                "inspect the schema contract and run the canonical migration path".to_string(),
                vec![format!(
                    "{} reported {:?}",
                    check.check_id(),
                    check.outcome()
                )],
            )
            .map_err(|error| WenlanError::Validation(error.to_string()))?,
        }),
        "serving.route_scope_contracts" => Ok(RepairResolution::SystemAction {
            system_action: RepairSystemAction::try_new(
                RepairSystemActionKind::CorrectRouteScopeContract,
                "update or restart the daemon so the route catalog matches the scope contract"
                    .to_string(),
                vec![format!(
                    "{} reported {:?}",
                    check.check_id(),
                    check.outcome()
                )],
            )
            .map_err(|error| WenlanError::Validation(error.to_string()))?,
        }),
        _ => blocked(
            RepairBlockedReasonCode::UnsupportedDeterministicWriter,
            format!(
                "{} is visible but has no typed repair adapter yet",
                check.check_id()
            ),
            "keep the finding visible and add a canonical target/writer adapter".to_string(),
        ),
    }
}

fn semantic_resolution(
    check_id: &str,
    action: LintSemanticAction,
) -> Result<RepairResolution, WenlanError> {
    let route = semantic_action_route(action);
    blocked(
        RepairBlockedReasonCode::MissingPrerequisite,
        format!(
            "{check_id} proposed {action:?}; its {route:?} still needs a durable owner and exact current-state resolution"
        ),
        "resolve the durable target before creating a manifest or Review Item".to_string(),
    )
}

fn push_entry(
    entries: &mut Vec<RepairPlanEntry>,
    finding_kind: RepairFindingKind,
    profile: LintProfile,
    check: &LintCheckResult,
    evidence: Option<&LintEvidenceRef>,
    resolution: RepairResolution,
) -> Result<(), WenlanError> {
    entries.push(
        RepairPlanEntry::try_new(
            finding_kind,
            check.check_id().to_string(),
            occurrence_digest(finding_kind, profile, check, evidence)?,
            vec![],
            resolution,
        )
        .map_err(|error| WenlanError::Validation(error.to_string()))?,
    );
    Ok(())
}

fn collect_deterministic(
    report: &wenlan_types::LintReport,
    included_check_ids: &mut BTreeSet<String>,
    entries: &mut Vec<RepairPlanEntry>,
    complete: &mut bool,
) -> Result<(), WenlanError> {
    for check in report.checks() {
        if is_semantic(check.check_id()) || !included_check_ids.insert(check.check_id().to_string())
        {
            continue;
        }
        if !is_complete(check.outcome()) {
            *complete = false;
        }
        if check.outcome() == LintOutcome::Pass {
            continue;
        }
        push_entry(
            entries,
            RepairFindingKind::Deterministic,
            report.profile(),
            check,
            None,
            deterministic_resolution(check)?,
        )?;
    }
    Ok(())
}

fn collect_semantic(
    report: &wenlan_types::LintReport,
    entries: &mut Vec<RepairPlanEntry>,
    complete: &mut bool,
) -> Result<(), WenlanError> {
    for check in report
        .checks()
        .iter()
        .filter(|check| is_semantic(check.check_id()))
    {
        if !is_complete(check.outcome()) {
            *complete = false;
            push_entry(
                entries,
                RepairFindingKind::Semantic,
                report.profile(),
                check,
                None,
                blocked(
                    RepairBlockedReasonCode::SourceIncomplete,
                    format!(
                        "{} did not complete: {:?}",
                        check.check_id(),
                        check.outcome()
                    ),
                    "rerun applicable agent-assisted Deep lint".to_string(),
                )?,
            )?;
            continue;
        }
        if check.outcome() == LintOutcome::Pass {
            continue;
        }
        let findings = check
            .evidence()
            .iter()
            .filter_map(|evidence| match evidence {
                LintEvidenceRef::SemanticFinding { finding } => Some((evidence, finding)),
                _ => None,
            })
            .collect::<Vec<_>>();
        if findings.is_empty() {
            *complete = false;
            push_entry(
                entries,
                RepairFindingKind::Semantic,
                report.profile(),
                check,
                None,
                blocked(
                    RepairBlockedReasonCode::MissingPrerequisite,
                    format!("{} has no final semantic finding", check.check_id()),
                    "complete agent-assisted Deep adjudication".to_string(),
                )?,
            )?;
            continue;
        }
        for (evidence, finding) in findings {
            push_entry(
                entries,
                RepairFindingKind::Semantic,
                report.profile(),
                check,
                Some(evidence),
                semantic_resolution(check.check_id(), finding.proposed_action())?,
            )?;
        }
    }
    Ok(())
}

pub fn build_repair_plan(request: RepairPlanRequest) -> Result<RepairPlan, WenlanError> {
    let general = request.general_report();
    let deep = request.deep_report();
    let mut entries = Vec::new();
    let mut included_check_ids = BTreeSet::new();
    let mut deterministic_complete = true;
    collect_deterministic(
        general,
        &mut included_check_ids,
        &mut entries,
        &mut deterministic_complete,
    )?;
    if let Some(deep) = deep {
        collect_deterministic(
            deep,
            &mut included_check_ids,
            &mut entries,
            &mut deterministic_complete,
        )?;
    }

    let mut semantic_complete = deep.is_some();
    if let Some(deep) = deep {
        collect_semantic(deep, &mut entries, &mut semantic_complete)?;
    }

    // Catalog membership is a premise of the report contract. Keep this
    // explicit so a future profile expansion cannot silently bypass planning.
    let expected_general = catalog_for_profile(LintProfile::General).count();
    if general.checks().len() != expected_general {
        return Err(WenlanError::Validation(
            "repair plan general catalog mismatch".to_string(),
        ));
    }

    let draft = RepairPlanDraft::try_new(
        format!("repair_plan_{}", Uuid::new_v4()),
        request.scope().clone(),
        RepairPlanReportReceipt::from_report(general),
        deep.map(RepairPlanReportReceipt::from_report),
        deterministic_complete,
        semantic_complete,
        entries,
    )
    .map_err(|error| WenlanError::Validation(error.to_string()))?;
    let plan_digest = digest_bytes(&draft.canonical_bytes()?)?;
    RepairPlan::try_new(draft, plan_digest)
        .map_err(|error| WenlanError::Validation(error.to_string()))
}

#[cfg(test)]
#[path = "repair_plan_tests.rs"]
mod tests;
