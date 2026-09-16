// SPDX-License-Identifier: Apache-2.0
//! Read-only resolution of an explicit relation repair selection.
//!
//! This is the preparation input, not an apply authorization or a durable
//! manifest. The writer must capture and recheck the resolved records in its
//! transaction, including every relation to retire and vocabulary side effect.

use super::{semantic, semantic_review_occurrence_digest};
use crate::{
    db::{MemoryDB, UNFILED_SPACE_ID},
    error::WenlanError,
    lint::snapshot::LintReadSnapshot,
    repair::{canonical_lint_review_source_ids, lint_review_owner_binding_digest},
};
use serde::Serialize;
use wenlan_types::{
    lint::{LintOutcome, LintSemanticAction, LintSemanticCheckId, LintSemanticFinding},
    repair::{RepairLintScope, RepairReviewBinding},
    repair_plan::{RepairAffectedRecord, RepairAffectedRecordKind, RepairPlanRequest},
    repair_relation::{EntityRelationRepairChoice, EntityRelationRepairSelection},
    RefinementPayload,
};

#[cfg(test)]
#[path = "relation_tests.rs"]
mod tests;

#[derive(Debug, Clone, Serialize)]
pub struct EntityRelationRepairResolution {
    pub review_binding: RepairReviewBinding,
    pub selected_finding: LintSemanticFinding,
    pub affected_records: Vec<RepairAffectedRecord>,
    pub selection: EntityRelationRepairSelection,
    pub from_entity: String,
    pub to_entity: String,
    pub target_relation_id: String,
    pub canonical_relation_type: String,
    /// Includes the target itself for an explicit retirement.
    pub retire_relation_ids: Vec<String>,
    /// Unknown predicates follow the canonical writer: related_to plus a
    /// vocabulary proposal. Neither is written during resolution.
    pub vocabulary_promotion: Option<String>,
}

/// Resolve only against daemon-fresh reports and the exact awaiting review
/// occurrence. Does not mutate the queue, vocabulary, graph, or repair store.
pub async fn resolve_entity_relation_repair(
    db: &MemoryDB,
    request: &RepairPlanRequest,
    selection: &EntityRelationRepairSelection,
) -> Result<EntityRelationRepairResolution, WenlanError> {
    let snapshot = db.open_lint_snapshot().await.map_err(snapshot_error)?;
    let resolved = resolve_entity_relation_on_snapshot(&snapshot, request, selection).await?;
    let deep = request
        .deep_report()
        .ok_or_else(|| conflict("repair_deep_report_missing"))?;
    let receipt = snapshot.finish().await.map_err(snapshot_error)?;
    super::validate_report_source_receipts(&[request.general_report(), deep], receipt)?;
    Ok(resolved)
}

pub(crate) async fn resolve_entity_relation_on_snapshot(
    snapshot: &LintReadSnapshot<'_>,
    request: &RepairPlanRequest,
    selection: &EntityRelationRepairSelection,
) -> Result<EntityRelationRepairResolution, WenlanError> {
    selection.validate().map_err(WenlanError::Validation)?;
    let deep = request
        .deep_report()
        .ok_or_else(|| conflict("repair_deep_report_missing"))?;
    if request.general_report().producer_receipt() != deep.producer_receipt() {
        return Err(conflict("repair_source_producers_mismatch"));
    }
    let check = deep
        .checks()
        .iter()
        .find(|check| check.check_id() == LintSemanticCheckId::EntityRelations.as_str())
        .ok_or_else(|| conflict("repair_current_check_unavailable"))?;
    if !matches!(check.outcome(), LintOutcome::Pass | LintOutcome::Finding) {
        return Err(conflict("repair_current_check_unavailable"));
    }
    super::validate_durable_scope(snapshot, request.scope(), request.general_report().scope())
        .await?;
    let candidates = semantic::resolve_current(snapshot, deep).await?;
    let mut selected = None;
    for candidate in candidates {
        let semantic::SemanticResolution::Review(candidate) = candidate else {
            continue;
        };
        if candidate.check_id != LintSemanticCheckId::EntityRelations.as_str() {
            continue;
        }
        let occurrence = semantic_review_occurrence_digest(
            &candidate.check_id,
            &candidate.finding,
            &candidate.affected_records,
        )?;
        if format!("lint_review_{}", occurrence.as_str()) != selection.review_id {
            continue;
        }
        if selected.replace(candidate).is_some() {
            return Err(conflict("repair_current_finding_ambiguous"));
        }
    }
    let candidate = selected.ok_or_else(|| conflict("repair_current_finding_missing"))?;
    resolve_on_snapshot(snapshot, request.scope(), selection, candidate).await
}

async fn resolve_on_snapshot(
    snapshot: &LintReadSnapshot<'_>,
    scope: &RepairLintScope,
    selection: &EntityRelationRepairSelection,
    candidate: semantic::SemanticReviewCandidate,
) -> Result<EntityRelationRepairResolution, WenlanError> {
    selection.validate().map_err(WenlanError::Validation)?;
    let required_action = match selection.choice {
        EntityRelationRepairChoice::Add { .. } => LintSemanticAction::AddEntityRelation,
        EntityRelationRepairChoice::Retire { .. } => LintSemanticAction::RemoveEntityRelation,
    };
    if candidate.check_id != LintSemanticCheckId::EntityRelations.as_str()
        || candidate.finding.proposed_action() != required_action
        || candidate.finding.unresolved_disagreement()
    {
        return Err(conflict("repair_relation_choice_mismatch"));
    }
    let occurrence = semantic_review_occurrence_digest(
        &candidate.check_id,
        &candidate.finding,
        &candidate.affected_records,
    )?;
    if selection.review_id != format!("lint_review_{}", occurrence.as_str()) {
        return Err(conflict("repair_current_review_binding_mismatch"));
    }
    let source_ids = canonical_lint_review_source_ids(
        &candidate
            .affected_records
            .iter()
            .map(|record| record.durable_id().to_string())
            .collect::<Vec<_>>(),
    )?;
    let binding = RepairReviewBinding::try_new(
        selection.review_id.clone(),
        occurrence.clone(),
        source_ids.clone(),
    )
    .map_err(|error| WenlanError::Validation(error.to_string()))?;
    let mut rows = snapshot
        .query(
            "SELECT action,source_ids,payload,status FROM refinement_queue WHERE id=?1",
            params(&[&selection.review_id]),
        )
        .await
        .map_err(snapshot_error)?;
    let row = rows
        .next()
        .await
        .map_err(snapshot_error)?
        .ok_or_else(|| conflict("repair_target_stale"))?;
    let stored_sources: Vec<String> =
        serde_json::from_str(&row.get::<String>(1).map_err(database_error)?)
            .map_err(|_| conflict("repair_target_stale"))?;
    if row.get::<String>(0).map_err(database_error)? != "lint_repair_review"
        || row.get::<String>(3).map_err(database_error)? != "awaiting_review"
        || stored_sources != source_ids
    {
        return Err(conflict("repair_target_stale"));
    }
    let payload = row
        .get::<Option<String>>(2)
        .map_err(database_error)?
        .ok_or_else(|| conflict("repair_target_stale"))?;
    let decoded =
        crate::db::validate_lint_review_contract(&selection.review_id, &source_ids, &payload)
            .map_err(|_| conflict("repair_target_stale"))?;
    let RefinementPayload::LintRepairReview {
        check_id,
        occurrence_digest,
        owner_binding_digest,
        ..
    } = decoded
    else {
        return Err(conflict("repair_target_stale"));
    };
    if check_id != candidate.check_id
        || occurrence_digest != occurrence
        || owner_binding_digest != lint_review_owner_binding_digest(&occurrence, &source_ids)?
    {
        return Err(conflict("repair_target_stale"));
    }
    drop(rows);

    let (from, to, canonical, target_id, retire_ids, promotion) = match &selection.choice {
        EntityRelationRepairChoice::Add {
            from_entity,
            to_entity,
            relation_type,
            source_memory_id,
        } => {
            if let Some(source) = source_memory_id {
                require_owner(&candidate, RepairAffectedRecordKind::Memory, source)?;
                let mut rows = snapshot.query(
                    "SELECT space FROM memories WHERE source='memory' AND source_id=?1 AND chunk_index=0
                     AND pending_revision=0 AND COALESCE(is_recap,0)=0 AND supersede_mode!='evicted'",
                    params(&[source]),
                ).await.map_err(snapshot_error)?;
                let row = rows
                    .next()
                    .await
                    .map_err(snapshot_error)?
                    .ok_or_else(|| conflict("repair_target_stale"))?;
                require_space(scope, &row.get::<String>(0).map_err(database_error)?)?;
                if rows.next().await.map_err(snapshot_error)?.is_some() {
                    return Err(conflict("repair_target_stale"));
                }
            }
            let (canonical, promotion) = resolve_vocabulary(snapshot, relation_type).await?;
            let target_id = crate::provenance::compute_edge_id(
                "relates",
                "entity",
                from_entity,
                "entity",
                to_entity,
                &canonical,
            );
            // The canonical relation writer preserves the first source on
            // reassertion (including reactivation). Do not approve a preview
            // that promises to replace it with a different memory.
            let mut fills_source = false;
            if let Some(selected_source) = source_memory_id {
                let mut prior = snapshot.query(
                    "SELECT json_extract(payload,'$.source_memory_id') FROM edges WHERE edge_id=?1",
                    params(&[&target_id]),
                ).await.map_err(snapshot_error)?;
                if let Some(row) = prior.next().await.map_err(snapshot_error)? {
                    let first_source = row.get::<Option<String>>(0).map_err(database_error)?;
                    if first_source
                        .as_deref()
                        .is_some_and(|source| source != selected_source)
                    {
                        return Err(conflict("repair_relation_source_binding_conflict"));
                    }
                    fills_source = first_source.is_none();
                }
            }
            let mut rows = snapshot.query(
                "SELECT edge_id,semantic_type,json_extract(payload,'$.confidence') FROM edges WHERE edge_type='relates' AND valid_until IS NULL
                 AND src_id=?1 AND dst_id=?2 ORDER BY edge_id", params(&[from_entity, to_entity]),
            ).await.map_err(snapshot_error)?;
            let mut retire_ids = Vec::new();
            let mut unchanged_target = false;
            while let Some(row) = rows.next().await.map_err(snapshot_error)? {
                let id = row.get::<String>(0).map_err(database_error)?;
                let predicate = row.get::<Option<String>>(1).map_err(database_error)?;
                if id == target_id {
                    // This route supplies the fresh finding's confidence to
                    // the canonical higher-confidence merge, even though the
                    // user selection has no editable confidence field.
                    let prior_confidence = row.get::<Option<f64>>(2).map_err(database_error)?;
                    let confidence =
                        f64::from(candidate.finding.confidence_basis_points()) / 10_000.0;
                    let raises_confidence = Some(confidence) > prior_confidence;
                    unchanged_target = !fills_source && !raises_confidence;
                }
                // Match the canonical post-write conflict rule, including aliases.
                if id != target_id
                    && predicate
                        .as_deref()
                        .is_some_and(|value| value != relation_type)
                {
                    retire_ids.push(id);
                }
            }
            if unchanged_target && retire_ids.is_empty() {
                return Err(conflict("repair_relation_unchanged"));
            }
            (
                from_entity.clone(),
                to_entity.clone(),
                canonical,
                target_id,
                retire_ids,
                promotion,
            )
        }
        EntityRelationRepairChoice::Retire { relation_id } => {
            let mut rows = snapshot
                .query(
                    "SELECT src_id,dst_id,semantic_type FROM edges WHERE edge_id=?1
                 AND edge_type='relates' AND valid_until IS NULL",
                    params(&[relation_id]),
                )
                .await
                .map_err(snapshot_error)?;
            let row = rows
                .next()
                .await
                .map_err(snapshot_error)?
                .ok_or_else(|| conflict("repair_target_stale"))?;
            let from = row.get::<String>(0).map_err(database_error)?;
            let to = row.get::<String>(1).map_err(database_error)?;
            let predicate = row
                .get::<Option<String>>(2)
                .map_err(database_error)?
                .ok_or_else(|| conflict("repair_relation_choice_mismatch"))?;
            // Endpoint ownership alone cannot distinguish two predicates between
            // the same pair. Require both exact relation evidence keys too.
            for key in [
                format!("relation-entity:{from}:{predicate}:from"),
                format!("relation-entity:{to}:{predicate}:to"),
            ] {
                let digest = crate::lint::semantic_record_key_digest(&key);
                if !candidate.finding.evidence_ids().contains(&digest)
                    && !candidate.finding.counterevidence_ids().contains(&digest)
                {
                    return Err(conflict("repair_relation_choice_mismatch"));
                }
            }
            (
                from,
                to,
                predicate,
                relation_id.clone(),
                vec![relation_id.clone()],
                None,
            )
        }
    };
    for endpoint in [&from, &to] {
        require_owner(&candidate, RepairAffectedRecordKind::Entity, endpoint)?;
        let mut rows = snapshot
            .query(
                "SELECT p.space FROM entity_page_map m JOIN pages p ON p.id=m.page_id
             WHERE m.entity_id=?1 AND p.kind='entity' AND p.status='active'",
                params(&[endpoint]),
            )
            .await
            .map_err(snapshot_error)?;
        let row = rows
            .next()
            .await
            .map_err(snapshot_error)?
            .ok_or_else(|| conflict("repair_target_stale"))?;
        require_space(scope, &row.get::<String>(0).map_err(database_error)?)?;
    }
    Ok(EntityRelationRepairResolution {
        review_binding: binding,
        selected_finding: candidate.finding,
        affected_records: candidate.affected_records,
        selection: selection.clone(),
        from_entity: from,
        to_entity: to,
        target_relation_id: target_id,
        canonical_relation_type: canonical,
        retire_relation_ids: retire_ids,
        vocabulary_promotion: promotion,
    })
}

fn require_owner(
    candidate: &semantic::SemanticReviewCandidate,
    kind: RepairAffectedRecordKind,
    id: &str,
) -> Result<(), WenlanError> {
    if candidate
        .affected_records
        .iter()
        .any(|record| record.kind() == kind && record.durable_id() == id)
    {
        Ok(())
    } else {
        Err(conflict("repair_relation_owner_mismatch"))
    }
}

fn require_space(scope: &RepairLintScope, space: &str) -> Result<(), WenlanError> {
    match scope {
        RepairLintScope::Global => Ok(()),
        RepairLintScope::Registered { space: expected } if expected == space => Ok(()),
        RepairLintScope::Uncategorized if space == UNFILED_SPACE_ID => Ok(()),
        _ => Err(conflict("repair_target_scope_mismatch")),
    }
}

async fn resolve_vocabulary(
    snapshot: &LintReadSnapshot<'_>,
    requested: &str,
) -> Result<(String, Option<String>), WenlanError> {
    let mut rows = snapshot
        .query(
            "SELECT canonical,aliases FROM relation_type_vocabulary ORDER BY canonical",
            params(&[]),
        )
        .await
        .map_err(snapshot_error)?;
    let mut alias_match = None;
    while let Some(row) = rows.next().await.map_err(snapshot_error)? {
        let canonical = row.get::<String>(0).map_err(database_error)?;
        if canonical == requested {
            return Ok((canonical, None));
        }
        let aliases = row.get::<Option<String>>(1).map_err(database_error)?;
        if alias_match.is_none()
            && aliases
                .as_deref()
                .and_then(|value| serde_json::from_str::<Vec<serde_json::Value>>(value).ok())
                .is_some_and(|aliases| {
                    aliases.iter().any(|value| {
                        value
                            .as_str()
                            .is_some_and(|alias| alias.to_lowercase() == requested)
                    })
                })
        {
            alias_match = Some(canonical);
        }
    }
    Ok(alias_match
        .map(|canonical| (canonical, None))
        .unwrap_or_else(|| ("related_to".to_string(), Some(requested.to_string()))))
}

fn params(values: &[&str]) -> libsql::params::Params {
    libsql::params::Params::Positional(
        values
            .iter()
            .map(|value| libsql::Value::Text((*value).to_string()))
            .collect(),
    )
}
fn conflict(code: &str) -> WenlanError {
    WenlanError::Conflict(code.to_string())
}
fn database_error(error: libsql::Error) -> WenlanError {
    WenlanError::VectorDb(format!("repair relation: {error}"))
}
fn snapshot_error(error: crate::lint::snapshot::SnapshotError) -> WenlanError {
    WenlanError::VectorDb(format!("repair relation snapshot: {error}"))
}
