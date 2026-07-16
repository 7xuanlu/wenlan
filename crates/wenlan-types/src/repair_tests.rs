// SPDX-License-Identifier: Apache-2.0
use crate::{
    lint::{
        LintDbSnapshotMode, LintDbSnapshotReceipt, LintDigest, LintEvidenceRef, LintGateEffect,
        LintOpaqueId, LintOutcome, LintPageSnapshotMode, LintPageSnapshotReceipt,
        LintProducerReceipt, LintScope, LintSemanticAction, LintSemanticFinding,
        LintSemanticProviderRoute, LintSemanticReasonCode, LintSnapshotReceipts,
    },
    repair::{
        ApplyRepairRequest, RepairAllowedEffects, RepairApplyReceipt, RepairApplyReceiptDraft,
        RepairCheckBaseline, RepairDigest, RepairExpectedState, RepairLintScope, RepairManifest,
        RepairManifestDraft, RepairMutation, RepairPostAssertions, RepairRollbackArtifact,
        RepairScope, RepairSource, RepairTarget, RepairVerificationReceipt,
        RepairVerificationReceiptDraft, RepairWriter, StoredRepairApplyReceipt,
        StoredRepairManifest, StoredRepairRollbackArtifact, StoredRepairVerificationReceipt,
    },
};

const SHA256_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA256_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const GOLDEN_ROLLBACK_DIGEST: &str =
    "13f263e12b211c197519cea152705c573fc21c2352cbca6f757043a04feae623";
const GOLDEN_PRE_BASELINE_MANIFEST_DIGEST: &str =
    "f1e21fb246b6d1bda32b9620798241d53e590adce683e231179f01e9f00599c1";
const GOLDEN_MANIFEST_DIGEST: &str =
    "6d79617ffac084a9668025d2a870aa569b5381ea62513c4fa57d9f1a1620bf34";
const GOLDEN_APPLY_RECEIPT_DIGEST: &str =
    "2c0107e820d5b6425effe0177d4a676835fe2b2bd11f7f69ee437d04ea085169";
const GOLDEN_VERIFICATION_RECEIPT_DIGEST: &str =
    "3377e9eb4aa9d1a1e1441e3df0aa6fed6446e64a1423f8525e6653d6acb05f2d";

fn repair_test_sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn frozen_v1_architecture_violations(source: &str) -> Vec<String> {
    use std::collections::BTreeSet;
    use syn::{visit::Visit, Fields, GenericArgument, Item, PathArguments, Type, UseTree};

    struct FrozenPathVisitor<'a> {
        violations: &'a mut Vec<String>,
    }

    impl<'ast> Visit<'ast> for FrozenPathVisitor<'_> {
        fn visit_path(&mut self, path: &'ast syn::Path) {
            if path.segments.first().is_some_and(|segment| {
                segment.ident == "crate"
                    || (path.segments.len() > 1
                        && matches!(segment.ident.to_string().as_str(), "super" | "self"))
            }) {
                self.violations.push(format!(
                    "live path anywhere in frozen module: {}",
                    path.segments[0].ident
                ));
            }
            syn::visit::visit_path(self, path);
        }

        fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
            self.violations
                .push(format!("submodule in frozen graph: {}", item.ident));
            syn::visit::visit_item_mod(self, item);
        }
    }

    fn declared_frozen_types(file: &syn::File) -> BTreeSet<String> {
        file.items
            .iter()
            .filter_map(|item| match item {
                Item::Struct(item) => Some(item.ident.to_string()),
                Item::Enum(item) => Some(item.ident.to_string()),
                _ => None,
            })
            .filter(|name| name.starts_with("Frozen") && name.ends_with("V1"))
            .collect()
    }

    fn inspect_use(tree: &UseTree, root: bool, violations: &mut Vec<String>) {
        match tree {
            UseTree::Path(path) => {
                if root && matches!(path.ident.to_string().as_str(), "crate" | "super" | "self") {
                    violations.push(format!("live path import: {}", path.ident));
                }
                inspect_use(&path.tree, false, violations);
            }
            UseTree::Group(group) => {
                for item in &group.items {
                    inspect_use(item, root, violations);
                }
            }
            UseTree::Glob(_) => violations.push("glob import".to_string()),
            UseTree::Name(_) | UseTree::Rename(_) => {}
        }
    }

    fn inspect_type(ty: &Type, declared: &BTreeSet<String>, violations: &mut Vec<String>) {
        match ty {
            Type::Path(path) => {
                if path.qself.is_some() {
                    violations.push("qualified live path in frozen field".to_string());
                }
                let Some(first) = path.path.segments.first() else {
                    violations.push("empty type path in frozen field".to_string());
                    return;
                };
                if matches!(first.ident.to_string().as_str(), "crate" | "super" | "self") {
                    violations.push(format!("live path in frozen field: {}", first.ident));
                }
                for segment in &path.path.segments {
                    if let PathArguments::AngleBracketed(arguments) = &segment.arguments {
                        for argument in &arguments.args {
                            if let GenericArgument::Type(inner) = argument {
                                inspect_type(inner, declared, violations);
                            }
                        }
                    }
                }
                let leaf = path.path.segments.last().unwrap().ident.to_string();
                let allowed_leaf = matches!(
                    leaf.as_str(),
                    "String"
                        | "Vec"
                        | "Option"
                        | "bool"
                        | "i8"
                        | "i16"
                        | "i32"
                        | "i64"
                        | "i128"
                        | "isize"
                        | "u8"
                        | "u16"
                        | "u32"
                        | "u64"
                        | "u128"
                        | "usize"
                ) || declared.contains(&leaf);
                if !allowed_leaf {
                    violations.push(format!("non-frozen field leaf: {leaf}"));
                }
            }
            Type::Array(array) => inspect_type(&array.elem, declared, violations),
            Type::Group(group) => inspect_type(&group.elem, declared, violations),
            Type::Paren(paren) => inspect_type(&paren.elem, declared, violations),
            Type::Ptr(pointer) => inspect_type(&pointer.elem, declared, violations),
            Type::Reference(reference) => inspect_type(&reference.elem, declared, violations),
            Type::Slice(slice) => inspect_type(&slice.elem, declared, violations),
            Type::Tuple(tuple) => {
                for element in &tuple.elems {
                    inspect_type(element, declared, violations);
                }
            }
            unsupported => violations.push(format!(
                "unsupported frozen field type: {:?}",
                std::mem::discriminant(unsupported)
            )),
        }
    }

    fn inspect_fields(fields: &Fields, declared: &BTreeSet<String>, violations: &mut Vec<String>) {
        for field in fields {
            inspect_type(&field.ty, declared, violations);
        }
    }

    let file = match syn::parse_file(source) {
        Ok(file) => file,
        Err(error) => return vec![format!("frozen_v1 parse failed: {error}")],
    };
    let declared = declared_frozen_types(&file);
    let mut violations = Vec::new();
    FrozenPathVisitor {
        violations: &mut violations,
    }
    .visit_file(&file);
    for item in &file.items {
        match item {
            Item::Use(item) => inspect_use(&item.tree, true, &mut violations),
            Item::Type(item) => violations.push(format!("type alias: {}", item.ident)),
            Item::Struct(item)
                if item.ident.to_string().starts_with("Frozen")
                    && item.ident.to_string().ends_with("V1") =>
            {
                inspect_fields(&item.fields, &declared, &mut violations);
            }
            Item::Enum(item)
                if item.ident.to_string().starts_with("Frozen")
                    && item.ident.to_string().ends_with("V1") =>
            {
                for variant in &item.variants {
                    inspect_fields(&variant.fields, &declared, &mut violations);
                }
            }
            Item::Struct(item) => {
                violations.push(format!("non-frozen struct in frozen graph: {}", item.ident))
            }
            Item::Enum(item) => {
                violations.push(format!("non-frozen enum in frozen graph: {}", item.ident))
            }
            Item::Fn(item) => violations.push(format!(
                "top-level function in frozen graph: {}",
                item.sig.ident
            )),
            _ => {}
        }
    }
    violations.sort();
    violations.dedup();
    violations
}

#[derive(Debug, PartialEq, Eq)]
struct SerdeEnumContract {
    serde_attributes: Vec<String>,
    variants: Vec<(String, Vec<String>)>,
}

fn serde_enum_contract(source: &str, enum_name: &str) -> SerdeEnumContract {
    fn serde_attributes(attributes: &[syn::Attribute]) -> Vec<String> {
        attributes
            .iter()
            .filter(|attribute| attribute.path().is_ident("serde"))
            .map(|attribute| match &attribute.meta {
                syn::Meta::List(list) => list.tokens.to_string(),
                _ => panic!("unexpected serde attribute shape"),
            })
            .collect()
    }

    let file = syn::parse_file(source).expect("contract source parses");
    let item = file
        .items
        .iter()
        .find_map(|item| match item {
            syn::Item::Enum(item) if item.ident == enum_name => Some(item),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing enum {enum_name}"));
    SerdeEnumContract {
        serde_attributes: serde_attributes(&item.attrs),
        variants: item
            .variants
            .iter()
            .map(|variant| (variant.ident.to_string(), serde_attributes(&variant.attrs)))
            .collect(),
    }
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

fn baselines(
    semantic_finding: Option<LintSemanticFinding>,
) -> (Vec<RepairCheckBaseline>, Vec<RepairCheckBaseline>) {
    let mut deep_evidence = vec![LintEvidenceRef::ReasonCode {
        reason_code: crate::lint::LintReasonCode::SemanticAgentAdjudicationRequired,
    }];
    if let Some(finding) = semantic_finding {
        deep_evidence.push(LintEvidenceRef::SemanticFinding { finding });
    }
    (
        vec![RepairCheckBaseline::try_new(
            "memories.structural.integrity".into(),
            LintOutcome::Pass,
            LintGateEffect::Actionable,
            vec![],
        )
        .unwrap()],
        vec![RepairCheckBaseline::try_new(
            "memories.semantic.classification".into(),
            LintOutcome::Finding,
            LintGateEffect::Actionable,
            deep_evidence,
        )
        .unwrap()],
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
    let (general_baseline, deep_baseline) = baselines(Some(finding));
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
            RepairDigest::parse(GOLDEN_ROLLBACK_DIGEST).unwrap(),
        )
        .unwrap(),
        RepairPostAssertions::try_new(evidence_id, general_baseline, deep_baseline, vec![])
            .unwrap(),
    )
    .unwrap();
    let digest =
        RepairDigest::parse(&repair_test_sha256(&draft.canonical_bytes().unwrap())).unwrap();
    RepairManifest::try_new(draft, digest).unwrap()
}

fn persisted_fixture(contents: &'static str) -> &'static [u8] {
    contents.strip_suffix('\n').unwrap_or(contents).as_bytes()
}

#[test]
fn repair_v1_manifest_decodes_through_frozen_envelope_without_byte_drift() {
    let fixture = persisted_fixture(include_str!("../testdata/repair/v1/manifest.json"));
    let stored = StoredRepairManifest::from_slice(fixture).expect("v1 manifest remains readable");

    assert!(matches!(stored, StoredRepairManifest::V1(_)));
    assert_eq!(
        stored.manifest_id(),
        "repair_550e8400-e29b-41d4-a716-446655440000"
    );
    assert_eq!(stored.manifest_digest().as_str(), GOLDEN_MANIFEST_DIGEST);
    assert_eq!(stored.persisted_bytes().unwrap(), fixture);
    assert!(stored
        .clone()
        .verify_and_try_into_current(|_, _| false)
        .is_err());
    let current = stored
        .clone()
        .verify_and_try_into_current(|canonical, expected| {
            repair_test_sha256(canonical) == expected.as_str()
        })
        .expect("verified v1 manifest maps explicitly to current operation values");
    assert_eq!(current.target().memory_source_id(), "mem_target");
    assert!(current
        .post_assertions()
        .verification_policy()
        .requires_whole_reports());

    let canonical_bytes = stored.canonical_unsigned_bytes().unwrap();
    assert_eq!(repair_test_sha256(&canonical_bytes), GOLDEN_MANIFEST_DIGEST);
    let compact_fixture =
        serde_json::to_vec(&serde_json::from_slice::<serde_json::Value>(fixture).unwrap()).unwrap();
    let compact_stored = StoredRepairManifest::from_slice(&compact_fixture).unwrap();
    assert_eq!(
        compact_stored.canonical_unsigned_bytes().unwrap(),
        canonical_bytes,
        "digest canonicalization is independent of persisted JSON whitespace"
    );
    let rollback_fixture = persisted_fixture(include_str!("../testdata/repair/v1/rollback.json"));
    assert_eq!(
        repair_test_sha256(rollback_fixture),
        stored.rollback_digest().as_str()
    );
    let canonical: serde_json::Value = serde_json::from_slice(&canonical_bytes).unwrap();
    assert!(canonical.get("manifest_digest").is_none());
    assert_eq!(
        canonical["post_assertions"]["deep_baseline"][0]["evidence"][1]["kind"],
        "semantic_finding"
    );
}

#[test]
fn repair_v2_manifest_roundtrips_through_current_envelope() {
    let manifest = fixture_manifest();
    let bytes = serde_json::to_vec_pretty(&manifest).unwrap();
    let stored = StoredRepairManifest::from_slice(&bytes).unwrap();

    assert!(matches!(stored, StoredRepairManifest::V2(_)));
    let current = stored
        .verify_and_try_into_current(|canonical, expected| {
            repair_test_sha256(canonical) == expected.as_str()
        })
        .unwrap();
    assert_eq!(current, manifest);
    assert_eq!(
        current
            .post_assertions()
            .verification_policy()
            .required_deep_check_ids()
            .unwrap(),
        ["memories.semantic.classification"]
    );
}

#[test]
fn repair_v1_pre_baseline_manifest_remains_loadable_with_original_digest() {
    let fixture = persisted_fixture(include_str!(
        "../testdata/repair/v1/manifest-pre-baseline.json"
    ));
    let stored =
        StoredRepairManifest::from_slice(fixture).expect("first durable v1 shape remains readable");

    assert!(matches!(stored, StoredRepairManifest::V1PreBaseline(_)));
    assert_eq!(
        stored.manifest_digest().as_str(),
        GOLDEN_PRE_BASELINE_MANIFEST_DIGEST
    );
    assert_eq!(stored.persisted_bytes().unwrap(), fixture);
    let canonical = stored.canonical_unsigned_bytes().unwrap();
    assert_eq!(
        repair_test_sha256(&canonical),
        GOLDEN_PRE_BASELINE_MANIFEST_DIGEST
    );
    let current = stored
        .verify_and_try_into_current(|bytes, expected| {
            repair_test_sha256(bytes) == expected.as_str()
        })
        .expect("verified historical v1 maps to conservative live operation values");
    assert!(current.post_assertions().general_baseline().is_empty());
    assert!(current.post_assertions().deep_baseline().is_empty());
}

#[test]
fn repair_v1_rollback_and_receipts_decode_without_byte_drift() {
    let rollback_fixture = persisted_fixture(include_str!("../testdata/repair/v1/rollback.json"));
    let rollback = StoredRepairRollbackArtifact::from_slice(rollback_fixture)
        .expect("v1 rollback remains readable");
    assert!(matches!(rollback, StoredRepairRollbackArtifact::V1(_)));
    assert_eq!(rollback.persisted_bytes().unwrap(), rollback_fixture);

    let apply_fixture = persisted_fixture(include_str!("../testdata/repair/v1/apply-receipt.json"));
    let apply = StoredRepairApplyReceipt::from_slice(apply_fixture)
        .expect("v1 apply receipt remains readable");
    assert!(matches!(apply, StoredRepairApplyReceipt::V1(_)));
    assert_eq!(apply.receipt_digest().as_str(), GOLDEN_APPLY_RECEIPT_DIGEST);
    assert_eq!(apply.persisted_bytes().unwrap(), apply_fixture);
    let current_apply = apply
        .clone()
        .verify_and_try_into_current(|canonical, expected| {
            repair_test_sha256(canonical) == expected.as_str()
        })
        .expect("verified v1 apply receipt maps explicitly");
    assert_eq!(
        current_apply.manifest_id(),
        "repair_550e8400-e29b-41d4-a716-446655440000"
    );
    let public_apply_bytes = serde_json::to_vec(&current_apply).unwrap();
    let public_apply: RepairApplyReceipt = serde_json::from_slice(&public_apply_bytes)
        .expect("HTTP/MCP public apply type roundtrips v1");
    assert_eq!(public_apply, current_apply);
    assert_eq!(
        repair_test_sha256(&public_apply.canonical_unsigned_bytes().unwrap()),
        public_apply.receipt_digest().as_str()
    );
    let apply_canonical_bytes = apply.canonical_unsigned_bytes().unwrap();
    assert_eq!(
        repair_test_sha256(&apply_canonical_bytes),
        GOLDEN_APPLY_RECEIPT_DIGEST
    );
    let apply_canonical: serde_json::Value =
        serde_json::from_slice(&apply_canonical_bytes).unwrap();
    assert!(apply_canonical.get("receipt_digest").is_none());

    let verification_fixture = persisted_fixture(include_str!(
        "../testdata/repair/v1/verification-receipt.json"
    ));
    let verification = StoredRepairVerificationReceipt::from_slice(verification_fixture)
        .expect("v1 verification receipt remains readable");
    assert!(matches!(
        verification,
        StoredRepairVerificationReceipt::V1(_)
    ));
    assert_eq!(
        verification.receipt_digest().as_str(),
        GOLDEN_VERIFICATION_RECEIPT_DIGEST
    );
    assert_eq!(
        verification.persisted_bytes().unwrap(),
        verification_fixture
    );
    let current_verification = verification
        .clone()
        .verify_and_try_into_current(|canonical, expected| {
            repair_test_sha256(canonical) == expected.as_str()
        })
        .expect("verified v1 verification receipt maps explicitly");
    assert_eq!(
        current_verification.manifest_id(),
        "repair_550e8400-e29b-41d4-a716-446655440000"
    );
    let public_verification_bytes = serde_json::to_vec(&current_verification).unwrap();
    let public_verification: RepairVerificationReceipt =
        serde_json::from_slice(&public_verification_bytes)
            .expect("HTTP/MCP public verification type roundtrips v1");
    assert_eq!(public_verification, current_verification);
    assert_eq!(
        repair_test_sha256(&public_verification.canonical_unsigned_bytes().unwrap()),
        public_verification.receipt_digest().as_str()
    );
    let verification_canonical_bytes = verification.canonical_unsigned_bytes().unwrap();
    assert_eq!(
        repair_test_sha256(&verification_canonical_bytes),
        GOLDEN_VERIFICATION_RECEIPT_DIGEST
    );
    let verification_canonical: serde_json::Value =
        serde_json::from_slice(&verification_canonical_bytes).unwrap();
    assert!(verification_canonical.get("receipt_digest").is_none());
}

#[test]
fn repair_v1_tagged_contracts_reject_unknown_fields_before_digest_verification() {
    let fixture = include_str!("../testdata/repair/v1/manifest.json");
    let mut lint_scope = serde_json::from_str::<serde_json::Value>(fixture).unwrap();
    lint_scope["source"]["lint_scope"]["unexpected"] = serde_json::json!(true);
    let mut target = serde_json::from_str::<serde_json::Value>(fixture).unwrap();
    target["target"]["unexpected"] = serde_json::json!(true);
    let mut target_scope = serde_json::from_str::<serde_json::Value>(fixture).unwrap();
    target_scope["target"]["scope"]["unexpected"] = serde_json::json!(true);
    let mut mutation = serde_json::from_str::<serde_json::Value>(fixture).unwrap();
    mutation["mutation"]["unexpected"] = serde_json::json!(true);

    for (label, invalid) in [
        ("lint_scope", lint_scope),
        ("target", target),
        ("target_scope", target_scope),
        ("mutation", mutation),
    ] {
        assert!(
            StoredRepairManifest::from_slice(&serde_json::to_vec(&invalid).unwrap()).is_err(),
            "{label} accepted an unknown field"
        );
    }
}

#[test]
fn repair_v1_outer_contracts_reject_unknown_fields_before_digest_verification() {
    for fixture in [
        include_str!("../testdata/repair/v1/manifest.json"),
        include_str!("../testdata/repair/v1/manifest-pre-baseline.json"),
    ] {
        let mut invalid = serde_json::from_str::<serde_json::Value>(fixture).unwrap();
        invalid["unexpected"] = serde_json::json!(true);
        assert!(
            StoredRepairManifest::from_slice(&serde_json::to_vec(&invalid).unwrap()).is_err(),
            "manifest wrapper accepted an unknown top-level field"
        );
    }

    let mut apply = serde_json::from_str::<serde_json::Value>(include_str!(
        "../testdata/repair/v1/apply-receipt.json"
    ))
    .unwrap();
    apply["unexpected"] = serde_json::json!(true);
    assert!(
        StoredRepairApplyReceipt::from_slice(&serde_json::to_vec(&apply).unwrap()).is_err(),
        "apply receipt wrapper accepted an unknown top-level field"
    );

    let mut verification = serde_json::from_str::<serde_json::Value>(include_str!(
        "../testdata/repair/v1/verification-receipt.json"
    ))
    .unwrap();
    verification["unexpected"] = serde_json::json!(true);
    assert!(
        StoredRepairVerificationReceipt::from_slice(&serde_json::to_vec(&verification).unwrap())
            .is_err(),
        "verification receipt wrapper accepted an unknown top-level field"
    );
}

#[test]
fn repair_frozen_v1_mirrored_enum_contracts_match_live_v1() {
    let frozen = include_str!("repair/frozen_v1.rs");
    let lint_agent = include_str!("lint_agent.rs");
    let lint_catalog = include_str!("lint_catalog.rs");
    for (live_source, live_name, frozen_name) in [
        (
            lint_agent,
            "LintSemanticAction",
            "FrozenLintSemanticActionV1",
        ),
        (
            lint_agent,
            "LintSemanticReasonCode",
            "FrozenLintSemanticReasonCodeV1",
        ),
        (lint_catalog, "LintReasonCode", "FrozenLintReasonCodeV1"),
        (
            lint_catalog,
            "LintSafeRootRelativePath",
            "FrozenLintSafeRootRelativePathV1",
        ),
    ] {
        assert_eq!(
            serde_enum_contract(frozen, frozen_name),
            serde_enum_contract(live_source, live_name),
            "frozen v1 enum drifted from live v1 contract: {live_name}"
        );
    }
}

#[test]
fn repair_frozen_v1_architecture_rejects_live_type_reachability() {
    let invalid = r#"
        use crate::lint::LintDigest;
        type FrozenAliasV1 = LintDigest;
        struct FrozenBadV1 { digest: crate::lint::LintDigest }
        impl FrozenBadV1 {
            fn into_live(&self) -> crate::repair::RepairManifest { panic!() }
        }
        mod hidden {}
    "#;
    let violations = frozen_v1_architecture_violations(invalid);
    assert!(violations.iter().any(|item| item.contains("live path")));
    assert!(violations.iter().any(|item| item.contains("type alias")));
    assert!(violations.iter().any(|item| item.contains("submodule")));

    let actual = include_str!("repair/frozen_v1.rs");
    assert_eq!(
        frozen_v1_architecture_violations(actual),
        Vec::<String>::new()
    );
}

#[test]
fn post_assertion_baselines_are_sorted_unique_and_strict() {
    let (general, deep) = baselines(None);
    let assertions =
        RepairPostAssertions::try_new(LintDigest::from_u64(42), general.clone(), deep, vec![])
            .unwrap();
    assert_eq!(assertions.general_baseline(), general);

    let duplicate = vec![general[0].clone(), general[0].clone()];
    assert!(
        RepairPostAssertions::try_new(LintDigest::from_u64(42), duplicate, vec![], vec![],)
            .is_err()
    );

    let mut value = serde_json::to_value(assertions).unwrap();
    value["general_baseline"][0]["hidden_content"] = serde_json::json!("secret");
    assert!(serde_json::from_value::<RepairPostAssertions>(value).is_err());
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
    let manifest_id = "repair_550e8400-e29b-41d4-a716-446655440000";
    let request = ApplyRepairRequest::try_new(
        manifest_id.into(),
        RepairDigest::parse(SHA256_A).unwrap(),
        format!("apply repair {manifest_id} {SHA256_A}"),
    )
    .expect("valid apply request");

    assert_eq!(
        request.manifest_id(),
        "repair_550e8400-e29b-41d4-a716-446655440000"
    );
    assert_eq!(request.approved_manifest_digest().as_str(), SHA256_A);
    assert!(ApplyRepairRequest::try_new(
        manifest_id.into(),
        RepairDigest::parse(SHA256_A).unwrap(),
        "yes apply it".into(),
    )
    .is_err());
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
    wrong_schema["manifest_schema_version"] = serde_json::json!(1);
    assert!(serde_json::from_value::<RepairManifest>(wrong_schema).is_err());

    let mut unknown = value;
    unknown["hidden_target"] = serde_json::json!("mem_other");
    assert!(serde_json::from_value::<RepairManifest>(unknown).is_err());
}

#[test]
fn apply_receipt_deserializer_revalidates_schema_and_effect_proof() {
    let target = RepairTarget::memory("mem_target".into(), RepairScope::uncategorized()).unwrap();
    let draft = RepairApplyReceiptDraft::try_new(
        "repair_550e8400-e29b-41d4-a716-446655440000".into(),
        RepairDigest::parse(SHA256_A).unwrap(),
        1_721_000_001,
        RepairDigest::parse(SHA256_A).unwrap(),
        RepairDigest::parse(SHA256_B).unwrap(),
        RepairDigest::parse(SHA256_A).unwrap(),
        RepairDigest::parse(SHA256_A).unwrap(),
        RepairDigest::parse(SHA256_B).unwrap(),
        RepairAllowedEffects::memory_type(target),
        RepairWriter::ReclassifyMemory,
    )
    .unwrap();
    let canonical = draft.canonical_bytes().unwrap();
    assert!(!String::from_utf8(canonical)
        .unwrap()
        .contains("receipt_digest"));
    let receipt = RepairApplyReceipt::from_draft(draft, RepairDigest::parse(SHA256_B).unwrap());
    let value = serde_json::to_value(receipt).unwrap();

    let mut wrong_schema = value.clone();
    wrong_schema["receipt_schema_version"] = serde_json::json!(1);
    assert!(serde_json::from_value::<RepairApplyReceipt>(wrong_schema).is_err());

    let mut escaped_effect = value;
    escaped_effect["non_target_after"] = serde_json::json!(SHA256_B);
    assert!(serde_json::from_value::<RepairApplyReceipt>(escaped_effect).is_err());
}

#[test]
fn verification_receipt_deserializer_rejects_unsupported_schema() {
    let draft = RepairVerificationReceiptDraft::try_new(
        "repair_550e8400-e29b-41d4-a716-446655440000".into(),
        RepairDigest::parse(SHA256_A).unwrap(),
        RepairDigest::parse(SHA256_B).unwrap(),
        1_721_000_002,
        snapshots(7),
        snapshots(9),
    )
    .unwrap();
    let canonical: serde_json::Value =
        serde_json::from_slice(&draft.canonical_bytes().unwrap()).unwrap();
    assert!(canonical.get("receipt_digest").is_none());
    let receipt =
        RepairVerificationReceipt::from_draft(draft, RepairDigest::parse(SHA256_A).unwrap());
    let mut value = serde_json::to_value(receipt).unwrap();
    value["receipt_schema_version"] = serde_json::json!(3);

    assert!(serde_json::from_value::<RepairVerificationReceipt>(value).is_err());
}
