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

// Fresh database receipts with explicitly synthetic semantic judgments. This
// exercises the real orchestration and durable queue, not model correctness.
async fn relation_flow_reports(
    db: &MemoryDB,
    finding: Option<LintSemanticFinding>,
) -> (
    wenlan_types::lint::LintReport,
    wenlan_types::lint::LintReport,
) {
    use wenlan_types::lint::*;
    let runner = || LintRunner::new(LintClock::fixed(), CancellationToken::new());
    let general = runner()
        .run(
            db,
            &LintQuery::new(Some(LintProfile::General), None),
            None,
            false,
        )
        .await
        .unwrap();
    let deep = runner()
        .run(
            db,
            &LintQuery::new(Some(LintProfile::Deep), None),
            None,
            false,
        )
        .await
        .unwrap();
    let mut checks = deep.checks().to_vec();
    let target = checks
        .iter_mut()
        .find(|check| check.check_id() == LintSemanticCheckId::EntityRelations.as_str())
        .unwrap();
    let has_finding = finding.is_some();
    *target = LintCheckResult::try_new_with_gate_effect(
        LintCheckResultInput {
            check_id: target.check_id().into(),
            outcome: if has_finding {
                LintOutcome::Finding
            } else {
                LintOutcome::Pass
            },
            severity: if has_finding {
                LintSeverity::Warning
            } else {
                LintSeverity::Info
            },
            applicability: LintApplicability::Applicable,
            precondition: LintPrecondition::Ready,
            coverage: LintCoverage::new(
                LintValidationMethod::FullEnumeration,
                1,
                1,
                LINT_MAX_EVIDENCE_PER_CHECK,
                false,
                u64::from(has_finding),
            )
            .unwrap(),
            metrics: vec![],
            summary_code: if has_finding {
                LintSummaryCode::FindingDetected
            } else {
                LintSummaryCode::CheckPassed
            },
            recommendation_code: has_finding.then_some(LintRecommendationCode::ReviewFinding),
            evidence: finding
                .into_iter()
                .map(|finding| LintEvidenceRef::SemanticFinding { finding })
                .collect(),
            duration_ms: 0,
        },
        target.gate_effect(),
    )
    .unwrap();
    let deep = LintReport::try_new_for_profile(
        deep.profile(),
        deep.scope().clone(),
        deep.capability_context(),
        deep.snapshots().clone(),
        deep.config_fingerprint().clone(),
        deep.producer_receipt().clone(),
        checks,
    )
    .unwrap();
    (
        general,
        super::super::tests::with_completed_agent_work(deep),
    )
}

#[tokio::test]
#[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
async fn relation_current_prepare_apply_verify_closes_only_bound_review_and_replays() {
    use crate::repair::{
        apply_repair, current::prepare_current_repair_with_pages, record_repair_verification,
        RepairArtifactStore,
    };
    use wenlan_types::{
        repair::{ApplyRepairRequest, VerifyRepairRequest},
        repair_current::{CurrentRepairChoice, PrepareCurrentRepairRequest},
    };
    let (db, dir, candidate, from, to) = fixture(LintSemanticAction::AddEntityRelation, None).await;
    let selected = selection(
        &db,
        &candidate,
        EntityRelationRepairChoice::Add {
            from_entity: from,
            to_entity: to,
            relation_type: "related_to".into(),
            source_memory_id: None,
        },
    )
    .await;
    // Another queued review must survive this repair's completion.
    let unrelated_owners = vec!["unrelated-owner".to_string()];
    let unrelated_occurrence = crate::repair::repair_digest(b"unrelated relation review");
    let unrelated_review = format!("lint_review_{}", unrelated_occurrence.as_str());
    let unrelated_payload = RefinementPayload::LintRepairReview {
        check_id: candidate.check_id.clone(),
        owner_binding_digest: lint_review_owner_binding_digest(
            &unrelated_occurrence,
            &unrelated_owners,
        )
        .unwrap(),
        occurrence_digest: unrelated_occurrence,
        issue: "Another review".into(),
        choices: vec!["Inspect".into()],
        suggested_research_queries: vec![],
    };
    db.insert_lint_review_if_absent(
        &unrelated_review,
        &unrelated_owners,
        &serde_json::to_string(&unrelated_payload).unwrap(),
    )
    .await
    .unwrap();
    let (general, deep) = relation_flow_reports(&db, Some(candidate.finding.clone())).await;
    let store = RepairArtifactStore::new(dir.path().join("flow-repairs"));
    let manifest = prepare_current_repair_with_pages(
        &db,
        &store,
        PrepareCurrentRepairRequest {
            lint_scope: RepairLintScope::global(),
            choice: CurrentRepairChoice::entity_relation(selected.clone()).unwrap(),
        },
        general,
        Some(deep),
        None,
        1721000000,
    )
    .await
    .unwrap();
    let apply_request = || {
        ApplyRepairRequest::try_new(
            manifest.manifest_id().into(),
            manifest.manifest_digest().clone(),
            format!(
                "apply repair {} {}",
                manifest.manifest_id(),
                manifest.manifest_digest().as_str()
            ),
        )
        .unwrap()
    };
    let applied = apply_repair(&db, &store, apply_request(), 1721000010)
        .await
        .unwrap();
    // Real background writes after apply must not strand semantic verification.
    // Fresh reports below still bind the resulting database state.
    db.test_primary_session()
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
    let verify_request = |general, deep| {
        VerifyRepairRequest::try_new(
            manifest.manifest_id().into(),
            manifest.manifest_digest().clone(),
            applied.receipt_digest().clone(),
            general,
            deep,
        )
        .unwrap()
    };
    let (general, deep) = relation_flow_reports(&db, Some(candidate.finding)).await;
    let failure =
        record_repair_verification(&db, &store, verify_request(general, deep), None, 1721000020)
            .await
            .unwrap_err();
    assert!(
        matches!(failure, WenlanError::Validation(code) if code == "repair_target_assertion_failed")
    );
    let review_status = |review_id: String| async {
        let session = db.test_primary_session().await;
        session
            .query(
                "SELECT status FROM refinement_queue WHERE id=?1",
                libsql::params![review_id],
            )
            .await
            .unwrap()
            .next()
            .await
            .unwrap()
            .unwrap()
            .get::<String>(0)
            .unwrap()
    };
    assert_eq!(
        review_status(selected.review_id.clone()).await,
        "awaiting_review"
    );
    let wenlan_types::repair::RepairTarget::EntityRelation {
        from_entity,
        to_entity,
        ..
    } = manifest.target()
    else {
        unreachable!()
    };
    let new_mismatch = LintSemanticFinding::try_new(
        LintOpaqueId::from_sorted_position(0).unwrap(),
        LintSemanticAction::RemoveEntityRelation,
        LintSemanticReasonCode::ExistingRelationMismatch,
        8000,
        LintSemanticProviderRoute::CallingAgent,
        vec![
            crate::lint::semantic_record_key_digest(&format!(
                "relation-entity:{from_entity}:related_to:from"
            )),
            crate::lint::semantic_record_key_digest(&format!(
                "relation-entity:{to_entity}:related_to:to"
            )),
        ],
        vec![],
    )
    .unwrap();
    let (general, deep) = relation_flow_reports(&db, Some(new_mismatch)).await;
    let failure =
        record_repair_verification(&db, &store, verify_request(general, deep), None, 1721000025)
            .await
            .unwrap_err();
    assert!(
        matches!(failure, WenlanError::Validation(code) if code == "repair_target_assertion_failed")
    );
    let (general, deep) = relation_flow_reports(&db, None).await;
    let request = verify_request(general, deep);
    let verified = record_repair_verification(&db, &store, request.clone(), None, 1721000030)
        .await
        .unwrap();
    assert_eq!(review_status(selected.review_id).await, "resolved");
    assert_eq!(review_status(unrelated_review).await, "awaiting_review");
    assert_eq!(
        record_repair_verification(&db, &store, request, None, 1721000040)
            .await
            .unwrap(),
        verified
    );
    assert_eq!(
        apply_repair(&db, &store, apply_request(), 1721000050)
            .await
            .unwrap(),
        applied
    );
    let activity_count = db
        .test_primary_session()
        .await
        .query(
            "SELECT count(*) FROM agent_activity WHERE query=?1",
            libsql::params![manifest.manifest_id()],
        )
        .await
        .unwrap()
        .next()
        .await
        .unwrap()
        .unwrap()
        .get::<i64>(0)
        .unwrap();
    assert_eq!(activity_count, 1);
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
    if let EntityRelationRepairChoice::Add {
        from_entity,
        to_entity,
        relation_type,
        ..
    } = &selected.choice
    {
        db.create_relation(
            from_entity,
            to_entity,
            relation_type,
            None,
            None,
            None,
            Some("original-source"),
        )
        .await
        .unwrap();
    }
    let snapshot = db.open_lint_snapshot().await.unwrap();
    assert!(
        matches!(resolve_on_snapshot(&snapshot, &RepairLintScope::global(), &selected, duplicate()).await, Err(WenlanError::Conflict(code)) if code == "repair_relation_source_binding_conflict")
    );
    snapshot.finish().await.unwrap();
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

#[tokio::test]
async fn relation_prepare_distinguishes_noop_source_fill_confidence_and_retirements() {
    // prior source, selected source, prior confidence, conflicting edge, accepted
    for (had_source, select_source, confidence, conflict_edge, accepted) in [
        (true, true, 0.9, false, false),
        (true, false, 0.9, false, false),
        (false, false, 0.9, false, false),
        (false, true, 0.9, false, true),
        (true, true, 0.6, false, true),
        (false, false, 0.6, false, true),
        (false, false, 0.9, true, true),
    ] {
        let (db, _dir, mut candidate, from, to) =
            fixture(LintSemanticAction::AddEntityRelation, None).await;
        db.test_primary_session().await.execute_batch(
            "INSERT INTO memories (id,content,source,source_id,title,chunk_index,last_modified,chunk_type,space)
             VALUES ('row-relation-source','Alpha works with Beta','memory','relation-source','Source',0,10,'text','work');"
        ).await.unwrap();
        candidate.affected_records.push(
            RepairAffectedRecord::try_new(
                RepairAffectedRecordKind::Memory,
                "relation-source".into(),
            )
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
        db.create_relation(
            &from,
            &to,
            "related_to",
            None,
            Some(confidence),
            None,
            had_source.then_some("relation-source"),
        )
        .await
        .unwrap();
        let conflicting = if conflict_edge {
            Some(
                db.create_relation(&from, &to, "works_on", None, Some(0.9), None, None)
                    .await
                    .unwrap(),
            )
        } else {
            None
        };
        let selected = selection(
            &db,
            &candidate,
            EntityRelationRepairChoice::Add {
                from_entity: from,
                to_entity: to,
                relation_type: "related_to".into(),
                source_memory_id: select_source.then_some("relation-source".into()),
            },
        )
        .await;
        let snapshot = db.open_lint_snapshot().await.unwrap();
        let result =
            resolve_on_snapshot(&snapshot, &RepairLintScope::global(), &selected, candidate).await;
        if accepted {
            assert_eq!(
                result.unwrap().retire_relation_ids,
                conflicting.into_iter().collect::<Vec<_>>()
            );
        } else {
            assert!(
                matches!(result, Err(WenlanError::Conflict(code)) if code == "repair_relation_unchanged")
            );
        }
        snapshot.finish().await.unwrap();
    }
}

#[tokio::test]
async fn relation_retire_null_predicate_returns_choice_mismatch() {
    let (db, _dir, candidate, from, to) =
        fixture(LintSemanticAction::RemoveEntityRelation, Some("related_to")).await;
    let target = db
        .create_relation(&from, &to, "related_to", None, None, None, None)
        .await
        .unwrap();
    db.test_primary_session()
        .await
        .execute(
            "UPDATE edges SET semantic_type=NULL WHERE edge_id=?1",
            libsql::params![target.clone()],
        )
        .await
        .unwrap();
    let selected = selection(
        &db,
        &candidate,
        EntityRelationRepairChoice::Retire {
            relation_id: target,
        },
    )
    .await;
    let snapshot = db.open_lint_snapshot().await.unwrap();
    let result =
        resolve_on_snapshot(&snapshot, &RepairLintScope::global(), &selected, candidate).await;
    assert!(
        matches!(result, Err(WenlanError::Conflict(code)) if code == "repair_relation_choice_mismatch")
    );
    snapshot.finish().await.unwrap();
}
