// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::lint::{
    LintDbSnapshotMode, LintDbSnapshotReceipt, LintDigest, LintEvidenceRef, LintGateEffect,
    LintOpaqueId, LintOutcome, LintPageSnapshotMode, LintPageSnapshotReceipt, LintProducerReceipt,
    LintScope, LintSemanticAction, LintSemanticFinding, LintSemanticProviderRoute,
    LintSemanticReasonCode, LintSnapshotReceipts,
};
use crate::repair_relation::{
    EntityRelationRepairChoice, EntityRelationRepairSelection, RepairRelationSnapshot,
    RepairRelationSqlValue, RepairRelationTable, RepairRelationTableSnapshot,
};

fn empty_relation_snapshot() -> RepairRelationSnapshot {
    RepairRelationSnapshot {
        tables: RepairRelationTable::ALL_TABLES
            .iter()
            .map(|table| RepairRelationTableSnapshot {
                table: *table,
                columns: vec!["id".to_string()],
                rows: Vec::new(),
            })
            .collect(),
    }
}

fn relation_selection() -> EntityRelationRepairSelection {
    EntityRelationRepairSelection {
        review_id: "review-relation-1".to_string(),
        choice: EntityRelationRepairChoice::Add {
            from_entity: "entity-a".to_string(),
            to_entity: "entity-b".to_string(),
            relation_type: "reports_to".to_string(),
            source_memory_id: Some("memory-1".to_string()),
        },
    }
}

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

fn relation_finding(confidence_basis_points: u16) -> LintSemanticFinding {
    LintSemanticFinding::try_new(
        LintOpaqueId::from_sorted_position(0).unwrap(),
        LintSemanticAction::AddEntityRelation,
        LintSemanticReasonCode::SharedContextWithoutRelation,
        confidence_basis_points,
        LintSemanticProviderRoute::OnDevice,
        vec![LintDigest::from_u64(42)],
        vec![],
    )
    .unwrap()
}

fn relation_manifest() -> RepairManifest {
    let target = RepairTarget::entity_relation(
        "relation-new".to_string(),
        "entity-a".to_string(),
        "entity-b".to_string(),
        vec![
            "entity-a".to_string(),
            "entity-b".to_string(),
            "memory-1".to_string(),
        ],
        RepairScope::global(),
    )
    .unwrap();
    let finding = relation_finding(8_750);
    let source = RepairSource::try_new_entity_relation(
        RepairLintScope::global(),
        LintScope::global(),
        finding.clone(),
        snapshots(1),
        snapshots(3),
        LintProducerReceipt::new(None),
        LintProducerReceipt::new(None),
        LintDigest::from_u64(5),
    )
    .unwrap()
    .try_with_review_binding(
        RepairReviewBinding::try_new(
            "lint_review_relation".to_string(),
            RepairDigest::parse(SHA256_A).unwrap(),
            vec![
                "entity-a".to_string(),
                "entity-b".to_string(),
                "memory-1".to_string(),
            ],
        )
        .unwrap(),
    )
    .unwrap();
    let change = RepairRelationMutation::Add {
        requested_relation_type: "reports_to".to_string(),
        canonical_relation_type: "reports_to".to_string(),
        source_memory_id: Some("memory-1".to_string()),
        confidence_basis_points: 8_750,
        retire_relation_ids: vec!["relation-old".to_string()],
        vocabulary_promotion: None,
    };
    let mutation = RepairMutation::entity_relation(change).unwrap();
    let baseline = RepairCheckBaseline::try_new_current(
        "memories.structural.integrity".to_string(),
        LintOutcome::Pass,
        LintGateEffect::Actionable,
        0,
        vec![],
    )
    .unwrap();
    let deep_baseline = RepairCheckBaseline::try_new_current(
        REPAIR_RELATION_CHECK_ID.to_string(),
        LintOutcome::Finding,
        LintGateEffect::Actionable,
        1,
        vec![LintEvidenceRef::SemanticFinding {
            finding: finding.clone(),
        }],
    )
    .unwrap();
    let assertions = RepairPostAssertions::try_new_for_check(
        REPAIR_RELATION_CHECK_ID.to_string(),
        LintDigest::from_u64(42),
        vec![baseline],
        vec![deep_baseline],
        vec![REPAIR_RELATION_CHECK_ID.to_string()],
        vec![],
    )
    .unwrap();
    let draft = RepairManifestDraft::try_new(
        "repair_550e8400-e29b-41d4-a716-446655440700".to_string(),
        1_721_000_700,
        source,
        target.clone(),
        RepairExpectedState::try_new(None, RepairDigest::parse(SHA256_A).unwrap()).unwrap(),
        RepairWriter::EntityRelation,
        mutation,
        RepairAllowedEffects::entity_relation(target),
        RepairRollbackArtifact::entity_relation(
            "rollback-v3.json".to_string(),
            RepairDigest::parse(SHA256_B).unwrap(),
        )
        .unwrap(),
        assertions,
    )
    .unwrap();
    RepairManifest::try_new(draft, RepairDigest::parse(SHA256_A).unwrap()).unwrap()
}

#[test]
fn relation_snapshot_roundtrips_all_tables_and_lossless_sql_values() {
    let mut snapshot = empty_relation_snapshot();
    snapshot.tables[0] = RepairRelationTableSnapshot {
        table: RepairRelationTable::Edges,
        columns: vec![
            "null_value".to_string(),
            "integer_value".to_string(),
            "real_value".to_string(),
            "text_value".to_string(),
            "blob_value".to_string(),
        ],
        rows: vec![vec![
            RepairRelationSqlValue::Null,
            RepairRelationSqlValue::Integer { value: i64::MIN },
            RepairRelationSqlValue::Real {
                bits: "3ff0000000000000".to_string(),
            },
            RepairRelationSqlValue::Text {
                value: "arbitrary\ntext\u{0000}終".to_string(),
            },
            RepairRelationSqlValue::Blob {
                hex: "00ff".to_string(),
            },
        ]],
    };
    assert_eq!(snapshot.validate(), Ok(()));

    let encoded = serde_json::to_value(&snapshot).unwrap();
    let decoded: RepairRelationSnapshot = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded, snapshot);
}

#[test]
fn relation_snapshot_rejects_wrong_order_duplicate_columns_and_row_widths() {
    let mut wrong_order = empty_relation_snapshot();
    wrong_order.tables.swap(0, 1);
    assert!(wrong_order.validate().is_err());

    let mut duplicate_columns = empty_relation_snapshot();
    duplicate_columns.tables[0].columns = vec!["id".to_string(), "id".to_string()];
    assert!(duplicate_columns.validate().is_err());

    let mut wrong_width = empty_relation_snapshot();
    wrong_width.tables[0].rows = vec![Vec::new()];
    assert!(wrong_width.validate().is_err());
}

#[test]
fn relation_mutation_target_effects_and_current_choice_roundtrip() {
    let mutation = RepairRelationMutation::Add {
        requested_relation_type: "reports_to".to_string(),
        canonical_relation_type: "reports_to".to_string(),
        source_memory_id: Some("memory-1".to_string()),
        confidence_basis_points: 8_750,
        retire_relation_ids: vec!["relation-old".to_string()],
        vocabulary_promotion: None,
    };
    assert_eq!(mutation.validate(), Ok(()));
    let mutation = RepairMutation::entity_relation(mutation).unwrap();
    let target = RepairTarget::entity_relation(
        "relation-new".to_string(),
        "entity-a".to_string(),
        "entity-b".to_string(),
        vec![
            "entity-a".to_string(),
            "entity-b".to_string(),
            "memory-1".to_string(),
        ],
        RepairScope::global(),
    )
    .unwrap();
    let effects = RepairAllowedEffects::entity_relation(target.clone());
    assert_eq!(
        effects.fields(),
        &[
            RepairMemoryField::RelationEdges,
            RepairMemoryField::CommunityGraphState,
            RepairMemoryField::RelationVocabulary,
            RepairMemoryField::RelationActivity,
            RepairMemoryField::RelationReviewQueue,
        ]
    );
    assert_eq!(
        target.review_owner_ids().unwrap(),
        vec![
            "entity-a".to_string(),
            "entity-b".to_string(),
            "memory-1".to_string(),
        ]
    );

    let choice = RepairChoice::entity_relation(
        relation_selection(),
        LintSemanticFinding::try_new(
            LintOpaqueId::from_sorted_position(0).unwrap(),
            LintSemanticAction::AddEntityRelation,
            LintSemanticReasonCode::SharedContextWithoutRelation,
            8_750,
            LintSemanticProviderRoute::OnDevice,
            vec![LintDigest::from_hex(&"00".repeat(8)).unwrap()],
            vec![],
        )
        .unwrap(),
    )
    .unwrap();
    let current = CurrentRepairChoice::entity_relation(relation_selection()).unwrap();
    assert_eq!(
        serde_json::from_value::<RepairMutation>(serde_json::to_value(&mutation).unwrap()).unwrap(),
        mutation
    );
    assert_eq!(
        serde_json::from_value::<RepairChoice>(serde_json::to_value(&choice).unwrap()).unwrap(),
        choice
    );
    assert_eq!(
        serde_json::from_value::<CurrentRepairChoice>(serde_json::to_value(&current).unwrap())
            .unwrap(),
        current
    );
}

#[test]
fn relation_rollback_v3_roundtrips_and_rejects_wrong_version() {
    let rollback = RepairRollbackV3::try_new(
        RepairRollbackPayloadV3::entity_relation(empty_relation_snapshot()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        rollback.format_version(),
        REPAIR_RELATION_ROLLBACK_FORMAT_VERSION
    );
    assert_eq!(rollback.payload().snapshot().tables.len(), 8);

    let bytes = serde_json::to_vec(&rollback).unwrap();
    let decoded: RepairRollbackV3 = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(decoded, rollback);
    assert!(matches!(
        StoredRepairRollbackArtifact::from_slice(&bytes).unwrap(),
        StoredRepairRollbackArtifact::V3(_)
    ));

    let mut wrong_version: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    wrong_version["format_version"] = serde_json::json!(2);
    assert!(serde_json::from_value::<RepairRollbackV3>(wrong_version).is_err());
}

#[test]
fn relation_wire_rejects_unknown_fields_and_malformed_values() {
    let mut value = serde_json::json!({
        "kind": "real",
        "bits": "3FF0000000000000"
    });
    assert!(serde_json::from_value::<RepairRelationSqlValue>(value.clone()).is_err());
    value["bits"] = serde_json::json!("3ff000000000000");
    assert!(serde_json::from_value::<RepairRelationSqlValue>(value).is_err());

    let unknown = serde_json::json!({
        "kind": "retire",
        "unexpected": true
    });
    assert!(serde_json::from_value::<RepairMutation>(unknown).is_err());
}

fn legacy_v5_manifest() -> RepairManifest {
    let check_id = "identity.memory_state_integrity";
    let source = RepairSource::try_new_general_only_deterministic(
        RepairLintScope::global(),
        LintScope::global(),
        check_id.to_string(),
        vec![],
        snapshots(10),
        LintProducerReceipt::new(None),
    )
    .unwrap();
    let target = RepairTarget::memory("memory-v5".to_string(), RepairScope::global()).unwrap();
    let assertions = RepairPostAssertions::try_new_general_only_for_check(
        check_id.to_string(),
        LintDigest::from_u64(11),
        vec![RepairCheckBaseline::try_new_current(
            check_id.to_string(),
            LintOutcome::Finding,
            LintGateEffect::Actionable,
            1,
            vec![],
        )
        .unwrap()],
        vec![],
    )
    .unwrap();
    let draft = RepairManifestDraft::try_new(
        "repair_550e8400-e29b-41d4-a716-446655440710".to_string(),
        1_721_000_710,
        source,
        target.clone(),
        RepairExpectedState::try_new(None, RepairDigest::parse(SHA256_A).unwrap()).unwrap(),
        RepairWriter::ClearMemorySupersedes,
        RepairMutation::clear_memory_supersedes("legacy-source".to_string()).unwrap(),
        RepairAllowedEffects::memory_supersedes(target),
        RepairRollbackArtifact::try_new(
            "rollback-v1.json".to_string(),
            RepairDigest::parse(SHA256_B).unwrap(),
        )
        .unwrap(),
        assertions,
    )
    .unwrap();
    RepairManifest::try_new(draft, RepairDigest::parse(SHA256_A).unwrap()).unwrap()
}

fn legacy_v6_manifest() -> RepairManifest {
    let page_id = "page-v6";
    let check_id = "pages.duplicate_active_titles";
    let source = RepairSource::try_new_general_only_deterministic(
        RepairLintScope::global(),
        LintScope::global(),
        check_id.to_string(),
        vec![],
        snapshots(20),
        LintProducerReceipt::new(None),
    )
    .unwrap()
    .try_with_review_binding(
        RepairReviewBinding::try_new(
            "lint_review_page_v6".to_string(),
            RepairDigest::parse(SHA256_A).unwrap(),
            vec![page_id.to_string()],
        )
        .unwrap(),
    )
    .unwrap();
    let target = RepairTarget::page_projection(page_id.to_string(), RepairScope::global()).unwrap();
    let assertions = RepairPostAssertions::try_new_general_only_for_check(
        check_id.to_string(),
        LintDigest::from_u64(21),
        vec![RepairCheckBaseline::try_new_current(
            check_id.to_string(),
            LintOutcome::Finding,
            LintGateEffect::Actionable,
            1,
            vec![],
        )
        .unwrap()],
        vec![],
    )
    .unwrap();
    let draft = RepairManifestDraft::try_new(
        "repair_550e8400-e29b-41d4-a716-446655440720".to_string(),
        1_721_000_720,
        source,
        target.clone(),
        RepairExpectedState::try_new(Some(2), RepairDigest::parse(SHA256_A).unwrap()).unwrap(),
        RepairWriter::RenamePageTitle,
        RepairMutation::rename_page_title(
            "Before".to_string(),
            "After".to_string(),
            "00000000".repeat(768),
        )
        .unwrap(),
        RepairAllowedEffects::page_title_rename(target),
        RepairRollbackArtifact::try_new_v2(
            "rollback-v2.json".to_string(),
            RepairDigest::parse(SHA256_B).unwrap(),
        )
        .unwrap(),
        assertions,
    )
    .unwrap();
    RepairManifest::try_new(draft, RepairDigest::parse(SHA256_A).unwrap()).unwrap()
}

#[test]
fn relation_manifest_v7_roundtrips_dispatches_and_rejects_cross_field_tampering() {
    let manifest = relation_manifest();
    assert_eq!(
        manifest.manifest_schema_version(),
        REPAIR_MANIFEST_SCHEMA_VERSION
    );
    assert_eq!(manifest.writer(), RepairWriter::EntityRelation);
    assert_eq!(
        manifest.rollback().format_version(),
        REPAIR_RELATION_ROLLBACK_FORMAT_VERSION
    );

    let bytes = serde_json::to_vec(&manifest).unwrap();
    let roundtrip: RepairManifest = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(roundtrip, manifest);
    let stored = StoredRepairManifest::from_slice(&bytes).unwrap();
    assert!(matches!(stored, StoredRepairManifest::V7(_)));
    assert_eq!(
        stored.canonical_unsigned_bytes().unwrap(),
        manifest.canonical_unsigned_bytes().unwrap()
    );
    assert_eq!(
        stored.persisted_bytes().unwrap(),
        serde_json::to_vec_pretty(&manifest).unwrap()
    );

    let reject = |value: serde_json::Value| {
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(serde_json::from_value::<RepairManifest>(value).is_err());
        assert!(StoredRepairManifest::from_slice(&bytes).is_err());
    };

    let mut mismatched_action = serde_json::to_value(&manifest).unwrap();
    mismatched_action["source"]["finding"]["proposed_action"] =
        serde_json::json!("remove_entity_relation");
    reject(mismatched_action);

    let mut mismatched_confidence = serde_json::to_value(&manifest).unwrap();
    mismatched_confidence["mutation"]["change"]["confidence_basis_points"] =
        serde_json::json!(8_751);
    reject(mismatched_confidence);

    let mut mismatched_owners = serde_json::to_value(&manifest).unwrap();
    mismatched_owners["source"]["review_binding"]["owner_ids"] =
        serde_json::json!(["entity-a", "memory-1"]);
    reject(mismatched_owners);

    let mut wrong_rollback_version = serde_json::to_value(&manifest).unwrap();
    wrong_rollback_version["rollback"]["format_version"] = serde_json::json!(2);
    reject(wrong_rollback_version);
}

#[test]
fn legacy_v5_and_v6_manifests_preserve_canonical_bytes_and_dispatch() {
    let cases = [
        (legacy_v5_manifest(), 5, "v5"),
        (legacy_v6_manifest(), 6, "v6"),
    ];
    for (manifest, schema_version, label) in cases {
        let canonical = manifest.canonical_unsigned_bytes().unwrap();
        let canonical_value: serde_json::Value = serde_json::from_slice(&canonical).unwrap();
        assert_eq!(
            canonical_value["manifest_schema_version"], schema_version,
            "{label}"
        );
        assert!(canonical_value.get("manifest_digest").is_none(), "{label}");

        let stored =
            StoredRepairManifest::from_slice(&serde_json::to_vec(&manifest).unwrap()).unwrap();
        match schema_version {
            5 => assert!(matches!(stored, StoredRepairManifest::V5(_)), "{label}"),
            6 => assert!(matches!(stored, StoredRepairManifest::V6(_)), "{label}"),
            _ => unreachable!(),
        }
        assert_eq!(
            stored.canonical_unsigned_bytes().unwrap(),
            canonical,
            "{label}"
        );
        assert_eq!(
            stored.persisted_bytes().unwrap(),
            serde_json::to_vec_pretty(&manifest).unwrap(),
            "{label}"
        );
    }
}

#[test]
fn relation_apply_receipt_uses_schema6_and_rejects_legacy_versions() {
    let target = RepairTarget::entity_relation(
        "relation-new".to_string(),
        "entity-a".to_string(),
        "entity-b".to_string(),
        vec![
            "entity-a".to_string(),
            "entity-b".to_string(),
            "memory-1".to_string(),
        ],
        RepairScope::global(),
    )
    .unwrap();
    let draft = RepairApplyReceiptDraft::try_new(
        "repair_550e8400-e29b-41d4-a716-446655440701".to_string(),
        RepairDigest::parse(SHA256_A).unwrap(),
        1_721_000_701,
        RepairDigest::parse(SHA256_A).unwrap(),
        RepairDigest::parse(SHA256_B).unwrap(),
        RepairDigest::parse(SHA256_A).unwrap(),
        RepairDigest::parse(SHA256_A).unwrap(),
        RepairDigest::parse(SHA256_B).unwrap(),
        RepairAllowedEffects::entity_relation(target),
        RepairWriter::EntityRelation,
    )
    .unwrap();
    let receipt = RepairApplyReceipt::from_draft(draft, RepairDigest::parse(SHA256_A).unwrap());
    let value = serde_json::to_value(&receipt).unwrap();
    assert_eq!(
        value["receipt_schema_version"],
        REPAIR_RELATION_RECEIPT_SCHEMA_VERSION
    );
    let stored =
        StoredRepairApplyReceipt::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(matches!(stored, StoredRepairApplyReceipt::V6(_)));
    assert_eq!(
        stored.canonical_unsigned_bytes().unwrap(),
        receipt.canonical_unsigned_bytes().unwrap()
    );
    assert_eq!(
        stored.persisted_bytes().unwrap(),
        serde_json::to_vec_pretty(&receipt).unwrap()
    );
    let roundtrip: RepairApplyReceipt = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(roundtrip, receipt);

    for version in [4, 5] {
        let mut legacy = value.clone();
        legacy["receipt_schema_version"] = serde_json::json!(version);
        assert!(serde_json::from_value::<RepairApplyReceipt>(legacy.clone()).is_err());
        assert!(
            StoredRepairApplyReceipt::from_slice(&serde_json::to_vec(&legacy).unwrap()).is_err()
        );
    }
}
