// SPDX-License-Identifier: Apache-2.0
//! Refinement queue phase: drain pending memories, apply merge-by-tier logic.

use crate::contradiction::ContradictionResult;
use crate::db::MemoryDB;
use crate::error::OriginError;
use crate::llm_provider::{LlmProvider, LlmRequest};
use crate::prompts::PromptRegistry;
use crate::synthesis::distill::apply_merge_by_tier;
use std::sync::Arc;

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
                db.resolve_refinement(&proposal.id, "dismissed").await?;
                processed += 1;
            }
            "detect_contradiction" => {
                if let Some(llm) = llm {
                    let contents = db.get_memory_contents(&proposal.source_ids).await?;
                    if contents.len() < 2 {
                        db.resolve_refinement(&proposal.id, "dismissed").await?;
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
                                db.resolve_refinement(&proposal.id, "dismissed").await?;
                            }
                            ContradictionResult::Contradicts { explanation } => {
                                log::info!("[refinery] contradiction detected: {}", explanation);
                                db.resolve_refinement(&proposal.id, "awaiting_review")
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
                db.resolve_refinement(&proposal.id, "awaiting_review")
                    .await?;
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
