// SPDX-License-Identifier: Apache-2.0
use super::*;
use serde_json::json;

fn coverage(authorized_denominator: u64) -> LintCoverage {
    LintCoverage::new(
        LintValidationMethod::FullEnumeration,
        authorized_denominator,
        authorized_denominator,
        100,
        false,
        if authorized_denominator == 0 { 0 } else { 1 },
    )
    .unwrap()
}

fn check(
    outcome: LintOutcome,
    severity: LintSeverity,
) -> Result<LintCheckResult, LintContractError> {
    check_with_id("catalog.open_check", outcome, severity)
}

fn check_with_id(
    check_id: &str,
    outcome: LintOutcome,
    severity: LintSeverity,
) -> Result<LintCheckResult, LintContractError> {
    let (applicability, precondition, summary_code, recommendation_code) = match outcome {
        LintOutcome::Pass => (
            LintApplicability::Applicable,
            LintPrecondition::Ready,
            LintSummaryCode::CheckPassed,
            None,
        ),
        LintOutcome::Finding => (
            LintApplicability::Applicable,
            LintPrecondition::Ready,
            LintSummaryCode::FindingDetected,
            Some(LintRecommendationCode::ReviewFinding),
        ),
        LintOutcome::NotRunPrerequisite => (
            LintApplicability::NotApplicable,
            LintPrecondition::MissingPrerequisite,
            LintSummaryCode::PrerequisiteUnavailable,
            Some(LintRecommendationCode::RestorePrerequisite),
        ),
        LintOutcome::InconsistentSnapshot => (
            LintApplicability::Applicable,
            LintPrecondition::SnapshotUnstable,
            LintSummaryCode::SnapshotInconsistent,
            Some(LintRecommendationCode::RerunAfterSnapshotStabilizes),
        ),
        LintOutcome::FailedToRun => (
            LintApplicability::Applicable,
            LintPrecondition::Ready,
            LintSummaryCode::ExecutionFailed,
            Some(LintRecommendationCode::InspectRuntime),
        ),
    };

    LintCheckResult::try_new(LintCheckResultInput {
        check_id: check_id.to_string(),
        outcome,
        severity,
        applicability,
        precondition,
        coverage: coverage(3),
        metrics: vec![LintMetric::new(
            LintMetricCode::ObservedRecords,
            LintMetricValue::CatalogCode {
                code: LintMetricStringCode::Ready,
            },
        )],
        summary_code,
        recommendation_code,
        evidence: vec![LintEvidenceRef::OpaqueId {
            opaque_id: LintOpaqueId::from_sorted_position(0).unwrap(),
        }],
        duration_ms: 4,
    })
}

fn wire_report(scope: LintScope) -> LintReport {
    report(
        scope,
        (0..LINT_GENERAL_CHECK_COUNT)
            .map(|index| {
                check_with_id(
                    &format!("catalog.open_check_{index:02}"),
                    LintOutcome::Pass,
                    LintSeverity::Info,
                )
                .unwrap()
            })
            .collect(),
    )
}

fn snapshots() -> LintSnapshotReceipts {
    LintSnapshotReceipts::new(
        LintDbSnapshotReceipt::new(
            LintDbSnapshotMode::TransactionalReadOnly,
            LintDigest::from_u64(1),
            Some(LintDigest::from_u64(2)),
        ),
        LintPageSnapshotReceipt::new(
            LintPageSnapshotMode::BestEffort,
            LintDigest::from_u64(3),
            Some(LintDigest::from_u64(4)),
        ),
    )
}

fn report(scope: LintScope, checks: Vec<LintCheckResult>) -> LintReport {
    LintReport::try_new(
        scope,
        LintCapabilityContext::daemon_operator_endpoint(),
        snapshots(),
        LintConfigFingerprint::from_effective_config(&[
            LintConfigSelection::new(LintConfigSetting::RerankerEnabled, LintConfigValue::Enabled),
            LintConfigSelection::new(
                LintConfigSetting::PageProjectionEnabled,
                LintConfigValue::Disabled,
            ),
        ]),
        LintProducerReceipt::new(None),
        checks,
    )
    .unwrap()
}

#[test]
fn report_roundtrips_v3_for_each_applied_scope_kind() {
    let scopes = [
        LintScope::global(),
        LintScope::registered(LintOpaqueId::from_sorted_position(0).unwrap()),
        LintScope::uncategorized(),
    ];

    for scope in scopes {
        let report = wire_report(scope);

        let encoded = serde_json::to_value(&report).unwrap();
        let decoded: LintReport = serde_json::from_value(encoded.clone()).unwrap();

        assert!(decoded.complete());
        assert_eq!(encoded["report_schema_version"], json!(3));
        assert_eq!(encoded["check_catalog_version"], json!(1));
        assert_eq!(encoded["profile"], json!("general"));
        assert_eq!(
            encoded["capability_context"],
            json!("daemon_operator_endpoint_unauthenticated_unverified")
        );
        assert_eq!(
            encoded["snapshots"]["db"]["mode"],
            json!("transactional_read_only")
        );
        assert_eq!(encoded["snapshots"]["pages"]["mode"], json!("best_effort"));
        assert!(encoded["producer_receipt"]["runtime_commit"].is_null());
    }
}

#[test]
fn result_accepts_only_legal_outcome_severity_pairs() {
    let legal_pairs = [
        (LintOutcome::Pass, LintSeverity::Info),
        (LintOutcome::Finding, LintSeverity::Warning),
        (LintOutcome::Finding, LintSeverity::Error),
        (LintOutcome::NotRunPrerequisite, LintSeverity::Error),
        (LintOutcome::InconsistentSnapshot, LintSeverity::Error),
        (LintOutcome::FailedToRun, LintSeverity::Error),
    ];

    for outcome in [
        LintOutcome::Pass,
        LintOutcome::Finding,
        LintOutcome::NotRunPrerequisite,
        LintOutcome::InconsistentSnapshot,
        LintOutcome::FailedToRun,
    ] {
        for severity in [
            LintSeverity::Info,
            LintSeverity::Warning,
            LintSeverity::Error,
        ] {
            let result = check(outcome, severity);
            assert_eq!(result.is_ok(), legal_pairs.contains(&(outcome, severity)));
        }
    }

    let expected_empty = LintCheckResult::try_new(LintCheckResultInput {
        check_id: "catalog.empty".to_string(),
        outcome: LintOutcome::Pass,
        severity: LintSeverity::Info,
        applicability: LintApplicability::ExpectedEmpty,
        precondition: LintPrecondition::ConfiguredOff,
        coverage: coverage(0),
        metrics: vec![],
        summary_code: LintSummaryCode::ExpectedEmpty,
        recommendation_code: None,
        evidence: vec![],
        duration_ms: 0,
    });
    assert!(expected_empty.is_ok());
}

#[test]
fn report_derives_completeness_and_totals_from_outcomes() {
    let complete = report(
        LintScope::global(),
        vec![
            check(LintOutcome::Pass, LintSeverity::Info).unwrap(),
            check(LintOutcome::Finding, LintSeverity::Warning).unwrap(),
        ],
    );
    assert!(complete.complete());
    assert_eq!(complete.totals().checks(), 2);
    assert_eq!(complete.totals().findings(), 1);
    assert_eq!(complete.totals().incomplete(), 0);

    let incomplete = report(
        LintScope::global(),
        vec![check(LintOutcome::FailedToRun, LintSeverity::Error).unwrap()],
    );
    assert!(!incomplete.complete());
    assert_eq!(incomplete.totals().incomplete(), 1);

    let mut mismatched = serde_json::to_value(&incomplete).unwrap();
    mismatched["complete"] = json!(true);
    assert!(serde_json::from_value::<LintReport>(mismatched).is_err());
}

#[test]
fn report_separates_actionable_and_advisory_findings() {
    let actionable = check(LintOutcome::Finding, LintSeverity::Error).unwrap();
    let mut advisory_wire =
        serde_json::to_value(check(LintOutcome::Finding, LintSeverity::Warning).unwrap()).unwrap();
    advisory_wire["gate_effect"] = json!("advisory");
    let advisory = serde_json::from_value(advisory_wire).unwrap();
    let report = report(LintScope::global(), vec![actionable, advisory]);

    assert_eq!(report.totals().findings(), 2);
    assert_eq!(report.totals().actionable_findings(), 1);
    assert_eq!(report.totals().advisory_findings(), 1);
}

#[test]
fn rejects_unknown_enums_and_unsupported_schema_versions() {
    let report = wire_report(LintScope::global());

    let mut unknown_enum = serde_json::to_value(&report).unwrap();
    unknown_enum["checks"][0]["outcome"] = json!("future_outcome");
    assert!(serde_json::from_value::<LintReport>(unknown_enum).is_err());

    let mut unsupported_schema = serde_json::to_value(&report).unwrap();
    unsupported_schema["report_schema_version"] = json!(4);
    assert!(serde_json::from_value::<LintReport>(unsupported_schema).is_err());
}

#[test]
fn permits_additive_object_fields_but_rejects_schema_drift() {
    let report = wire_report(LintScope::global());
    let mut additive = serde_json::to_value(&report).unwrap();
    additive["future_top_level"] = json!({ "added": true });
    additive["checks"][0]["future_check_field"] = json!(1);

    assert!(serde_json::from_value::<LintReport>(additive).is_ok());
}

#[test]
fn deserialization_rejects_incomplete_or_duplicate_catalog_shapes() {
    let report = wire_report(LintScope::global());
    let mut missing = serde_json::to_value(&report).unwrap();
    missing["checks"].as_array_mut().unwrap().pop();
    missing["totals"]["checks"] = json!(LINT_GENERAL_CHECK_COUNT - 1);
    assert!(serde_json::from_value::<LintReport>(missing).is_err());

    let mut duplicate = serde_json::to_value(&report).unwrap();
    duplicate["checks"][1] = duplicate["checks"][0].clone();
    assert!(serde_json::from_value::<LintReport>(duplicate).is_err());
}

#[test]
fn bounds_evidence_and_rejects_raw_identifier_or_path_attempts() {
    let too_many = vec![
        LintEvidenceRef::ReasonCode {
            reason_code: LintReasonCode::MissingArtifact,
        };
        101
    ];
    let bounded = LintCheckResult::try_new(LintCheckResultInput {
        check_id: "catalog.bounded".to_string(),
        outcome: LintOutcome::Pass,
        severity: LintSeverity::Info,
        applicability: LintApplicability::Applicable,
        precondition: LintPrecondition::Ready,
        coverage: coverage(101),
        metrics: vec![],
        summary_code: LintSummaryCode::CheckPassed,
        recommendation_code: None,
        evidence: too_many,
        duration_ms: 0,
    });
    assert!(bounded.is_err());

    let arbitrary_path = json!({
        "kind": "safe_root_relative_path",
        "safe_root_relative_path": "../../private/raw-content-canary"
    });
    assert!(serde_json::from_value::<LintEvidenceRef>(arbitrary_path).is_err());

    let arbitrary_identifier = json!({ "kind": "opaque_id", "opaque_id": "person-42" });
    assert!(serde_json::from_value::<LintEvidenceRef>(arbitrary_identifier).is_err());

    let encoded = serde_json::to_string(&report(
        LintScope::registered(LintOpaqueId::from_sorted_position(0).unwrap()),
        vec![check(LintOutcome::Pass, LintSeverity::Info).unwrap()],
    ))
    .unwrap();
    for forbidden in [
        "raw-content-canary",
        "requested_space",
        "config_values",
        "error_message",
        "arbitrary_identifier",
        "\"path\"",
    ] {
        assert!(
            !encoded.contains(forbidden),
            "serialized forbidden value: {forbidden}"
        );
    }
}

#[test]
fn fingerprints_are_deterministic_and_string_metrics_are_closed() {
    let forward = LintConfigFingerprint::from_effective_config(&[
        LintConfigSelection::new(LintConfigSetting::RerankerEnabled, LintConfigValue::Enabled),
        LintConfigSelection::new(
            LintConfigSetting::PageProjectionEnabled,
            LintConfigValue::Disabled,
        ),
    ]);
    let reverse = LintConfigFingerprint::from_effective_config(&[
        LintConfigSelection::new(
            LintConfigSetting::PageProjectionEnabled,
            LintConfigValue::Disabled,
        ),
        LintConfigSelection::new(LintConfigSetting::RerankerEnabled, LintConfigValue::Enabled),
    ]);
    assert_eq!(forward, reverse);

    let report = report(
        LintScope::global(),
        vec![check(LintOutcome::Pass, LintSeverity::Info).unwrap()],
    );
    let mut invalid_metric = serde_json::to_value(&report).unwrap();
    invalid_metric["checks"][0]["metrics"][0]["value"] =
        json!({ "kind": "catalog_code", "code": "untrusted-string" });
    assert!(serde_json::from_value::<LintReport>(invalid_metric).is_err());

    let encoded = serde_json::to_string(&report).unwrap();
    assert!(!encoded.contains("reranker_enabled"));
    assert!(!encoded.contains("page_projection_enabled"));
}

#[test]
fn query_defaults_to_general_and_accepts_only_closed_profiles() {
    let query: LintQuery = serde_json::from_value(json!({ "space": "requested-space" })).unwrap();
    assert_eq!(query.space.as_deref(), Some("requested-space"));
    assert_eq!(query.applied_profile(), LintProfile::General);

    let deep: LintQuery = serde_json::from_value(json!({ "profile": "deep" })).unwrap();
    assert_eq!(deep.applied_profile(), LintProfile::Deep);
    assert!(serde_json::from_value::<LintQuery>(json!({ "profile": "future" })).is_err());

    let general = report(
        LintScope::global(),
        vec![check(LintOutcome::Pass, LintSeverity::Info).unwrap()],
    );
    assert_eq!(general.profile(), LintProfile::General);

    let request: LintRequestQuery = serde_json::from_value(json!({
        "profile": "deep",
        "space": "requested-space",
        "external_egress": true,
        "agent_assist": true
    }))
    .unwrap();
    assert_eq!(request.lint().applied_profile(), LintProfile::Deep);
    assert_eq!(request.lint().space.as_deref(), Some("requested-space"));
    assert!(request.external_egress());
    assert!(request.agent_assist());
    assert_eq!(
        serde_json::to_value(LintRequestQuery::new(
            LintQuery::new(Some(LintProfile::Deep), None),
            false,
            false,
        ))
        .unwrap(),
        json!({ "profile": "deep" })
    );
}

#[test]
fn agent_work_and_submission_are_bounded_typed_contracts() {
    let work = LintAgentWork::try_new(
        LintDigest::from_u64(9),
        2,
        1,
        1,
        vec![
            LintAgentRecord::try_new(
                1,
                LintAgentRecordKind::Memory,
                "bounded memory excerpt".to_string(),
                Some("fact".to_string()),
                None,
                None,
            )
            .unwrap(),
            LintAgentRecord::try_new(
                2,
                LintAgentRecordKind::Page,
                "bounded page excerpt".to_string(),
                None,
                Some(1),
                Some("bounded source excerpt".to_string()),
            )
            .unwrap(),
        ],
    )
    .unwrap();
    assert_eq!(work.records().len(), 2);

    let verdicts = LintSemanticCheckId::ALL
        .into_iter()
        .map(|check_id| {
            LintAgentVerdict::try_new(
                check_id,
                if check_id == LintSemanticCheckId::MemoryContradiction {
                    vec![1]
                } else {
                    vec![]
                },
            )
            .unwrap()
        })
        .collect();
    let submission = LintAgentSubmission::try_new(work.work_digest().clone(), verdicts).unwrap();
    let encoded = serde_json::to_value(&submission).unwrap();
    let decoded: LintAgentSubmission = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded, submission);

    let report = LintReport::try_new_for_profile_with_agent_work(
        LintProfile::Deep,
        LintScope::global(),
        LintCapabilityContext::daemon_operator_endpoint(),
        snapshots(),
        LintConfigFingerprint::from_effective_config(&[]),
        LintProducerReceipt::new(None),
        (0..LINT_DEEP_CHECK_COUNT)
            .map(|index| {
                check_with_id(
                    &format!("catalog.open_deep_check_{index:02}"),
                    LintOutcome::Pass,
                    LintSeverity::Info,
                )
                .unwrap()
            })
            .collect(),
        Some(work.clone()),
    )
    .unwrap();
    let decoded: LintReport =
        serde_json::from_value(serde_json::to_value(&report).unwrap()).unwrap();
    assert_eq!(decoded.agent_work(), Some(&work));

    assert_eq!(
        LintReport::try_new_for_profile_with_agent_work(
            LintProfile::General,
            LintScope::global(),
            LintCapabilityContext::daemon_operator_endpoint(),
            snapshots(),
            LintConfigFingerprint::from_effective_config(&[]),
            LintProducerReceipt::new(None),
            (0..LINT_GENERAL_CHECK_COUNT)
                .map(|index| {
                    check_with_id(
                        &format!("catalog.open_general_check_{index:02}"),
                        LintOutcome::Pass,
                        LintSeverity::Info,
                    )
                    .unwrap()
                })
                .collect(),
            Some(work.clone()),
        ),
        Err(LintContractError::InvalidAgentWork)
    );

    assert!(LintAgentRecord::try_new(
        1,
        LintAgentRecordKind::Memory,
        "x".repeat(LINT_AGENT_EXCERPT_CHAR_CAP + 1),
        None,
        None,
        None,
    )
    .is_err());
    assert!(
        LintAgentVerdict::try_new(LintSemanticCheckId::MemoryContradiction, vec![2, 1],).is_err()
    );
}

#[test]
fn check_group_resolver_uses_the_canonical_seven_groups() {
    assert_eq!(LintCheckGroup::ALL.len(), 7);
    assert_eq!(
        LintCheckGroup::for_check_id("memory_entities.integrity"),
        Some(LintCheckGroup::KnowledgeGraph)
    );
    assert_eq!(
        LintCheckGroup::for_check_id("pages.projection.identity"),
        Some(LintCheckGroup::Pages)
    );
    assert_eq!(LintCheckGroup::for_check_id("fixture.padding"), None);
}

#[test]
fn full_enumeration_rejects_partial_population() {
    let coverage = LintCoverage::new(
        LintValidationMethod::FullEnumeration,
        10,
        1,
        LINT_MAX_EVIDENCE_PER_CHECK,
        false,
        0,
    );

    assert_eq!(coverage, Err(LintContractError::InvalidCoverage));
}

#[test]
fn full_enumeration_accepts_complete_and_legitimately_empty_populations() {
    for (authorized_denominator, evaluated) in [(10, 10), (0, 0)] {
        let coverage = LintCoverage::new(
            LintValidationMethod::FullEnumeration,
            authorized_denominator,
            evaluated,
            LINT_MAX_EVIDENCE_PER_CHECK,
            false,
            0,
        );

        assert!(coverage.is_ok());
    }
}

#[test]
fn intrinsic_sample_preserves_partial_coverage() {
    let coverage = LintCoverage::new(
        LintValidationMethod::IntrinsicSample,
        10,
        1,
        LINT_MAX_EVIDENCE_PER_CHECK,
        false,
        0,
    );

    assert!(coverage.is_ok());
}

#[test]
fn coverage_deserialization_rejects_partial_full_enumeration() {
    let result = serde_json::from_value::<LintCoverage>(json!({
        "method": "full_enumeration",
        "authorized_denominator": 10,
        "evaluated": 1,
        "evidence_cap": 100,
        "truncated": false,
        "evidence_returned": 0
    }));

    let error = result.expect_err("partial full enumeration must fail typed deserialization");
    assert_eq!(error.to_string(), "invalid_lint_coverage");
}

#[test]
fn coverage_deserialization_preserves_valid_methods_and_additive_fields() {
    let cases = [
        ("full_enumeration", 10, 10),
        ("full_enumeration", 0, 0),
        ("exact_aggregate", 10, 1),
        ("intrinsic_sample", 10, 1),
    ];

    for (method, authorized_denominator, evaluated) in cases {
        let coverage = serde_json::from_value::<LintCoverage>(json!({
            "method": method,
            "authorized_denominator": authorized_denominator,
            "evaluated": evaluated,
            "evidence_cap": 100,
            "truncated": false,
            "evidence_returned": 0,
            "future_coverage_field": { "additive": true }
        }));

        assert!(coverage.is_ok());
    }
}
