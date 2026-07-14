// SPDX-License-Identifier: Apache-2.0
use crate::{
    lint::{
        LintDbSnapshotMode, LintDbSnapshotReceipt, LintDigest, LintOpaqueId, LintPageSnapshotMode,
        LintPageSnapshotReceipt, LintProducerReceipt, LintScope, LintSemanticAction,
        LintSemanticFinding, LintSemanticProviderRoute, LintSemanticReasonCode,
        LintSnapshotReceipts,
    },
    repair::{
        ApplyRepairRequest, RepairAllowedEffects, RepairApplyReceipt, RepairDigest,
        RepairExpectedState, RepairLintScope, RepairManifest, RepairManifestDraft, RepairMutation,
        RepairPostAssertions, RepairRollbackArtifact, RepairScope, RepairSource, RepairTarget,
        RepairVerificationReceipt, RepairWriter,
    },
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
        finding,
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
            "rollback-v1.json".into(),
            RepairDigest::parse(SHA256_B).unwrap(),
        )
        .unwrap(),
        RepairPostAssertions::try_new(evidence_id, vec![]).unwrap(),
    )
    .unwrap();
    RepairManifest::try_new(draft, RepairDigest::parse(SHA256_A).unwrap()).unwrap()
}

#[test]
fn repair_digest_requires_exact_lowercase_sha256() {
    let digest = RepairDigest::parse(SHA256_A).expect("valid digest");
    assert_eq!(digest.as_str(), SHA256_A);

    for invalid in [
        "aaaaaaaaaaaaaaaa",
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "gggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggg",
    ] {
        assert!(RepairDigest::parse(invalid).is_err(), "accepted {invalid}");
    }
}

#[test]
fn durable_lint_scope_is_typed_and_matches_report_scope_kind() {
    let registered = RepairLintScope::registered("work".into()).unwrap();
    assert!(registered.matches_report_scope_kind(&LintScope::registered(
        LintOpaqueId::from_sorted_position(0).unwrap()
    )));
    assert!(!registered.matches_report_scope_kind(&LintScope::global()));
    assert!(RepairLintScope::registered(" ".into()).is_err());

    let value = serde_json::json!({"kind": "registered", "space": "work", "extra": true});
    assert!(serde_json::from_value::<RepairLintScope>(value).is_err());
}

#[test]
fn reclassification_rejects_noop_and_noncanonical_types() {
    assert!(RepairMutation::try_reclassify(Some("fact"), "fact").is_err());
    assert!(RepairMutation::try_reclassify(Some("fact"), "custom").is_err());

    let mutation = RepairMutation::try_reclassify(Some("fact"), "decision")
        .expect("supported reclassification");
    assert_eq!(mutation.before_memory_type(), Some("fact"));
    assert_eq!(mutation.after_memory_type(), "decision");
}

#[test]
fn mutation_deserializer_rejects_unknown_fields() {
    let value = serde_json::json!({
        "kind": "reclassify_memory",
        "before_memory_type": "fact",
        "after_memory_type": "decision",
        "hidden_target": "mem_other"
    });

    assert!(serde_json::from_value::<RepairMutation>(value).is_err());
}

#[test]
fn apply_request_binds_exact_manifest_digest() {
    let request = ApplyRepairRequest::try_new(
        "repair_550e8400-e29b-41d4-a716-446655440000".into(),
        RepairDigest::parse(SHA256_A).unwrap(),
    )
    .expect("valid apply request");

    assert_eq!(
        request.manifest_id(),
        "repair_550e8400-e29b-41d4-a716-446655440000"
    );
    assert_eq!(request.approved_manifest_digest().as_str(), SHA256_A);
}

#[test]
fn manifest_roundtrips_and_rejects_unsupported_writer() {
    let manifest = fixture_manifest();
    let value = serde_json::to_value(&manifest).unwrap();
    let roundtrip: RepairManifest = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(roundtrip, manifest);

    let mut unsupported = value;
    unsupported["writer"] = serde_json::json!("refresh_page");
    assert!(serde_json::from_value::<RepairManifest>(unsupported).is_err());
}

#[test]
fn manifest_rejects_noop_wrong_schema_and_unknown_fields() {
    let value = serde_json::to_value(fixture_manifest()).unwrap();

    let mut noop = value.clone();
    noop["mutation"]["after_memory_type"] = noop["mutation"]["before_memory_type"].clone();
    assert!(serde_json::from_value::<RepairManifest>(noop).is_err());

    let mut wrong_schema = value.clone();
    wrong_schema["manifest_schema_version"] = serde_json::json!(2);
    assert!(serde_json::from_value::<RepairManifest>(wrong_schema).is_err());

    let mut unknown = value;
    unknown["hidden_target"] = serde_json::json!("mem_other");
    assert!(serde_json::from_value::<RepairManifest>(unknown).is_err());
}

#[test]
fn apply_receipt_deserializer_revalidates_schema_and_effect_proof() {
    let target = RepairTarget::memory("mem_target".into(), RepairScope::uncategorized()).unwrap();
    let receipt = RepairApplyReceipt::try_new(
        "repair_550e8400-e29b-41d4-a716-446655440000".into(),
        RepairDigest::parse(SHA256_A).unwrap(),
        1_721_000_001,
        RepairDigest::parse(SHA256_A).unwrap(),
        RepairDigest::parse(SHA256_B).unwrap(),
        RepairDigest::parse(SHA256_A).unwrap(),
        RepairDigest::parse(SHA256_A).unwrap(),
        RepairAllowedEffects::memory_type(target),
        RepairWriter::ReclassifyMemory,
        RepairDigest::parse(SHA256_B).unwrap(),
    )
    .unwrap();
    let value = serde_json::to_value(receipt).unwrap();

    let mut wrong_schema = value.clone();
    wrong_schema["receipt_schema_version"] = serde_json::json!(2);
    assert!(serde_json::from_value::<RepairApplyReceipt>(wrong_schema).is_err());

    let mut escaped_effect = value;
    escaped_effect["non_target_after"] = serde_json::json!(SHA256_B);
    assert!(serde_json::from_value::<RepairApplyReceipt>(escaped_effect).is_err());
}

#[test]
fn verification_receipt_deserializer_revalidates_schema() {
    let receipt = RepairVerificationReceipt::try_new(
        "repair_550e8400-e29b-41d4-a716-446655440000".into(),
        RepairDigest::parse(SHA256_A).unwrap(),
        RepairDigest::parse(SHA256_B).unwrap(),
        1_721_000_002,
        snapshots(7),
        snapshots(9),
        RepairDigest::parse(SHA256_A).unwrap(),
    )
    .unwrap();
    let mut value = serde_json::to_value(receipt).unwrap();
    value["receipt_schema_version"] = serde_json::json!(2);

    assert!(serde_json::from_value::<RepairVerificationReceipt>(value).is_err());
}
