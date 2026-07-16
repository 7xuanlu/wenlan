// SPDX-License-Identifier: Apache-2.0
use crate::{
    lint::{
        LintDbSnapshotMode, LintDbSnapshotReceipt, LintDigest, LintEvidenceRef, LintGateEffect,
        LintOpaqueId, LintOutcome, LintPageSnapshotMode, LintPageSnapshotReceipt,
        LintProducerReceipt, LintProfile, LintScope, LintSemanticAction, LintSemanticFinding,
        LintSemanticProviderRoute, LintSemanticReasonCode, LintSnapshotReceipts,
    },
    repair::{
        RepairAllowedEffects, RepairCheckBaseline, RepairDigest, RepairExpectedState,
        RepairLintScope, RepairManifest, RepairManifestDraft, RepairMutation, RepairPostAssertions,
        RepairRollbackArtifact, RepairScope, RepairSource, RepairTarget, RepairWriter,
    },
    repair_plan::{
        RepairAffectedRecord, RepairAffectedRecordKind, RepairBlocked, RepairBlockedReasonCode,
        RepairFindingKind, RepairPlan, RepairPlanDraft, RepairPlanEntry, RepairPlanReportReceipt,
        RepairResolution, RepairReviewItem, RepairSystemAction, RepairSystemActionKind,
        REPAIR_PLAN_SCHEMA_VERSION,
    },
    ProposalAction, RefinementPayload,
};

const SHA256_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA256_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

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

fn report_receipt(profile: LintProfile, seed: u64) -> RepairPlanReportReceipt {
    RepairPlanReportReceipt::try_new(
        profile,
        LintScope::global(),
        snapshots(seed),
        LintProducerReceipt::new(None),
    )
    .unwrap()
}

fn record(kind: RepairAffectedRecordKind, id: &str) -> RepairAffectedRecord {
    RepairAffectedRecord::try_new(kind, id.to_string()).unwrap()
}

fn fixture_manifest() -> RepairManifest {
    let evidence_id = LintDigest::from_u64(42);
    let finding = LintSemanticFinding::try_new(
        LintOpaqueId::from_sorted_position(0).unwrap(),
        LintSemanticAction::ReclassifyMemory,
        LintSemanticReasonCode::ClassificationMismatch,
        9_000,
        LintSemanticProviderRoute::CallingAgent,
        vec![evidence_id.clone()],
        vec![],
    )
    .unwrap();
    let source = RepairSource::try_new(
        RepairLintScope::global(),
        LintScope::global(),
        finding.clone(),
        snapshots(1),
        snapshots(3),
        LintProducerReceipt::new(None),
        LintProducerReceipt::new(None),
        LintDigest::from_u64(5),
    )
    .unwrap();
    let target = RepairTarget::memory(
        "mem_target".into(),
        RepairScope::registered("work".into()).unwrap(),
    )
    .unwrap();
    let general_baseline = vec![RepairCheckBaseline::try_new(
        "memories.structural.integrity".into(),
        LintOutcome::Pass,
        LintGateEffect::Actionable,
        vec![],
    )
    .unwrap()];
    let deep_baseline = vec![RepairCheckBaseline::try_new(
        "memories.semantic.classification".into(),
        LintOutcome::Finding,
        LintGateEffect::Actionable,
        vec![LintEvidenceRef::SemanticFinding { finding }],
    )
    .unwrap()];
    let draft = RepairManifestDraft::try_new(
        "repair_550e8400-e29b-41d4-a716-446655440000".into(),
        1_721_000_000,
        source,
        target.clone(),
        RepairExpectedState::try_new(None, RepairDigest::parse(SHA256_A).unwrap()).unwrap(),
        RepairWriter::ReclassifyMemory,
        RepairMutation::try_reclassify(Some("fact"), "decision").unwrap(),
        RepairAllowedEffects::memory_type(target),
        RepairRollbackArtifact::try_new(
            "rollback-v2.json".into(),
            RepairDigest::parse(SHA256_B).unwrap(),
        )
        .unwrap(),
        RepairPostAssertions::try_new(evidence_id, general_baseline, deep_baseline, vec![])
            .unwrap(),
    )
    .unwrap();
    RepairManifest::try_new(draft, RepairDigest::parse(SHA256_B).unwrap()).unwrap()
}

fn blocked_entry(check_id: &str, digest: &str) -> RepairPlanEntry {
    RepairPlanEntry::try_new(
        RepairFindingKind::Deterministic,
        check_id.to_string(),
        RepairDigest::parse(digest).unwrap(),
        vec![record(RepairAffectedRecordKind::Memory, "mem_1")],
        RepairResolution::Blocked {
            blocked: RepairBlocked::try_new(
                RepairBlockedReasonCode::UnsupportedDeterministicWriter,
                "writer not registered".to_string(),
                "show the finding and add a typed adapter".to_string(),
            )
            .unwrap(),
        },
    )
    .unwrap()
}

#[test]
fn all_resolution_variants_round_trip_with_stable_tags() {
    let ready = RepairResolution::Ready {
        manifest: Box::new(fixture_manifest()),
    };
    let review = RepairResolution::Review {
        review_item: RepairReviewItem::try_new(
            format!("lint_review_{SHA256_A}"),
            "pages.links.orphan_labels".to_string(),
            "label has two active same-scope targets".to_string(),
            vec!["bind page_a".to_string(), "bind page_b".to_string()],
            vec!["search the source memories for the intended page".to_string()],
        )
        .unwrap(),
    };
    let system_action = RepairResolution::SystemAction {
        system_action: RepairSystemAction::try_new(
            RepairSystemActionKind::RestartDaemon,
            "restart the updated daemon".to_string(),
            vec!["route catalog differs from the running binary".to_string()],
        )
        .unwrap(),
    };
    let blocked = blocked_entry("identity.registry_integrity", SHA256_A)
        .resolution()
        .clone();

    for (resolution, expected_tag) in [
        (ready, "ready"),
        (review, "review"),
        (system_action, "system_action"),
        (blocked, "blocked"),
    ] {
        let value = serde_json::to_value(&resolution).unwrap();
        assert_eq!(value["disposition"], expected_tag);
        assert_eq!(
            serde_json::from_value::<RepairResolution>(value).unwrap(),
            resolution
        );
    }
}

#[test]
fn target_resolutions_reject_blank_ids_and_missing_affected_records() {
    assert!(
        RepairAffectedRecord::try_new(RepairAffectedRecordKind::Memory, " mem_1 ".to_string())
            .is_err()
    );

    let review = RepairResolution::Review {
        review_item: RepairReviewItem::try_new(
            format!("lint_review_{SHA256_A}"),
            "identity.memory_state_integrity".to_string(),
            "ambiguous state".to_string(),
            vec!["confirm".to_string(), "unpin".to_string()],
            vec![],
        )
        .unwrap(),
    };
    assert!(RepairPlanEntry::try_new(
        RepairFindingKind::Deterministic,
        " ".to_string(),
        RepairDigest::parse(SHA256_A).unwrap(),
        vec![record(RepairAffectedRecordKind::Memory, "mem_1")],
        review.clone(),
    )
    .is_err());
    assert!(RepairPlanEntry::try_new(
        RepairFindingKind::Deterministic,
        "identity.memory_state_integrity".to_string(),
        RepairDigest::parse(SHA256_A).unwrap(),
        vec![],
        review,
    )
    .is_err());
}

#[test]
fn plan_rejects_duplicate_occurrences_and_tampered_totals() {
    let first = blocked_entry("identity.registry_integrity", SHA256_A);
    assert!(RepairPlanDraft::try_new(
        "repair_plan_550e8400-e29b-41d4-a716-446655440000".to_string(),
        RepairLintScope::global(),
        report_receipt(LintProfile::General, 1),
        None,
        true,
        false,
        vec![first.clone(), first],
    )
    .is_err());

    let draft = RepairPlanDraft::try_new(
        "repair_plan_550e8400-e29b-41d4-a716-446655440000".to_string(),
        RepairLintScope::global(),
        report_receipt(LintProfile::General, 1),
        Some(report_receipt(LintProfile::Deep, 3)),
        true,
        true,
        vec![blocked_entry("identity.registry_integrity", SHA256_A)],
    )
    .unwrap();
    let plan = RepairPlan::try_new(draft, RepairDigest::parse(SHA256_B).unwrap()).unwrap();
    let mut value = serde_json::to_value(plan).unwrap();
    value["totals"]["blocked"] = serde_json::json!(99);
    assert!(serde_json::from_value::<RepairPlan>(value).is_err());
}

#[test]
fn plan_round_trips_and_canonical_bytes_exclude_digest() {
    let draft = RepairPlanDraft::try_new(
        "repair_plan_550e8400-e29b-41d4-a716-446655440000".to_string(),
        RepairLintScope::global(),
        report_receipt(LintProfile::General, 1),
        None,
        true,
        false,
        vec![blocked_entry("identity.registry_integrity", SHA256_A)],
    )
    .unwrap();
    let plan = RepairPlan::try_new(draft, RepairDigest::parse(SHA256_B).unwrap()).unwrap();
    assert_eq!(plan.schema_version(), REPAIR_PLAN_SCHEMA_VERSION);
    assert_eq!(plan.totals().blocked(), 1);
    assert_eq!(plan.entries().len(), 1);

    let canonical: serde_json::Value =
        serde_json::from_slice(&plan.canonical_unsigned_bytes().unwrap()).unwrap();
    assert!(canonical.get("plan_digest").is_none());
    assert_eq!(
        serde_json::from_slice::<RepairPlan>(&serde_json::to_vec(&plan).unwrap()).unwrap(),
        plan
    );
}

#[test]
fn lint_repair_review_is_a_typed_refinement_payload() {
    let action: ProposalAction = serde_json::from_str("\"lint_repair_review\"").unwrap();
    assert_eq!(action, ProposalAction::LintRepairReview);

    let payload = RefinementPayload::LintRepairReview {
        check_id: "pages.links.orphan_labels".to_string(),
        occurrence_digest: RepairDigest::parse(SHA256_A).unwrap(),
        issue: "ambiguous page label".to_string(),
        choices: vec!["bind page_a".to_string(), "bind page_b".to_string()],
        suggested_research_queries: vec!["which page owns label X".to_string()],
    };
    let value = serde_json::to_value(&payload).unwrap();
    assert_eq!(value["action"], "lint_repair_review");
    assert_eq!(
        serde_json::from_value::<RefinementPayload>(value).unwrap(),
        payload
    );
}
