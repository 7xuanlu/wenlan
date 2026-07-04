// SPDX-License-Identifier: Apache-2.0
//! Emergence phase: orphan memory → page assignment + periodic global review.

use crate::db::MemoryDB;
use crate::error::WenlanError;
use crate::llm_provider::{LlmProvider, LlmRequest};
use crate::prompts::PromptRegistry;
use std::sync::Arc;
use wenlan_types::requests::UpdatePageRequest;

/// Layer 2: LLM assigns orphan memories to existing concepts or proposes new ones.
pub(crate) async fn assign_orphan_memories(
    db: &MemoryDB,
    llm: &Arc<dyn LlmProvider>,
    prompts: &PromptRegistry,
    _tuning: &crate::tuning::DistillationConfig,
    knowledge_path: Option<&std::path::Path>,
) -> Result<usize, WenlanError> {
    // Find orphan memories: no entity_id, not already in a page, not recap/merged
    let orphans = db.get_unlinked_memories(30).await?;
    // Filter out merged memories
    let orphans: Vec<(String, String)> = orphans
        .into_iter()
        .filter(|(sid, _)| !sid.starts_with("merged_"))
        .collect();

    if orphans.is_empty() {
        return Ok(0);
    }

    // Get existing page titles
    let concepts = db.list_pages("active", 100, 0).await?;
    if concepts.is_empty() && orphans.len() < 3 {
        return Ok(0); // Not enough material
    }

    // Build prompt
    let memories_text: String = orphans
        .iter()
        .enumerate()
        .map(|(i, (_, c))| format!("{}. {}", i, c.chars().take(200).collect::<String>()))
        .collect::<Vec<_>>()
        .join("\n");

    let concepts_text: String = concepts
        .iter()
        .map(|c| {
            format!(
                "[{}] {}: {}",
                c.id,
                c.title,
                c.summary.as_deref().unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Surface labels that 2+ other pages link to but no page is named for —
    // those are first-class topic candidates the LLM should consider when
    // proposing new pages from orphan memories. The orphan-by-count signal
    // is the "the rest of the wiki is asking for this page" feed; feeding
    // it here closes the loop instead of leaving it as a queryable metric
    // nothing consumes. Best-effort: a failure logs and falls through.
    let orphan_labels = db.list_orphan_link_labels(2).await.unwrap_or_default();
    let orphan_hint = if orphan_labels.is_empty() {
        String::new()
    } else {
        let formatted = orphan_labels
            .iter()
            .map(|(label, count)| format!("[[{label}]] ({count} other pages)"))
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "\n\nTopics other pages already link to but aren't pages yet — \
             promote a memory cluster into a new page when the orphan memories \
             match one of these labels:\n{formatted}"
        )
    };

    let user_prompt = format!(
        "Unassigned memories:\n{}\n\nExisting concepts:\n{}{}",
        memories_text, concepts_text, orphan_hint
    );

    let response = llm
        .generate(LlmRequest {
            system_prompt: Some(prompts.assign_orphans.clone()),
            user_prompt,
            max_tokens: 1024,
            temperature: 0.3,
            label: Some("orphan_assign".into()),
            timeout_secs: None,
        })
        .await
        .map_err(|e| WenlanError::Llm(format!("orphan assignment: {}", e)))?;

    let clean = crate::llm_provider::strip_think_tags(&response);

    // Create the writer once, outside the loop
    let knowledge_writer =
        knowledge_path.map(|kp| crate::export::knowledge::KnowledgeWriter::new(kp.to_path_buf()));

    // Parse assignments
    let mut assigned = 0usize;
    if let Some(json_str) = crate::llm_provider::extract_json(&clean) {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
            // Process assignments to existing concepts
            if let Some(assignments) = parsed.get("assignments").and_then(|a| a.as_array()) {
                for assignment in assignments {
                    let idx = assignment
                        .get("memory_index")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(999) as usize;
                    let page_id = assignment
                        .get("page_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if idx < orphans.len() && !page_id.is_empty() {
                        let source_id = &orphans[idx].0;
                        // Add this memory to the page's source list
                        if let Ok(Some(page)) = db.get_page(page_id).await {
                            if !page.source_memory_ids.contains(&source_id.to_string()) {
                                let mut merged_sources = page.source_memory_ids.clone();
                                merged_sources.push(source_id.to_string());
                                let _ = crate::post_write::update_page(
                                    db,
                                    page_id,
                                    UpdatePageRequest {
                                        content: page.content.clone(),
                                        source_memory_ids: merged_sources,
                                    },
                                    "page_growth",
                                    false,
                                    knowledge_path,
                                    None,
                                )
                                .await;
                                assigned += 1;
                            }
                        }
                    }
                }
            }

            // Process proposals (new concepts from orphan groups)
            if let Some(proposals) = parsed.get("proposals").and_then(|a| a.as_array()) {
                for proposal in proposals {
                    let title = proposal.get("title").and_then(|v| v.as_str()).unwrap_or("");
                    let indices = proposal
                        .get("memory_indices")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_u64().map(|n| n as usize))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();

                    if title.is_empty() || indices.len() < 2 {
                        continue;
                    }

                    let valid_indices: Vec<usize> =
                        indices.into_iter().filter(|&i| i < orphans.len()).collect();
                    if valid_indices.len() < 2 {
                        continue;
                    }

                    // Create a new page from these orphan memories
                    let source_ids: Vec<&str> = valid_indices
                        .iter()
                        .map(|&i| orphans[i].0.as_str())
                        .collect();
                    let contents: Vec<String> = valid_indices
                        .iter()
                        .map(|&i| orphans[i].1.clone())
                        .collect();
                    let content_text = contents.join("\n\n");

                    let page_id = crate::pages::new_page_id();
                    let now = chrono::Utc::now().to_rfc3339();

                    let _ = db
                        .insert_page(
                            &page_id,
                            title,
                            Some(&format!(
                                "Auto-grouped from {} orphan memories",
                                source_ids.len()
                            )),
                            &content_text,
                            None, // no entity_id
                            None, // no domain
                            &source_ids,
                            &now,
                        )
                        .await;
                    assigned += source_ids.len();

                    if let Some(ref writer) = knowledge_writer {
                        if let Ok(Some(c)) = db.get_page(&page_id).await {
                            match writer.write_page(&c) {
                                Ok(p) => log::info!("[distill] wrote page to {p}"),
                                Err(e) => log::warn!("[distill] knowledge write failed: {e}"),
                            }
                        }
                    }
                }
            }
        }
    }

    if assigned > 0 {
        log::info!(
            "[distill] orphan assignment: {} memories processed",
            assigned
        );
    }
    Ok(assigned)
}

/// Layer 3: Periodic global review -- merge/split/create concepts based on holistic analysis.
pub(crate) async fn global_page_review(
    db: &MemoryDB,
    llm: &Arc<dyn LlmProvider>,
    prompts: &PromptRegistry,
    concepts: &[crate::pages::Page],
    knowledge_path: Option<&std::path::Path>,
) -> Result<usize, WenlanError> {
    let concepts_text: String = concepts
        .iter()
        .map(|c| {
            format!(
                "[{}] {}: {}",
                c.id,
                c.title,
                c.summary.as_deref().unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let response = llm
        .generate(LlmRequest {
            system_prompt: Some(prompts.global_page_review.clone()),
            user_prompt: concepts_text,
            max_tokens: 1024,
            temperature: 0.3,
            label: Some("global_review".into()),
            timeout_secs: None,
        })
        .await
        .map_err(|e| WenlanError::Llm(format!("global review: {}", e)))?;

    let clean = crate::llm_provider::strip_think_tags(&response);
    let mut changes = 0usize;

    if let Some(json_str) = crate::llm_provider::extract_json(&clean) {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
            // Process merges
            if let Some(merges) = parsed.get("merges").and_then(|a| a.as_array()) {
                for merge in merges {
                    let keep_id = merge.get("keep").and_then(|v| v.as_str()).unwrap_or("");
                    let remove_id = merge.get("remove").and_then(|v| v.as_str()).unwrap_or("");
                    if keep_id.is_empty() || remove_id.is_empty() {
                        continue;
                    }

                    // Merge: transfer source_memory_ids from remove to keep, archive remove
                    if let (Ok(Some(keep)), Ok(Some(remove))) =
                        (db.get_page(keep_id).await, db.get_page(remove_id).await)
                    {
                        // Skip the merge if either side is user-edited. Archiving a
                        // user-edited remove page would discard authored prose; merging
                        // into a user-edited keep would orphan the watcher's view.
                        if keep.user_edited || remove.user_edited {
                            log::info!("[merge] skipping {}→{}: user_edited", remove_id, keep_id);
                            continue;
                        }

                        let mut merged_sources = keep.source_memory_ids.clone();
                        for sid in &remove.source_memory_ids {
                            if !merged_sources.contains(sid) {
                                merged_sources.push(sid.clone());
                            }
                        }

                        let result = crate::post_write::update_page(
                            db,
                            keep_id,
                            UpdatePageRequest {
                                content: keep.content.clone(),
                                source_memory_ids: merged_sources,
                            },
                            "refinery_merge",
                            true,
                            knowledge_path,
                            None,
                        )
                        .await;

                        match result {
                            Ok(crate::post_write::WriteResult { wrote: true, .. }) => {
                                let _ = db.archive_page(remove_id).await;
                                changes += 1;
                                log::info!(
                                    "[distill] merged page '{}' into '{}'",
                                    remove.title,
                                    keep.title
                                );
                            }
                            Ok(crate::post_write::WriteResult { wrote: false, .. }) => {
                                // CAS lost the race: fs_edit landed between pre-check and SQL UPDATE.
                                log::info!(
                                    "[merge] CAS skipped {}→{}: late fs_edit",
                                    remove_id,
                                    keep_id
                                );
                            }
                            Err(e) => log::warn!("[merge] update_page failed for {keep_id}: {e}"),
                        }
                    }
                }
            }
            // Note: splits and missing concepts logged but not auto-applied (too risky)
            if let Some(splits) = parsed.get("splits").and_then(|a| a.as_array()) {
                for split in splits {
                    let cid = split.get("page_id").and_then(|v| v.as_str()).unwrap_or("");
                    let titles = split.get("sub_titles").and_then(|v| v.as_array());
                    if !cid.is_empty() {
                        log::info!(
                            "[distill] global review suggests splitting page {}: {:?}",
                            cid,
                            titles
                        );
                    }
                }
            }
        }
    }

    Ok(changes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm_provider::MockProvider;
    use std::sync::Arc;

    #[tokio::test]
    async fn global_page_review_skips_merge_when_keep_is_user_edited() {
        let (db, _tmp) = crate::db::tests::test_db().await;
        let now = chrono::Utc::now().to_rfc3339();
        let now_ts = chrono::Utc::now().timestamp();

        // Insert two seed memory rows directly.
        {
            let conn = db.conn.lock().await;
            conn.execute(
                "INSERT INTO memories (id, source_id, title, content, chunk_index, chunk_type, memory_type, space, source_agent, created_at, last_modified, confirmed, stability, source) \
                 VALUES (?1, ?1, 'seed', 'fact', 0, 'text', 'fact', 'test', 'claude-code', ?2, ?2, 1, 'confirmed', 'memory')",
                libsql::params!["mem_1".to_string(), now_ts],
            )
            .await
            .unwrap();
            conn.execute(
                "INSERT INTO memories (id, source_id, title, content, chunk_index, chunk_type, memory_type, space, source_agent, created_at, last_modified, confirmed, stability, source) \
                 VALUES (?1, ?1, 'seed two', 'fact', 0, 'text', 'fact', 'test', 'claude-code', ?2, ?2, 1, 'confirmed', 'memory')",
                libsql::params!["mem_2".to_string(), now_ts],
            )
            .await
            .unwrap();
        }

        db.insert_page(
            "page_keep",
            "Keep",
            None,
            "keep body",
            None,
            None,
            &["mem_1"],
            &now,
        )
        .await
        .unwrap();
        db.insert_page(
            "page_remove",
            "Remove",
            None,
            "remove body",
            None,
            None,
            &["mem_2"],
            &now,
        )
        .await
        .unwrap();

        // Lock the keep page via fs_edit (sets user_edited=1).
        db.try_update_page_content_with_changelog(
            "page_keep",
            "user prose",
            &["mem_1"],
            "fs_edit",
            false,
            "user-edited",
            None,
        )
        .await
        .unwrap();

        let llm: Arc<dyn LlmProvider> = Arc::new(MockProvider::new(
            r#"{"merges": [{"keep": "page_keep", "remove": "page_remove"}], "splits": [], "missing": []}"#,
        ));
        let prompts = PromptRegistry::default();
        let active = db.list_pages("active", 100, 0).await.unwrap();

        let changes = global_page_review(&db, &llm, &prompts, &active, None)
            .await
            .unwrap();

        assert_eq!(
            changes, 0,
            "merge should be skipped when keep is user_edited"
        );
        let remove = db.get_page("page_remove").await.unwrap().unwrap();
        assert_eq!(remove.status, "active", "remove page must NOT be archived");
    }
}
