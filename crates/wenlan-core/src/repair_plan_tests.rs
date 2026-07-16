// SPDX-License-Identifier: Apache-2.0
use super::{build_repair_plan, semantic_action_route, SemanticActionRoute};
use crate::lint::catalog::catalog_for_profile;
use wenlan_types::{
    lint::{
        canonical_gate_effect, LintApplicability, LintCapabilityContext, LintCheckResult,
        LintCheckResultInput, LintConfigFingerprint, LintCoverage, LintDbSnapshotMode,
        LintDbSnapshotReceipt, LintDigest, LintEvidenceRef, LintGateEffect, LintMetric,
        LintOutcome, LintPageSnapshotMode, LintPageSnapshotReceipt, LintPrecondition,
        LintProducerReceipt, LintProfile, LintRecommendationCode, LintReport, LintScope,
        LintSemanticAction, LintSeverity, LintSnapshotReceipts, LintSummaryCode,
        LintValidationMethod, LINT_MAX_EVIDENCE_PER_CHECK,
    },
    repair::RepairLintScope,
    repair_plan::{
        RepairBlockedReasonCode, RepairPlanRequest, RepairResolution, RepairSystemActionKind,
    },
};

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

fn result(profile: LintProfile, check_id: &str, outcome: LintOutcome) -> LintCheckResult {
    let (severity, applicability, precondition, summary, recommendation) = match outcome {
        LintOutcome::Pass => (
            LintSeverity::Info,
            LintApplicability::Applicable,
            LintPrecondition::Ready,
            LintSummaryCode::CheckPassed,
            None,
        ),
        LintOutcome::Finding => (
            LintSeverity::Warning,
            LintApplicability::Applicable,
            LintPrecondition::Ready,
            LintSummaryCode::FindingDetected,
            Some(LintRecommendationCode::ReviewFinding),
        ),
        LintOutcome::NotRunPrerequisite => (
            LintSeverity::Error,
            LintApplicability::NotApplicable,
            LintPrecondition::MissingPrerequisite,
            LintSummaryCode::PrerequisiteUnavailable,
            Some(LintRecommendationCode::RestorePrerequisite),
        ),
        LintOutcome::InconsistentSnapshot => (
            LintSeverity::Error,
            LintApplicability::Applicable,
            LintPrecondition::SnapshotUnstable,
            LintSummaryCode::SnapshotInconsistent,
            Some(LintRecommendationCode::RerunAfterSnapshotStabilizes),
        ),
        LintOutcome::FailedToRun => (
            LintSeverity::Error,
            LintApplicability::Applicable,
            LintPrecondition::Ready,
            LintSummaryCode::ExecutionFailed,
            Some(LintRecommendationCode::InspectRuntime),
        ),
    };
    LintCheckResult::try_new_with_gate_effect(
        LintCheckResultInput {
            check_id: check_id.to_string(),
            outcome,
            severity,
            applicability,
            precondition,
            coverage: LintCoverage::new(
                LintValidationMethod::FullEnumeration,
                0,
                0,
                LINT_MAX_EVIDENCE_PER_CHECK,
                false,
                0,
            )
            .unwrap(),
            metrics: Vec::<LintMetric>::new(),
            summary_code: summary,
            recommendation_code: recommendation,
            evidence: Vec::<LintEvidenceRef>::new(),
            duration_ms: 0,
        },
        canonical_gate_effect(profile, check_id).unwrap_or(LintGateEffect::Actionable),
    )
    .unwrap()
}

fn report(profile: LintProfile, outcomes: &[(&str, LintOutcome)], seed: u64) -> LintReport {
    let checks = catalog_for_profile(profile)
        .map(|entry| {
            let outcome = outcomes
                .iter()
                .find_map(|(check_id, outcome)| (*check_id == entry.id).then_some(*outcome))
                .unwrap_or(LintOutcome::Pass);
            result(profile, entry.id, outcome)
        })
        .collect();
    LintReport::try_new_for_profile(
        profile,
        LintScope::global(),
        LintCapabilityContext::daemon_operator_endpoint(),
        snapshots(seed),
        LintConfigFingerprint::from_effective_config(&[]),
        LintProducerReceipt::new(None),
        checks,
    )
    .unwrap()
}

#[test]
fn all_55_general_findings_have_a_visible_resolution() {
    let outcomes = catalog_for_profile(LintProfile::General)
        .map(|entry| (entry.id, LintOutcome::Finding))
        .collect::<Vec<_>>();
    let request = RepairPlanRequest::try_new(
        RepairLintScope::global(),
        report(LintProfile::General, &outcomes, 1),
        None,
    )
    .unwrap();

    let plan = build_repair_plan(request).unwrap();
    assert_eq!(plan.entries().len(), 55);
    assert_eq!(plan.totals().deterministic(), 55);
    assert_eq!(plan.totals().semantic(), 0);
    assert!(plan.deterministic_complete());
    assert!(!plan.semantic_complete());
}

#[test]
fn unsupported_deterministic_writer_is_blocked_not_filtered() {
    let request = RepairPlanRequest::try_new(
        RepairLintScope::global(),
        report(
            LintProfile::General,
            &[("identity.registry_integrity", LintOutcome::Finding)],
            1,
        ),
        None,
    )
    .unwrap();

    let plan = build_repair_plan(request).unwrap();
    assert_eq!(plan.entries().len(), 1);
    assert_eq!(plan.entries()[0].check_id(), "identity.registry_integrity");
    assert!(matches!(
        plan.entries()[0].resolution(),
        RepairResolution::Blocked { blocked }
            if blocked.reason_code() == RepairBlockedReasonCode::UnsupportedDeterministicWriter
    ));
}

#[test]
fn incomplete_semantic_deep_never_hides_complete_deterministic_entries() {
    let general = report(
        LintProfile::General,
        &[("identity.registry_integrity", LintOutcome::Finding)],
        1,
    );
    let deep = report(
        LintProfile::Deep,
        &[(
            "memories.semantic.classification",
            LintOutcome::NotRunPrerequisite,
        )],
        3,
    );
    let request =
        RepairPlanRequest::try_new(RepairLintScope::global(), general, Some(deep)).unwrap();

    let plan = build_repair_plan(request).unwrap();
    assert!(plan.deterministic_complete());
    assert!(!plan.semantic_complete());
    assert!(plan
        .entries()
        .iter()
        .any(|entry| entry.check_id() == "identity.registry_integrity"));
    assert!(plan.entries().iter().any(|entry| {
        entry.check_id() == "memories.semantic.classification"
            && matches!(
                entry.resolution(),
                RepairResolution::Blocked { blocked }
                    if blocked.reason_code() == RepairBlockedReasonCode::SourceIncomplete
            )
    }));
}

#[test]
fn schema_and_route_contracts_are_system_actions_not_data_manifests() {
    let request = RepairPlanRequest::try_new(
        RepairLintScope::global(),
        report(
            LintProfile::General,
            &[
                ("runtime.schema_contract", LintOutcome::Finding),
                ("serving.route_scope_contracts", LintOutcome::Finding),
            ],
            1,
        ),
        None,
    )
    .unwrap();

    let plan = build_repair_plan(request).unwrap();
    let kinds = plan
        .entries()
        .iter()
        .map(|entry| match entry.resolution() {
            RepairResolution::SystemAction { system_action } => system_action.kind(),
            other => panic!("expected system action, got {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        [
            RepairSystemActionKind::RunSchemaMigration,
            RepairSystemActionKind::CorrectRouteScopeContract,
        ]
    );
}

#[test]
fn every_semantic_action_has_an_explicit_resolution_route() {
    use LintSemanticAction::*;

    let routes = [
        ReclassifyMemory,
        ReviewContradiction,
        ReviewStaleness,
        SupersedeMemory,
        AddMemoryEntityLink,
        RemoveMemoryEntityLink,
        AddEntityRelation,
        RemoveEntityRelation,
        ReviewPageClaim,
        AddPageEvidence,
        RemovePageEvidence,
        ReviewRetrieval,
    ]
    .map(semantic_action_route);
    assert_eq!(routes[0], SemanticActionRoute::ExactCandidate);
    assert_eq!(
        routes
            .iter()
            .filter(|route| **route == SemanticActionRoute::Review)
            .count(),
        4
    );
    assert_eq!(
        routes
            .iter()
            .filter(|route| **route == SemanticActionRoute::MutationCandidate)
            .count(),
        7
    );
}

#[tokio::test]
async fn lint_review_enqueue_is_idempotent_and_does_not_resurrect_terminal_items() {
    let (db, _dir) = crate::db::tests::test_db().await;
    let review_id = format!(
        "lint_review_{}",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    let source_ids = vec!["mem_a".to_string()];
    let payload = serde_json::json!({
        "action": "lint_repair_review",
        "check_id": "identity.memory_state_integrity",
        "occurrence_digest": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "issue": "ambiguous state",
        "choices": ["confirm", "unpin"],
        "suggested_research_queries": []
    })
    .to_string();

    assert!(db
        .insert_lint_review_if_absent(&review_id, &source_ids, &payload)
        .await
        .unwrap());
    assert!(!db
        .insert_lint_review_if_absent(&review_id, &source_ids, &payload)
        .await
        .unwrap());
    db.conn
        .lock()
        .await
        .execute(
            "UPDATE refinement_queue SET status = 'dismissed' WHERE id = ?1",
            libsql::params![review_id.clone()],
        )
        .await
        .unwrap();
    assert!(!db
        .insert_lint_review_if_absent(&review_id, &source_ids, &payload)
        .await
        .unwrap());
    assert_eq!(
        db.get_refinement_proposal(&review_id)
            .await
            .unwrap()
            .unwrap()
            .status,
        "dismissed"
    );
}
