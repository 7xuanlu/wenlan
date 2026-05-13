// SPDX-License-Identifier: Apache-2.0
//! Refinement queue phase: drain pending memories, apply merge-by-tier logic.

use crate::contradiction::ContradictionResult;
use crate::db::MemoryDB;
use crate::error::OriginError;
use crate::llm_provider::{LlmProvider, LlmRequest};
use crate::post_write::WriteResult;
use crate::prompts::PromptRegistry;
use crate::synthesis::distill::apply_merge_by_tier;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveStatus {
    Dismissed,
    AwaitingReview,
    AutoApplied,
    Resolved,
}

impl ResolveStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ResolveStatus::Dismissed => "dismissed",
            ResolveStatus::AwaitingReview => "awaiting_review",
            ResolveStatus::AutoApplied => "auto_applied",
            ResolveStatus::Resolved => "resolved",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            ResolveStatus::Dismissed | ResolveStatus::AutoApplied | ResolveStatus::Resolved
        )
    }
}

/// Resolve a refinement queue proposal. Convergent capability fn used by both the
/// daemon scheduler (refinery phase) and the HTTP reject route. Wraps
/// `db.resolve_refinement_if_open` with idempotency semantics + agent activity logging.
pub async fn resolve_proposal(
    db: &MemoryDB,
    id: &str,
    status: ResolveStatus,
    agent: &str,
) -> Result<WriteResult, OriginError> {
    let rows = db.resolve_refinement_if_open(id, status.as_str()).await?;

    if rows == 0 {
        // Distinguish 404 (missing) vs 422 (already terminal) via a follow-up lookup.
        return match db.get_refinement_proposal(id).await? {
            None => Err(OriginError::NotFound(format!(
                "refinement proposal {id} not found"
            ))),
            Some(p) => Err(OriginError::Validation(format!(
                "refinement proposal {id} already resolved (status={})",
                p.status
            ))),
        };
    }

    // Read back for activity payload — proposal still exists (we just resolved it).
    if let Ok(Some(prop)) = db.get_refinement_proposal(id).await {
        let payload = serde_json::json!({
            "action": prop.action,
            "new_status": status.as_str(),
            "source_ids": prop.source_ids,
        })
        .to_string();
        let _ = db
            .log_agent_activity(agent, "refinement_resolve", &[], None, &payload)
            .await;
    }

    Ok(WriteResult {
        id: id.to_string(),
        warnings: Vec::new(),
        wrote: true,
    })
}

/// Outcome of `apply_refinement`. Returned by the HTTP route and MCP tool.
#[derive(Debug)]
pub struct AcceptOutcome {
    pub id: String,
    pub action_applied: String,
}

/// Apply a refinement queue proposal using sensible defaults per action variant.
/// Single dispatch point for both daemon-internal accept paths (none today; reserved)
/// and agent HTTP triggers. Wraps PR1's idempotent primitives + atomic `resolve_proposal`.
///
/// Defaults:
/// - `entity_merge`: existing entity (source_ids[1]) wins as canonical, new (source_ids[0]) folds in as alias.
/// - `relation_conflict`: new relation (source_ids[0]) wins, existing (source_ids[1]) deleted.
/// - `detect_contradiction`: previously-stored memory (source_ids[1]) flagged pending_revision=1.
/// - `suggest_entity`, `dedup_merge`, unknown: returns Validation (422).
pub async fn apply_refinement(
    db: &MemoryDB,
    id: &str,
    agent: &str,
) -> Result<AcceptOutcome, OriginError> {
    let prop = db
        .get_refinement_proposal(id)
        .await?
        .ok_or_else(|| OriginError::NotFound(format!("refinement proposal {id} not found")))?;

    // Mirror the terminal-status set from resolve_refinement_if_open SQL.
    if matches!(
        prop.status.as_str(),
        "dismissed" | "auto_applied" | "resolved"
    ) {
        return Err(OriginError::Validation(format!(
            "refinement proposal {id} already resolved (status={})",
            prop.status
        )));
    }

    match prop.action.as_str() {
        "entity_merge" => {
            let new_id = prop.source_ids.first().ok_or_else(|| {
                OriginError::Validation("entity_merge missing source_ids[0]".into())
            })?;
            let existing_id = prop.source_ids.get(1).ok_or_else(|| {
                OriginError::Validation("entity_merge missing source_ids[1]".into())
            })?;
            db.merge_entities(existing_id, new_id).await?;
        }
        "relation_conflict" => {
            let new_id = prop.source_ids.first().ok_or_else(|| {
                OriginError::Validation("relation_conflict missing source_ids[0]".into())
            })?;
            let existing_id = prop.source_ids.get(1).ok_or_else(|| {
                OriginError::Validation("relation_conflict missing source_ids[1]".into())
            })?;
            db.supersede_relation(existing_id, new_id).await?;
        }
        "detect_contradiction" => {
            let existing_mem = prop.source_ids.get(1).ok_or_else(|| {
                OriginError::Validation("detect_contradiction missing source_ids[1]".into())
            })?;
            db.flag_memory_for_revision(existing_mem).await?;
        }
        "suggest_entity" => {
            return Err(OriginError::Validation(
                "action 'suggest_entity' has no accept path (reserved for future producer)".into(),
            ));
        }
        "dedup_merge" => {
            return Err(OriginError::Validation(
                "action 'dedup_merge' has no accept path (deprecated stale-v1 variant)".into(),
            ));
        }
        other => {
            return Err(OriginError::Validation(format!("unknown action: {other}")));
        }
    }

    resolve_proposal(db, id, ResolveStatus::Resolved, agent).await?;

    let payload = serde_json::json!({
        "action": prop.action,
        "source_ids": prop.source_ids,
    })
    .to_string();
    let _ = db
        .log_agent_activity(agent, "refinement_apply", &prop.source_ids, None, &payload)
        .await;

    Ok(AcceptOutcome {
        id: id.to_string(),
        action_applied: prop.action,
    })
}

/// Process pending refinement queue items via LLM.
pub(crate) async fn process_refinement_queue(
    db: &MemoryDB,
    llm: Option<&Arc<dyn LlmProvider>>,
    prompts: &PromptRegistry,
    tuning: &crate::tuning::RefineryConfig,
) -> Result<usize, OriginError> {
    let pending = db.get_pending_refinements().await?;
    let mut processed = 0usize;

    for proposal in pending.iter().take(tuning.max_proposals_per_steep) {
        match proposal.action.as_str() {
            "dedup_merge" => {
                // Stale v1 proposal — dismiss (distillation handles merges now)
                resolve_proposal(db, &proposal.id, ResolveStatus::Dismissed, "daemon").await?;
                processed += 1;
            }
            "detect_contradiction" => {
                if let Some(llm) = llm {
                    let contents = db.get_memory_contents(&proposal.source_ids).await?;
                    if contents.len() < 2 {
                        resolve_proposal(db, &proposal.id, ResolveStatus::Dismissed, "daemon")
                            .await?;
                        continue;
                    }

                    let existing_content = contents.get(1).cloned().unwrap_or_default();
                    let new_content = contents.first().cloned().unwrap_or_default();

                    let response = llm
                        .generate(LlmRequest {
                            system_prompt: Some(prompts.detect_contradiction.clone()),
                            user_prompt: format!(
                                "Existing: {}\nNew: {}",
                                existing_content, new_content
                            ),
                            max_tokens: 256,
                            temperature: 0.1,
                            label: None,
                            timeout_secs: None,
                        })
                        .await;

                    if let Ok(r) = response {
                        let r = crate::llm_provider::strip_think_tags(&r);
                        let r = r.trim().to_string();
                        let result = if r.starts_with("CONTRADICTS:") {
                            ContradictionResult::Contradicts {
                                explanation: r
                                    .strip_prefix("CONTRADICTS:")
                                    .unwrap_or("")
                                    .trim()
                                    .to_string(),
                            }
                        } else if r.starts_with("SUPERSEDES:") {
                            ContradictionResult::Supersedes {
                                merged_content: r
                                    .strip_prefix("SUPERSEDES:")
                                    .unwrap_or("")
                                    .trim()
                                    .to_string(),
                            }
                        } else {
                            ContradictionResult::Consistent
                        };

                        match result {
                            ContradictionResult::Consistent => {
                                resolve_proposal(
                                    db,
                                    &proposal.id,
                                    ResolveStatus::Dismissed,
                                    "daemon",
                                )
                                .await?;
                            }
                            ContradictionResult::Contradicts { explanation } => {
                                log::info!("[refinery] contradiction detected: {}", explanation);
                                resolve_proposal(
                                    db,
                                    &proposal.id,
                                    ResolveStatus::AwaitingReview,
                                    "daemon",
                                )
                                .await?;
                            }
                            ContradictionResult::Supersedes { merged_content } => {
                                let tier = db.get_highest_tier(&proposal.source_ids).await?;
                                apply_merge_by_tier(
                                    db,
                                    &proposal.source_ids,
                                    &merged_content,
                                    &proposal.id,
                                    &tier,
                                )
                                .await?;
                            }
                        }
                        processed += 1;
                    }
                }
            }
            "suggest_entity" => {
                // Entity suggestion: payload contains the suggested entity name.
                // Mark as awaiting_review so the UI can surface it for approval.
                resolve_proposal(db, &proposal.id, ResolveStatus::AwaitingReview, "daemon").await?;
                log::info!(
                    "[refinery] entity suggestion queued for review: {:?}",
                    proposal.payload
                );
                processed += 1;
            }
            _ => {
                log::debug!("[refinery] unknown action: {}", proposal.action);
            }
        }
    }
    Ok(processed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::MemoryDB;
    use crate::events::NoopEmitter;
    use std::sync::Arc;
    use tempfile::TempDir;

    async fn test_db() -> (MemoryDB, TempDir) {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let db = MemoryDB::new(&db_path, Arc::new(NoopEmitter))
            .await
            .unwrap();
        (db, tmp)
    }

    #[tokio::test]
    async fn resolve_proposal_dismissed_updates_status() {
        let (db, _tmp) = test_db().await;
        db.insert_refinement_proposal(
            "ref_test_1",
            "detect_contradiction",
            &["mem_a".to_string(), "mem_b".to_string()],
            None,
            0.8,
        )
        .await
        .unwrap();

        let result = resolve_proposal(&db, "ref_test_1", ResolveStatus::Dismissed, "test-agent")
            .await
            .unwrap();
        assert_eq!(result.id, "ref_test_1");
        assert!(result.warnings.is_empty());

        let pending = db.get_pending_refinements().await.unwrap();
        assert!(
            !pending.iter().any(|p| p.id == "ref_test_1"),
            "dismissed proposal should not appear in pending list"
        );
    }

    #[tokio::test]
    async fn resolve_proposal_not_found_returns_not_found_error() {
        let (db, _tmp) = test_db().await;
        let err = resolve_proposal(
            &db,
            "ref_nonexistent",
            ResolveStatus::Dismissed,
            "test-agent",
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, crate::error::OriginError::NotFound(_)),
            "expected NotFound, got {err:?}"
        );
    }

    #[tokio::test]
    async fn resolve_proposal_already_terminal_returns_validation_error() {
        let (db, _tmp) = test_db().await;
        db.insert_refinement_proposal(
            "ref_test_2",
            "entity_merge",
            &["e1".to_string(), "e2".to_string()],
            None,
            0.87,
        )
        .await
        .unwrap();
        resolve_proposal(&db, "ref_test_2", ResolveStatus::Dismissed, "test-agent")
            .await
            .unwrap();

        let err = resolve_proposal(&db, "ref_test_2", ResolveStatus::Dismissed, "test-agent")
            .await
            .unwrap_err();
        assert!(
            matches!(err, crate::error::OriginError::Validation(_)),
            "expected Validation, got {err:?}"
        );
    }

    #[tokio::test]
    async fn daemon_dismiss_logs_activity_via_convergence() {
        let (db, _tmp) = test_db().await;
        db.insert_refinement_proposal(
            "ref_daemon_1",
            "dedup_merge",
            &["a".to_string(), "b".to_string()],
            None,
            0.5,
        )
        .await
        .unwrap();

        resolve_proposal(&db, "ref_daemon_1", ResolveStatus::Dismissed, "daemon")
            .await
            .unwrap();

        let acts = db
            .list_agent_activity(10, Some("daemon"), None)
            .await
            .unwrap_or_default();
        assert!(
            acts.iter().any(|a| a.action == "refinement_resolve"),
            "daemon path should log refinement_resolve via convergence"
        );
    }

    #[tokio::test]
    async fn resolve_proposal_logs_activity() {
        let (db, _tmp) = test_db().await;
        db.insert_refinement_proposal(
            "ref_test_3",
            "suggest_entity",
            &["mem_x".to_string()],
            Some("{\"name\":\"Acme\"}"),
            0.9,
        )
        .await
        .unwrap();

        resolve_proposal(&db, "ref_test_3", ResolveStatus::Dismissed, "test-agent")
            .await
            .unwrap();

        let acts = db
            .list_agent_activity(10, Some("test-agent"), None)
            .await
            .unwrap_or_default();
        assert!(
            acts.iter().any(|a| a.action == "refinement_resolve"),
            "expected refinement_resolve activity row, found: {acts:?}"
        );
    }

    #[tokio::test]
    async fn resolve_proposal_concurrent_rejects_logs_once() {
        let (db, _tmp) = test_db().await;
        db.insert_refinement_proposal(
            "ref_race_1",
            "entity_merge",
            &["a".into(), "b".into()],
            None,
            0.85,
        )
        .await
        .unwrap();

        let db_arc = std::sync::Arc::new(db);
        let h1 = {
            let d = db_arc.clone();
            tokio::spawn(async move {
                resolve_proposal(&d, "ref_race_1", ResolveStatus::Dismissed, "client-a").await
            })
        };
        let h2 = {
            let d = db_arc.clone();
            tokio::spawn(async move {
                resolve_proposal(&d, "ref_race_1", ResolveStatus::Dismissed, "client-b").await
            })
        };
        let r1 = h1.await.unwrap();
        let r2 = h2.await.unwrap();

        let ok_count = [&r1, &r2].iter().filter(|r| r.is_ok()).count();
        let err_count = [&r1, &r2].iter().filter(|r| r.is_err()).count();
        assert_eq!(
            ok_count, 1,
            "exactly one resolve should succeed, got {r1:?} / {r2:?}"
        );
        assert_eq!(err_count, 1, "exactly one should fail with Validation");
        let err = if let Err(e) = r1 { e } else { r2.unwrap_err() };
        assert!(
            matches!(err, crate::error::OriginError::Validation(_)),
            "concurrent loser should see Validation, got {err:?}"
        );

        let acts = db_arc
            .list_agent_activity(10, None, None)
            .await
            .unwrap_or_default();
        let resolve_count = acts
            .iter()
            .filter(|a| a.action == "refinement_resolve")
            .count();
        assert_eq!(
            resolve_count, 1,
            "activity log should record exactly one resolve, got {resolve_count}"
        );
    }

    // ==================== apply_refinement ====================

    async fn seed_entity_merge_proposal(db: &MemoryDB, id: &str) -> (String, String) {
        let new_ent = db
            .create_entity("Acme Corporation", "organization", None)
            .await
            .unwrap();
        let existing_ent = db
            .create_entity("Acme Corp", "organization", None)
            .await
            .unwrap();
        db.insert_refinement_proposal(
            id,
            "entity_merge",
            &[new_ent.clone(), existing_ent.clone()],
            None,
            0.87,
        )
        .await
        .unwrap();
        {
            let conn = db.conn.lock().await;
            conn.execute(
                "UPDATE refinement_queue SET status = 'awaiting_review' WHERE id = ?1",
                libsql::params![id],
            )
            .await
            .unwrap();
        }
        (new_ent, existing_ent)
    }

    #[tokio::test]
    async fn apply_refinement_entity_merge_default_existing_wins() {
        let (db, _tmp) = test_db().await;
        let (new_ent, existing_ent) = seed_entity_merge_proposal(&db, "ref_em_1").await;

        let outcome = apply_refinement(&db, "ref_em_1", "test-agent")
            .await
            .unwrap();
        assert_eq!(outcome.action_applied, "entity_merge");

        let conn = db.conn.lock().await;
        let mut rows = conn
            .query(
                "SELECT COUNT(*) FROM entities WHERE id = ?1",
                libsql::params![existing_ent],
            )
            .await
            .unwrap();
        let count: i64 = rows.next().await.unwrap().unwrap().get(0).unwrap();
        assert_eq!(count, 1, "existing entity should remain canonical");

        let mut rows = conn
            .query(
                "SELECT COUNT(*) FROM entities WHERE id = ?1",
                libsql::params![new_ent],
            )
            .await
            .unwrap();
        let count: i64 = rows.next().await.unwrap().unwrap().get(0).unwrap();
        assert_eq!(count, 0, "new entity should be merged away");
    }

    #[tokio::test]
    async fn apply_refinement_relation_conflict_default_new_wins() {
        let (db, _tmp) = test_db().await;
        let from = db.create_entity("Alice", "person", None).await.unwrap();
        let to = db
            .create_entity("Acme", "organization", None)
            .await
            .unwrap();
        let existing_rel = db
            .create_relation(&from, &to, "works_at", None, Some(0.5), None, None)
            .await
            .unwrap();
        let new_rel = db
            .create_relation(&from, &to, "leads", None, Some(0.9), None, None)
            .await
            .unwrap();
        db.insert_refinement_proposal(
            "ref_rc_1",
            "relation_conflict",
            &[new_rel.clone(), existing_rel.clone()],
            None,
            0.7,
        )
        .await
        .unwrap();
        {
            let conn = db.conn.lock().await;
            conn.execute(
                "UPDATE refinement_queue SET status = 'awaiting_review' WHERE id = ?1",
                libsql::params!["ref_rc_1"],
            )
            .await
            .unwrap();
        }

        let outcome = apply_refinement(&db, "ref_rc_1", "test-agent")
            .await
            .unwrap();
        assert_eq!(outcome.action_applied, "relation_conflict");

        let conn = db.conn.lock().await;
        let mut rows = conn
            .query(
                "SELECT COUNT(*) FROM relations WHERE id = ?1",
                libsql::params![new_rel],
            )
            .await
            .unwrap();
        let count: i64 = rows.next().await.unwrap().unwrap().get(0).unwrap();
        assert_eq!(count, 1, "new relation (winner) should remain");

        let mut rows = conn
            .query(
                "SELECT COUNT(*) FROM relations WHERE id = ?1",
                libsql::params![existing_rel],
            )
            .await
            .unwrap();
        let count: i64 = rows.next().await.unwrap().unwrap().get(0).unwrap();
        assert_eq!(count, 0, "existing relation (loser) should be deleted");
    }

    #[tokio::test]
    async fn apply_refinement_detect_contradiction_flags_existing() {
        let (db, _tmp) = test_db().await;
        let new_mem = format!("mem_{}", uuid::Uuid::new_v4().simple());
        let existing_mem = format!("mem_{}", uuid::Uuid::new_v4().simple());
        {
            let conn = db.conn.lock().await;
            for sid in &[new_mem.clone(), existing_mem.clone()] {
                conn.execute(
                    "INSERT INTO memories (id, content, source, source_id, title, chunk_index, \
                                            chunk_type, source_agent, domain, confidence, confirmed, \
                                            last_modified, memory_type, pending_revision) \
                     VALUES (?1, ?2, 'memory', ?3, 'test', 0, 'text', NULL, 'general', 1.0, 0, 1712707200, 'fact', 0)",
                    libsql::params![sid.clone(), "x".to_string(), sid.clone()],
                )
                .await
                .unwrap();
            }
        }
        db.insert_refinement_proposal(
            "ref_dc_1",
            "detect_contradiction",
            &[new_mem.clone(), existing_mem.clone()],
            None,
            0.8,
        )
        .await
        .unwrap();
        {
            let conn = db.conn.lock().await;
            conn.execute(
                "UPDATE refinement_queue SET status = 'awaiting_review' WHERE id = ?1",
                libsql::params!["ref_dc_1"],
            )
            .await
            .unwrap();
        }

        let outcome = apply_refinement(&db, "ref_dc_1", "test-agent")
            .await
            .unwrap();
        assert_eq!(outcome.action_applied, "detect_contradiction");

        let conn = db.conn.lock().await;
        let mut rows = conn
            .query(
                "SELECT pending_revision FROM memories WHERE source_id = ?1",
                libsql::params![existing_mem],
            )
            .await
            .unwrap();
        let f: i64 = rows.next().await.unwrap().unwrap().get(0).unwrap();
        assert_eq!(f, 1, "existing memory should be flagged for revision");

        let mut rows = conn
            .query(
                "SELECT pending_revision FROM memories WHERE source_id = ?1",
                libsql::params![new_mem],
            )
            .await
            .unwrap();
        let f: i64 = rows.next().await.unwrap().unwrap().get(0).unwrap();
        assert_eq!(f, 0, "new memory should NOT be flagged");
    }

    #[tokio::test]
    async fn apply_refinement_suggest_entity_returns_422() {
        let (db, _tmp) = test_db().await;
        db.insert_refinement_proposal(
            "ref_se_1",
            "suggest_entity",
            &["x".into()],
            Some("{\"name\":\"Acme\"}"),
            0.9,
        )
        .await
        .unwrap();
        let err = apply_refinement(&db, "ref_se_1", "test-agent")
            .await
            .unwrap_err();
        assert!(
            matches!(err, crate::error::OriginError::Validation(_)),
            "expected Validation for suggest_entity, got {err:?}"
        );
    }

    #[tokio::test]
    async fn apply_refinement_dedup_merge_returns_422() {
        let (db, _tmp) = test_db().await;
        db.insert_refinement_proposal(
            "ref_dm_1",
            "dedup_merge",
            &["a".into(), "b".into()],
            None,
            0.95,
        )
        .await
        .unwrap();
        let err = apply_refinement(&db, "ref_dm_1", "test-agent")
            .await
            .unwrap_err();
        assert!(
            matches!(err, crate::error::OriginError::Validation(_)),
            "expected Validation for dedup_merge, got {err:?}"
        );
    }

    #[tokio::test]
    async fn apply_refinement_unknown_action_returns_422() {
        let (db, _tmp) = test_db().await;
        db.insert_refinement_proposal("ref_uk_1", "future_action_xyz", &["a".into()], None, 0.5)
            .await
            .unwrap();
        let err = apply_refinement(&db, "ref_uk_1", "test-agent")
            .await
            .unwrap_err();
        assert!(
            matches!(err, crate::error::OriginError::Validation(_)),
            "expected Validation for unknown action, got {err:?}"
        );
    }

    #[tokio::test]
    async fn apply_refinement_missing_id_returns_404() {
        let (db, _tmp) = test_db().await;
        let err = apply_refinement(&db, "ref_nonexistent", "test-agent")
            .await
            .unwrap_err();
        assert!(
            matches!(err, crate::error::OriginError::NotFound(_)),
            "expected NotFound, got {err:?}"
        );
    }

    #[tokio::test]
    async fn apply_refinement_terminal_proposal_returns_422() {
        let (db, _tmp) = test_db().await;
        db.insert_refinement_proposal(
            "ref_term_1",
            "entity_merge",
            &["a".into(), "b".into()],
            None,
            0.85,
        )
        .await
        .unwrap();
        {
            let conn = db.conn.lock().await;
            conn.execute(
                "UPDATE refinement_queue SET status = 'resolved' WHERE id = ?1",
                libsql::params!["ref_term_1"],
            )
            .await
            .unwrap();
        }
        let err = apply_refinement(&db, "ref_term_1", "test-agent")
            .await
            .unwrap_err();
        assert!(
            matches!(err, crate::error::OriginError::Validation(_)),
            "expected Validation (terminal), got {err:?}"
        );
    }

    #[tokio::test]
    async fn apply_refinement_logs_both_apply_and_resolve_activity() {
        let (db, _tmp) = test_db().await;
        let _ = seed_entity_merge_proposal(&db, "ref_log_1").await;

        apply_refinement(&db, "ref_log_1", "test-agent")
            .await
            .unwrap();

        let acts = db
            .list_agent_activity(20, Some("test-agent"), None)
            .await
            .unwrap_or_default();
        let apply_count = acts
            .iter()
            .filter(|a| a.action == "refinement_apply")
            .count();
        let resolve_count = acts
            .iter()
            .filter(|a| a.action == "refinement_resolve")
            .count();
        assert_eq!(
            apply_count, 1,
            "exactly one refinement_apply row, got {apply_count}"
        );
        assert_eq!(
            resolve_count, 1,
            "exactly one refinement_resolve row, got {resolve_count}"
        );
    }

    #[tokio::test]
    async fn apply_refinement_idempotent_via_resolve_race() {
        let (db, _tmp) = test_db().await;
        let _ = seed_entity_merge_proposal(&db, "ref_race_1").await;

        apply_refinement(&db, "ref_race_1", "client-a")
            .await
            .unwrap();
        let err = apply_refinement(&db, "ref_race_1", "client-b")
            .await
            .unwrap_err();
        assert!(
            matches!(err, crate::error::OriginError::Validation(_)),
            "second accept should 422, got {err:?}"
        );
    }
}
