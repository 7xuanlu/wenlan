// SPDX-License-Identifier: Apache-2.0
//! Canonical relation actions inside the repair caller's transaction.
//! The caller must check the complete before snapshot and effect guard before
//! committing, then publish returned graph dirty nodes only after commit.

use super::{CommunityGenerationUpdate, MemoryDB, RelationWriteInput};
use crate::error::WenlanError;
use wenlan_types::{
    repair::{RepairManifest, RepairMutation, RepairTarget},
    repair_relation::RepairRelationMutation,
};

pub(crate) struct RelationActivity {
    pub action: &'static str,
    pub memory_ids: Vec<String>,
    pub detail: String,
}

pub(crate) struct RelationWriteEffects {
    pub graph_updates: Vec<CommunityGenerationUpdate>,
    pub activity: Vec<RelationActivity>,
    pub vocabulary_incremented: bool,
    pub vocabulary_proposed: bool,
}

impl MemoryDB {
    pub(super) async fn execute_relation_repair_on_connection(
        conn: &libsql::Connection,
        manifest: &RepairManifest,
        now: i64,
    ) -> Result<RelationWriteEffects, WenlanError> {
        let (
            RepairTarget::EntityRelation {
                relation_id,
                from_entity,
                to_entity,
                ..
            },
            RepairMutation::EntityRelation { change },
        ) = (manifest.target(), manifest.mutation())
        else {
            return Err(WenlanError::Validation(
                "repair_relation_writer_mismatch".into(),
            ));
        };
        let mut effects = RelationWriteEffects {
            graph_updates: vec![],
            activity: vec![],
            vocabulary_incremented: false,
            vocabulary_proposed: false,
        };
        match change {
            RepairRelationMutation::Add {
                requested_relation_type,
                canonical_relation_type,
                source_memory_id,
                confidence_basis_points,
                retire_relation_ids,
                vocabulary_promotion,
            } => {
                if let Some(proposed) = vocabulary_promotion {
                    effects.vocabulary_proposed = Self::insert_vocab_promote_proposal_on_connection(
                        conn, "relation", proposed, None,
                    )
                    .await
                    .map_err(write_error)? > 0;
                }
                let (written_id, existed, generations) = Self::create_relation_on_connection(
                    conn,
                    RelationWriteInput {
                        from_entity,
                        to_entity,
                        canonical: canonical_relation_type,
                        source_agent: Some("source-repair"),
                        confidence: Some(f64::from(*confidence_basis_points) / 10_000.0),
                        explanation: None,
                        source_memory_id: source_memory_id.as_deref(),
                        span_quote: None,
                        source_content: None,
                        model_version: None,
                        prompt_version: None,
                        now,
                    },
                )
                .await
                .map_err(write_error)?;
                if &written_id != relation_id {
                    return Err(WenlanError::Conflict(
                        "repair_relation_identity_mismatch".into(),
                    ));
                }
                effects.graph_updates.extend(generations);
                if !existed {
                    effects.vocabulary_incremented =
                        Self::increment_relation_type_count_on_connection(
                            conn,
                            canonical_relation_type,
                        )
                        .await
                        .map_err(write_error)?
                            > 0;
                }
                for old_id in retire_relation_ids {
                    let (archived, generations) = Self::retire_relation_on_connection(conn, old_id)
                        .await
                        .map_err(write_error)?;
                    let archived = archived.ok_or_else(|| {
                        WenlanError::Conflict("repair_relation_set_changed".into())
                    })?;
                    let old_type = archived.get("relation_type").cloned().unwrap_or_default();
                    effects.activity.push(RelationActivity {
                        action: "relation_supersede_auto", memory_ids: vec![relation_id.clone(), old_id.clone()],
                        detail: serde_json::json!({ "existing_id": old_id, "new_id": relation_id, "from": from_entity, "to": to_entity, "old_type": old_type, "new_type": requested_relation_type, "archived": archived }).to_string(),
                    });
                    effects.graph_updates.extend(generations);
                }
                effects.activity.push(RelationActivity {
                    action: "relation_create",
                    memory_ids: vec![relation_id.clone()],
                    detail: format!(
                        "from={from_entity}, to={to_entity}, type={requested_relation_type}"
                    ),
                });
            }
            RepairRelationMutation::Retire => {
                let (archived, generations) =
                    Self::retire_relation_on_connection(conn, relation_id)
                        .await
                        .map_err(write_error)?;
                let archived =
                    archived.ok_or_else(|| WenlanError::Conflict("repair_target_stale".into()))?;
                effects.graph_updates.extend(generations);
                effects.activity.push(RelationActivity { action: "relation_retire", memory_ids: vec![relation_id.clone()], detail: serde_json::json!({"archived": archived, "repair_manifest_id": manifest.manifest_id()}).to_string() });
            }
        }
        for entry in &effects.activity {
            let affected = Self::log_agent_activity_on_connection(
                conn,
                "source-repair",
                entry.action,
                &entry.memory_ids,
                Some(manifest.manifest_id()),
                &entry.detail,
                now,
            )
            .await
            .map_err(write_error)?;
            if affected != 1 {
                return Err(WenlanError::VectorDb(
                    "repair_relation_activity_unproven".into(),
                ));
            }
        }
        Ok(effects)
    }
}

fn write_error(error: libsql::Error) -> WenlanError {
    WenlanError::VectorDb(format!("repair relation write: {error}"))
}
