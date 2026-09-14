// SPDX-License-Identifier: Apache-2.0
//! Direct writer/receipt tests with synthetic findings, not semantic-model proof.
use super::*;
use wenlan_types::lint::{
    LintDbSnapshotMode, LintDbSnapshotReceipt, LintDigest, LintEvidenceRef, LintGateEffect,
    LintOpaqueId, LintOutcome, LintPageSnapshotMode, LintPageSnapshotReceipt, LintProducerReceipt,
    LintScope, LintSemanticAction, LintSemanticProviderRoute, LintSemanticReasonCode,
    LintSnapshotReceipts,
};

struct Fixture {
    db: MemoryDB,
    _dir: tempfile::TempDir,
    store: RepairArtifactStore,
    manifest: RepairManifest,
    rollback: RepairRelationSnapshot,
}

#[test]
fn relation_verification_distinguishes_findings_that_share_one_endpoint() {
    let finding = |position, ids: &[u64]| {
        LintSemanticFinding::try_new(
            LintOpaqueId::from_sorted_position(position).unwrap(),
            LintSemanticAction::AddEntityRelation,
            LintSemanticReasonCode::SharedContextWithoutRelation,
            8000,
            LintSemanticProviderRoute::OnDevice,
            ids.iter().copied().map(LintDigest::from_u64).collect(),
            vec![],
        )
        .unwrap()
    };
    let selected = finding(0, &[10, 20, 30]);
    let other_relation = finding(0, &[10, 40, 50]);
    assert!(!finding_matches_subject(
        &selected,
        "a",
        "b",
        "related_to",
        &other_relation
    ));
    // Candidate positions and evidence ordering may change in a fresh report.
    let same_relation = finding(3, &[30, 10, 20]);
    assert!(finding_matches_subject(
        &selected,
        "a",
        "b",
        "related_to",
        &same_relation
    ));
}

#[test]
fn relation_verification_tracks_the_pair_when_the_finding_changes_kind() {
    let finding = |action, evidence| {
        LintSemanticFinding::try_new(
            LintOpaqueId::from_sorted_position(0).unwrap(),
            action,
            if action == LintSemanticAction::AddEntityRelation {
                LintSemanticReasonCode::SharedContextWithoutRelation
            } else {
                LintSemanticReasonCode::ExistingRelationMismatch
            },
            8000,
            LintSemanticProviderRoute::OnDevice,
            evidence,
            vec![],
        )
        .unwrap()
    };
    let missing = finding(
        LintSemanticAction::AddEntityRelation,
        vec![
            crate::lint::semantic_record_digest("entity", "a"),
            crate::lint::semantic_record_digest("entity", "b"),
            crate::lint::semantic_record_digest("memory", "source"),
        ],
    );
    let mismatch = finding(
        LintSemanticAction::RemoveEntityRelation,
        vec![
            crate::lint::semantic_record_key_digest("relation-entity:a:related_to:from"),
            crate::lint::semantic_record_key_digest("relation-entity:b:related_to:to"),
        ],
    );
    assert!(finding_matches_subject(
        &missing,
        "a",
        "b",
        "related_to",
        &mismatch
    ));
    assert!(finding_matches_subject(&mismatch, "a", "b", "", &missing));
    assert!(!finding_matches_subject(
        &missing,
        "a",
        "other",
        "related_to",
        &mismatch
    ));
}

async fn fixture(retire_grounded: bool, escape_trigger: bool) -> Fixture {
    fixture_with_replacement(retire_grounded, escape_trigger, false).await
}

async fn fixture_with_replacement(
    retire_grounded: bool,
    escape_trigger: bool,
    replace_unknown: bool,
) -> Fixture {
    let (db, dir) = crate::db::tests::test_db().await;
    let from = db
        .create_entity("Relation CAS Alpha", "concept", Some("work"))
        .await
        .unwrap();
    let to = db
        .create_entity("Relation CAS Beta", "concept", Some("work"))
        .await
        .unwrap();
    let target_id =
        crate::provenance::compute_edge_id("relates", "entity", &from, "entity", &to, "related_to");
    let mut retire_ids = Vec::new();
    if retire_grounded || replace_unknown {
        let id = db
            .create_relation(
                &from,
                &to,
                if replace_unknown {
                    "works_on"
                } else {
                    "related_to"
                },
                None,
                Some(0.8),
                None,
                None,
            )
            .await
            .unwrap();
        if retire_grounded {
            assert_eq!(id, target_id);
        }
        if replace_unknown {
            retire_ids.push(id.clone());
        }
        db.test_primary_session()
            .await
            .execute(
                "UPDATE edges SET grounded=1 WHERE edge_id=?1",
                libsql::params![id],
            )
            .await
            .unwrap();
    }
    if escape_trigger {
        db.test_primary_session().await.execute_batch(
            "INSERT INTO memories (id,content,source,source_id,title,chunk_index,last_modified,chunk_type,space)
             VALUES ('escape-row','preserve me','memory','escape-memory','Other',0,1,'text','work');
             CREATE TRIGGER relation_repair_escape AFTER INSERT ON edges WHEN NEW.edge_type='relates'
             BEGIN UPDATE memories SET content='unexpected write' WHERE id='escape-row'; END;"
        ).await.unwrap();
    }
    let mut owners = vec![from.clone(), to.clone()];
    owners.sort();
    let occurrence = repair_digest(format!("{from}:{to}:{retire_grounded}").as_bytes());
    let review_id = format!("lint_review_{}", occurrence.as_str());
    let binding =
        RepairReviewBinding::try_new(review_id.clone(), occurrence.clone(), owners.clone())
            .unwrap();
    let payload = wenlan_types::RefinementPayload::LintRepairReview {
        check_id: "kg.semantic.entity_relations".into(),
        occurrence_digest: occurrence.clone(),
        owner_binding_digest: lint_review_owner_binding_digest(&occurrence, &owners).unwrap(),
        issue: "Synthetic relation repair".into(),
        choices: vec!["Apply explicit relation choice".into()],
        suggested_research_queries: vec![],
    };
    db.insert_lint_review_if_absent(
        &review_id,
        &owners,
        &serde_json::to_string(&payload).unwrap(),
    )
    .await
    .unwrap();
    let manifest_id = format!("repair_{}", Uuid::new_v4());
    let promotion = replace_unknown.then_some("novel_relation_cas");
    let context = RelationCaptureContext {
        manifest_id: &manifest_id,
        review_id: &review_id,
        from_entity: &from,
        to_entity: &to,
        owner_ids: &owners,
        canonical_relation_type: (!retire_grounded).then_some("related_to"),
        vocabulary_promotion: promotion,
    };
    let snapshot = db.open_lint_snapshot().await.unwrap();
    let rollback = relation_snapshot::capture(&RelationReader::Snapshot(&snapshot), &context)
        .await
        .unwrap();
    assert!(snapshot.finish().await.unwrap().is_consistent());
    let snapshots = || {
        LintSnapshotReceipts::new(
            LintDbSnapshotReceipt::new(
                LintDbSnapshotMode::TransactionalReadOnly,
                LintDigest::from_u64(1),
                Some(LintDigest::from_u64(1)),
            ),
            LintPageSnapshotReceipt::new(
                LintPageSnapshotMode::BestEffort,
                LintDigest::from_u64(2),
                Some(LintDigest::from_u64(2)),
            ),
        )
    };
    let finding = LintSemanticFinding::try_new(
        LintOpaqueId::from_sorted_position(0).unwrap(),
        if retire_grounded {
            LintSemanticAction::RemoveEntityRelation
        } else {
            LintSemanticAction::AddEntityRelation
        },
        if retire_grounded {
            LintSemanticReasonCode::ExistingRelationMismatch
        } else {
            LintSemanticReasonCode::SharedContextWithoutRelation
        },
        8000,
        LintSemanticProviderRoute::OnDevice,
        vec![LintDigest::from_u64(42)],
        vec![],
    )
    .unwrap();
    let source = RepairSource::try_new_entity_relation(
        RepairLintScope::global(),
        LintScope::global(),
        finding.clone(),
        snapshots(),
        snapshots(),
        LintProducerReceipt::new(None),
        LintProducerReceipt::new(None),
        LintDigest::from_u64(5),
    )
    .unwrap()
    .try_with_review_binding(binding)
    .unwrap();
    let target =
        RepairTarget::entity_relation(target_id, from, to, owners, RepairScope::global()).unwrap();
    let mutation = if retire_grounded {
        RepairRelationMutation::Retire
    } else {
        RepairRelationMutation::Add {
            requested_relation_type: promotion.unwrap_or("related_to").into(),
            canonical_relation_type: "related_to".into(),
            source_memory_id: None,
            confidence_basis_points: 8000,
            retire_relation_ids: retire_ids,
            vocabulary_promotion: promotion.map(str::to_string),
        }
    };
    let assertions = RepairPostAssertions::try_new_for_check(
        "kg.semantic.entity_relations".into(),
        LintDigest::from_u64(42),
        vec![RepairCheckBaseline::try_new_current(
            "memories.structural.integrity".into(),
            LintOutcome::Pass,
            LintGateEffect::Actionable,
            0,
            vec![],
        )
        .unwrap()],
        vec![RepairCheckBaseline::try_new_current(
            "kg.semantic.entity_relations".into(),
            LintOutcome::Finding,
            LintGateEffect::Actionable,
            1,
            vec![LintEvidenceRef::SemanticFinding { finding }],
        )
        .unwrap()],
        vec!["kg.semantic.entity_relations".into()],
        vec![],
    )
    .unwrap();
    let rollback_bytes = serde_json::to_vec(
        &RepairRollbackV3::try_new(RepairRollbackPayloadV3::EntityRelation {
            snapshot: rollback.clone(),
        })
        .unwrap(),
    )
    .unwrap();
    let draft = RepairManifestDraft::try_new(
        manifest_id,
        1721000000,
        source,
        target.clone(),
        RepairExpectedState::try_new(None, relation_snapshot::receipt(&rollback).unwrap()).unwrap(),
        RepairWriter::EntityRelation,
        RepairMutation::entity_relation(mutation).unwrap(),
        RepairAllowedEffects::entity_relation(target),
        RepairRollbackArtifact::entity_relation(
            "rollback-v3.json".into(),
            repair_digest(&rollback_bytes),
        )
        .unwrap(),
        assertions,
    )
    .unwrap();
    let digest = repair_digest(&draft.canonical_bytes().unwrap());
    let manifest = RepairManifest::try_new(draft, digest).unwrap();
    let store = RepairArtifactStore::new(dir.path().join("relation-repairs"));
    store.persist_prepared(&manifest, &rollback_bytes).unwrap();
    Fixture {
        db,
        _dir: dir,
        store,
        manifest,
        rollback,
    }
}

#[tokio::test]
#[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
async fn relation_apply_replays_same_receipt_without_duplicate_activity() {
    let f = fixture(false, false).await;
    let request = || {
        ApplyRepairRequest::try_new(
            f.manifest.manifest_id().into(),
            f.manifest.manifest_digest().clone(),
            format!(
                "apply repair {} {}",
                f.manifest.manifest_id(),
                f.manifest.manifest_digest().as_str()
            ),
        )
        .unwrap()
    };
    let receipt = apply_repair(&f.db, &f.store, request(), 1721000010)
        .await
        .unwrap();
    let committed =
        f.db.capture_relation_repair_state(&f.manifest)
            .await
            .unwrap();
    // Crash after SQL commit, before the prepared receipt's final rename.
    // Recovery must publish this exact receipt without rerunning the writer.
    let directory = f.store.manifest_dir(f.manifest.manifest_id()).unwrap();
    let final_path = directory.join(APPLY_RECEIPT_FILE);
    let pending_path = directory.join(APPLY_RECEIPT_PENDING_FILE);
    fs::rename(&final_path, &pending_path).unwrap();
    sync_dir(&directory).unwrap();
    // Normal daemon work between commit and receipt publication must not
    // turn a proven commit into an unrecoverable/duplicate apply.
    f.db.test_primary_session()
        .await
        .execute_batch(
            "UPDATE relation_type_vocabulary SET count=COALESCE(count,0)+1;
         INSERT INTO relation_type_vocabulary(canonical,aliases,category,count)
         VALUES ('background_predicate','[]','other',1);
         UPDATE pages SET content=content || ' background enrichment';
         UPDATE space_graph_state SET graph_generation=graph_generation+1;",
        )
        .await
        .unwrap();
    let after =
        f.db.capture_relation_repair_state(&f.manifest)
            .await
            .unwrap();
    assert_ne!(committed, after);
    let again = apply_repair(&f.db, &f.store, request(), 1721000020)
        .await
        .unwrap();
    assert_eq!(receipt, again);
    assert!(final_path.exists());
    assert!(!pending_path.exists());
    assert_eq!(
        after,
        f.db.capture_relation_repair_state(&f.manifest)
            .await
            .unwrap()
    );
    assert_eq!(
        receipt.after_target_receipt(),
        &relation_snapshot::applied_receipt(
            &after,
            &crate::db::repair_relation_cas::capture_context(&f.manifest).unwrap()
        )
        .unwrap()
    );
}

#[tokio::test]
#[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
async fn relation_retirement_preserves_grounded_provenance_and_updates_graph_atomically() {
    let f = fixture(true, false).await;
    let receipt =
        f.db.relation_repair_cas(&f.manifest, &f.rollback, |_| Ok(()))
            .await
            .unwrap();
    assert_ne!(
        receipt.before_target_receipt(),
        receipt.after_target_receipt()
    );
    let after =
        f.db.capture_relation_repair_state(&f.manifest)
            .await
            .unwrap();
    let edges = after
        .tables
        .iter()
        .find(|table| table.table == wenlan_types::repair_relation::RepairRelationTable::Edges)
        .unwrap();
    let valid = edges
        .columns
        .iter()
        .position(|name| name == "valid_until")
        .unwrap();
    assert!(
        matches!(edges.rows[0][valid], wenlan_types::repair_relation::RepairRelationSqlValue::Integer { value } if value > 0)
    );
}

#[tokio::test]
#[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
async fn relation_replacement_atomically_retires_conflict_and_proposes_vocabulary() {
    let f = fixture_with_replacement(false, false, true).await;
    f.db.relation_repair_cas(&f.manifest, &f.rollback, |_| Ok(()))
        .await
        .unwrap();
    let after =
        f.db.capture_relation_repair_state(&f.manifest)
            .await
            .unwrap();
    let edges = after
        .tables
        .iter()
        .find(|table| table.table == wenlan_types::repair_relation::RepairRelationTable::Edges)
        .unwrap();
    let active = edges
        .columns
        .iter()
        .position(|name| name == "valid_until")
        .unwrap();
    assert_eq!(edges.rows.len(), 2);
    assert_eq!(
        edges
            .rows
            .iter()
            .filter(|row| matches!(
                row[active],
                wenlan_types::repair_relation::RepairRelationSqlValue::Null
            ))
            .count(),
        1
    );
    let queue = after
        .tables
        .iter()
        .find(|table| {
            table.table == wenlan_types::repair_relation::RepairRelationTable::RefinementQueue
        })
        .unwrap();
    assert_eq!(
        queue.rows.len(),
        2,
        "review row and declared vocabulary proposal"
    );
}

#[tokio::test]
#[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
async fn relation_before_commit_failure_and_trigger_escape_roll_back() {
    for escape in [false, true] {
        let f = fixture(false, escape).await;
        let before =
            f.db.capture_relation_repair_state(&f.manifest)
                .await
                .unwrap();
        let error =
            f.db.relation_repair_cas(&f.manifest, &f.rollback, |_| {
                Err(WenlanError::Validation("test before commit failure".into()))
            })
            .await
            .unwrap_err();
        if escape {
            assert!(
                error.to_string().contains("repair_effect_escape"),
                "{error}"
            );
        } else {
            assert!(
                error.to_string().contains("test before commit failure"),
                "{error}"
            );
        }
        assert_eq!(
            before,
            f.db.capture_relation_repair_state(&f.manifest)
                .await
                .unwrap()
        );
        let session = f.db.test_primary_session().await;
        let mut rows = session
            .query("SELECT content FROM memories WHERE id='escape-row'", ())
            .await
            .unwrap();
        if escape {
            assert_eq!(
                rows.next()
                    .await
                    .unwrap()
                    .unwrap()
                    .get::<String>(0)
                    .unwrap(),
                "preserve me"
            );
        }
    }
}
