// SPDX-License-Identifier: Apache-2.0
//! Post-ingest enrichment pipeline.
//!
//! Runs asynchronously after `store_memory` returns. Each step is
//! best-effort: failures are logged but do not block the store response
//! or subsequent steps.
//!
//! Steps:
//! 1. Entity auto-linking (vector search entities > 0.85 distance → set entity_id)
//!    1b. Store-time entity extraction (LLM extract if auto-link found no match)
//! 2. Entity creation suggestion (stub — full impl in refinery Task 5)
//! 3. Title enrichment (LLM short title if current looks truncated)
//! 4. (Removed — recaps now handled by event-driven scheduler)
//! 5. Concept growth (update matching page with new memory)
//! 6. (Removed -- enrichment status derived from per-step outcomes in enrichment_steps table)
//!
//! Removed steps (dedup_merge and detect_contradiction proposals):
//! - Dedup check: emitted dedup_merge proposals; accept path is deprecated stale-v1.
//!   Distillation handles dedup. Removed in post-PR #109 cleanup.
//! - Contradiction check: emitted detect_contradiction proposals; accept path calls
//!   flag_memory_for_revision which does not set supersedes IS NOT NULL, so proposals
//!   never surface in /brief. The topic-match-protected path in memory_routes.rs is
//!   the only working contradiction-detection path. Removed in post-PR #109 cleanup.

use crate::db::MemoryDB;
use crate::error::WenlanError;
use crate::llm_provider::LlmProvider;
use crate::prompts::PromptRegistry;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use wenlan_types::requests::UpdatePageRequest;

/// Result of the title enrichment step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TitleEnrichResult {
    /// Title was replaced with an LLM-generated short title.
    Enriched,
    /// Title was not truncated (or memory is recap/merged) -- no action needed.
    NotNeeded,
    /// Title IS truncated but LLM output was rejected (too long, generic, etc.).
    LlmRejected,
}

/// True iff the caller has signalled cancellation. `None` (the default-OFF
/// path) is never cancelled, so the flag is fully inert unless an operator
/// opts into `WENLAN_ENABLE_REFLECTION_DEBOUNCE` and a newer same-agent write
/// supersedes this one mid-burst. Checked only BETWEEN best-effort steps so a
/// step is never half-applied (clean-boundary cancellation).
fn is_cancelled(cancel: Option<&AtomicBool>) -> bool {
    cancel.map(|c| c.load(Ordering::SeqCst)).unwrap_or(false)
}

/// Run post-ingest enrichment (async, non-blocking).
/// Called after store_memory fast track returns.
///
/// `cancel` is an opt-in cooperative-cancellation signal (T22 debounced
/// reflection). When `Some` and flipped to `true` by a newer same-agent write,
/// enrichment returns early at the next clean step boundary. `None` (the
/// default) keeps the path byte-identical to pre-T22 behaviour — every step
/// runs to completion.
#[allow(clippy::too_many_arguments)]
pub async fn run_post_ingest_enrichment(
    db: &MemoryDB,
    source_id: &str,
    content: &str,
    entity_id: Option<&str>,
    _memory_type: Option<&str>,
    _domain: Option<&str>,
    _structured_fields: Option<&str>,
    llm: Option<&Arc<dyn LlmProvider>>,
    prompts: &PromptRegistry,
    tuning: &crate::tuning::RefineryConfig,
    distillation: &crate::tuning::DistillationConfig,
    knowledge_path: Option<&std::path::Path>,
    cancel: Option<&AtomicBool>,
    precomputed_kg: Option<Vec<crate::extract::KgExtractionResult>>,
) -> Result<(), WenlanError> {
    log::info!("[post_ingest] enriching {source_id}");

    // Checkpoint 0: bail before any work if a newer write already superseded us.
    if is_cancelled(cancel) {
        log::info!("[post_ingest] {source_id}: cancelled before first step");
        return Ok(());
    }

    // 1. Entity auto-linking (only if not already linked)
    if entity_id.is_none() {
        match auto_link_entity(db, source_id, content, tuning).await {
            Ok(true) => {
                db.record_enrichment_step(source_id, "entity_link", "ok", None)
                    .await
                    .ok();
            }
            Ok(false) => {
                db.record_enrichment_step(source_id, "entity_link", "ok", None)
                    .await
                    .ok();
            }
            Err(e) => {
                log::warn!("[post_ingest] entity linking failed: {e}");
                db.record_enrichment_step(source_id, "entity_link", "failed", Some(&e.to_string()))
                    .await
                    .ok();
            }
        }
    } else {
        db.record_enrichment_step(source_id, "entity_link", "skipped", None)
            .await
            .ok();
    }

    if is_cancelled(cancel) {
        log::info!("[post_ingest] {source_id}: cancelled after entity_link");
        return Ok(());
    }

    // 2b. Store-time entity extraction with time-windowed batching
    // Re-check entity_id since auto_link_entity may have set it
    let current_entity_id = db
        .get_memory_entity_id(source_id)
        .await
        .unwrap_or(entity_id.map(|s| s.to_string()));
    if current_entity_id.is_none() {
        if let Some(llm_ref) = llm {
            // Look up source_agent from the DB for batch window queries
            let agent = db.get_memory_source_agent(source_id).await.unwrap_or(None);

            // Check for recent memories from the same agent for batched extraction
            let batch = match &agent {
                Some(a) => db
                    .find_recent_batch(a, tuning.batch_window_secs)
                    .await
                    .unwrap_or_default(),
                None => Vec::new(),
            };

            if batch.len() > 1 {
                // Batch extraction -- combine all recent memories for richer entity/relation extraction
                let combined: String = batch
                    .iter()
                    .enumerate()
                    .map(|(i, (_, c))| {
                        format!("{}. {}", i + 1, c.chars().take(500).collect::<String>())
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                log::info!(
                    "[post_ingest] batch entity extraction: {} memories in window",
                    batch.len()
                );

                // Extract from combined content, link entities to all batch memories
                match crate::refinery::extract_single_memory_entities(
                    db, llm_ref, prompts, source_id, &combined,
                )
                .await
                {
                    Ok(Some(eid)) => {
                        // Link all batch memories to the extracted entity
                        for (batch_sid, _) in &batch {
                            if batch_sid != source_id {
                                let _ = db.update_memory_entity_id(batch_sid, &eid).await;
                            }
                        }
                        log::info!(
                            "[post_ingest] batch extraction linked {} memories to entity",
                            batch.len()
                        );
                        db.record_enrichment_step(source_id, "entity_extract", "ok", None)
                            .await
                            .ok();
                    }
                    Ok(None) => {
                        db.record_enrichment_step(source_id, "entity_extract", "ok", None)
                            .await
                            .ok();
                    }
                    Err(e) => {
                        log::warn!("[post_ingest] batch entity extraction failed: {e}");
                        db.record_enrichment_step(
                            source_id,
                            "entity_extract",
                            "failed",
                            Some(&e.to_string()),
                        )
                        .await
                        .ok();
                    }
                }
            } else {
                // Single memory extraction (no batch or source_agent unknown).
                // If a pre-computed KG was supplied by the caller, commit it directly
                // (skipping the inline LLM extract). Otherwise, run the LLM extract
                // as today. Both paths return Result<Option<String>, WenlanError>.
                match match precomputed_kg {
                    Some(kg) => crate::refinery::commit_kg(db, source_id, &kg).await,
                    None => {
                        crate::refinery::extract_single_memory_entities(
                            db, llm_ref, prompts, source_id, content,
                        )
                        .await
                    }
                } {
                    Ok(Some(eid)) => {
                        let eid_prefix: String = eid.chars().take(12).collect();
                        log::info!("[post_ingest] {source_id}: extracted entity {eid_prefix}");
                        db.record_enrichment_step(source_id, "entity_extract", "ok", None)
                            .await
                            .ok();
                    }
                    Ok(None) => {
                        db.record_enrichment_step(source_id, "entity_extract", "ok", None)
                            .await
                            .ok();
                    }
                    Err(e) => {
                        log::warn!("[post_ingest] store-time entity extraction failed: {e}");
                        db.record_enrichment_step(
                            source_id,
                            "entity_extract",
                            "failed",
                            Some(&e.to_string()),
                        )
                        .await
                        .ok();
                    }
                }
            }
        } else {
            db.record_enrichment_step(source_id, "entity_extract", "skipped", None)
                .await
                .ok();
        }
    } else {
        db.record_enrichment_step(source_id, "entity_extract", "skipped", None)
            .await
            .ok();
    }

    if is_cancelled(cancel) {
        log::info!("[post_ingest] {source_id}: cancelled after entity_extract");
        return Ok(());
    }

    // 3. Concept contradiction check — flag related concepts for re-distill if new memory contradicts
    match check_page_contradiction(db, source_id, content).await {
        Ok(n) if n > 0 => {
            log::info!("[post_ingest] {source_id}: flagged {n} page(s) for re-distill");
            db.record_enrichment_step(source_id, "page_contradiction", "ok", None)
                .await
                .ok();
        }
        Ok(_) => {
            db.record_enrichment_step(source_id, "page_contradiction", "ok", None)
                .await
                .ok();
        }
        Err(e) => {
            log::warn!("[post_ingest] page contradiction check failed: {e}");
            db.record_enrichment_step(
                source_id,
                "page_contradiction",
                "failed",
                Some(&e.to_string()),
            )
            .await
            .ok();
        }
    }

    // 4. Entity creation suggestion (stub — full extraction runs in refinery steep)
    match suggest_entity_creation(db, content).await {
        Ok(()) => {
            db.record_enrichment_step(source_id, "entity_suggestion", "ok", None)
                .await
                .ok();
        }
        Err(e) => {
            log::warn!("[post_ingest] entity suggestion failed: {e}");
            db.record_enrichment_step(
                source_id,
                "entity_suggestion",
                "failed",
                Some(&e.to_string()),
            )
            .await
            .ok();
        }
    }

    if is_cancelled(cancel) {
        log::info!("[post_ingest] {source_id}: cancelled before title_enrich");
        return Ok(());
    }

    // 5. Title enrichment — generate short topic title if current title is a truncation
    if let Some(llm_ref) = llm {
        match enrich_title(db, source_id, content, llm_ref, false).await {
            Ok(TitleEnrichResult::Enriched) => {
                log::info!("[post_ingest] {source_id}: title enriched");
                db.record_enrichment_step(source_id, "title_enrich", "ok", None)
                    .await
                    .ok();
            }
            Ok(TitleEnrichResult::NotNeeded) => {
                db.record_enrichment_step(source_id, "title_enrich", "ok", None)
                    .await
                    .ok();
            }
            Ok(TitleEnrichResult::LlmRejected) => {
                log::info!("[post_ingest] {source_id}: title LLM-rejected, queuing for retry");
                db.record_enrichment_step(
                    source_id,
                    "title_enrich",
                    "needs_retry",
                    Some("llm_rejected"),
                )
                .await
                .ok();
            }
            Err(e) => {
                log::warn!("[post_ingest] title enrichment failed: {e}");
                db.record_enrichment_step(
                    source_id,
                    "title_enrich",
                    "failed",
                    Some(&e.to_string()),
                )
                .await
                .ok();
            }
        }
    } else {
        db.record_enrichment_step(source_id, "title_enrich", "skipped", None)
            .await
            .ok();
    }

    // 6. Recap trigger — REMOVED. Recaps are now generated by the event-driven
    // scheduler (BurstEnd trigger) at the natural end of agent work sessions,
    // not on every write. generate_recaps_public remains in refinery's public
    // API for standalone core consumers. See 2026-04-12-event-driven-steep-triggers.

    if is_cancelled(cancel) {
        log::info!("[post_ingest] {source_id}: cancelled before page_growth");
        return Ok(());
    }

    // 7. Concept growth — update matching page with new memory
    match grow_page(
        db,
        source_id,
        content,
        entity_id,
        llm,
        prompts,
        distillation.page_growth_threshold,
    )
    .await
    {
        Ok(true) => {
            log::info!("[post_ingest] {source_id}: updated matching page");
            db.record_enrichment_step(source_id, "page_growth", "ok", None)
                .await
                .ok();
            if let Some(kp) = knowledge_path {
                write_grown_page(db, source_id, kp).await;
            }
        }
        Ok(false) => {
            // grow_page returns false when LLM is unavailable — treat as skipped
            if llm.map(|l| l.is_available()).unwrap_or(false) {
                db.record_enrichment_step(source_id, "page_growth", "ok", None)
                    .await
                    .ok();
            } else {
                db.record_enrichment_step(source_id, "page_growth", "skipped", None)
                    .await
                    .ok();
            }
        }
        Err(e) => {
            log::warn!("[post_ingest] page growth failed: {e}");
            db.record_enrichment_step(source_id, "page_growth", "failed", Some(&e.to_string()))
                .await
                .ok();
        }
    }

    // 7b. KG quality verification — check entity self-retrieval after all linking/extraction
    let final_entity_id = db
        .get_memory_entity_id(source_id)
        .await
        .unwrap_or(entity_id.map(|s| s.to_string()));
    if let Some(ref eid) = final_entity_id {
        if let Ok(detail) = db.get_entity_detail(eid).await {
            match crate::kg_quality::verify_entity(db, eid, &detail.entity.name).await {
                Ok(ref result) => {
                    for warning in &result.warnings {
                        log::warn!("[post_ingest] {}", warning);
                    }
                }
                Err(e) => log::warn!("[post_ingest] entity verification failed: {e}"),
            }
        }
    }

    Ok(())
}

/// Auto-link memory to an existing entity via vector search.
/// Links to the best matching entity with distance < 0.15 (cosine similarity > 0.85).
pub(crate) async fn auto_link_entity(
    db: &MemoryDB,
    source_id: &str,
    content: &str,
    tuning: &crate::tuning::RefineryConfig,
) -> Result<bool, WenlanError> {
    let entities = db.search_entities_by_vector(content, 3).await?;
    for entity in &entities {
        // distance is cosine distance — lower = more similar
        if entity.distance < tuning.entity_link_distance as f32 {
            db.update_memory_entity_id(source_id, &entity.entity.id)
                .await?;
            log::info!(
                "[post_ingest] auto-linked {} to entity '{}' (distance={:.3})",
                source_id,
                entity.entity.name,
                entity.distance,
            );
            return Ok(true);
        }
    }
    Ok(false)
}

/// Check if new memory content contradicts any related page.
/// Uses FTS5 search to find related concepts, then checks for negation signals
/// with topic overlap. Flags contradicting concepts for re-distill by adding the
/// new memory to their source list.
pub(crate) async fn check_page_contradiction(
    db: &MemoryDB,
    source_id: &str,
    content: &str,
) -> Result<usize, WenlanError> {
    // Find concepts related to this memory via FTS5 (use first 100 chars as query)
    let query: String = content
        .split_whitespace()
        .take(15)
        .collect::<Vec<_>>()
        .join(" ");
    let concepts = db.search_pages(&query, 3, None).await.unwrap_or_default();
    if concepts.is_empty() {
        return Ok(0);
    }

    let mut flagged = 0usize;
    let content_lower = content.to_lowercase();

    for page in &concepts {
        // Quick heuristic: if the memory contains negation/update signals,
        // it might contradict existing page content
        let contradiction_signals = [
            "not ",
            "no longer",
            "instead of",
            "rather than",
            "changed from",
            "replaced",
            "deprecated",
            "wrong",
            "incorrect",
            "actually ",
        ];

        let has_signal = contradiction_signals
            .iter()
            .any(|s| content_lower.contains(s));
        if !has_signal {
            continue;
        }

        // Check if memory overlaps with page topic (bigram jaccard >= 0.15)
        let overlap = crate::contradiction::bigram_jaccard(content, &page.title);
        if overlap < 0.15 {
            continue;
        }

        // This memory likely contradicts or updates the page — add it to sources and flag for re-distill
        if !page.source_memory_ids.contains(&source_id.to_string()) {
            let mut new_sources = page.source_memory_ids.clone();
            new_sources.push(source_id.to_string());
            // Update sources without changing content — re-distill will recompile
            let _ = crate::post_write::update_page(
                db,
                &page.id,
                UpdatePageRequest {
                    content: page.content.clone(),
                    source_memory_ids: new_sources,
                },
                "page_growth",
                false,
                None,
            )
            .await;
            log::info!("[post_ingest] page '{}' flagged for re-distill due to potential contradiction from {}",
                page.title, source_id);
            flagged += 1;
        }
    }

    Ok(flagged)
}

/// Stub for entity creation suggestion. Full implementation in Task 5 (refinery).
pub(crate) async fn suggest_entity_creation(
    _db: &MemoryDB,
    _content: &str,
) -> Result<(), WenlanError> {
    // TODO: Detect entity-like proper nouns in content and queue
    // 'suggest_entity' refinement action if no matching entity exists.
    Ok(())
}

/// Generate a short topic title if the current title looks like a content truncation.
///
/// By default, enrichment only fires when the title appears truncated (ends with "...",
/// matches the first content line verbatim, or is 75+ characters long). Eval benchmarks
/// use short synthetic titles that never trigger this heuristic. Set the env var
/// `EVAL_FORCE_TITLE_ENRICHMENT=1` to bypass the truncation check and always enrich.
/// Default behavior (env unset) is identical to before.
pub(crate) async fn enrich_title(
    db: &MemoryDB,
    source_id: &str,
    content: &str,
    llm: &Arc<dyn LlmProvider>,
    force: bool,
) -> Result<TitleEnrichResult, WenlanError> {
    // Skip recaps and merged memories — they get titles during generation
    if source_id.starts_with("recap_") || source_id.starts_with("merged_") {
        return Ok(TitleEnrichResult::NotNeeded);
    }

    let force_enrichment = force;

    // Check if current title is a truncation (ends with "..." or matches first line)
    let detail = db.get_memory_detail(source_id).await?;
    let current_title = match &detail {
        Some(d) => &d.title,
        None => return Ok(TitleEnrichResult::NotNeeded),
    };

    if !force_enrichment {
        let first_line = content.lines().next().unwrap_or(content);
        let is_truncated = current_title.ends_with("...")
            || current_title == first_line
            || current_title.len() >= 75;
        if !is_truncated {
            return Ok(TitleEnrichResult::NotNeeded);
        }
    }

    if let Some(short_title) = crate::refinery::generate_short_title(llm, content).await {
        db.update_title(source_id, &short_title).await?;
        Ok(TitleEnrichResult::Enriched)
    } else {
        Ok(TitleEnrichResult::LlmRejected)
    }
}

/// Check if new memory matches an existing page; if so, update it.
pub(crate) async fn grow_page(
    db: &MemoryDB,
    source_id: &str,
    content: &str,
    entity_id: Option<&str>,
    llm: Option<&Arc<dyn LlmProvider>>,
    prompts: &PromptRegistry,
    growth_threshold: f64,
) -> Result<bool, WenlanError> {
    let llm = match llm {
        Some(l) if l.is_available() => l,
        _ => return Ok(false),
    };

    // Get memory embedding for similarity check
    let embeddings = db.generate_embeddings(&[content.to_string()])?;
    let mem_embedding = match embeddings.first() {
        Some(e) => e,
        None => return Ok(false),
    };

    let page = match db
        .find_matching_page(entity_id, mem_embedding, growth_threshold)
        .await?
    {
        Some(c) => c,
        None => return Ok(false),
    };

    // LLM: update page with new memory
    let user_prompt = format!(
        "## Current Concept\n{}\n\n## New Memory\n[{}] {}",
        page.content, source_id, content
    );

    let response = llm
        .generate(crate::llm_provider::LlmRequest {
            system_prompt: Some(prompts.update_page.clone()),
            user_prompt,
            max_tokens: 1024,
            temperature: 0.1,
            label: None,
            timeout_secs: None,
        })
        .await
        .map_err(|e| WenlanError::Llm(format!("page growth LLM: {e}")))?;

    let updated = crate::llm_provider::strip_think_tags(&response);
    let updated = updated.trim();

    if updated.is_empty() {
        return Ok(false);
    }

    // Shrink-guard pre-check (T17): early-exit before calling update_page.
    // update_page has its own shrink-guard backstop, but this early-exit
    // preserves the Ok(false) contract (skipped-growth) rather than Err.
    // Matches the is_empty() early-return contract at line 572.
    if let Some(threshold) = crate::post_write::merge_shrink_threshold() {
        if !crate::retrieval::integrity::body_shrink_ok(&page.content, updated, threshold) {
            log::warn!(
                "[grow_page] shrink-guard skipped growth for page {}: new body ({} chars) < {}% of old ({} chars)",
                page.id,
                updated.chars().count(),
                (threshold * 100.0) as u32,
                page.content.chars().count(),
            );
            return Ok(false);
        }
    }

    // Update page with new content + add source memory
    let mut source_ids = page.source_memory_ids.clone();
    if !source_ids.contains(&source_id.to_string()) {
        source_ids.push(source_id.to_string());
    }
    let _ = crate::post_write::update_page(
        db,
        &page.id,
        UpdatePageRequest {
            content: updated.to_string(),
            source_memory_ids: source_ids,
        },
        "page_growth",
        false,
        None,
    )
    .await?;

    // Log activity: attribute to the agent who authored the triggering memory.
    let agent = db
        .get_memory_source_agent(source_id)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "system".to_string());
    let detail = format!("grew \"{}\"", page.title);
    let ids = vec![source_id.to_string()];
    if let Err(e) = db
        .log_agent_activity(&agent, "page_grow", &ids, None, &detail)
        .await
    {
        log::warn!("[post_ingest] log page_grow activity failed: {e}");
    }

    Ok(true)
}

async fn write_grown_page(db: &MemoryDB, source_id: &str, knowledge_path: &std::path::Path) {
    match db.find_page_by_source_memory(source_id).await {
        Ok(Some(page)) => {
            let writer =
                crate::export::knowledge::KnowledgeWriter::new(knowledge_path.to_path_buf());
            match writer.write_page(&page) {
                Ok(path) => log::info!("[post_ingest] wrote page to {path}"),
                Err(e) => log::warn!("[post_ingest] knowledge write failed: {e}"),
            }
        }
        Ok(None) => {}
        Err(e) => log::warn!("[post_ingest] page lookup for knowledge write failed: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create an in-memory test database.
    async fn test_db() -> (MemoryDB, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let db = MemoryDB::new(
            db_path.as_path(),
            std::sync::Arc::new(crate::events::NoopEmitter),
        )
        .await
        .unwrap();
        (db, dir)
    }

    /// Helper to build a minimal RawDocument for testing.
    fn make_doc(source_id: &str, content: &str) -> crate::sources::RawDocument {
        crate::sources::RawDocument {
            source: "memory".to_string(),
            source_id: source_id.to_string(),
            title: content.chars().take(40).collect(),
            summary: None,
            content: content.to_string(),
            url: None,
            last_modified: chrono::Utc::now().timestamp(),
            metadata: std::collections::HashMap::new(),
            memory_type: Some("fact".to_string()),
            space: None,
            source_agent: Some("test".to_string()),
            confidence: Some(0.7),
            confirmed: Some(false),
            stability: None,
            supersedes: None,
            pending_revision: false,
            entity_id: None,
            quality: None,
            importance: None,
            is_recap: false,
            enrichment_status: "raw".to_string(),
            supersede_mode: "hide".to_string(),
            structured_fields: None,
            retrieval_cue: None,
            source_text: None,
            content_hash: None,
        }
    }

    #[tokio::test]
    async fn test_auto_link_entity_no_entities() {
        let (db, _dir) = test_db().await;
        let tuning = crate::tuning::RefineryConfig::default();
        let linked = auto_link_entity(&db, "mem_test", "Some content about nothing", &tuning)
            .await
            .unwrap();
        assert!(!linked, "should not link when no entities exist");
    }

    #[tokio::test]
    async fn test_suggest_entity_creation_stub() {
        let (db, _dir) = test_db().await;
        // Stub should always succeed
        suggest_entity_creation(&db, "Alice uses PostgreSQL")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_full_enrichment_pipeline() {
        let (db, _dir) = test_db().await;
        let doc = make_doc("mem_enrich_test", "The capital of France is Paris");
        db.upsert_documents(vec![doc]).await.unwrap();

        // Run enrichment — should complete without error
        run_post_ingest_enrichment(
            &db,
            "mem_enrich_test",
            "The capital of France is Paris",
            None,
            Some("fact"),
            None,
            None,
            None,
            &crate::prompts::PromptRegistry::default(),
            &crate::tuning::RefineryConfig::default(),
            &crate::tuning::DistillationConfig::default(),
            None,
            None, // cancel — T22, inert
            None, // precomputed_kg
        )
        .await
        .unwrap();

        // Verify enrichment_summary is derived from per-step outcomes
        let summary = db.get_enrichment_summary("mem_enrich_test").await.unwrap();
        assert_eq!(summary, "enriched");
    }

    #[tokio::test]
    async fn test_enrichment_records_per_step_outcomes() {
        let (db, _dir) = test_db().await;
        let doc = make_doc("mem_step_record", "The capital of France is Paris");
        db.upsert_documents(vec![doc]).await.unwrap();

        run_post_ingest_enrichment(
            &db,
            "mem_step_record",
            "The capital of France is Paris",
            None,
            Some("fact"),
            None,
            None,
            None,
            &crate::prompts::PromptRegistry::default(),
            &crate::tuning::RefineryConfig::default(),
            &crate::tuning::DistillationConfig::default(),
            None,
            None, // cancel — T22, inert
            None, // precomputed_kg
        )
        .await
        .unwrap();

        let steps = db.get_enrichment_steps("mem_step_record").await.unwrap();
        assert!(!steps.is_empty(), "should have recorded enrichment steps");

        // entity_extract should be skipped (no LLM)
        let extract = steps.iter().find(|s| s.step == "entity_extract").unwrap();
        assert_eq!(extract.status, "skipped");

        // title_enrich should be skipped (no LLM)
        let title = steps.iter().find(|s| s.step == "title_enrich").unwrap();
        assert_eq!(title.status, "skipped");

        // page_growth should be skipped (no LLM)
        let growth = steps.iter().find(|s| s.step == "page_growth").unwrap();
        assert_eq!(growth.status, "skipped");

        // Summary should be enriched (no failures)
        let summary = db.get_enrichment_summary("mem_step_record").await.unwrap();
        assert_eq!(summary, "enriched");
    }

    /// Regression guard for time-windowed store-time entity-extraction batching.
    ///
    /// `post_ingest::run_post_ingest_enrichment` at lines 62–134 looks for
    /// other recent same-agent memories lacking an `entity_id` and — if the
    /// batch size is >1 — runs one combined entity-extraction LLM call
    /// across all of them, then links the resulting entity to every batch
    /// member. This lets 10 `mcp__origin__remember` calls from the same
    /// agent coalesce into fewer LLM invocations instead of 10 isolated
    /// ones.
    ///
    /// The primitive under test is `MemoryDB::find_recent_batch` at
    /// `db.rs:6786`, which filters by:
    ///   - source = 'memory' AND chunk_index = 0
    ///   - source_agent = <agent>
    ///   - entity_id IS NULL
    ///   - last_modified > now - window_secs
    ///
    /// This test seeds three memories from the same agent through the
    /// normal `upsert_documents` path (so chunking + FTS triggers fire
    /// exactly as in production) and asserts they all show up in the
    /// batch window. Then it confirms that filling in an `entity_id` on
    /// one of them causes it to drop out of the batch — matching the
    /// real-world pattern where a prior enrichment pass has already
    /// linked the memory, so it shouldn't be redundantly re-extracted.
    ///
    /// NOTE: this test requires the FastEmbed model cache. In offline /
    /// cert-failing environments `test_db()` will fail at init — skip at
    /// the harness level, not by gating this test, since the blocker is
    /// shared with ~240 other tests in the crate.
    #[tokio::test]
    async fn find_recent_batch_collects_same_agent_memories_without_entity_id() {
        let (db, _dir) = test_db().await;
        let agent = "mcp-batch-test";
        let mut doc_a = make_doc(
            "mem_batch_a",
            "Alice joined the research team this quarter.",
        );
        doc_a.source_agent = Some(agent.to_string());
        let mut doc_b = make_doc(
            "mem_batch_b",
            "Alice presented the eval results at standup.",
        );
        doc_b.source_agent = Some(agent.to_string());
        let mut doc_c = make_doc("mem_batch_c", "Alice is migrating the daemon to axum 0.8.");
        doc_c.source_agent = Some(agent.to_string());
        db.upsert_documents(vec![doc_a, doc_b, doc_c])
            .await
            .unwrap();

        let window_secs = 600; // 10 min — generous, covers any test timing jitter
        let batch = db.find_recent_batch(agent, window_secs).await.unwrap();
        let ids: Vec<String> = batch.iter().map(|(id, _)| id.clone()).collect();
        assert!(ids.contains(&"mem_batch_a".to_string()));
        assert!(ids.contains(&"mem_batch_b".to_string()));
        assert!(ids.contains(&"mem_batch_c".to_string()));
        assert_eq!(
            batch.len(),
            3,
            "all three same-agent memories must be in the batch window, got: {ids:?}"
        );

        // Flip one memory's entity_id — simulate a prior enrichment pass
        // that already linked it. find_recent_batch must drop it so the
        // next extraction doesn't re-process it.
        db.update_memory_entity_id("mem_batch_b", "entity_alice")
            .await
            .unwrap();
        let batch_after = db.find_recent_batch(agent, window_secs).await.unwrap();
        let ids_after: Vec<String> = batch_after.iter().map(|(id, _)| id.clone()).collect();
        assert!(
            !ids_after.contains(&"mem_batch_b".to_string()),
            "memories with a linked entity_id must be filtered out, got: {ids_after:?}"
        );
        assert_eq!(
            batch_after.len(),
            2,
            "batch should shrink by exactly one after linking one entity"
        );
    }

    /// Second regression guard: agent isolation. Two agents storing at the
    /// same time should each see their own batch; one agent's memories
    /// must never leak into the other's extraction window.
    #[tokio::test]
    async fn find_recent_batch_isolates_by_source_agent() {
        let (db, _dir) = test_db().await;
        let mut doc_claude = make_doc("mem_iso_claude", "Claude Code scaffolded the plugin.");
        doc_claude.source_agent = Some("claude-code".to_string());
        let mut doc_cursor = make_doc("mem_iso_cursor", "Cursor produced a diff for the hook.");
        doc_cursor.source_agent = Some("cursor".to_string());
        db.upsert_documents(vec![doc_claude, doc_cursor])
            .await
            .unwrap();

        let claude_batch = db.find_recent_batch("claude-code", 600).await.unwrap();
        let cursor_batch = db.find_recent_batch("cursor", 600).await.unwrap();
        let claude_ids: Vec<String> = claude_batch.iter().map(|(id, _)| id.clone()).collect();
        let cursor_ids: Vec<String> = cursor_batch.iter().map(|(id, _)| id.clone()).collect();

        assert_eq!(claude_ids, vec!["mem_iso_claude"]);
        assert_eq!(cursor_ids, vec!["mem_iso_cursor"]);
    }

    // ---- T22: cooperative-cancellation (debounced reflection) ----

    /// Default-OFF guarantee, expressed at the core boundary: passing a cancel
    /// flag that is `false` must be inert — every enrichment step still runs.
    #[tokio::test]
    async fn test_enrichment_runs_all_steps_when_not_cancelled() {
        let (db, _dir) = test_db().await;
        let doc = make_doc("mem_t22_inert", "The capital of France is Paris");
        db.upsert_documents(vec![doc]).await.unwrap();

        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        run_post_ingest_enrichment(
            &db,
            "mem_t22_inert",
            "The capital of France is Paris",
            None,
            Some("fact"),
            None,
            None,
            None,
            &crate::prompts::PromptRegistry::default(),
            &crate::tuning::RefineryConfig::default(),
            &crate::tuning::DistillationConfig::default(),
            None,
            Some(cancel.as_ref()),
            None, // precomputed_kg
        )
        .await
        .unwrap();

        let steps = db.get_enrichment_steps("mem_t22_inert").await.unwrap();
        let names: std::collections::HashSet<&str> =
            steps.iter().map(|s| s.step.as_str()).collect();
        // Same step coverage as the no-cancel path (test_enrichment_records_per_step_outcomes).
        assert!(
            names.contains("entity_link"),
            "entity_link must run when cancel=false"
        );
        assert!(
            names.contains("entity_extract"),
            "entity_extract must run when cancel=false"
        );
        assert!(
            names.contains("title_enrich"),
            "title_enrich must run when cancel=false"
        );
        assert!(
            names.contains("page_growth"),
            "page_growth must run when cancel=false"
        );
    }

    /// Cancelled before the first step → return Ok(()) with NO steps written.
    #[tokio::test]
    async fn test_enrichment_early_returns_when_cancelled_before_first_step() {
        let (db, _dir) = test_db().await;
        let doc = make_doc("mem_t22_precancel", "The capital of France is Paris");
        db.upsert_documents(vec![doc]).await.unwrap();

        let cancel = std::sync::Arc::new(AtomicBool::new(true));
        run_post_ingest_enrichment(
            &db,
            "mem_t22_precancel",
            "The capital of France is Paris",
            None,
            Some("fact"),
            None,
            None,
            None,
            &crate::prompts::PromptRegistry::default(),
            &crate::tuning::RefineryConfig::default(),
            &crate::tuning::DistillationConfig::default(),
            None,
            Some(cancel.as_ref()),
            None, // precomputed_kg
        )
        .await
        .unwrap();

        let steps = db.get_enrichment_steps("mem_t22_precancel").await.unwrap();
        assert!(
            steps.is_empty(),
            "no enrichment steps must be written when cancelled before the first step, got {steps:?}"
        );
    }

    /// Cancel concurrently with enrichment: whatever steps did run must form a
    /// clean contiguous PREFIX of the canonical step sequence, never a step that
    /// was half-applied or a later step that ran after an earlier one was
    /// skipped. This proves the `is_cancelled` checkpoints cut only at clean
    /// boundaries between whole steps (combined with the cancelled-before-first
    /// test, which is the empty-prefix case, and the not-cancelled test, which
    /// is the full-prefix case).
    ///
    /// The exact cut point depends on the scheduler race (the no-LLM path is
    /// fast), so we assert the invariant that must hold for EVERY cut point
    /// rather than a fixed one. Run on a multi-thread runtime so the flipper and
    /// the enrichment make progress in parallel.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_enrichment_cancel_midway_preserves_committed_steps() {
        // Canonical order steps are recorded in for the no-LLM path.
        const CANON: [&str; 6] = [
            "entity_link",
            "entity_extract",
            "page_contradiction",
            "entity_suggestion",
            "title_enrich",
            "page_growth",
        ];

        let (db, _dir) = test_db().await;
        let doc = make_doc("mem_t22_midway", "The capital of France is Paris");
        db.upsert_documents(vec![doc]).await.unwrap();
        let db = std::sync::Arc::new(db);

        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let cancel_task = cancel.clone();
        let db_task = db.clone();
        let handle = tokio::spawn(async move {
            run_post_ingest_enrichment(
                &db_task,
                "mem_t22_midway",
                "The capital of France is Paris",
                None,
                Some("fact"),
                None,
                None,
                None,
                &crate::prompts::PromptRegistry::default(),
                &crate::tuning::RefineryConfig::default(),
                &crate::tuning::DistillationConfig::default(),
                None,
                Some(cancel_task.as_ref()),
                None, // precomputed_kg
            )
            .await
            .unwrap();
        });

        // Flip cancel the moment the first step is observed, then let the rest
        // settle. Bounded spin so a logic bug can't hang the test.
        for _ in 0..2000 {
            let steps = db.get_enrichment_steps("mem_t22_midway").await.unwrap();
            if steps.iter().any(|s| s.step == "entity_link") {
                cancel.store(true, Ordering::SeqCst);
                break;
            }
            tokio::task::yield_now().await;
        }
        handle.await.unwrap();

        let steps = db.get_enrichment_steps("mem_t22_midway").await.unwrap();
        let names: std::collections::HashSet<&str> =
            steps.iter().map(|s| s.step.as_str()).collect();

        // Invariant 1: the recorded steps are exactly the first K of CANON for
        // some K — a contiguous prefix. No later step ever ran while an earlier
        // one was skipped (that would mean a checkpoint failed to cut cleanly).
        let k = CANON.iter().take_while(|s| names.contains(**s)).count();
        for (i, step) in CANON.iter().enumerate() {
            let present = names.contains(*step);
            let expected = i < k;
            assert_eq!(
                present, expected,
                "step {step:?} present={present} but prefix length is {k}: recorded steps must be a contiguous prefix of the canonical order, got {names:?}"
            );
        }
        // Invariant 2: every recorded step is complete (has a non-empty status),
        // i.e. no step was half-written when cancellation hit.
        for st in &steps {
            assert!(
                !st.status.is_empty(),
                "step {:?} was recorded without a status — a half-written step",
                st.step
            );
        }
    }

    /// The memory row stored synchronously before enrichment must remain
    /// retrievable after a cancelled enrichment — cancellation only delays
    /// enrichment, it never drops data.
    #[tokio::test]
    async fn test_memory_row_intact_after_cancel() {
        let (db, _dir) = test_db().await;
        let doc = make_doc("mem_t22_rowintact", "Tokyo is the capital of Japan");
        db.upsert_documents(vec![doc]).await.unwrap();

        let cancel = std::sync::Arc::new(AtomicBool::new(true));
        run_post_ingest_enrichment(
            &db,
            "mem_t22_rowintact",
            "Tokyo is the capital of Japan",
            None,
            Some("fact"),
            None,
            None,
            None,
            &crate::prompts::PromptRegistry::default(),
            &crate::tuning::RefineryConfig::default(),
            &crate::tuning::DistillationConfig::default(),
            None,
            Some(cancel.as_ref()),
            None, // precomputed_kg
        )
        .await
        .unwrap();

        // Row still present and retrievable despite the cancelled enrichment.
        let items = db.list_memories(None, None, None, None, 10).await.unwrap();
        let row = items.iter().find(|i| i.source_id == "mem_t22_rowintact");
        assert!(
            row.is_some(),
            "the stored memory row must survive a cancelled enrichment"
        );
    }

    #[tokio::test]
    async fn test_enrichment_honesty_end_to_end() {
        let (db, _dir) = test_db().await;

        // Store memory A -- full enrichment (no LLM, so LLM steps get skipped)
        let doc_a = make_doc("mem_honest_a", "The Eiffel Tower is in Paris");
        db.upsert_documents(vec![doc_a]).await.unwrap();
        run_post_ingest_enrichment(
            &db,
            "mem_honest_a",
            "The Eiffel Tower is in Paris",
            None,
            Some("fact"),
            None,
            None,
            None,
            &crate::prompts::PromptRegistry::default(),
            &crate::tuning::RefineryConfig::default(),
            &crate::tuning::DistillationConfig::default(),
            None,
            None, // cancel — T22, inert
            None, // precomputed_kg
        )
        .await
        .unwrap();

        // Summary should be enriched (all non-LLM steps ok, LLM steps skipped)
        let summary_a = db.get_enrichment_summary("mem_honest_a").await.unwrap();
        assert_eq!(summary_a, "enriched");

        // list_memories should show enriched
        let items = db.list_memories(None, None, None, None, 10).await.unwrap();
        let item_a = items
            .iter()
            .find(|i| i.source_id == "mem_honest_a")
            .unwrap();
        assert_eq!(item_a.enrichment_status, "enriched");

        // Store memory B -- no enrichment run yet
        let doc_b = make_doc("mem_honest_b", "Tokyo is the capital of Japan");
        db.upsert_documents(vec![doc_b]).await.unwrap();

        // Should be raw (no steps recorded)
        let summary_b = db.get_enrichment_summary("mem_honest_b").await.unwrap();
        assert_eq!(summary_b, "raw");

        let items = db.list_memories(None, None, None, None, 10).await.unwrap();
        let item_b = items
            .iter()
            .find(|i| i.source_id == "mem_honest_b")
            .unwrap();
        assert_eq!(item_b.enrichment_status, "raw");
    }

    /// TDD gate for Task 1.2 — precomputed_kg parameter.
    ///
    /// DB-A: run_post_ingest_enrichment with precomputed_kg = None (canned LLM called for extract).
    /// DB-B: run_post_ingest_enrichment with precomputed_kg = Some(kg) (precomputed KG passed in;
    ///        LLM must NOT be called for extract on this arm).
    ///
    /// Asserts: both link source_id to an entity with the same name, and both record
    /// entity_extract = "ok".
    #[tokio::test]
    async fn precomputed_kg_matches_inline_extract() {
        use crate::llm_provider::{CannedLlmProvider, LlmProvider};
        use std::sync::Arc;

        let prompts = crate::prompts::PromptRegistry::default();

        // Build a CannedLlmProvider that returns a KG with "Alice" as entity.
        // Key = first 30 chars of the extract_knowledge_graph prompt (matches the `sys.contains(key)` check).
        let key_fragment: String = prompts.extract_knowledge_graph.chars().take(30).collect();
        let kg_json =
            r#"[{"entities":[{"name":"Alice","type":"person"}],"observations":[],"relations":[]}]"#;
        let canned: Arc<dyn LlmProvider> =
            Arc::new(CannedLlmProvider::new("DEFAULT").with(key_fragment.clone(), kg_json));

        // Pre-compute the KG using extract_kg (pure, no DB).
        let precomputed = crate::refinery::extract_kg(&canned, &prompts, "Alice joined Acme")
            .await
            .expect("extract_kg failed in test setup");
        assert!(
            !precomputed.is_empty(),
            "precomputed KG must be non-empty for a useful test"
        );

        // ----- DB-A: None arm (inline LLM extract) -----
        let (db_a, _dir_a) = test_db().await;
        let doc_a = make_doc("mem_pkg_a", "Alice joined Acme");
        db_a.upsert_documents(vec![doc_a]).await.unwrap();
        let canned_a: Arc<dyn LlmProvider> =
            Arc::new(CannedLlmProvider::new("DEFAULT").with(key_fragment.clone(), kg_json));
        run_post_ingest_enrichment(
            &db_a,
            "mem_pkg_a",
            "Alice joined Acme",
            None,
            Some("fact"),
            None,
            None,
            Some(&canned_a),
            &prompts,
            &crate::tuning::RefineryConfig::default(),
            &crate::tuning::DistillationConfig::default(),
            None,
            None,
            None, // precomputed_kg = None
        )
        .await
        .unwrap();

        // ----- DB-B: Some(kg) arm (precomputed KG, LLM should NOT be called for extract) -----
        let (db_b, _dir_b) = test_db().await;
        let doc_b = make_doc("mem_pkg_b", "Alice joined Acme");
        db_b.upsert_documents(vec![doc_b]).await.unwrap();
        // Use a CannedLlmProvider that returns DEFAULT (no entities) — if extract_kg is mistakenly
        // called on the B arm, it would extract nothing and the test would fail.
        let canned_b_no_extract: Arc<dyn LlmProvider> = Arc::new(CannedLlmProvider::new("[]"));
        run_post_ingest_enrichment(
            &db_b,
            "mem_pkg_b",
            "Alice joined Acme",
            None,
            Some("fact"),
            None,
            None,
            Some(&canned_b_no_extract),
            &prompts,
            &crate::tuning::RefineryConfig::default(),
            &crate::tuning::DistillationConfig::default(),
            None,
            None,
            Some(precomputed), // precomputed_kg = Some(kg)
        )
        .await
        .unwrap();

        // Both arms must record entity_extract = "ok"
        let steps_a = db_a.get_enrichment_steps("mem_pkg_a").await.unwrap();
        let extract_a = steps_a
            .iter()
            .find(|s| s.step == "entity_extract")
            .expect("entity_extract step missing on arm A");
        assert_eq!(
            extract_a.status, "ok",
            "arm A entity_extract status must be ok"
        );

        let steps_b = db_b.get_enrichment_steps("mem_pkg_b").await.unwrap();
        let extract_b = steps_b
            .iter()
            .find(|s| s.step == "entity_extract")
            .expect("entity_extract step missing on arm B");
        assert_eq!(
            extract_b.status, "ok",
            "arm B entity_extract status must be ok"
        );

        // Both arms must link source_id to an entity, and that entity must be "Alice"
        let eid_a = db_a
            .get_memory_entity_id("mem_pkg_a")
            .await
            .unwrap()
            .expect("arm A must have an entity_id");
        let eid_b = db_b
            .get_memory_entity_id("mem_pkg_b")
            .await
            .unwrap()
            .expect("arm B must have an entity_id");

        // Verify entity name for each arm
        let (name_a, _) = db_a
            .get_entity_name_type(&eid_a)
            .await
            .unwrap()
            .expect("entity from arm A not found");
        assert_eq!(
            name_a, "Alice",
            "arm A entity name must be Alice, got: {name_a}"
        );

        let (name_b, _) = db_b
            .get_entity_name_type(&eid_b)
            .await
            .unwrap()
            .expect("entity from arm B not found");
        assert_eq!(
            name_b, "Alice",
            "arm B entity name must be Alice, got: {name_b}"
        );
    }
}
