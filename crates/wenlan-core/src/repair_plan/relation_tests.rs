use super::*;
use crate::lint::{
    context::{CancellationToken, LintClock},
    runner::LintRunner,
};
use wenlan_types::lint::{
    LintOpaqueId, LintProfile, LintQuery, LintSemanticProviderRoute, LintSemanticReasonCode,
};

async fn fixture(
    action: LintSemanticAction,
    predicate: Option<&str>,
) -> (
    MemoryDB,
    tempfile::TempDir,
    semantic::SemanticReviewCandidate,
    String,
    String,
) {
    let (db, dir) = crate::db::tests::test_db().await;
    let from = db
        .create_entity("Relation repair Alpha", "concept", Some("work"))
        .await
        .unwrap();
    let to = db
        .create_entity("Relation repair Beta", "concept", Some("work"))
        .await
        .unwrap();
    let mut records = vec![
        RepairAffectedRecord::try_new(RepairAffectedRecordKind::Entity, from.clone()).unwrap(),
        RepairAffectedRecord::try_new(RepairAffectedRecordKind::Entity, to.clone()).unwrap(),
    ];
    records.sort();
    let evidence = if let Some(predicate) = predicate {
        vec![
            crate::lint::semantic_record_key_digest(&format!(
                "relation-entity:{from}:{predicate}:from"
            )),
            crate::lint::semantic_record_key_digest(&format!(
                "relation-entity:{to}:{predicate}:to"
            )),
        ]
    } else {
        vec![
            crate::lint::semantic_record_digest("entity", &from),
            crate::lint::semantic_record_digest("entity", &to),
        ]
    };
    let candidate = semantic::SemanticReviewCandidate {
        check_id: LintSemanticCheckId::EntityRelations.as_str().into(),
        finding: LintSemanticFinding::try_new(
            LintOpaqueId::from_sorted_position(0).unwrap(),
            action,
            if predicate.is_some() {
                LintSemanticReasonCode::ExistingRelationMismatch
            } else {
                LintSemanticReasonCode::SharedContextWithoutRelation
            },
            8000,
            LintSemanticProviderRoute::CallingAgent,
            evidence,
            vec![],
        )
        .unwrap(),
        affected_records: records,
    };
    (db, dir, candidate, from, to)
}

async fn selection(
    db: &MemoryDB,
    candidate: &semantic::SemanticReviewCandidate,
    choice: EntityRelationRepairChoice,
) -> EntityRelationRepairSelection {
    let occurrence = semantic_review_occurrence_digest(
        &candidate.check_id,
        &candidate.finding,
        &candidate.affected_records,
    )
    .unwrap();
    let review_id = format!("lint_review_{}", occurrence.as_str());
    let sources = canonical_lint_review_source_ids(
        &candidate
            .affected_records
            .iter()
            .map(|r| r.durable_id().to_string())
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let payload = RefinementPayload::LintRepairReview {
        check_id: candidate.check_id.clone(),
        occurrence_digest: occurrence.clone(),
        owner_binding_digest: lint_review_owner_binding_digest(&occurrence, &sources).unwrap(),
        issue: "Inspect this relation".into(),
        choices: vec!["Apply explicit relation choice".into()],
        suggested_research_queries: vec![],
    };
    db.insert_lint_review_if_absent(
        &review_id,
        &sources,
        &serde_json::to_string(&payload).unwrap(),
    )
    .await
    .unwrap();
    EntityRelationRepairSelection { review_id, choice }
}

#[tokio::test]
async fn relation_resolution_lists_conflicts_without_writing_and_rejects_stale_report() {
    let (db, _dir, candidate, from, to) =
        fixture(LintSemanticAction::AddEntityRelation, None).await;
    let old = db
        .create_relation(&from, &to, "related_to", None, None, None, None)
        .await
        .unwrap();
    let canonical = db
        .relation_canonicals()
        .await
        .unwrap()
        .into_iter()
        .find(|v| v != "related_to")
        .unwrap();
    let selected = selection(
        &db,
        &candidate,
        EntityRelationRepairChoice::Add {
            from_entity: from.clone(),
            to_entity: to.clone(),
            relation_type: canonical.clone(),
            source_memory_id: None,
        },
    )
    .await;
    let runner = || LintRunner::new(LintClock::fixed(), CancellationToken::new());
    let general = runner()
        .run(
            &db,
            &LintQuery::new(Some(LintProfile::General), None),
            None,
            false,
        )
        .await
        .unwrap();
    let deep = runner()
        .run(
            &db,
            &LintQuery::new(Some(LintProfile::Deep), None),
            None,
            false,
        )
        .await
        .unwrap();
    let deep = super::super::tests::with_semantic_finding(
        deep,
        &candidate.check_id,
        candidate.finding.clone(),
    );
    let request =
        RepairPlanRequest::try_new(RepairLintScope::global(), general, Some(deep)).unwrap();
    let before = db
        .test_primary_session()
        .await
        .repair_database_content_digest()
        .await
        .unwrap();
    let result = resolve_entity_relation_repair(&db, &request, &selected)
        .await
        .unwrap();
    assert_eq!(result.retire_relation_ids, vec![old]);
    assert_eq!(result.canonical_relation_type, canonical);
    assert!(result.vocabulary_promotion.is_none());
    let after = db
        .test_primary_session()
        .await
        .repair_database_content_digest()
        .await
        .unwrap();
    assert_eq!(
        before, after,
        "resolution must leave all DB tables unchanged"
    );
    db.test_primary_session()
        .await
        .execute(
            "UPDATE refinement_queue SET status='accepted' WHERE id=?1",
            libsql::params![selected.review_id.clone()],
        )
        .await
        .unwrap();
    assert!(resolve_entity_relation_repair(&db, &request, &selected)
        .await
        .is_err());
}

#[tokio::test]
async fn relation_resolution_unknown_predicate_declares_promotion_without_enqueuing_it() {
    let (db, _dir, candidate, from, to) =
        fixture(LintSemanticAction::AddEntityRelation, None).await;
    let selected = selection(
        &db,
        &candidate,
        EntityRelationRepairChoice::Add {
            from_entity: from,
            to_entity: to,
            relation_type: "never_seen_repair_predicate".into(),
            source_memory_id: None,
        },
    )
    .await;
    let before = db
        .test_primary_session()
        .await
        .repair_database_content_digest()
        .await
        .unwrap();
    let snapshot = db.open_lint_snapshot().await.unwrap();
    let result = resolve_on_snapshot(&snapshot, &RepairLintScope::global(), &selected, candidate)
        .await
        .unwrap();
    snapshot.finish().await.unwrap();
    assert_eq!(result.canonical_relation_type, "related_to");
    assert_eq!(
        result.vocabulary_promotion.as_deref(),
        Some("never_seen_repair_predicate")
    );
    assert_eq!(
        before,
        db.test_primary_session()
            .await
            .repair_database_content_digest()
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn relation_retirement_is_bound_to_exact_predicate_not_just_endpoint_pair() {
    let (db, _dir, candidate, from, to) =
        fixture(LintSemanticAction::RemoveEntityRelation, Some("related_to")).await;
    let target = db
        .create_relation(&from, &to, "related_to", None, None, None, None)
        .await
        .unwrap();
    let other_type = db
        .relation_canonicals()
        .await
        .unwrap()
        .into_iter()
        .find(|v| v != "related_to")
        .unwrap();
    let other = db
        .create_relation(&from, &to, &other_type, None, None, None, None)
        .await
        .unwrap();
    let mut selected = selection(
        &db,
        &candidate,
        EntityRelationRepairChoice::Retire { relation_id: other },
    )
    .await;
    let snapshot = db.open_lint_snapshot().await.unwrap();
    let duplicate = semantic::SemanticReviewCandidate {
        check_id: candidate.check_id.clone(),
        finding: candidate.finding.clone(),
        affected_records: candidate.affected_records.clone(),
    };
    assert!(
        matches!(resolve_on_snapshot(&snapshot, &RepairLintScope::global(), &selected, duplicate).await, Err(WenlanError::Conflict(code)) if code == "repair_relation_choice_mismatch")
    );
    selected.choice = EntityRelationRepairChoice::Retire {
        relation_id: target.clone(),
    };
    let result = resolve_on_snapshot(&snapshot, &RepairLintScope::global(), &selected, candidate)
        .await
        .unwrap();
    assert_eq!(result.retire_relation_ids, vec![target]);
    snapshot.finish().await.unwrap();
    assert_eq!(
        db.list_relations_between(&from, &to).await.unwrap().len(),
        2
    );
}

#[tokio::test]
async fn relation_resolution_rejects_unrelated_endpoint_and_out_of_scope_owner() {
    let (db, _dir, candidate, from, to) =
        fixture(LintSemanticAction::AddEntityRelation, None).await;
    let unrelated = db
        .create_entity("Unrelated", "concept", Some("work"))
        .await
        .unwrap();
    let mut selected = selection(
        &db,
        &candidate,
        EntityRelationRepairChoice::Add {
            from_entity: from.clone(),
            to_entity: unrelated,
            relation_type: "related_to".into(),
            source_memory_id: None,
        },
    )
    .await;
    let snapshot = db.open_lint_snapshot().await.unwrap();
    let duplicate = semantic::SemanticReviewCandidate {
        check_id: candidate.check_id.clone(),
        finding: candidate.finding.clone(),
        affected_records: candidate.affected_records.clone(),
    };
    assert!(
        matches!(resolve_on_snapshot(&snapshot, &RepairLintScope::global(), &selected, duplicate).await, Err(WenlanError::Conflict(code)) if code == "repair_relation_owner_mismatch")
    );
    selected.choice = EntityRelationRepairChoice::Add {
        from_entity: from,
        to_entity: to,
        relation_type: "related_to".into(),
        source_memory_id: None,
    };
    assert!(
        matches!(resolve_on_snapshot(&snapshot, &RepairLintScope::uncategorized(), &selected, candidate).await, Err(WenlanError::Conflict(code)) if code == "repair_target_scope_mismatch")
    );
}

#[tokio::test]
async fn relation_provenance_requires_live_memory_bound_to_the_finding() {
    let (db, _dir, mut candidate, from, to) =
        fixture(LintSemanticAction::AddEntityRelation, None).await;
    db.test_primary_session().await.execute_batch(
        "INSERT INTO memories (id,content,source,source_id,title,chunk_index,last_modified,chunk_type,space)
         VALUES ('row-relation-source','Alpha works with Beta','memory','relation-source','Source',0,10,'text','work');"
    ).await.unwrap();
    candidate.affected_records.push(
        RepairAffectedRecord::try_new(RepairAffectedRecordKind::Memory, "relation-source".into())
            .unwrap(),
    );
    candidate.affected_records.sort();
    let mut evidence = candidate.finding.evidence_ids().to_vec();
    evidence.push(crate::lint::semantic_record_digest(
        "memory",
        "relation-source",
    ));
    candidate.finding = LintSemanticFinding::try_new(
        candidate.finding.candidate_id(),
        candidate.finding.proposed_action(),
        candidate.finding.reason_code(),
        8000,
        LintSemanticProviderRoute::CallingAgent,
        evidence,
        vec![],
    )
    .unwrap();
    let mut selected = selection(
        &db,
        &candidate,
        EntityRelationRepairChoice::Add {
            from_entity: from,
            to_entity: to,
            relation_type: "related_to".into(),
            source_memory_id: Some("relation-source".into()),
        },
    )
    .await;
    let duplicate = || semantic::SemanticReviewCandidate {
        check_id: candidate.check_id.clone(),
        finding: candidate.finding.clone(),
        affected_records: candidate.affected_records.clone(),
    };
    let snapshot = db.open_lint_snapshot().await.unwrap();
    resolve_on_snapshot(
        &snapshot,
        &RepairLintScope::global(),
        &selected,
        duplicate(),
    )
    .await
    .unwrap();
    if let EntityRelationRepairChoice::Add {
        source_memory_id, ..
    } = &mut selected.choice
    {
        *source_memory_id = Some("unrelated-source".into());
    }
    assert!(
        matches!(resolve_on_snapshot(&snapshot, &RepairLintScope::global(), &selected, duplicate()).await, Err(WenlanError::Conflict(code)) if code == "repair_relation_owner_mismatch")
    );
    snapshot.finish().await.unwrap();
    if let EntityRelationRepairChoice::Add {
        source_memory_id, ..
    } = &mut selected.choice
    {
        *source_memory_id = Some("relation-source".into());
    }
    db.test_primary_session()
        .await
        .execute(
            "UPDATE memories SET supersede_mode='evicted' WHERE source_id='relation-source'",
            (),
        )
        .await
        .unwrap();
    let snapshot = db.open_lint_snapshot().await.unwrap();
    assert!(
        matches!(resolve_on_snapshot(&snapshot, &RepairLintScope::global(), &selected, candidate).await, Err(WenlanError::Conflict(code)) if code == "repair_target_stale")
    );
}
