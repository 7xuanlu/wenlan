// SPDX-License-Identifier: Apache-2.0
//! Total lint-repair plan construction.

use crate::{
    db::MemoryDB,
    error::WenlanError,
    lint::catalog::catalog_for_profile,
    repair::{
        canonical_lint_review_source_ids, lint_review_owner_binding_digest, repair_check_baseline,
        repair_digest, target_receipt, validate_current_page_receipts,
        validate_current_page_report_receipt, validate_durable_scope,
        validate_report_source_receipts, RepairArtifactStore,
    },
};
use sha2::{Digest as _, Sha256};
use std::collections::BTreeSet;
use uuid::Uuid;
use wenlan_types::{
    lint::{
        LintCheckResult, LintEvidenceRef, LintGateEffect, LintOutcome, LintProfile,
        LintSemanticAction, LintSemanticCheckId, LintSemanticFinding,
    },
    repair::{
        RepairDigest, RepairExpectedState, RepairLintScope, RepairManifest, RepairManifestDraft,
        RepairPostAssertions, RepairRecordSetBaseline, RepairRollbackArtifact, RepairSource,
        RepairTarget, RepairWriter,
    },
    repair_plan::{
        RepairAffectedRecord, RepairAffectedRecordKind, RepairBlocked, RepairBlockedReasonCode,
        RepairFindingKind, RepairPlan, RepairPlanDraft, RepairPlanEntry, RepairPlanReportReceipt,
        RepairPlanRequest, RepairResolution, RepairReviewItem, RepairSystemAction,
        RepairSystemActionKind,
    },
    RefinementPayload,
};

mod deterministic;
mod semantic;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SemanticActionRoute {
    ExactCandidate,
    Review,
    MutationCandidate,
}

pub(crate) async fn deterministic_target_still_actionable(
    snapshot: &crate::lint::snapshot::LintReadSnapshot<'_>,
    scope: &RepairLintScope,
    page_root: Option<&std::path::Path>,
    target: &RepairTarget,
    writer: wenlan_types::repair::RepairWriter,
) -> Result<bool, WenlanError> {
    deterministic::target_still_actionable(snapshot, scope, page_root, target, writer).await
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

fn deterministic_resolution(
    check: &LintCheckResult,
    scope: &RepairLintScope,
) -> Result<RepairResolution, WenlanError> {
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
        deterministic::TAG_INTEGRITY if !matches!(scope, RepairLintScope::Global) => blocked(
            RepairBlockedReasonCode::MissingPrerequisite,
            "identity.tag_integrity is globally owned and cannot be repaired from a scoped plan"
                .to_string(),
            "rerun /lint repair global to inspect and approve exact tag-row deletions".to_string(),
        ),
        "runtime.schema_contract" | "runtime.search_index_contract" => blocked(
            RepairBlockedReasonCode::MissingPrerequisite,
            format!(
                "{} requires a current typed schema diagnostic before selecting an operation",
                check.check_id()
            ),
            "prepare the plan against the current daemon snapshot".to_string(),
        ),
        "serving.route_scope_contracts" => route_scope_resolution(),
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

fn route_scope_resolution() -> Result<RepairResolution, WenlanError> {
    let violations = crate::lint::serving::routes::scope_contract_violations()
        .map(|route| {
            let method = match route.method {
                crate::lint::serving::routes::Method::Get => "GET",
                crate::lint::serving::routes::Method::Post => "POST",
            };
            format!(
                "{method} {}: selector={:?}, gate={:?}, unknown_scope={:?}",
                route.path, route.selector_precedence, route.selection_gate, route.unknown_scope
            )
        })
        .collect::<Vec<_>>();
    let (kind, summary, evidence) = if violations.is_empty() {
        (
            RepairSystemActionKind::UpdateDaemon,
            "the report found route-scope drift, but this binary's route catalog is clean; update and restart the reporting daemon".to_string(),
            vec!["no violating routes exist in the current compiled route catalog".to_string()],
        )
    } else {
        (
            RepairSystemActionKind::CorrectRouteScopeContract,
            "correct the listed route contracts, then update and restart the daemon".to_string(),
            violations,
        )
    };
    Ok(RepairResolution::SystemAction {
        system_action: RepairSystemAction::try_new(kind, summary, evidence)
            .map_err(|error| WenlanError::Validation(error.to_string()))?,
    })
}

async fn resolve_current_system_actions(
    base: RepairPlan,
    snapshot: &crate::lint::snapshot::LintReadSnapshot<'_>,
) -> Result<RepairPlan, WenlanError> {
    let needs_schema = base.entries().iter().any(|entry| {
        matches!(
            entry.check_id(),
            "runtime.schema_contract" | "runtime.search_index_contract"
        ) && matches!(
            entry.resolution(),
            RepairResolution::Blocked { blocked }
                if blocked.reason_code() == RepairBlockedReasonCode::MissingPrerequisite
        )
    });
    if !needs_schema {
        return Ok(base);
    }
    let schema = crate::lint::runtime::schema::load_from_snapshot(snapshot).await;
    let mut entries = Vec::with_capacity(base.entries().len());
    for entry in base.entries() {
        let resolution = match entry.check_id() {
            "runtime.schema_contract" => current_schema_resolution(schema.as_ref().ok())?,
            "runtime.search_index_contract" => current_search_resolution(schema.as_ref().ok())?,
            _ => entry.resolution().clone(),
        };
        entries.push(
            RepairPlanEntry::try_new(
                entry.finding_kind(),
                entry.check_id().to_string(),
                entry.occurrence_digest().clone(),
                entry.affected_records().to_vec(),
                resolution,
            )
            .map_err(|error| WenlanError::Validation(error.to_string()))?,
        );
    }
    let draft = RepairPlanDraft::try_new(
        base.plan_id().to_string(),
        base.scope().clone(),
        base.general_report_receipt().clone(),
        base.deep_report_receipt().cloned(),
        base.deterministic_complete(),
        base.semantic_complete(),
        entries,
    )
    .map_err(|error| WenlanError::Validation(error.to_string()))?;
    let digest = digest_bytes(&draft.canonical_bytes()?)?;
    RepairPlan::try_new(draft, digest, |canonical, expected| {
        repair_digest(canonical) == *expected
    })
    .map_err(|error| WenlanError::Validation(error.to_string()))
}

fn current_schema_resolution(
    schema: Option<&crate::lint::runtime::schema::SchemaSnapshot>,
) -> Result<RepairResolution, WenlanError> {
    let Some(schema) = schema else {
        return blocked(
            RepairBlockedReasonCode::MissingPrerequisite,
            "the current schema could not be inspected".to_string(),
            "restore database readability and rerun lint repair".to_string(),
        );
    };
    let current = u64::from(crate::db::SCHEMA_VERSION);
    if schema.user_version() < current {
        let mut evidence = vec![format!(
            "PRAGMA user_version={} is older than canonical version {current}",
            schema.user_version()
        )];
        if !schema.missing_tables().is_empty() {
            evidence.push(format!(
                "missing tables covered by the canonical migration path: {}",
                schema.missing_tables().join(", ")
            ));
        }
        return Ok(RepairResolution::SystemAction {
            system_action: RepairSystemAction::try_new(
                RepairSystemActionKind::RunSchemaMigration,
                "run the daemon's canonical schema migration path".to_string(),
                evidence,
            )
            .map_err(|error| WenlanError::Validation(error.to_string()))?,
        });
    }
    let detail = if schema.user_version() > current {
        format!(
            "database schema version {} is newer than this daemon's version {current}",
            schema.user_version()
        )
    } else {
        format!(
            "schema version is current, but required tables are missing: {}",
            schema.missing_tables().join(", ")
        )
    };
    blocked(
        RepairBlockedReasonCode::UnknownSchemaShape,
        detail,
        "use a compatible daemon or restore the canonical schema before retrying".to_string(),
    )
}

fn current_search_resolution(
    schema: Option<&crate::lint::runtime::schema::SchemaSnapshot>,
) -> Result<RepairResolution, WenlanError> {
    let Some(schema) = schema else {
        return blocked(
            RepairBlockedReasonCode::MissingPrerequisite,
            "the current search schema could not be inspected".to_string(),
            "restore database readability and rerun lint repair".to_string(),
        );
    };
    if schema.invalid_search_objects().is_empty() {
        return blocked(
            RepairBlockedReasonCode::UnknownSchemaShape,
            "the source report found search-index drift that is not reproducible in the current schema".to_string(),
            "rerun lint with the current daemon before selecting an operation".to_string(),
        );
    }
    Ok(RepairResolution::SystemAction {
        system_action: RepairSystemAction::try_new(
            RepairSystemActionKind::RebuildSearchIndex,
            "rebuild the invalid search objects through the canonical database path".to_string(),
            schema
                .invalid_search_objects()
                .iter()
                .map(|name| format!("invalid or missing search object: {name}"))
                .collect(),
        )
        .map_err(|error| WenlanError::Validation(error.to_string()))?,
    })
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
    scope: &RepairLintScope,
    included_check_ids: &mut BTreeSet<String>,
    entries: &mut Vec<RepairPlanEntry>,
    complete: &mut bool,
) -> Result<(), WenlanError> {
    for check in report.checks() {
        let occurrence_key = format!("{}:{}", report.profile(), check.check_id());
        if is_semantic(check.check_id()) || !included_check_ids.insert(occurrence_key) {
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
            deterministic_resolution(check, scope)?,
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
        request.scope(),
        &mut included_check_ids,
        &mut entries,
        &mut deterministic_complete,
    )?;
    if let Some(deep) = deep {
        collect_deterministic(
            deep,
            request.scope(),
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
    RepairPlan::try_new(draft, plan_digest, |canonical, expected| {
        repair_digest(canonical) == *expected
    })
    .map_err(|error| WenlanError::Validation(error.to_string()))
}

/// Compact, non-cryptographic evidence handle scoped to one generated plan.
/// Durable authorization continues to bind the full 256-bit repair digest.
fn plan_lint_digest(digest: &RepairDigest) -> Result<wenlan_types::lint::LintDigest, WenlanError> {
    let prefix = digest
        .as_str()
        .get(..16)
        .ok_or_else(|| WenlanError::Validation("repair occurrence digest truncated".to_string()))?;
    let value = u64::from_str_radix(prefix, 16)
        .map_err(|error| WenlanError::Validation(error.to_string()))?;
    Ok(wenlan_types::lint::LintDigest::from_u64(value))
}

fn candidate_occurrence_digest<T: serde::Serialize>(
    check_id: &str,
    value: &T,
) -> Result<RepairDigest, WenlanError> {
    digest_bytes(&serde_json::to_vec(&serde_json::json!({
        "check_id": check_id,
        "candidate": value,
    }))?)
}

pub(crate) fn semantic_review_occurrence_digest(
    check_id: &str,
    finding: &LintSemanticFinding,
    affected_records: &[RepairAffectedRecord],
) -> Result<RepairDigest, WenlanError> {
    candidate_occurrence_digest(
        check_id,
        &serde_json::json!({
            "finding": finding,
            "affected_records": affected_records,
        }),
    )
}

fn affected_record_for_target(target: &RepairTarget) -> Result<RepairAffectedRecord, WenlanError> {
    let (kind, durable_id) = match target {
        RepairTarget::Memory { source_id, .. } => {
            (RepairAffectedRecordKind::Memory, source_id.clone())
        }
        RepairTarget::MemoryEntityLink {
            memory_id,
            entity_id,
            ..
        } => (
            RepairAffectedRecordKind::Entity,
            serde_json::to_string(&[memory_id, entity_id])?,
        ),
        RepairTarget::MemoryEntityExtraction { memory_id, .. } => {
            (RepairAffectedRecordKind::Memory, memory_id.clone())
        }
        RepairTarget::Tag {
            source,
            source_id,
            tag,
            ..
        } => (
            RepairAffectedRecordKind::Tag,
            serde_json::to_string(&[source, source_id, tag])?,
        ),
        RepairTarget::PageLink {
            source_page_id,
            label_key,
            ..
        } => (
            RepairAffectedRecordKind::PageLink,
            serde_json::to_string(&[source_page_id, label_key])?,
        ),
        RepairTarget::PageProjection { page_id, .. } => {
            (RepairAffectedRecordKind::Page, page_id.clone())
        }
        RepairTarget::Page { page_id, .. } => (RepairAffectedRecordKind::Page, page_id.clone()),
    };
    RepairAffectedRecord::try_new(kind, durable_id)
        .map_err(|error| WenlanError::Validation(error.to_string()))
}

fn source_check_in_report<'a>(
    report: &'a wenlan_types::lint::LintReport,
    check_id: &str,
) -> Option<&'a LintCheckResult> {
    report.checks().iter().find(|check| {
        check.check_id() == check_id
            && check.outcome() == LintOutcome::Finding
            && check.gate_effect() == LintGateEffect::Actionable
    })
}

fn source_check<'a>(request: &'a RepairPlanRequest, check_id: &str) -> Option<&'a LintCheckResult> {
    source_check_in_report(request.general_report(), check_id).or_else(|| {
        request
            .deep_report()
            .and_then(|report| source_check_in_report(report, check_id))
    })
}

fn prepare_exact_manifest(
    store: &RepairArtifactStore,
    request: &RepairPlanRequest,
    tag_record_set: Option<&RepairRecordSetBaseline>,
    exact: deterministic::ExactDeterministicRepair,
    now_epoch: i64,
) -> Result<RepairManifest, WenlanError> {
    let check_id = exact
        .source_check_ids
        .first()
        .copied()
        .ok_or_else(|| WenlanError::Validation("repair source check missing".to_string()))?;
    let check = source_check(request, check_id).ok_or_else(|| {
        WenlanError::Validation(format!("repair source check not in reports: {check_id}"))
    })?;
    let allowed_non_target_check_deltas = exact
        .source_check_ids
        .iter()
        .skip(1)
        .map(|check_id| (*check_id).to_string())
        .collect::<Vec<_>>();
    // Deep is agent-assisted semantic input for the plan, not a verification
    // prerequisite for an exact deterministic writer. Binding its full
    // baseline here also makes the same General repair depend on plan profile.
    let source = RepairSource::try_new_general_only_deterministic(
        request.scope().clone(),
        request.general_report().scope().clone(),
        check_id.to_string(),
        check.evidence().to_vec(),
        request.general_report().snapshots().clone(),
        request.general_report().producer_receipt().clone(),
    )
    .map_err(|error| WenlanError::Validation(error.to_string()))?;
    let occurrence = candidate_occurrence_digest(
        check_id,
        &serde_json::json!({
            "target": &exact.target,
            "writer": exact.writer,
            "mutation": &exact.mutation,
        }),
    )?;
    let post_assertions = RepairPostAssertions::try_new_general_only_for_check(
        check_id.to_string(),
        plan_lint_digest(&occurrence)?,
        repair_check_baseline(request.general_report())?,
        allowed_non_target_check_deltas,
    )
    .map_err(|error| WenlanError::Validation(error.to_string()))?;
    let post_assertions = if exact.writer == RepairWriter::DeleteTagRow {
        post_assertions
            .try_with_target_record_set(tag_record_set.cloned().ok_or_else(|| {
                WenlanError::Validation("repair tag set baseline missing".to_string())
            })?)
            .map_err(|error| WenlanError::Validation(error.to_string()))?
    } else {
        post_assertions
    };
    let rollback_bytes = serde_json::to_vec_pretty(&exact.rollback)?;
    let rollback_contract = RepairRollbackArtifact::try_new(
        "rollback-v1.json".to_string(),
        repair_digest(&rollback_bytes),
    )
    .map_err(|error| WenlanError::Validation(error.to_string()))?;
    let draft = RepairManifestDraft::try_new(
        format!("repair_{}", Uuid::new_v4()),
        now_epoch,
        source,
        exact.target,
        RepairExpectedState::try_new(exact.expected_version, target_receipt(&exact.rollback)?)
            .map_err(|error| WenlanError::Validation(error.to_string()))?,
        exact.writer,
        exact.mutation,
        exact.allowed_effects,
        rollback_contract,
        post_assertions,
    )
    .map_err(|error| WenlanError::Validation(error.to_string()))?;
    let manifest = RepairManifest::try_new(draft.clone(), repair_digest(&draft.canonical_bytes()?))
        .map_err(|error| WenlanError::Validation(error.to_string()))?;
    store.persist_prepared(&manifest, &rollback_bytes)?;
    Ok(manifest)
}

async fn materialize_deterministic(
    db: &MemoryDB,
    store: &RepairArtifactStore,
    request: &RepairPlanRequest,
    resolutions: Vec<deterministic::DeterministicResolution>,
    tag_record_set: Option<&RepairRecordSetBaseline>,
    now_epoch: i64,
) -> Result<(BTreeSet<String>, Vec<RepairPlanEntry>), WenlanError> {
    let mut resolved_check_ids = BTreeSet::new();
    let mut entries = Vec::new();
    for resolution in resolutions {
        match resolution {
            deterministic::DeterministicResolution::Exact(exact) => {
                let mut exact = *exact;
                let actionable_check_ids = exact
                    .source_check_ids
                    .iter()
                    .copied()
                    .filter(|check_id| source_check(request, check_id).is_some())
                    .collect::<Vec<_>>();
                if actionable_check_ids.is_empty() {
                    continue;
                }
                exact.source_check_ids.retain(|check_id| {
                    source_check_in_report(request.general_report(), check_id).is_some()
                });
                let affected = affected_record_for_target(&exact.target)?;
                let occurrence_base = serde_json::json!({
                    "target": &exact.target,
                    "writer": exact.writer,
                    "mutation": &exact.mutation,
                });
                if exact.source_check_ids.is_empty() {
                    let primary_check_id = actionable_check_ids[0];
                    for check_id in &actionable_check_ids {
                        resolved_check_ids.insert((*check_id).to_string());
                    }
                    entries.push(
                        RepairPlanEntry::try_new(
                            RepairFindingKind::Deterministic,
                            primary_check_id.to_string(),
                            candidate_occurrence_digest(
                                primary_check_id,
                                &serde_json::json!({
                                    "candidate": &occurrence_base,
                                    "source_profile": LintProfile::Deep,
                                }),
                            )?,
                            vec![affected],
                            blocked(
                                RepairBlockedReasonCode::MissingPrerequisite,
                                format!(
                                    "{primary_check_id} exact deterministic target is only backed by Deep"
                                ),
                                "rerun General with coverage that includes this target before preparing its deterministic repair"
                                    .to_string(),
                            )?,
                        )
                        .map_err(|error| WenlanError::Validation(error.to_string()))?,
                    );
                    continue;
                }
                let check_ids = exact.source_check_ids.clone();
                let primary_check_id = check_ids[0];
                let manifest =
                    prepare_exact_manifest(store, request, tag_record_set, exact, now_epoch)?;
                for check_id in check_ids {
                    resolved_check_ids.insert(check_id.to_string());
                }
                entries.push(
                    RepairPlanEntry::try_new(
                        RepairFindingKind::Deterministic,
                        primary_check_id.to_string(),
                        candidate_occurrence_digest(primary_check_id, &occurrence_base)?,
                        vec![affected],
                        RepairResolution::Ready {
                            manifest: Box::new(manifest),
                        },
                    )
                    .map_err(|error| WenlanError::Validation(error.to_string()))?,
                );
            }
            deterministic::DeterministicResolution::Review(review) => {
                if source_check(request, review.check_id).is_none() {
                    continue;
                }
                resolved_check_ids.insert(review.check_id.to_string());
                let occurrence = candidate_occurrence_digest(
                    review.check_id,
                    &serde_json::json!({
                        "record": review.affected_record.durable_id(),
                        "issue": &review.issue,
                    }),
                )?;
                let review_item = RepairReviewItem::try_new(
                    format!("lint_review_{}", occurrence.as_str()),
                    review.check_id.to_string(),
                    review.issue,
                    review.choices,
                    review.suggested_research_queries,
                )
                .map_err(|error| WenlanError::Validation(error.to_string()))?;
                let source_ids = canonical_lint_review_source_ids(&[review
                    .affected_record
                    .durable_id()
                    .to_string()])?;
                let payload = RefinementPayload::LintRepairReview {
                    check_id: review_item.check_id().to_string(),
                    occurrence_digest: occurrence.clone(),
                    owner_binding_digest: lint_review_owner_binding_digest(
                        &occurrence,
                        &source_ids,
                    )?,
                    issue: review_item.issue().to_string(),
                    choices: review_item.choices().to_vec(),
                    suggested_research_queries: review_item.suggested_research_queries().to_vec(),
                };
                db.insert_lint_review_if_absent(
                    review_item.review_id(),
                    &source_ids,
                    &serde_json::to_string(&payload)?,
                )
                .await?;
                entries.push(
                    RepairPlanEntry::try_new(
                        RepairFindingKind::Deterministic,
                        review.check_id.to_string(),
                        occurrence,
                        vec![review.affected_record],
                        RepairResolution::Review { review_item },
                    )
                    .map_err(|error| WenlanError::Validation(error.to_string()))?,
                );
            }
            deterministic::DeterministicResolution::Blocked(blocked_candidate) => {
                if source_check(request, blocked_candidate.check_id).is_none() {
                    continue;
                }
                resolved_check_ids.insert(blocked_candidate.check_id.to_string());
                let occurrence = candidate_occurrence_digest(
                    blocked_candidate.check_id,
                    &serde_json::json!({
                        "record": blocked_candidate.affected_record.durable_id(),
                        "detail": &blocked_candidate.detail,
                    }),
                )?;
                entries.push(
                    RepairPlanEntry::try_new(
                        RepairFindingKind::Deterministic,
                        blocked_candidate.check_id.to_string(),
                        occurrence,
                        vec![blocked_candidate.affected_record],
                        blocked(
                            RepairBlockedReasonCode::MissingPrerequisite,
                            blocked_candidate.detail,
                            blocked_candidate.next_action,
                        )?,
                    )
                    .map_err(|error| WenlanError::Validation(error.to_string()))?,
                );
            }
        }
    }
    Ok((resolved_check_ids, entries))
}

fn deterministic_tag_record_set(
    resolutions: &[deterministic::DeterministicResolution],
) -> Result<Option<RepairRecordSetBaseline>, WenlanError> {
    let targets = resolutions
        .iter()
        .filter_map(|resolution| match resolution {
            deterministic::DeterministicResolution::Exact(exact)
                if exact.writer == RepairWriter::DeleteTagRow =>
            {
                Some(exact.target.clone())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if targets.is_empty() {
        return Ok(None);
    }
    crate::repair::tag_record_set_baseline(&targets).map(Some)
}

async fn materialize_semantic(
    db: &MemoryDB,
    resolutions: Vec<semantic::SemanticResolution>,
) -> Result<(BTreeSet<String>, Vec<RepairPlanEntry>), WenlanError> {
    let mut resolved_check_ids = BTreeSet::new();
    let mut entries = Vec::new();
    for resolution in resolutions {
        match resolution {
            semantic::SemanticResolution::Review(review) => {
                resolved_check_ids.insert(review.check_id.clone());
                let occurrence = semantic_review_occurrence_digest(
                    &review.check_id,
                    &review.finding,
                    &review.affected_records,
                )?;
                let action = review.finding.proposed_action();
                let issue = format!(
                    "{:?} proposed for {:?} at {} basis points{}",
                    action,
                    review.finding.reason_code(),
                    review.finding.confidence_basis_points(),
                    if review.finding.unresolved_disagreement() {
                        "; judges disagree"
                    } else {
                        ""
                    }
                );
                let choices = semantic::review_choices(action);
                let suggested_research_queries =
                    semantic::suggested_research_queries(action, &review.affected_records);
                let review_item = RepairReviewItem::try_new(
                    format!("lint_review_{}", occurrence.as_str()),
                    review.check_id.clone(),
                    issue,
                    choices,
                    suggested_research_queries,
                )
                .map_err(|error| WenlanError::Validation(error.to_string()))?;
                let source_ids = canonical_lint_review_source_ids(
                    &review
                        .affected_records
                        .iter()
                        .map(|record| record.durable_id().to_string())
                        .collect::<Vec<_>>(),
                )?;
                let payload = RefinementPayload::LintRepairReview {
                    check_id: review_item.check_id().to_string(),
                    occurrence_digest: occurrence.clone(),
                    owner_binding_digest: lint_review_owner_binding_digest(
                        &occurrence,
                        &source_ids,
                    )?,
                    issue: review_item.issue().to_string(),
                    choices: review_item.choices().to_vec(),
                    suggested_research_queries: review_item.suggested_research_queries().to_vec(),
                };
                db.insert_lint_review_if_absent(
                    review_item.review_id(),
                    &source_ids,
                    &serde_json::to_string(&payload)?,
                )
                .await?;
                entries.push(
                    RepairPlanEntry::try_new(
                        RepairFindingKind::Semantic,
                        review.check_id,
                        occurrence,
                        review.affected_records,
                        RepairResolution::Review { review_item },
                    )
                    .map_err(|error| WenlanError::Validation(error.to_string()))?,
                );
            }
            semantic::SemanticResolution::Blocked(blocked_candidate) => {
                resolved_check_ids.insert(blocked_candidate.check_id.clone());
                let occurrence = candidate_occurrence_digest(
                    &blocked_candidate.check_id,
                    &serde_json::json!({
                        "finding": &blocked_candidate.finding,
                        "affected_records": &blocked_candidate.affected_records,
                    }),
                )?;
                entries.push(
                    RepairPlanEntry::try_new(
                        RepairFindingKind::Semantic,
                        blocked_candidate.check_id,
                        occurrence,
                        blocked_candidate.affected_records,
                        blocked(
                            RepairBlockedReasonCode::MissingPrerequisite,
                            blocked_candidate.detail,
                            "rerun Deep lint so semantic evidence binds current durable records"
                                .to_string(),
                        )?,
                    )
                    .map_err(|error| WenlanError::Validation(error.to_string()))?,
                );
            }
        }
    }
    Ok((resolved_check_ids, entries))
}

pub async fn prepare_repair_plan(
    db: &MemoryDB,
    store: &RepairArtifactStore,
    request: RepairPlanRequest,
    page_root: Option<&std::path::Path>,
    now_epoch: i64,
) -> Result<RepairPlan, WenlanError> {
    if now_epoch <= 0 {
        return Err(WenlanError::Validation(
            "invalid repair plan prepared_at".to_string(),
        ));
    }
    let base = build_repair_plan(request.clone())?;
    let Some(deep) = request.deep_report() else {
        let snapshot = db
            .open_lint_snapshot()
            .await
            .map_err(|error| WenlanError::VectorDb(format!("repair plan snapshot: {error}")))?;
        validate_durable_scope(&snapshot, request.scope(), request.general_report().scope())
            .await?;
        let base = resolve_current_system_actions(base, &snapshot).await?;
        let resolutions =
            deterministic::resolve_current(&snapshot, request.scope(), page_root).await?;
        let tag_record_set = deterministic_tag_record_set(&resolutions)?;
        let receipt = snapshot
            .finish()
            .await
            .map_err(|error| WenlanError::VectorDb(format!("repair plan snapshot: {error}")))?;
        validate_report_source_receipts(&[request.general_report()], receipt)?;
        validate_current_page_report_receipt(request.general_report(), page_root).await?;
        let (resolved_check_ids, mut deterministic_entries) = materialize_deterministic(
            db,
            store,
            &request,
            resolutions,
            tag_record_set.as_ref(),
            now_epoch,
        )
        .await?;
        let mut entries = base
            .entries()
            .iter()
            .filter(|entry| !resolved_check_ids.contains(entry.check_id()))
            .cloned()
            .collect::<Vec<_>>();
        entries.append(&mut deterministic_entries);
        let draft = RepairPlanDraft::try_new(
            format!("repair_plan_{}", Uuid::new_v4()),
            request.scope().clone(),
            RepairPlanReportReceipt::from_report(request.general_report()),
            None,
            base.deterministic_complete(),
            false,
            entries,
        )
        .map_err(|error| WenlanError::Validation(error.to_string()))?;
        let digest = digest_bytes(&draft.canonical_bytes()?)?;
        let plan = RepairPlan::try_new(draft, digest, |canonical, expected| {
            repair_digest(canonical) == *expected
        })
        .map_err(|error| WenlanError::Validation(error.to_string()))?;
        store.persist_plan(&plan)?;
        return Ok(plan);
    };
    if request.general_report().producer_receipt() != deep.producer_receipt() {
        return Err(WenlanError::Conflict(
            "repair source producers mismatch".to_string(),
        ));
    }

    let snapshot = db
        .open_lint_snapshot()
        .await
        .map_err(|error| WenlanError::VectorDb(format!("repair plan snapshot: {error}")))?;
    validate_durable_scope(&snapshot, request.scope(), request.general_report().scope()).await?;
    let base = resolve_current_system_actions(base, &snapshot).await?;
    let resolutions = deterministic::resolve_current(&snapshot, request.scope(), page_root).await?;
    let tag_record_set = deterministic_tag_record_set(&resolutions)?;
    let semantic_resolutions = semantic::resolve_current(&snapshot, deep).await?;
    let receipt = snapshot
        .finish()
        .await
        .map_err(|error| WenlanError::VectorDb(format!("repair plan snapshot: {error}")))?;
    validate_report_source_receipts(&[request.general_report(), deep], receipt)?;
    validate_current_page_receipts(request.general_report(), Some(deep), page_root).await?;

    let (resolved_check_ids, mut deterministic_entries) = materialize_deterministic(
        db,
        store,
        &request,
        resolutions,
        tag_record_set.as_ref(),
        now_epoch,
    )
    .await?;
    let (resolved_semantic_check_ids, mut semantic_entries) =
        materialize_semantic(db, semantic_resolutions).await?;
    let mut entries = base
        .entries()
        .iter()
        .filter(|entry| {
            !resolved_check_ids.contains(entry.check_id())
                && !resolved_semantic_check_ids.contains(entry.check_id())
        })
        .cloned()
        .collect::<Vec<_>>();
    entries.append(&mut deterministic_entries);
    entries.append(&mut semantic_entries);
    let draft = RepairPlanDraft::try_new(
        format!("repair_plan_{}", Uuid::new_v4()),
        request.scope().clone(),
        RepairPlanReportReceipt::from_report(request.general_report()),
        Some(RepairPlanReportReceipt::from_report(deep)),
        base.deterministic_complete(),
        base.semantic_complete(),
        entries,
    )
    .map_err(|error| WenlanError::Validation(error.to_string()))?;
    let digest = digest_bytes(&draft.canonical_bytes()?)?;
    let plan = RepairPlan::try_new(draft, digest, |canonical, expected| {
        repair_digest(canonical) == *expected
    })
    .map_err(|error| WenlanError::Validation(error.to_string()))?;
    store.persist_plan(&plan)?;
    Ok(plan)
}

#[cfg(test)]
#[path = "repair_plan_tests.rs"]
mod tests;
