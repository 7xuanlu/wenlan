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

fn status_is_terminal(s: &str) -> bool {
    matches!(s, "dismissed" | "auto_applied" | "resolved")
}

/// Resolve a refinement queue proposal. Convergent capability fn used by both the
/// daemon scheduler (refinery phase) and the HTTP reject route. Wraps
/// `db.resolve_refinement` with idempotency pre-check + agent activity logging.
pub async fn resolve_proposal(
    db: &MemoryDB,
    id: &str,
    status: ResolveStatus,
    agent: &str,
) -> Result<WriteResult, OriginError> {
    let proposal = db
        .get_refinement_proposal(id)
        .await?
        .ok_or_else(|| OriginError::NotFound(format!("refinement proposal {id} not found")))?;

    if status_is_terminal(&proposal.status) {
        return Err(OriginError::Validation(format!(
            "refinement proposal {id} already resolved (status={})",
            proposal.status
        )));
    }

    db.resolve_refinement(id, status.as_str()).await?;

    let payload = serde_json::json!({
        "action": proposal.action,
        "new_status": status.as_str(),
        "source_ids": proposal.source_ids,
    })
    .to_string();
    let _ = db
        .log_agent_activity(agent, "refinement_resolve", &[], None, &payload)
        .await;

    Ok(WriteResult {
        id: id.to_string(),
        warnings: Vec::new(),
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
}
