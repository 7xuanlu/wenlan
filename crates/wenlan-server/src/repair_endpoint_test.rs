// SPDX-License-Identifier: Apache-2.0
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;
use wenlan_types::{
    lint::{
        canonical_check_ids, canonical_gate_effect, LintAgentWork, LintApplicability,
        LintCapabilityContext, LintCheckResult, LintCheckResultInput, LintConfigFingerprint,
        LintCoverage, LintDbSnapshotMode, LintDbSnapshotReceipt, LintDigest, LintGateEffect,
        LintOpaqueId, LintOutcome, LintPageSnapshotMode, LintPageSnapshotReceipt, LintPrecondition,
        LintProducerReceipt, LintProfile, LintReport, LintScope, LintSemanticAction,
        LintSemanticCheckId, LintSemanticFinding, LintSemanticPopulation,
        LintSemanticProviderRoute, LintSemanticReasonCode, LintSeverity, LintSnapshotReceipts,
        LintSummaryCode, LintValidationMethod, LINT_MAX_EVIDENCE_PER_CHECK,
    },
    repair::{PrepareRepairRequest, RepairChoice, RepairLintScope},
    repair_plan::RepairPlanRequest,
    MemoryType,
};

fn report(scope: LintScope) -> LintReport {
    report_for_profile(LintProfile::General, scope, None)
}

fn report_for_profile(
    profile: LintProfile,
    scope: LintScope,
    agent_work: Option<LintAgentWork>,
) -> LintReport {
    let digest = || LintDigest::from_u64(1);
    LintReport::try_new_for_profile_with_agent_work(
        profile,
        scope,
        LintCapabilityContext::daemon_operator_endpoint(),
        LintSnapshotReceipts::new(
            LintDbSnapshotReceipt::new(
                LintDbSnapshotMode::TransactionalReadOnly,
                digest(),
                Some(digest()),
            ),
            LintPageSnapshotReceipt::new(
                LintPageSnapshotMode::BestEffort,
                digest(),
                Some(digest()),
            ),
        ),
        LintConfigFingerprint::from_effective_config(&[]),
        LintProducerReceipt::new(None),
        canonical_check_ids(profile)
            .map(|check_id| {
                LintCheckResult::try_new_with_gate_effect(
                    LintCheckResultInput {
                        check_id: check_id.to_string(),
                        outcome: LintOutcome::Pass,
                        severity: LintSeverity::Info,
                        applicability: LintApplicability::Inventory,
                        precondition: LintPrecondition::Ready,
                        coverage: LintCoverage::new(
                            LintValidationMethod::FullEnumeration,
                            0,
                            0,
                            LINT_MAX_EVIDENCE_PER_CHECK,
                            false,
                            0,
                        )
                        .unwrap(),
                        metrics: Vec::new(),
                        summary_code: LintSummaryCode::CheckPassed,
                        recommendation_code: None,
                        evidence: Vec::new(),
                        duration_ms: 0,
                    },
                    canonical_gate_effect(profile, check_id).unwrap_or(LintGateEffect::Actionable),
                )
                .unwrap()
            })
            .collect(),
        agent_work,
    )
    .unwrap()
}

fn plan_request(scope: RepairLintScope, report_scope: LintScope) -> RepairPlanRequest {
    RepairPlanRequest::try_new(scope, report(report_scope), None).unwrap()
}

async fn post_plan(request: RepairPlanRequest, header_space: Option<&str>) -> StatusCode {
    let state = Arc::new(RwLock::new(crate::state::ServerState::default()));
    let mut request_builder = Request::builder()
        .method("POST")
        .uri("/api/repairs/plan")
        .header("Content-Type", "application/json");
    if let Some(space) = header_space {
        request_builder = request_builder.header("X-Wenlan-Space", space);
    }
    crate::router::build_router(state)
        .oneshot(
            request_builder
                .body(Body::from(serde_json::to_vec(&request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

async fn post_prepare(body: serde_json::Value) -> StatusCode {
    let state = Arc::new(RwLock::new(crate::state::ServerState::default()));
    crate::router::build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/repairs/prepare")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn repair_routes_are_distinct_typed_posts() {
    let state = Arc::new(RwLock::new(crate::state::ServerState::default()));
    for path in [
        "/api/repairs/plan",
        "/api/repairs/plan/entries",
        "/api/repairs/prepare",
        "/api/repairs/apply",
        "/api/repairs/verify",
    ] {
        let response = crate::router::build_router(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(path)
                    .header("Content-Type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{path}"
        );
    }
}

#[tokio::test]
async fn repair_plan_rejects_global_body_and_report_when_space_header_is_present() {
    let status = post_plan(
        plan_request(RepairLintScope::global(), LintScope::global()),
        Some("career"),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn repair_plan_rejects_other_registered_body_scope_when_space_header_is_present() {
    let opaque_scope = wenlan_types::lint::LintOpaqueId::from_sorted_position(0).unwrap();
    let status = post_plan(
        plan_request(
            RepairLintScope::registered("ideas".to_string()).unwrap(),
            LintScope::registered(opaque_scope),
        ),
        Some("career"),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn repair_plan_accepts_exact_registered_body_scope_when_space_header_is_present() {
    let opaque_scope = wenlan_types::lint::LintOpaqueId::from_sorted_position(0).unwrap();
    let status = post_plan(
        plan_request(
            RepairLintScope::registered("career".to_string()).unwrap(),
            LintScope::registered(opaque_scope),
        ),
        Some("career"),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "matching scope must reach the existing database precondition"
    );
}

#[tokio::test]
async fn repair_prepare_accepts_every_tagged_choice_and_one_release_legacy_reclassification() {
    let general = report(LintScope::global());
    let deterministic = [
        RepairChoice::rename_page_title(
            "lint_review_title".to_string(),
            "page_exact".to_string(),
            "Before".to_string(),
            "After".to_string(),
        )
        .unwrap(),
        RepairChoice::complete_entity_extraction(
            "lint_review_entities".to_string(),
            "mem_exact".to_string(),
            vec!["entity_exact".to_string()],
        )
        .unwrap(),
    ];
    for choice in deterministic {
        let request = PrepareRepairRequest::try_new_with_choice(
            RepairLintScope::global(),
            general.clone(),
            None,
            choice,
        )
        .unwrap();
        assert_eq!(
            post_prepare(serde_json::to_value(request).unwrap()).await,
            StatusCode::SERVICE_UNAVAILABLE,
            "a valid tagged deterministic choice must reach the DB precondition"
        );
    }

    let populations = LintSemanticCheckId::ALL
        .into_iter()
        .map(|check_id| LintSemanticPopulation::try_new(check_id, 0, 0, 0, false).unwrap())
        .collect();
    let agent_work =
        LintAgentWork::try_new(LintDigest::from_u64(2), populations, vec![], vec![]).unwrap();
    let deep = report_for_profile(LintProfile::Deep, LintScope::global(), Some(agent_work));
    let finding = LintSemanticFinding::try_new(
        LintOpaqueId::from_sorted_position(0).unwrap(),
        LintSemanticAction::ReclassifyMemory,
        LintSemanticReasonCode::ClassificationMismatch,
        9_000,
        LintSemanticProviderRoute::CallingAgent,
        vec![LintDigest::from_u64(3)],
        vec![],
    )
    .unwrap();
    let reclassification = PrepareRepairRequest::try_new_with_choice(
        RepairLintScope::global(),
        general,
        Some(deep),
        RepairChoice::reclassify_memory(finding, MemoryType::Decision).unwrap(),
    )
    .unwrap();
    let tagged = serde_json::to_value(reclassification).unwrap();
    assert_eq!(
        post_prepare(tagged.clone()).await,
        StatusCode::SERVICE_UNAVAILABLE,
        "a valid tagged reclassification must reach the DB precondition"
    );

    let mut legacy = tagged;
    let choice = legacy.as_object_mut().unwrap().remove("choice").unwrap();
    legacy["selected_finding"] = choice["selected_finding"].clone();
    legacy["after_memory_type"] = choice["after_memory_type"].clone();
    assert_eq!(
        post_prepare(legacy).await,
        StatusCode::SERVICE_UNAVAILABLE,
        "the one-release legacy reclassification request remains accepted"
    );
}

#[test]
fn repair_prepare_rejects_global_body_and_reports_when_space_header_is_present() {
    assert!(crate::repair_routes::validate_repair_scope_binding(
        Some("career"),
        &RepairLintScope::global(),
        &LintScope::global(),
        Some(&LintScope::global()),
    )
    .is_err());
}

#[test]
fn repair_prepare_accepts_exact_registered_body_and_report_scopes() {
    let opaque_scope = wenlan_types::lint::LintOpaqueId::from_sorted_position(0).unwrap();
    let report_scope = LintScope::registered(opaque_scope);
    assert!(crate::repair_routes::validate_repair_scope_binding(
        Some("career"),
        &RepairLintScope::registered("career".to_string()).unwrap(),
        &report_scope,
        Some(&report_scope),
    )
    .is_ok());
}

#[test]
fn repair_scope_guard_rejects_non_registered_report_under_matching_header() {
    assert!(crate::repair_routes::validate_repair_scope_binding(
        Some("career"),
        &RepairLintScope::registered("career".to_string()).unwrap(),
        &LintScope::global(),
        None,
    )
    .is_err());
}

#[test]
fn repair_scope_guard_preserves_explicit_global_scope_without_header() {
    assert!(crate::repair_routes::validate_repair_scope_binding(
        None,
        &RepairLintScope::global(),
        &LintScope::global(),
        Some(&LintScope::global()),
    )
    .is_ok());
}
