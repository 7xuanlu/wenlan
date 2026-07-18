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
        PrepareRepairPlanResponse, RepairAffectedRecord, RepairAffectedRecordKind, RepairBlocked,
        RepairBlockedReasonCode, RepairFindingKind, RepairPlan, RepairPlanDraft,
        RepairPlanEntriesPage, RepairPlanEntriesRequest, RepairPlanEntry, RepairPlanReportReceipt,
        RepairResolution, RepairReviewItem, RepairSystemAction, RepairSystemActionKind,
        StoredRepairPlan, REPAIR_PLAN_PAGE_MAX_BYTES, REPAIR_PLAN_PAGE_MAX_ENTRIES,
        REPAIR_PLAN_SCHEMA_VERSION,
    },
    ProposalAction, RefinementPayload,
};

const SHA256_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA256_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn plan_digest(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn signed_plan(draft: RepairPlanDraft) -> RepairPlan {
    let digest = RepairDigest::parse(&plan_digest(&draft.canonical_bytes().unwrap())).unwrap();
    RepairPlan::try_new(draft, digest, |canonical, expected| {
        plan_digest(canonical) == expected.as_str()
    })
    .unwrap()
}

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
    let general_baseline = vec![RepairCheckBaseline::try_new_current(
        "memories.structural.integrity".into(),
        LintOutcome::Pass,
        LintGateEffect::Actionable,
        0,
        vec![],
    )
    .unwrap()];
    let deep_baseline = vec![RepairCheckBaseline::try_new_current(
        "memories.semantic.classification".into(),
        LintOutcome::Finding,
        LintGateEffect::Actionable,
        1,
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
    let plan = signed_plan(draft);
    let mut value = serde_json::to_value(plan).unwrap();
    value["totals"]["blocked"] = serde_json::json!(99);
    let stored: StoredRepairPlan = serde_json::from_value(value).unwrap();
    assert!(stored
        .verify_and_try_into_current(|canonical, expected| {
            plan_digest(canonical) == expected.as_str()
        })
        .is_err());
}

#[test]
fn plan_rejects_duplicate_ready_manifest_tuples() {
    let manifest = fixture_manifest();
    let ready_entry = |occurrence_digest: &str| {
        RepairPlanEntry::try_new(
            RepairFindingKind::Semantic,
            "memories.semantic.classification".to_string(),
            RepairDigest::parse(occurrence_digest).unwrap(),
            vec![record(RepairAffectedRecordKind::Memory, "mem_target")],
            RepairResolution::Ready {
                manifest: Box::new(manifest.clone()),
            },
        )
        .unwrap()
    };

    assert!(
        RepairPlanDraft::try_new(
            "repair_plan_550e8400-e29b-41d4-a716-446655440000".to_string(),
            RepairLintScope::global(),
            report_receipt(LintProfile::General, 1),
            Some(report_receipt(LintProfile::Deep, 3)),
            true,
            true,
            vec![ready_entry(SHA256_A), ready_entry(SHA256_B)],
        )
        .is_err(),
        "one prepared manifest must map to exactly one approval tuple"
    );
}

#[test]
fn plan_rejects_complete_phase_with_source_incomplete_entry() {
    let incomplete = RepairPlanEntry::try_new(
        RepairFindingKind::Deterministic,
        "identity.registry_integrity".to_string(),
        RepairDigest::parse(SHA256_A).unwrap(),
        vec![],
        RepairResolution::Blocked {
            blocked: RepairBlocked::try_new(
                RepairBlockedReasonCode::SourceIncomplete,
                "check failed to run".to_string(),
                "rerun lint".to_string(),
            )
            .unwrap(),
        },
    )
    .unwrap();

    assert!(RepairPlanDraft::try_new(
        "repair_plan_550e8400-e29b-41d4-a716-446655440000".to_string(),
        RepairLintScope::global(),
        report_receipt(LintProfile::General, 1),
        None,
        true,
        false,
        vec![incomplete],
    )
    .is_err());

    assert!(RepairPlanDraft::try_new(
        "repair_plan_550e8400-e29b-41d4-a716-446655440000".to_string(),
        RepairLintScope::global(),
        report_receipt(LintProfile::General, 1),
        None,
        false,
        false,
        vec![blocked_entry("identity.registry_integrity", SHA256_A)],
    )
    .is_err());
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
    let plan = signed_plan(draft);
    assert_eq!(plan.schema_version(), REPAIR_PLAN_SCHEMA_VERSION);
    assert_eq!(plan.totals().blocked(), 1);
    assert_eq!(plan.entries().len(), 1);

    let canonical: serde_json::Value =
        serde_json::from_slice(&plan.canonical_unsigned_bytes().unwrap()).unwrap();
    assert!(canonical.get("plan_digest").is_none());
    let stored: StoredRepairPlan =
        serde_json::from_slice(&serde_json::to_vec(&plan).unwrap()).unwrap();
    assert_eq!(
        stored
            .verify_and_try_into_current(|canonical, expected| {
                plan_digest(canonical) == expected.as_str()
            })
            .unwrap(),
        plan
    );

    let mut tampered = serde_json::to_value(&plan).unwrap();
    tampered["plan_digest"] = serde_json::json!(SHA256_A);
    let stored: StoredRepairPlan = serde_json::from_value(tampered).unwrap();
    assert!(stored
        .verify_and_try_into_current(|canonical, expected| {
            plan_digest(canonical) == expected.as_str()
        })
        .is_err());
}

#[test]
fn compact_plan_summary_omits_entries_and_pages_bind_the_exact_digest() {
    let entry = blocked_entry("identity.registry_integrity", SHA256_A);
    let draft = RepairPlanDraft::try_new(
        "repair_plan_550e8400-e29b-41d4-a716-446655440000".to_string(),
        RepairLintScope::global(),
        report_receipt(LintProfile::General, 1),
        None,
        true,
        false,
        vec![entry.clone()],
    )
    .unwrap();
    let plan = signed_plan(draft);
    let response = PrepareRepairPlanResponse::try_new(
        plan.clone(),
        "/private/repairs/plans/plan.jsonl".to_string(),
    )
    .unwrap();
    let summary = serde_json::to_value(response.compact_summary().unwrap()).unwrap();
    assert!(summary.get("entries").is_none());
    assert_eq!(summary["entry_count"], 1);
    assert_eq!(summary["plan_digest"], plan.plan_digest().as_str());
    let _: super::repair_plan::RepairPlanSummary = serde_json::from_value(summary.clone()).unwrap();
    let mut oversized_summary = summary.clone();
    oversized_summary["entries"] = serde_json::json!([]);
    assert!(
        serde_json::from_value::<super::repair_plan::RepairPlanSummary>(oversized_summary).is_err()
    );

    let request = RepairPlanEntriesRequest::try_new(
        plan.plan_id().to_string(),
        plan.plan_digest().clone(),
        0,
        1,
    )
    .unwrap();
    let request: RepairPlanEntriesRequest =
        serde_json::from_value(serde_json::to_value(request).unwrap()).unwrap();
    assert_eq!(request.offset(), 0);
    assert_eq!(request.limit(), 1);
    assert!(RepairPlanEntriesRequest::try_new(
        plan.plan_id().to_string(),
        plan.plan_digest().clone(),
        0,
        0,
    )
    .is_err());
    assert!(RepairPlanEntriesRequest::try_new(
        plan.plan_id().to_string(),
        plan.plan_digest().clone(),
        0,
        REPAIR_PLAN_PAGE_MAX_ENTRIES + 1,
    )
    .is_err());

    let page = RepairPlanEntriesPage::try_new(
        plan.plan_id().to_string(),
        plan.plan_digest().clone(),
        plan.scope().clone(),
        0,
        1,
        vec![entry],
    )
    .unwrap();
    let page_value = serde_json::to_value(page).unwrap();
    assert!(page_value.get("next_offset").is_none());
    assert_eq!(
        serde_json::from_value::<RepairPlanEntriesPage>(page_value)
            .unwrap()
            .entries()
            .len(),
        1
    );

    let oversized_entry = RepairPlanEntry::try_new(
        RepairFindingKind::Deterministic,
        "identity.registry_integrity".to_string(),
        RepairDigest::parse(SHA256_B).unwrap(),
        vec![],
        RepairResolution::Blocked {
            blocked: RepairBlocked::try_new(
                RepairBlockedReasonCode::UnsupportedDeterministicWriter,
                "x".repeat(REPAIR_PLAN_PAGE_MAX_BYTES),
                "add a typed adapter".to_string(),
            )
            .unwrap(),
        },
    )
    .unwrap();
    assert!(RepairPlanEntriesPage::try_new(
        plan.plan_id().to_string(),
        plan.plan_digest().clone(),
        plan.scope().clone(),
        0,
        1,
        vec![oversized_entry],
    )
    .is_err());
}

#[test]
fn lint_repair_review_is_a_typed_refinement_payload() {
    let action: ProposalAction = serde_json::from_str("\"lint_repair_review\"").unwrap();
    assert_eq!(action, ProposalAction::LintRepairReview);

    let payload = RefinementPayload::LintRepairReview {
        check_id: "pages.links.orphan_labels".to_string(),
        occurrence_digest: RepairDigest::parse(SHA256_A).unwrap(),
        owner_binding_digest: RepairDigest::parse(SHA256_B).unwrap(),
        issue: "ambiguous page label".to_string(),
        choices: vec!["bind page_a".to_string(), "bind page_b".to_string()],
        suggested_research_queries: vec!["which page owns label X".to_string()],
    };
    let value = serde_json::to_value(&payload).unwrap();
    assert_eq!(value["action"], "lint_repair_review");
    let mut missing_owner_binding = value.clone();
    missing_owner_binding
        .as_object_mut()
        .unwrap()
        .remove("owner_binding_digest");
    assert!(serde_json::from_value::<RefinementPayload>(missing_owner_binding).is_err());
    assert_eq!(
        serde_json::from_value::<RefinementPayload>(value).unwrap(),
        payload
    );
}

#[test]
fn deterministic_manifest_targets_writers_and_mutations_are_typed() {
    let source = RepairSource::try_new_deterministic(
        RepairLintScope::global(),
        LintScope::global(),
        "identity.memory_state_integrity".to_string(),
        vec![],
        snapshots(1),
        snapshots(3),
        LintProducerReceipt::new(None),
        LintProducerReceipt::new(None),
    )
    .unwrap();
    let target = RepairTarget::memory(
        "mem_target".to_string(),
        RepairScope::registered("work".to_string()).unwrap(),
    )
    .unwrap();
    let general = vec![RepairCheckBaseline::try_new_current(
        "identity.memory_state_integrity".to_string(),
        LintOutcome::Finding,
        LintGateEffect::Actionable,
        1,
        vec![],
    )
    .unwrap()];
    let deep = general.clone();
    let assertions = RepairPostAssertions::try_new_for_check(
        "identity.memory_state_integrity".to_string(),
        LintDigest::from_u64(8),
        general,
        deep,
        vec![],
        vec![],
    )
    .unwrap();
    let draft = RepairManifestDraft::try_new(
        "repair_550e8400-e29b-41d4-a716-446655440001".to_string(),
        1_721_000_000,
        source,
        target.clone(),
        RepairExpectedState::try_new(None, RepairDigest::parse(SHA256_A).unwrap()).unwrap(),
        RepairWriter::NormalizeMemorySourceAgent,
        RepairMutation::normalize_memory_source_agent("   ".to_string()).unwrap(),
        RepairAllowedEffects::memory_source_agent(target),
        RepairRollbackArtifact::try_new(
            "rollback-v2.json".to_string(),
            RepairDigest::parse(SHA256_B).unwrap(),
        )
        .unwrap(),
        assertions,
    )
    .unwrap();
    let manifest = RepairManifest::try_new(draft, RepairDigest::parse(SHA256_B).unwrap()).unwrap();
    assert_eq!(manifest.writer(), RepairWriter::NormalizeMemorySourceAgent);
    assert!(matches!(
        manifest.mutation(),
        RepairMutation::NormalizeMemorySourceAgent {
            before_source_agent
        } if before_source_agent == "   "
    ));
    assert_eq!(
        serde_json::from_slice::<RepairManifest>(&serde_json::to_vec(&manifest).unwrap()).unwrap(),
        manifest
    );

    let tag = RepairTarget::tag(
        "memory".to_string(),
        "missing".to_string(),
        "stale".to_string(),
    )
    .unwrap();
    let link = RepairTarget::page_link(
        "page_a".to_string(),
        "topic".to_string(),
        RepairScope::registered("work".to_string()).unwrap(),
    )
    .unwrap();
    assert_eq!(tag.scope(), &RepairScope::global());
    assert_eq!(link.scope().space(), Some("work"));
    assert!(RepairMutation::delete_tag_row("memory", "missing", "stale").is_ok());
    assert!(RepairMutation::bind_page_link(None, "page_target".to_string()).is_ok());
    assert!(RepairMutation::clear_memory_supersedes("mem_target".to_string()).is_ok());
}
