// SPDX-License-Identifier: Apache-2.0
//! One transaction owns relation input validation, canonical writes and proof.

use super::{repair_memory_cas::rollback_repair_transaction, MemoryDB};
use crate::{
    error::WenlanError,
    post_write::RepairWriteProof,
    repair::{
        self,
        relation_snapshot::{self, RelationCaptureContext, RelationReader},
    },
};
use std::collections::BTreeSet;
use wenlan_types::{
    repair::{RepairManifest, RepairMutation, RepairTarget, RepairWriter},
    repair_relation::{
        RepairRelationMutation, RepairRelationSnapshot, RepairRelationSqlValue, RepairRelationTable,
    },
};

pub(crate) fn capture_context(
    manifest: &RepairManifest,
) -> Result<RelationCaptureContext<'_>, WenlanError> {
    let (
        RepairTarget::EntityRelation {
            from_entity,
            to_entity,
            review_owner_ids,
            ..
        },
        RepairMutation::EntityRelation { change },
    ) = (manifest.target(), manifest.mutation())
    else {
        return Err(WenlanError::Validation(
            "repair_relation_writer_mismatch".into(),
        ));
    };
    if manifest.writer() != RepairWriter::EntityRelation {
        return Err(WenlanError::Validation(
            "repair_relation_writer_mismatch".into(),
        ));
    }
    let binding = manifest
        .source()
        .review_binding()
        .ok_or_else(|| WenlanError::Validation("repair_relation_review_binding_missing".into()))?;
    if binding.owner_ids() != review_owner_ids {
        return Err(WenlanError::Validation(
            "repair_relation_owner_mismatch".into(),
        ));
    }
    let promotion = match change {
        RepairRelationMutation::Add {
            vocabulary_promotion,
            ..
        } => vocabulary_promotion.as_deref(),
        RepairRelationMutation::Retire => None,
    };
    Ok(RelationCaptureContext {
        manifest_id: manifest.manifest_id(),
        review_id: binding.review_id(),
        from_entity,
        to_entity,
        owner_ids: review_owner_ids,
        canonical_relation_type: match change {
            RepairRelationMutation::Add {
                canonical_relation_type,
                ..
            } => Some(canonical_relation_type),
            RepairRelationMutation::Retire => None,
        },
        vocabulary_promotion: promotion,
    })
}

fn potential_graph_spaces(snapshot: &RepairRelationSnapshot) -> Result<Vec<String>, WenlanError> {
    let mut spaces = BTreeSet::new();
    for table in &snapshot.tables {
        if !matches!(
            table.table,
            RepairRelationTable::Pages
                | RepairRelationTable::Edges
                | RepairRelationTable::SpaceGraphState
        ) {
            continue;
        }
        let index = table
            .columns
            .iter()
            .position(|name| name == "space")
            .ok_or_else(|| {
                WenlanError::Validation("repair_relation_snapshot_schema_mismatch".into())
            })?;
        for row in &table.rows {
            match row.get(index) {
                Some(RepairRelationSqlValue::Text { value }) => {
                    spaces.insert(value.clone());
                }
                Some(RepairRelationSqlValue::Null) => {}
                _ => {
                    return Err(WenlanError::Validation(
                        "repair_relation_snapshot_schema_mismatch".into(),
                    ))
                }
            }
        }
    }
    Ok(spaces.into_iter().collect())
}

fn validate_prepared_review(
    snapshot: &RepairRelationSnapshot,
    manifest: &RepairManifest,
) -> Result<(), WenlanError> {
    let stale = || WenlanError::Conflict("repair_target_stale".into());
    let context = capture_context(manifest)?;
    let table = snapshot
        .tables
        .iter()
        .find(|table| table.table == RepairRelationTable::RefinementQueue)
        .ok_or_else(stale)?;
    let column = |name: &str| {
        table
            .columns
            .iter()
            .position(|column| column == name)
            .ok_or_else(stale)
    };
    let id_index = column("id")?;
    let mut rows = table.rows.iter().filter(|row| matches!(row.get(id_index), Some(RepairRelationSqlValue::Text { value }) if value == context.review_id));
    let row = rows.next().ok_or_else(stale)?;
    if rows.next().is_some() {
        return Err(stale());
    }
    let text = |name: &str| -> Result<&str, WenlanError> {
        match row.get(column(name)?) {
            Some(RepairRelationSqlValue::Text { value }) => Ok(value),
            _ => Err(stale()),
        }
    };
    if text("action")? != "lint_repair_review" || text("status")? != "awaiting_review" {
        return Err(stale());
    }
    let owners: Vec<String> = serde_json::from_str(text("source_ids")?).map_err(|_| stale())?;
    if owners != context.owner_ids {
        return Err(stale());
    }
    let payload =
        super::validate_lint_review_contract(context.review_id, &owners, text("payload")?)
            .map_err(|_| stale())?;
    let wenlan_types::RefinementPayload::LintRepairReview {
        check_id,
        occurrence_digest,
        owner_binding_digest,
        ..
    } = payload
    else {
        return Err(stale());
    };
    let binding = manifest.source().review_binding().ok_or_else(stale)?;
    if check_id != manifest.source().check_id()
        || occurrence_digest != *binding.occurrence_digest()
        || owner_binding_digest
            != repair::lint_review_owner_binding_digest(&occurrence_digest, &owners)?
    {
        return Err(stale());
    }
    Ok(())
}

impl MemoryDB {
    pub(crate) async fn relation_repair_cas<F>(
        &self,
        manifest: &RepairManifest,
        expected: &RepairRelationSnapshot,
        before_commit: F,
    ) -> Result<RepairWriteProof, WenlanError>
    where
        F: FnOnce(&RepairWriteProof) -> Result<(), WenlanError>,
    {
        let context = capture_context(manifest)?;
        let RepairTarget::EntityRelation { relation_id, .. } = manifest.target() else {
            unreachable!()
        };
        let RepairMutation::EntityRelation { change } = manifest.mutation() else {
            unreachable!()
        };
        let (edge_ids, canonical) = match change {
            RepairRelationMutation::Add {
                canonical_relation_type,
                retire_relation_ids,
                ..
            } => {
                let computed = crate::provenance::compute_edge_id(
                    "relates",
                    "entity",
                    context.from_entity,
                    "entity",
                    context.to_entity,
                    canonical_relation_type,
                );
                if computed != *relation_id {
                    return Err(WenlanError::Validation(
                        "repair_relation_identity_mismatch".into(),
                    ));
                }
                let mut ids = retire_relation_ids.clone();
                ids.push(relation_id.clone());
                (ids, Some(canonical_relation_type.as_str()))
            }
            RepairRelationMutation::Retire => (vec![relation_id.clone()], None),
        };
        let spaces = potential_graph_spaces(expected)?;
        let exclusions = relation_snapshot::effect_exclusions(
            &edge_ids,
            &spaces,
            canonical,
            context.vocabulary_promotion,
            manifest.manifest_id(),
        )?;
        let connection = self.conn.lock().await;
        connection
            .execute("BEGIN IMMEDIATE", ())
            .await
            .map_err(|e| WenlanError::VectorDb(format!("repair begin: {e}")))?;
        let result = async {
            let before =
                relation_snapshot::capture(&RelationReader::Connection(&connection), &context)
                    .await?;
            let before_receipt = relation_snapshot::receipt(&before)?;
            if before != *expected
                || before_receipt != *manifest.expected_state().canonical_receipt()
            {
                return Err(WenlanError::Conflict("repair_target_stale".into()));
            }
            validate_prepared_review(&before, manifest)?;
            let non_target_before =
                repair::database_content_digest_with_exclusions(&connection, &exclusions).await?;
            let parity_before = repair::parity_input_generation_on_connection(&connection).await?;
            let started_at = chrono::Utc::now().timestamp();
            let effects =
                Self::execute_relation_repair_on_connection(&connection, manifest, started_at)
                    .await?;
            let finished_at = chrono::Utc::now().timestamp();
            let after =
                relation_snapshot::capture(&RelationReader::Connection(&connection), &context)
                    .await?;
            let expected_parity_delta = repair::relation_effects::validate(
                &before,
                &after,
                manifest,
                &effects,
                started_at,
                finished_at,
            )?;
            let parity_after = repair::parity_input_generation_on_connection(&connection).await?;
            if parity_after.checked_sub(parity_before) != Some(expected_parity_delta) {
                return Err(WenlanError::VectorDb("repair_effect_escape".into()));
            }
            let non_target_after =
                repair::database_content_digest_with_exclusions(&connection, &exclusions).await?;
            if non_target_before != non_target_after {
                return Err(WenlanError::VectorDb("repair_effect_escape".into()));
            }
            let after_receipt = relation_snapshot::applied_receipt(&after, &context)?;
            let post_apply_db_digest = repair::database_content_digest(&connection).await?;
            Ok((
                RepairWriteProof::from_parts(
                    before_receipt,
                    after_receipt,
                    non_target_before,
                    non_target_after,
                    post_apply_db_digest,
                ),
                effects.graph_updates,
            ))
        }
        .await;
        let (proof, graph_updates) = match result {
            Ok(value) => value,
            Err(error) => {
                rollback_repair_transaction(&connection, &error, false).await?;
                return Err(error);
            }
        };
        if let Err(error) = before_commit(&proof) {
            rollback_repair_transaction(&connection, &error, false).await?;
            return Err(error);
        }
        if let Err(error) = connection.execute("COMMIT", ()).await {
            let error = WenlanError::VectorDb(format!("repair commit failed: {error}"));
            rollback_repair_transaction(&connection, &error, false).await?;
            return Err(error);
        }
        drop(connection);
        self.record_community_dirty_nodes(graph_updates);
        Ok(proof)
    }

    pub(crate) async fn capture_relation_repair_state(
        &self,
        manifest: &RepairManifest,
    ) -> Result<RepairRelationSnapshot, WenlanError> {
        let context = capture_context(manifest)?;
        let snapshot = self
            .open_lint_snapshot()
            .await
            .map_err(|e| WenlanError::VectorDb(format!("repair relation snapshot: {e}")))?;
        let state =
            relation_snapshot::capture(&RelationReader::Snapshot(&snapshot), &context).await?;
        let receipt = snapshot
            .finish()
            .await
            .map_err(|e| WenlanError::VectorDb(format!("repair relation snapshot: {e}")))?;
        if !receipt.is_consistent() {
            return Err(WenlanError::Conflict("repair_target_stale".into()));
        }
        Ok(state)
    }
}
