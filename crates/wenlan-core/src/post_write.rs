// SPDX-License-Identifier: Apache-2.0
//! Canonical write-path capability functions. Each fn owns the full create
//! flow for one kind: validation, resolve-or-create (where applicable),
//! storage primitive call, post-write enrichment (verify, log, refinery
//! enqueue). Both HTTP route handlers and daemon-internal extractors call
//! these -- eliminating drift between agent-LLM and daemon-LLM trigger paths.

use crate::db::MemoryDB;
use crate::error::WenlanError;
use std::path::Path;
use wenlan_types::requests::{
    AddObservationRequest, CreateConceptRequest, CreateEntityRequest, CreateRelationRequest,
    UpdatePageRequest,
};

#[derive(Debug, Clone, serde::Serialize)]
pub struct WriteResult {
    pub id: String,
    pub warnings: Vec<String>,
    pub wrote: bool,
}

/// Best-effort activity logger used by curation-mutate capability fns.
/// Failure to log does not fail the operation — matches the pattern in
/// `create_entity`, `create_relation`, etc.
pub(crate) async fn log_activity_best_effort(
    db: &MemoryDB,
    agent: &str,
    action: &str,
    target_id: &str,
) {
    let target = target_id.to_string();
    if let Err(e) = db
        .log_agent_activity(agent, action, std::slice::from_ref(&target), None, "")
        .await
    {
        log::warn!("[{}] activity log failed: {}", action, e);
    }
}

/// Create or resolve an entity. Canonical entry point for both
/// agent-triggered (`/api/memory/entities`) and daemon-internal
/// (`kg/entity_extraction.rs`) writes.
///
/// Resolution order (4-step, matches `importer::resolve_entity_bulk` used for bulk/eval paths):
///   1. Alias lookup
///   2. Exact name search
///   3. Vector similarity (distance < 0.1 => sim > 0.9)
///   4. Create new
///
/// Post-write enrichment fires only on newly-created entities. Resolved-existing
/// returns immediately with empty warnings.
pub async fn create_entity(
    db: &MemoryDB,
    req: CreateEntityRequest,
    agent: &str,
) -> Result<WriteResult, WenlanError> {
    // Pre-write validation
    let name = req.name.trim();
    if name.is_empty() {
        return Err(WenlanError::Validation(
            "entity name must not be empty".into(),
        ));
    }
    let entity_type = req.entity_type.trim();
    if entity_type.is_empty() {
        return Err(WenlanError::Validation(
            "entity_type must not be empty".into(),
        ));
    }
    if let Some(c) = req.confidence {
        if !(0.0..=1.0).contains(&c) {
            return Err(WenlanError::Validation(format!(
                "confidence {c} out of range [0.0, 1.0]"
            )));
        }
    }

    let name_lower = name.to_lowercase();

    // Step 1: Alias lookup
    if let Some(id) = db.resolve_entity_by_alias(&name_lower).await? {
        return Ok(WriteResult {
            id,
            warnings: vec![],
            wrote: false,
        });
    }

    // Step 2: Exact name search
    let name_results = db.search_entities_by_name(name).await?;
    if let Some(existing) = name_results.first() {
        let _ = db.add_entity_alias(&name_lower, &existing.id, "auto").await;
        return Ok(WriteResult {
            id: existing.id.clone(),
            warnings: vec![],
            wrote: false,
        });
    }

    // Step 2.5: deterministic MinHash/LSH near-dedup (T16, opt-in).
    // Catches char-level near-dups like "PostgreSQL"/"Postgres" that exact-name
    // match misses and that the vector step may also miss. Gated behind
    // WENLAN_ENABLE_ENTITY_MINHASH; the entropy gate inside
    // minhash_resolve_candidate punts short/low-entropy names to the vector
    // step. Same-type-only; auto-merge requires exact Jaccard >= 0.9.
    if crate::db::entity_minhash_enabled() {
        if let Some(cand_id) = db.minhash_resolve_candidate(name, entity_type).await? {
            let _ = db.add_entity_alias(&name_lower, &cand_id, "minhash").await;
            return Ok(WriteResult {
                id: cand_id,
                warnings: vec![],
                wrote: false,
            });
        }
    }

    // Step 3: Vector similarity (distance < 0.1 => sim > 0.9)
    let vec_results = db.search_entities_by_vector(name, 1).await?;
    if let Some(result) = vec_results.first() {
        if result.distance < 0.1 {
            let _ = db
                .add_entity_alias(&name_lower, &result.entity.id, "auto")
                .await;
            return Ok(WriteResult {
                id: result.entity.id.clone(),
                warnings: vec![],
                wrote: false,
            });
        }
    }

    // Step 4: Create new
    let id = db
        .store_entity(
            name,
            entity_type,
            req.space.as_deref(),
            req.source_agent.as_deref(),
            req.confidence,
        )
        .await?;

    // T16: index the new entity's LSH bands so future high-entropy names can
    // find it via Step 2.5. Only fires when the flag is on; the helper skips
    // short/low-entropy names so the band table stays small. Best-effort.
    if crate::db::entity_minhash_enabled() {
        if let Err(e) = db.index_entity_minhash_if_eligible(&id, name).await {
            log::warn!("[create_entity] minhash band index failed for {id}: {e}");
        }
    }

    // Post-write enrichment (LLM-free, non-blocking)
    let mut warnings: Vec<String> = Vec::new();

    // 1. Self-retrieval verification
    if let Ok(result) = crate::kg_quality::verify_entity(db, &id, name).await {
        for w in &result.warnings {
            log::warn!("[create_entity] {w}");
            warnings.push(w.clone());
        }
    }

    // 2. Merge-candidate refinery enqueue: similar entity in [0.85, 0.9) with same type
    if let Ok(results) = db.search_entities_by_vector(name, 5).await {
        for r in &results {
            if r.entity.id == id {
                continue;
            }
            if r.entity.entity_type != entity_type {
                continue;
            }
            let sim = 1.0 - r.distance;
            if (0.85..0.9).contains(&sim) {
                let id_len = id.len().min(8);
                let r_id_len = r.entity.id.len().min(8);
                let proposal_id = format!("merge_{}_{}", &id[..id_len], &r.entity.id[..r_id_len]);
                let payload = serde_json::json!({
                    "existing_id": r.entity.id,
                    "new_id": id,
                    "similarity": sim,
                })
                .to_string();
                let _ = db
                    .insert_refinement_proposal(
                        &proposal_id,
                        "entity_merge",
                        &[id.clone(), r.entity.id.clone()],
                        Some(&payload),
                        sim as f64,
                    )
                    .await;
            }
        }
    }

    // 3. Activity log
    let detail = format!("name={name}, type={entity_type}");
    if let Err(e) = db
        .log_agent_activity(
            agent,
            "entity_create",
            std::slice::from_ref(&id),
            None,
            &detail,
        )
        .await
    {
        log::warn!("[create_entity] activity log failed: {e}");
    }

    Ok(WriteResult {
        id,
        warnings,
        wrote: true,
    })
}

/// Create a directed relation between two entities. Canonical entry for
/// both agent-triggered (`/api/memory/relations`) and daemon-internal
/// extraction.
pub async fn create_relation(
    db: &MemoryDB,
    req: CreateRelationRequest,
    agent: &str,
) -> Result<WriteResult, WenlanError> {
    // Pre-write validation
    if !db.entity_exists(&req.from_entity).await? {
        return Err(WenlanError::Validation(format!(
            "from_entity '{}' does not exist",
            req.from_entity
        )));
    }
    if !db.entity_exists(&req.to_entity).await? {
        return Err(WenlanError::Validation(format!(
            "to_entity '{}' does not exist",
            req.to_entity
        )));
    }
    let rt = req.relation_type.trim();
    if !is_valid_snake_case_relation(rt) {
        return Err(WenlanError::Validation(format!(
            "relation_type '{rt}' must be lowercase snake_case (^[a-z][a-z0-9_]*$)"
        )));
    }

    // Idempotency: if an identical (from, to, type) triple already exists,
    // return its id immediately — no log, no refinery enqueue.
    if let Ok(existing) = db
        .list_relations_between(&req.from_entity, &req.to_entity)
        .await
    {
        if let Some((existing_id, _)) = existing.into_iter().find(|(_, t)| t == rt) {
            return Ok(WriteResult {
                id: existing_id,
                warnings: vec![],
                wrote: false,
            });
        }
    }

    let id = db
        .create_relation(
            &req.from_entity,
            &req.to_entity,
            rt,
            req.source_agent.as_deref(),
            req.confidence,
            req.explanation.as_deref(),
            req.source_memory_id.as_deref(),
        )
        .await?;

    // Post-write enrichment
    let mut warnings: Vec<String> = Vec::new();

    // Conflict check: existing relation between same (from, to) with different
    // type → auto-supersede (last-write-wins). The /refinery skill no longer
    // surfaces queue proposals to users (PR #109), so enqueuing for human review
    // would silently accumulate forever. The same outcome the user would have
    // hand-applied via `accept_refinement(relation_conflict)` runs immediately
    // here. Activity log records the daemon's decision for power-user audit
    // (queryable via list_agent_activity).
    if let Ok(existing) = db
        .list_relations_between(&req.from_entity, &req.to_entity)
        .await
    {
        for (existing_id, existing_type) in &existing {
            if existing_id != &id && existing_type != rt {
                match db.supersede_relation(existing_id, &id).await {
                    Ok(archived) => {
                        warnings.push(format!(
                            "auto-superseded existing relation ({}-{}-{}); newer relation now active",
                            req.from_entity, existing_type, req.to_entity
                        ));
                        let payload = serde_json::json!({
                            "existing_id": existing_id,
                            "new_id": id,
                            "from": req.from_entity,
                            "to": req.to_entity,
                            "old_type": existing_type,
                            "new_type": rt,
                            "archived": archived,
                        })
                        .to_string();
                        if let Err(e) = db
                            .log_agent_activity(
                                agent,
                                "relation_supersede_auto",
                                &[id.clone(), existing_id.clone()],
                                None,
                                &payload,
                            )
                            .await
                        {
                            log::warn!("[create_relation] auto-supersede activity log failed: {e}");
                        }
                    }
                    Err(e) => {
                        log::warn!(
                            "[create_relation] auto-supersede of {} → {} failed: {e}",
                            existing_id,
                            id
                        );
                        warnings.push(format!(
                            "conflicting relation exists ({}-{}-{}); auto-supersede failed",
                            req.from_entity, existing_type, req.to_entity
                        ));
                    }
                }
            }
        }
    }

    // Activity log
    let detail = format!(
        "from={}, to={}, type={}",
        req.from_entity, req.to_entity, rt
    );
    if let Err(e) = db
        .log_agent_activity(
            agent,
            "relation_create",
            std::slice::from_ref(&id),
            None,
            &detail,
        )
        .await
    {
        log::warn!("[create_relation] activity log failed: {e}");
    }

    Ok(WriteResult {
        id,
        warnings,
        wrote: true,
    })
}

/// Add an observation to an existing entity. Canonical entry for both
/// agent-triggered (`/api/memory/observations`) and daemon-internal callers.
pub async fn add_observation(
    db: &MemoryDB,
    req: AddObservationRequest,
    agent: &str,
) -> Result<WriteResult, WenlanError> {
    // Pre-write validation
    if !db.entity_exists(&req.entity_id).await? {
        return Err(WenlanError::Validation(format!(
            "entity_id '{}' does not exist",
            req.entity_id
        )));
    }
    let content = req.content.trim();
    if content.chars().count() < 5 {
        return Err(WenlanError::Validation(
            "observation content must be at least 5 characters".into(),
        ));
    }
    if let Some(c) = req.confidence {
        if !(0.0..=1.0).contains(&c) {
            return Err(WenlanError::Validation(format!(
                "confidence {c} out of range [0.0, 1.0]"
            )));
        }
    }

    let id = db
        .add_observation(
            &req.entity_id,
            content,
            req.source_agent.as_deref(),
            req.confidence,
        )
        .await?;

    // Activity log (no verify step yet — observations have no canonical quality check)
    let detail = format!("entity_id={}, content_len={}", req.entity_id, content.len());
    if let Err(e) = db
        .log_agent_activity(
            agent,
            "observation_add",
            std::slice::from_ref(&id),
            None,
            &detail,
        )
        .await
    {
        log::warn!("[add_observation] activity log failed: {e}");
    }

    Ok(WriteResult {
        id,
        warnings: vec![],
        wrote: true,
    })
}

/// Create a distilled wiki page. Canonical entry for both agent-triggered
/// (`/api/pages`) and daemon-internal distillation callers.
pub async fn create_page(
    db: &MemoryDB,
    req: CreateConceptRequest,
    agent: &str,
    knowledge_path: Option<&Path>,
) -> Result<WriteResult, WenlanError> {
    // Pre-write validation
    if req.title.trim().is_empty() {
        return Err(WenlanError::Validation(
            "page title must not be empty".into(),
        ));
    }
    if req.content.trim().is_empty() {
        return Err(WenlanError::Validation(
            "page content must not be empty".into(),
        ));
    }
    let creation_kind = req.creation_kind.as_deref().unwrap_or("distilled");
    if creation_kind == "distilled" && req.source_memory_ids.is_empty() {
        return Err(WenlanError::Validation(
            "distilled page must cite at least one source memory".into(),
        ));
    }
    const VALID_KINDS: [&str; 4] = ["distilled", "authored", "research", "imported"];
    if !VALID_KINDS.contains(&creation_kind) {
        return Err(WenlanError::Validation(format!(
            "invalid creation_kind '{creation_kind}' (expected one of: distilled, authored, research, imported)"
        )));
    }
    let review_status = if creation_kind == "distilled" {
        "confirmed"
    } else {
        "unconfirmed"
    };
    // Resolution check: every source id must exist
    for sid in &req.source_memory_ids {
        if db.get_memory_detail(sid).await?.is_none() {
            return Err(WenlanError::Validation(format!(
                "source memory '{sid}' does not exist"
            )));
        }
    }
    // Hallucination guard (only when sources are present)
    if !req.source_memory_ids.is_empty() {
        let passed =
            crate::kg_quality::hallucination_guard(db, &req.content, &req.source_memory_ids)
                .await?;
        if !passed {
            return Err(WenlanError::Validation(
                "page body diverges from cited sources (cos sim < 0.6)".into(),
            ));
        }
    }

    // Build page
    let id = crate::pages::new_page_id();
    let now = chrono::Utc::now().to_rfc3339();
    let page = crate::pages::Page {
        id: id.clone(),
        title: req.title.clone(),
        summary: req.summary.clone(),
        content: req.content.clone(),
        entity_id: req.entity_id.clone(),
        space: req.space.clone(),
        source_memory_ids: req.source_memory_ids.clone(),
        version: 1,
        status: "active".to_string(),
        created_at: now.clone(),
        last_compiled: now.clone(),
        last_modified: now.clone(),
        sources_updated_count: 0,
        stale_reason: None,
        user_edited: false,
        relevance_score: 0.0,
        last_edited_by: None,
        last_edited_at: None,
        last_delta_summary: None,
        changelog: None,
        creation_kind: creation_kind.to_string(),
        review_status: review_status.to_string(),
        workspace: req.workspace.clone(),
        citations: Vec::new(),
    };

    // md-first write (only if a knowledge_path was provided)
    let writer_opt =
        knowledge_path.map(|p| crate::export::knowledge::KnowledgeWriter::new(p.to_path_buf()));
    if let Some(ref writer) = writer_opt {
        writer
            .write_page(&page)
            .map_err(|e| WenlanError::VectorDb(format!("write_page: {e}")))?;
    }

    // DB index
    let source_refs: Vec<&str> = req.source_memory_ids.iter().map(|s| s.as_str()).collect();
    if let Err(e) = db
        .insert_page_with_kind(
            &id,
            &req.title,
            req.summary.as_deref(),
            &req.content,
            req.entity_id.as_deref(),
            req.space.as_deref(),
            &source_refs,
            &now,
            creation_kind,
            review_status,
            req.workspace.as_deref(),
        )
        .await
    {
        // Rollback md if it was written
        if let Some(ref writer) = writer_opt {
            if let Err(rb) = writer.remove_page(&id) {
                log::warn!(
                    "[create_page] DB insert failed and md rollback also failed for {id}: db_err={e}, rollback_err={rb}"
                );
            }
        }
        return Err(WenlanError::VectorDb(e.to_string()));
    }

    // Post-write enrichment
    let mut warnings: Vec<String> = Vec::new();

    // 1. Orphan-link resolution (best-effort)
    if let Err(e) = db.resolve_orphan_page_links().await {
        log::warn!("[create_page] orphan link resolve failed for {id}: {e}");
        warnings.push(format!("orphan link resolve failed: {e}"));
    }

    // 2. Self-retrieval verification
    if let Ok(result) = crate::kg_quality::verify_page(db, &id, &req.title).await {
        for w in &result.warnings {
            log::warn!("[create_page] {w}");
            warnings.push(w.clone());
        }
    }

    // 3. Activity log (matches synthesis/distill.rs:498 shape — source ids as the memory ids list)
    let detail = format!(
        "title={}, sources={}",
        req.title,
        req.source_memory_ids.len()
    );
    if let Err(e) = db
        .log_agent_activity(agent, "page_create", &req.source_memory_ids, None, &detail)
        .await
    {
        log::warn!("[create_page] activity log failed: {e}");
    }

    Ok(WriteResult {
        id,
        warnings,
        wrote: true,
    })
}

/// Daemon-internal `edited_by` values that bypass the hallucination guard.
/// Incremental updates can push aggregate cosine sim below 0.6; running the
/// guard on these paths would silently drop legitimate refinery writes.
fn skip_hallucination_guard(edited_by: &str) -> bool {
    matches!(
        edited_by,
        "distill" | "re_distill" | "page_growth" | "refinery_merge"
    )
}

/// True iff edited_by names an LLM-rewrite path checked by the shrink-guard.
/// Inverse-intent twin of skip_hallucination_guard -- same match arms.
/// Do NOT merge: skip_hallucination_guard skips; is_llm_rewrite enables.
fn is_llm_rewrite(edited_by: &str) -> bool {
    matches!(
        edited_by,
        "distill" | "re_distill" | "page_growth" | "refinery_merge"
    )
}

/// Parse WENLAN_MERGE_SHRINK_GUARD env var as f64 threshold.
/// Returns Some(t) when set to a valid float; None when unset/unparseable
/// (guard OFF = byte-identical behavior to pre-T17).
/// Mirrors page_channel_enabled() env-read discipline in db.rs.
pub(crate) fn merge_shrink_threshold() -> Option<f64> {
    std::env::var("WENLAN_MERGE_SHRINK_GUARD")
        .ok()
        .and_then(|v| v.trim().parse::<f64>().ok())
}

/// Update a distilled wiki page. Canonical entry for all page-update paths:
/// daemon-internal distillation, refinery re-distill, fs watcher, and
/// future agent-HTTP routes.
///
/// Two write modes via `require_stale`:
/// - `false` — unconditional write (post-ingest, distill, page_growth callers)
/// - `true`  — CAS: only writes when `stale_reason IS NOT NULL` (refinery callers).
///   Returns `Ok(WriteResult { warnings: vec![] })` without writing when not stale.
///
/// Hallucination guard runs only for `edited_by ∈ {manual_edit, api}`.
/// Daemon-internal callers (`distill`, `re_distill`, `page_growth`,
/// `refinery_merge`) skip it — incremental updates may push aggregate cosine
/// sim below 0.6 and would silently drop legitimate writes.
pub async fn update_page(
    db: &MemoryDB,
    page_id: &str,
    req: UpdatePageRequest,
    edited_by: &str,
    require_stale: bool,
    knowledge_path: Option<&Path>,
) -> Result<WriteResult, WenlanError> {
    // ── Pre-write validation ────────────────────────────────────────────────
    if req.content.trim().is_empty() {
        return Err(WenlanError::Validation(
            "page content must not be empty".into(),
        ));
    }
    if req.source_memory_ids.is_empty() {
        return Err(WenlanError::Validation(
            "page must cite at least one source memory".into(),
        ));
    }
    // Source-existence check removed. create_page validates sources at
    // creation time. Updates only carry forward or extend an already-valid
    // source list; re-checking on every update would break daemon-internal
    // callers (fs_edit, re_distill) whose sources may reference pruned
    // memories.

    // ── Conditional hallucination guard ────────────────────────────────────
    if !skip_hallucination_guard(edited_by) {
        let passed =
            crate::kg_quality::hallucination_guard(db, &req.content, &req.source_memory_ids)
                .await?;
        if !passed {
            return Err(WenlanError::Validation(
                "page body diverges from cited sources (cos sim < 0.6)".into(),
            ));
        }
    }

    // ── Load current page for delta computation ─────────────────────────────
    let current = db
        .get_page(page_id)
        .await?
        .ok_or_else(|| WenlanError::Validation(format!("page '{page_id}' does not exist")))?;
    let current_version = current.version;
    let new_version = current_version + 1;

    // Shrink-guard (T17): opt-in via WENLAN_MERGE_SHRINK_GUARD=<f64>.
    // OFF by default: unset/unparseable = None = zero regression.
    // Only fires for LLM-rewrite edited_by; human edits are never blocked.
    // Placed AFTER current page load (needs old body), BEFORE early-return.
    // NOT inside the skip_hallucination_guard block: that skips page_growth/re_distill.
    if is_llm_rewrite(edited_by) {
        if let Some(threshold) = merge_shrink_threshold() {
            if !crate::retrieval::integrity::body_shrink_ok(
                &current.content,
                &req.content,
                threshold,
            ) {
                log::warn!(
                    "[update_page] shrink-guard rejected {edited_by} on {page_id}: new body ({} chars) < {}% of old ({} chars)",
                    req.content.chars().count(),
                    (threshold * 100.0) as u32,
                    current.content.chars().count(),
                );
                return Err(WenlanError::Validation(format!(
                    "page body shrank below {:.0}% of original (shrink-guard); update rejected",
                    threshold * 100.0
                )));
            }
        }
    }

    let source_refs: Vec<&str> = req.source_memory_ids.iter().map(|s| s.as_str()).collect();

    // ── Build changelog entry ───────────────────────────────────────────────
    let delta_summary = crate::db::compute_page_delta_summary(
        &current.content,
        &current.source_memory_ids,
        &req.content,
        &source_refs,
        edited_by,
    );

    // Compute added sources for the changelog entry
    let old_set: std::collections::HashSet<&str> = current
        .source_memory_ids
        .iter()
        .map(|s| s.as_str())
        .collect();
    let new_set: std::collections::HashSet<&str> = source_refs.iter().copied().collect();

    // Early return: identical content and identical source set — nothing to write.
    if delta_summary.is_none() && old_set == new_set {
        return Ok(WriteResult {
            id: page_id.to_string(),
            warnings: vec![],
            wrote: false,
        });
    }

    let mut added_sources: Vec<&str> = new_set.difference(&old_set).copied().collect();
    added_sources.sort_unstable();
    let added_sources_json = serde_json::Value::Array(
        added_sources
            .iter()
            .map(|s| serde_json::Value::String(s.to_string()))
            .collect(),
    );

    let entry = serde_json::json!({
        "version": new_version,
        "at": chrono::Utc::now().timestamp(),
        "edited_by": edited_by,
        "delta_summary": delta_summary,
        "incoming_source_ids": added_sources_json,
    });

    // Read existing changelog and append the new entry
    let existing_cl = db.get_page_changelog(page_id).await?;
    const DEFAULT_CHANGELOG_CAP: usize = 20;
    let new_changelog =
        crate::db::append_changelog_entry(&existing_cl, entry, DEFAULT_CHANGELOG_CAP)?;

    // ── Apply DB update ─────────────────────────────────────────────────────
    let wrote = db
        .try_update_page_content_with_changelog(
            page_id,
            &req.content,
            &source_refs,
            edited_by,
            require_stale,
            &new_changelog,
        )
        .await?;

    if !wrote {
        // CAS skipped — page was not stale; return empty warnings (no-op)
        return Ok(WriteResult {
            id: page_id.to_string(),
            warnings: vec![],
            wrote: false,
        });
    }

    // ── md re-write ─────────────────────────────────────────────────────────
    if let Some(kp) = knowledge_path {
        if let Ok(Some(updated_page)) = db.get_page(page_id).await {
            let writer = crate::export::knowledge::KnowledgeWriter::new(kp.to_path_buf());
            if let Err(e) = writer.write_page(&updated_page) {
                log::warn!("[update_page] md re-write failed for {page_id}: {e}");
            }
        }
    }

    // ── Build warnings ──────────────────────────────────────────────────────
    let warnings = match delta_summary {
        Some(ref summary) => vec![format!("v{current_version} → v{new_version}: {summary}")],
        None => vec![],
    };

    Ok(WriteResult {
        id: page_id.to_string(),
        warnings,
        wrote: true,
    })
}

/// Accept a pending memory revision. Canonical entry for both agent-triggered
/// (`/api/memory/revision/{id}/accept`) and daemon-internal accept-dispatch.
/// Activates the revision row, suppresses the original, and logs activity.
/// Returns `NotFound` if no pending revision exists for the supplied id.
pub async fn accept_pending_revision(
    db: &MemoryDB,
    id: &str,
    agent: &str,
) -> Result<wenlan_types::RevisionAcceptResponse, WenlanError> {
    // `id` may be the revision's own source_id (exact) or its target's (legacy);
    // the DB resolves it and returns the actual (target, revision) pair acted on.
    let (target_source_id, revision_source_id) = db.accept_pending_revision(id).await?;
    log_activity_best_effort(db, agent, "revision_accept", &target_source_id).await;

    Ok(wenlan_types::RevisionAcceptResponse {
        target_source_id,
        revision_source_id,
        wrote: true,
    })
}

/// Dismiss a pending memory revision. Canonical entry for both
/// agent-triggered (`/api/memory/revision/{id}/dismiss`) and daemon-internal
/// triggers. Unstages the pending revision (clears its false revision link,
/// keeping it as an independent row); the original is untouched.
/// Returns `NotFound` if no pending revision exists for the supplied id.
pub async fn dismiss_pending_revision(
    db: &MemoryDB,
    id: &str,
    agent: &str,
) -> Result<wenlan_types::RevisionDismissResponse, WenlanError> {
    let (target_source_id, _revision_source_id) = db.dismiss_pending_revision(id).await?;
    log_activity_best_effort(db, agent, "revision_dismiss", &target_source_id).await;
    Ok(wenlan_types::RevisionDismissResponse {
        target_source_id,
        wrote: true,
    })
}

/// Dismiss all awaiting-review contradiction flags for a memory. Canonical
/// entry for both agent-triggered (`/api/memory/contradiction/{source_id}/dismiss`)
/// and daemon-internal triggers. `wrote: true` is best-effort: the DB method
/// silently no-ops when no rows match. See spec §3 for the caveat.
pub async fn dismiss_contradiction(
    db: &MemoryDB,
    source_id: &str,
    agent: &str,
) -> Result<wenlan_types::ContradictionDismissResponse, WenlanError> {
    db.dismiss_contradiction_for_source(source_id).await?;
    log_activity_best_effort(db, agent, "contradiction_dismiss", source_id).await;
    Ok(wenlan_types::ContradictionDismissResponse {
        source_id: source_id.to_string(),
        wrote: true,
    })
}

fn is_valid_snake_case_relation(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::NoopEmitter;
    use std::sync::Arc;

    // Serialize env-var-sensitive tests to avoid races.
    // Uses tokio::sync::Mutex so the guard can safely span .await points.
    async fn env_lock() -> tokio::sync::MutexGuard<'static, ()> {
        static ENV_MUTEX: tokio::sync::OnceCell<tokio::sync::Mutex<()>> =
            tokio::sync::OnceCell::const_new();
        ENV_MUTEX
            .get_or_init(|| async { tokio::sync::Mutex::new(()) })
            .await
            .lock()
            .await
    }

    async fn test_db() -> (MemoryDB, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = MemoryDB::new(&path, Arc::new(NoopEmitter)).await.unwrap();
        (db, dir)
    }

    #[tokio::test]
    async fn create_entity_rejects_empty_name() {
        let (db, _dir) = test_db().await;
        let req = CreateEntityRequest {
            name: "".to_string(),
            entity_type: "person".to_string(),
            space: None,
            source_agent: Some("test".to_string()),
            confidence: None,
        };
        assert!(matches!(
            create_entity(&db, req, "test").await,
            Err(WenlanError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn create_entity_rejects_empty_type() {
        let (db, _dir) = test_db().await;
        let req = CreateEntityRequest {
            name: "Alice".to_string(),
            entity_type: "".to_string(),
            space: None,
            source_agent: Some("test".to_string()),
            confidence: None,
        };
        assert!(matches!(
            create_entity(&db, req, "test").await,
            Err(WenlanError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn create_entity_rejects_out_of_range_confidence() {
        let (db, _dir) = test_db().await;
        let req = CreateEntityRequest {
            name: "Alice".to_string(),
            entity_type: "person".to_string(),
            space: None,
            source_agent: Some("test".to_string()),
            confidence: Some(1.5),
        };
        assert!(matches!(
            create_entity(&db, req, "test").await,
            Err(WenlanError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn create_entity_happy_path_returns_id() {
        let (db, _dir) = test_db().await;
        let req = CreateEntityRequest {
            name: "Alice".to_string(),
            entity_type: "person".to_string(),
            space: None,
            source_agent: Some("test".to_string()),
            confidence: Some(0.9),
        };
        let result = create_entity(&db, req, "test").await.unwrap();
        assert!(!result.id.is_empty());
    }

    #[tokio::test]
    async fn create_entity_resolves_to_existing_by_name() {
        let (db, _dir) = test_db().await;
        let req1 = CreateEntityRequest {
            name: "Alice".to_string(),
            entity_type: "person".to_string(),
            space: None,
            source_agent: Some("test".to_string()),
            confidence: None,
        };
        let first = create_entity(&db, req1, "test").await.unwrap();
        let req2 = CreateEntityRequest {
            name: "Alice".to_string(),
            entity_type: "person".to_string(),
            space: None,
            source_agent: Some("test".to_string()),
            confidence: None,
        };
        let second = create_entity(&db, req2, "test").await.unwrap();
        assert_eq!(first.id, second.id);
    }

    #[tokio::test]
    async fn create_relation_rejects_missing_from_entity() {
        let (db, _dir) = test_db().await;
        let req = CreateRelationRequest {
            from_entity: "missing-1".to_string(),
            to_entity: "missing-2".to_string(),
            relation_type: "knows".to_string(),
            source_agent: Some("test".to_string()),
            confidence: None,
            explanation: None,
            source_memory_id: None,
        };
        assert!(matches!(
            create_relation(&db, req, "test").await,
            Err(WenlanError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn create_relation_rejects_bad_relation_type() {
        let (db, _dir) = test_db().await;
        let alice = db
            .store_entity("Alice", "person", None, Some("test"), None)
            .await
            .unwrap();
        let bob = db
            .store_entity("Bob", "person", None, Some("test"), None)
            .await
            .unwrap();
        let req = CreateRelationRequest {
            from_entity: alice,
            to_entity: bob,
            relation_type: "Knows!".to_string(),
            source_agent: Some("test".to_string()),
            confidence: None,
            explanation: None,
            source_memory_id: None,
        };
        assert!(matches!(
            create_relation(&db, req, "test").await,
            Err(WenlanError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn create_relation_happy_path() {
        let (db, _dir) = test_db().await;
        let alice = db
            .store_entity("Alice", "person", None, Some("test"), None)
            .await
            .unwrap();
        let bob = db
            .store_entity("Bob", "person", None, Some("test"), None)
            .await
            .unwrap();
        let req = CreateRelationRequest {
            from_entity: alice,
            to_entity: bob,
            relation_type: "knows".to_string(),
            source_agent: Some("test".to_string()),
            confidence: None,
            explanation: None,
            source_memory_id: None,
        };
        let result = create_relation(&db, req, "test").await.unwrap();
        assert!(!result.id.is_empty());
    }

    #[tokio::test]
    async fn create_relation_idempotent_no_double_log() {
        let (db, _dir) = test_db().await;
        let alice = db
            .store_entity("Alice", "person", None, Some("test"), None)
            .await
            .unwrap();
        let bob = db
            .store_entity("Bob", "person", None, Some("test"), None)
            .await
            .unwrap();
        let req1 = CreateRelationRequest {
            from_entity: alice.clone(),
            to_entity: bob.clone(),
            relation_type: "knows".to_string(),
            source_agent: Some("test".to_string()),
            confidence: None,
            explanation: None,
            source_memory_id: None,
        };
        let first = create_relation(&db, req1, "agent-x").await.unwrap();
        let req2 = CreateRelationRequest {
            from_entity: alice,
            to_entity: bob,
            relation_type: "knows".to_string(),
            source_agent: Some("test".to_string()),
            confidence: None,
            explanation: None,
            source_memory_id: None,
        };
        let second = create_relation(&db, req2, "agent-x").await.unwrap();
        // Idempotent re-post must resolve to the same relation id.
        // The second call returns early before logging, so no duplicate activity row.
        assert_eq!(
            first.id, second.id,
            "should resolve to existing relation id"
        );
        assert!(
            second.warnings.is_empty(),
            "idempotent resolve should have no warnings"
        );
    }

    #[tokio::test]
    async fn create_relation_conflict_auto_supersedes_existing() {
        let (db, _dir) = test_db().await;
        let alice = db
            .store_entity("Alice", "person", None, Some("test"), None)
            .await
            .unwrap();
        let bob = db
            .store_entity("Bob", "person", None, Some("test"), None)
            .await
            .unwrap();

        // Create existing relation: A-knows-B
        let req_knows = CreateRelationRequest {
            from_entity: alice.clone(),
            to_entity: bob.clone(),
            relation_type: "knows".to_string(),
            source_agent: Some("test".to_string()),
            confidence: None,
            explanation: None,
            source_memory_id: None,
        };
        let knows_result = create_relation(&db, req_knows, "test-agent").await.unwrap();
        let knows_id = knows_result.id.clone();

        // Create conflicting relation: A-likes-B (different type, same pair)
        let req_likes = CreateRelationRequest {
            from_entity: alice.clone(),
            to_entity: bob.clone(),
            relation_type: "likes".to_string(),
            source_agent: Some("test".to_string()),
            confidence: None,
            explanation: None,
            source_memory_id: None,
        };
        let likes_result = create_relation(&db, req_likes, "test-agent").await.unwrap();

        // Warning should indicate auto-supersede
        assert!(
            likes_result
                .warnings
                .iter()
                .any(|w| w.contains("auto-superseded existing relation")),
            "expected auto-supersede warning, got: {:?}",
            likes_result.warnings
        );

        // Activity log should contain relation_supersede_auto entry
        let activity = db.list_agent_activity(50, None, None).await.unwrap();
        assert!(
            activity
                .iter()
                .any(|a| a.action == "relation_supersede_auto"),
            "expected relation_supersede_auto in activity log"
        );

        // The old knows relation should be gone (superseded / deleted)
        let active = db.list_relations_between(&alice, &bob).await.unwrap();
        let active_ids: Vec<&str> = active.iter().map(|(id, _)| id.as_str()).collect();
        assert!(
            !active_ids.contains(&knows_id.as_str()),
            "old knows relation should be archived/deleted"
        );
        assert!(
            active_ids.contains(&likes_result.id.as_str()),
            "new likes relation should be active"
        );

        // No relation_conflict proposal should have been inserted
        let pending = db.get_pending_refinements().await.unwrap();
        assert!(
            !pending.iter().any(|p| p.action == "relation_conflict"),
            "no relation_conflict proposal should be queued"
        );
    }

    #[tokio::test]
    async fn create_relation_conflict_payload_contains_archived_snapshot() {
        let (db, _dir) = test_db().await;
        let alice = db
            .store_entity("Alice", "person", None, Some("test"), None)
            .await
            .unwrap();
        let bob = db
            .store_entity("Bob", "person", None, Some("test"), None)
            .await
            .unwrap();

        // Existing relation carries full metadata that hard-delete would lose.
        let req_knows = CreateRelationRequest {
            from_entity: alice.clone(),
            to_entity: bob.clone(),
            relation_type: "knows".to_string(),
            source_agent: Some("test".to_string()),
            confidence: Some(0.72),
            explanation: Some("met at offsite".to_string()),
            source_memory_id: Some("mem_seed".to_string()),
        };
        let knows_id = create_relation(&db, req_knows, "test-agent")
            .await
            .unwrap()
            .id;

        // Conflicting different-type relation triggers auto-supersede.
        let req_likes = CreateRelationRequest {
            from_entity: alice.clone(),
            to_entity: bob.clone(),
            relation_type: "likes".to_string(),
            source_agent: Some("test".to_string()),
            confidence: None,
            explanation: None,
            source_memory_id: None,
        };
        create_relation(&db, req_likes, "test-agent").await.unwrap();

        let activity = db.list_agent_activity(50, None, None).await.unwrap();
        let entry = activity
            .iter()
            .find(|a| a.action == "relation_supersede_auto")
            .expect("relation_supersede_auto activity entry");

        let detail = entry.detail.as_ref().expect("payload detail present");
        let payload: serde_json::Value = serde_json::from_str(detail).expect("payload is JSON");
        let archived = &payload["archived"];
        assert_eq!(archived["id"], serde_json::json!(knows_id));
        assert_eq!(archived["relation_type"], serde_json::json!("knows"));
        assert_eq!(archived["confidence"], serde_json::json!(0.72));
        assert_eq!(archived["explanation"], serde_json::json!("met at offsite"));
        assert_eq!(archived["source_memory_id"], serde_json::json!("mem_seed"));
        assert_eq!(archived["source_agent"], serde_json::json!("test"));
        assert!(
            archived["created_at"].is_i64(),
            "archived created_at present"
        );
    }

    #[tokio::test]
    async fn create_relation_conflict_no_op_when_existing_same_type() {
        let (db, _dir) = test_db().await;
        let alice = db
            .store_entity("Alice", "person", None, Some("test"), None)
            .await
            .unwrap();
        let bob = db
            .store_entity("Bob", "person", None, Some("test"), None)
            .await
            .unwrap();

        // Create A-knows-B
        let req1 = CreateRelationRequest {
            from_entity: alice.clone(),
            to_entity: bob.clone(),
            relation_type: "knows".to_string(),
            source_agent: Some("test".to_string()),
            confidence: None,
            explanation: None,
            source_memory_id: None,
        };
        let first = create_relation(&db, req1, "test-agent").await.unwrap();

        // Create A-knows-B again (same type → idempotent early return)
        let req2 = CreateRelationRequest {
            from_entity: alice.clone(),
            to_entity: bob.clone(),
            relation_type: "knows".to_string(),
            source_agent: Some("test".to_string()),
            confidence: None,
            explanation: None,
            source_memory_id: None,
        };
        let second = create_relation(&db, req2, "test-agent").await.unwrap();

        // Should resolve to same id, no supersede warning
        assert_eq!(first.id, second.id, "idempotent call should return same id");
        assert!(
            !second
                .warnings
                .iter()
                .any(|w| w.contains("auto-superseded")),
            "no supersede warning expected for same-type idempotent call"
        );

        // No relation_supersede_auto activity
        let activity = db.list_agent_activity(50, None, None).await.unwrap();
        assert!(
            !activity
                .iter()
                .any(|a| a.action == "relation_supersede_auto"),
            "no relation_supersede_auto activity expected for same-type call"
        );
    }

    #[tokio::test]
    async fn create_relation_no_conflict_when_no_existing_relation() {
        let (db, _dir) = test_db().await;
        let alice = db
            .store_entity("Alice", "person", None, Some("test"), None)
            .await
            .unwrap();
        let bob = db
            .store_entity("Bob", "person", None, Some("test"), None)
            .await
            .unwrap();

        // First relation — no prior relation exists
        let req = CreateRelationRequest {
            from_entity: alice.clone(),
            to_entity: bob.clone(),
            relation_type: "likes".to_string(),
            source_agent: Some("test".to_string()),
            confidence: None,
            explanation: None,
            source_memory_id: None,
        };
        let result = create_relation(&db, req, "test-agent").await.unwrap();

        assert!(!result.id.is_empty());
        assert!(
            !result
                .warnings
                .iter()
                .any(|w| w.contains("auto-superseded") || w.contains("supersede")),
            "no supersede warning expected when no prior relation exists"
        );

        // No relation_supersede_auto activity
        let activity = db.list_agent_activity(50, None, None).await.unwrap();
        assert!(
            !activity
                .iter()
                .any(|a| a.action == "relation_supersede_auto"),
            "no relation_supersede_auto activity expected on first relation create"
        );
    }

    #[tokio::test]
    async fn add_observation_rejects_missing_entity() {
        let (db, _dir) = test_db().await;
        let req = AddObservationRequest {
            entity_id: "no-such-entity".to_string(),
            content: "Alice prefers Rust".to_string(),
            source_agent: Some("test".to_string()),
            confidence: None,
        };
        assert!(matches!(
            add_observation(&db, req, "test").await,
            Err(WenlanError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn add_observation_rejects_short_content() {
        let (db, _dir) = test_db().await;
        let alice = db
            .store_entity("Alice", "person", None, Some("test"), None)
            .await
            .unwrap();
        let req = AddObservationRequest {
            entity_id: alice,
            content: "hi".to_string(),
            source_agent: Some("test".to_string()),
            confidence: None,
        };
        assert!(matches!(
            add_observation(&db, req, "test").await,
            Err(WenlanError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn add_observation_happy_path() {
        let (db, _dir) = test_db().await;
        let alice = db
            .store_entity("Alice", "person", None, Some("test"), None)
            .await
            .unwrap();
        let req = AddObservationRequest {
            entity_id: alice.clone(),
            content: "Alice prefers Rust over Python".to_string(),
            source_agent: Some("test".to_string()),
            confidence: Some(0.9),
        };
        let result = add_observation(&db, req, "test").await.unwrap();
        assert!(!result.id.is_empty());

        // Verify the observation was actually persisted
        let observations = db
            .get_observations_for_entities(&[alice], 10)
            .await
            .unwrap();
        assert_eq!(observations.len(), 1);
        assert!(observations[0].content.contains("Alice prefers Rust"));
    }

    // ── create_page ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn create_page_rejects_missing_source_memory() {
        let (db, _dir) = test_db().await;
        let req = CreateConceptRequest {
            title: "Some Page".to_string(),
            content: "body content that is long enough".to_string(),
            summary: None,
            entity_id: None,
            space: None,
            source_memory_ids: vec!["mem_does_not_exist".to_string()],
            creation_kind: None,
            workspace: None,
        };
        assert!(matches!(
            create_page(&db, req, "test", None).await,
            Err(WenlanError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn create_page_rejects_hallucinated_body() {
        let (db, _dir) = test_db().await;
        // Seed a memory about Rust
        let doc = crate::sources::RawDocument {
            source: "memory".to_string(),
            source_id: "mem-rust".to_string(),
            title: "mem-rust".to_string(),
            content: "Rust is a systems programming language".to_string(),
            last_modified: chrono::Utc::now().timestamp(),
            memory_type: Some("fact".to_string()),
            source_agent: Some("test".to_string()),
            confidence: Some(0.9),
            ..Default::default()
        };
        db.upsert_documents(vec![doc]).await.unwrap();
        let req = CreateConceptRequest {
            title: "Cooking".to_string(),
            content: "Pasta carbonara needs eggs and pancetta".to_string(),
            summary: None,
            entity_id: None,
            space: None,
            source_memory_ids: vec!["mem-rust".to_string()],
            creation_kind: None,
            workspace: None,
        };
        // Hallucination guard should reject (cos sim < 0.6)
        assert!(matches!(
            create_page(&db, req, "test", None).await,
            Err(WenlanError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn create_page_happy_path() {
        let (db, _dir) = test_db().await;
        // Seed a memory about Rust
        let doc = crate::sources::RawDocument {
            source: "memory".to_string(),
            source_id: "mem-rust-happy".to_string(),
            title: "mem-rust-happy".to_string(),
            content: "Rust is a systems programming language with memory safety guarantees"
                .to_string(),
            last_modified: chrono::Utc::now().timestamp(),
            memory_type: Some("fact".to_string()),
            source_agent: Some("test".to_string()),
            confidence: Some(0.9),
            ..Default::default()
        };
        db.upsert_documents(vec![doc]).await.unwrap();
        let req = CreateConceptRequest {
            title: "Rust".to_string(),
            content: "Rust is a systems programming language providing memory safety guarantees"
                .to_string(),
            summary: Some("memory-safe systems language".to_string()),
            entity_id: None,
            space: None,
            source_memory_ids: vec!["mem-rust-happy".to_string()],
            creation_kind: None,
            workspace: None,
        };
        let result = create_page(&db, req, "test", None).await.unwrap();
        assert!(result.id.starts_with("page_"));
    }

    // ── update_page ──────────────────────────────────────────────────────────

    /// Helper: seed a memory and return its source_id.
    async fn seed_memory(db: &MemoryDB, source_id: &str, content: &str) {
        let doc = crate::sources::RawDocument {
            source: "memory".to_string(),
            source_id: source_id.to_string(),
            title: source_id.to_string(),
            content: content.to_string(),
            last_modified: chrono::Utc::now().timestamp(),
            memory_type: Some("fact".to_string()),
            source_agent: Some("test".to_string()),
            confidence: Some(0.9),
            ..Default::default()
        };
        db.upsert_documents(vec![doc]).await.unwrap();
    }

    /// Helper: create a page via create_page for an existing memory, return page id.
    async fn seed_page(db: &MemoryDB, source_id: &str, content: &str) -> String {
        let req = CreateConceptRequest {
            title: format!("Page {source_id}"),
            content: content.to_string(),
            summary: None,
            entity_id: None,
            space: None,
            source_memory_ids: vec![source_id.to_string()],
            creation_kind: None,
            workspace: None,
        };
        create_page(db, req, "test", None).await.unwrap().id
    }

    #[tokio::test]
    async fn update_page_round_trip() {
        let (db, _dir) = test_db().await;
        let mem_id = "mem-rpt-1";
        let content_v1 = "Rust is a systems language with memory safety";
        seed_memory(&db, mem_id, content_v1).await;
        let page_id = seed_page(&db, mem_id, content_v1).await;

        // First update → version=2
        let content_v2 = "Rust is a systems language with memory safety and zero-cost abstractions";
        let req2 = UpdatePageRequest {
            content: content_v2.to_string(),
            source_memory_ids: vec![mem_id.to_string()],
        };
        let r2 = update_page(&db, &page_id, req2, "re_distill", false, None)
            .await
            .unwrap();
        assert_eq!(r2.id, page_id);

        // Second update → version=3
        let content_v3 = "Rust is a systems language with memory safety, zero-cost abstractions and concurrency without data races";
        let req3 = UpdatePageRequest {
            content: content_v3.to_string(),
            source_memory_ids: vec![mem_id.to_string()],
        };
        let r3 = update_page(&db, &page_id, req3, "re_distill", false, None)
            .await
            .unwrap();

        // Check page version=3
        let page = db.get_page(&page_id).await.unwrap().unwrap();
        assert_eq!(page.version, 3);

        // Changelog has 2 entries (v1→v2 and v2→v3)
        let cl = db.get_page_changelog(&page_id).await.unwrap();
        let entries: Vec<serde_json::Value> = serde_json::from_str(&cl).unwrap();
        assert_eq!(entries.len(), 2, "expected 2 changelog entries");
        assert!(
            !r3.warnings.is_empty(),
            "warnings should carry delta summary"
        );
    }

    #[tokio::test]
    async fn update_page_cas_skips_when_not_stale() {
        let (db, _dir) = test_db().await;
        let mem_id = "mem-cas-skip";
        let content = "Rust is a systems language with memory safety";
        seed_memory(&db, mem_id, content).await;
        let page_id = seed_page(&db, mem_id, content).await;

        // Page has no stale_reason — CAS with require_stale=true should skip
        let req = UpdatePageRequest {
            content: "Rust is a systems language with memory safety and performance".to_string(),
            source_memory_ids: vec![mem_id.to_string()],
        };
        let result = update_page(&db, &page_id, req, "re_distill", true, None)
            .await
            .unwrap();

        // Version unchanged (page stays at v1)
        let page = db.get_page(&page_id).await.unwrap().unwrap();
        assert_eq!(page.version, 1, "version should not change when CAS skips");
        assert!(!result.wrote, "wrote must be false when CAS skips");
        assert!(result.warnings.is_empty(), "no warnings on CAS skip");
    }

    #[tokio::test]
    async fn update_page_cas_writes_when_stale() {
        let (db, _dir) = test_db().await;
        let mem_id = "mem-cas-write";
        let content = "Rust is a systems language with memory safety";
        seed_memory(&db, mem_id, content).await;
        let page_id = seed_page(&db, mem_id, content).await;

        // Mark page stale
        db.set_page_stale(&page_id, "source_updated").await.unwrap();

        // CAS with require_stale=true should write when stale
        let new_content = "Rust is a systems language with memory safety and ownership model";
        let req = UpdatePageRequest {
            content: new_content.to_string(),
            source_memory_ids: vec![mem_id.to_string()],
        };
        let result = update_page(&db, &page_id, req, "re_distill", true, None)
            .await
            .unwrap();

        let page = db.get_page(&page_id).await.unwrap().unwrap();
        assert_eq!(page.version, 2, "version should bump on CAS write");
        assert!(result.wrote, "wrote must be true on CAS write");
        assert!(
            !result.warnings.is_empty(),
            "warnings should carry delta summary"
        );
    }

    #[tokio::test]
    async fn update_page_hallucination_guard_manual_edit_rejects() {
        let (db, _dir) = test_db().await;
        let mem_id = "mem-guard-reject";
        let rust_content = "Rust is a systems programming language with memory safety";
        seed_memory(&db, mem_id, rust_content).await;
        let page_id = seed_page(&db, mem_id, rust_content).await;

        // Body completely unrelated to the Rust memory source
        let req = UpdatePageRequest {
            content: "Pasta carbonara needs eggs pancetta and pecorino romano cheese".to_string(),
            source_memory_ids: vec![mem_id.to_string()],
        };
        let result = update_page(&db, &page_id, req, "manual_edit", false, None).await;
        assert!(
            matches!(result, Err(WenlanError::Validation(_))),
            "hallucination guard should reject manual_edit with unrelated body"
        );
    }

    #[tokio::test]
    async fn update_page_skip_guard_re_distill() {
        let (db, _dir) = test_db().await;
        let mem_id = "mem-guard-skip";
        let rust_content = "Rust is a systems programming language with memory safety";
        seed_memory(&db, mem_id, rust_content).await;
        let page_id = seed_page(&db, mem_id, rust_content).await;

        // Same unrelated body — but re_distill skips the guard
        let req = UpdatePageRequest {
            content: "Pasta carbonara needs eggs pancetta and pecorino romano cheese".to_string(),
            source_memory_ids: vec![mem_id.to_string()],
        };
        // Should succeed without hallucination check
        update_page(&db, &page_id, req, "re_distill", false, None)
            .await
            .unwrap();
        let page = db.get_page(&page_id).await.unwrap().unwrap();
        assert_eq!(page.version, 2);
    }

    #[tokio::test]
    async fn update_page_user_edit_flag_set() {
        let (db, _dir) = test_db().await;
        let mem_id = "mem-flag-test";
        let content = "Rust is a systems language with memory safety features and ownership";
        seed_memory(&db, mem_id, content).await;
        let page_id = seed_page(&db, mem_id, content).await;

        // fs_edit should set user_edited=1
        let req = UpdatePageRequest {
            content:
                "Rust is a systems language with memory safety features, ownership and borrowing"
                    .to_string(),
            source_memory_ids: vec![mem_id.to_string()],
        };
        update_page(&db, &page_id, req, "fs_edit", false, None)
            .await
            .unwrap();
        let page = db.get_page(&page_id).await.unwrap().unwrap();
        assert!(page.user_edited, "user_edited should be true for fs_edit");
    }

    #[tokio::test]
    async fn update_page_fs_edit_with_nonexistent_source_succeeds() {
        // Regression: update_page must not reject fs_edit (or any daemon-internal
        // caller) when source_memory_ids references a memory that no longer exists.
        // The source list is carried forward from the existing page; re-validating
        // on update would break page_watcher for pages whose sources were pruned.
        // Insert the page directly (bypassing create_page validation) to simulate
        // a page whose source was valid at creation but since pruned.
        let (db, _dir) = test_db().await;
        let ghost_source = "mem-ghost-pruned";
        let now = chrono::Utc::now().to_rfc3339();
        let page_id = "page_ghost_src_test";
        db.insert_page(
            page_id,
            "Ghost Source Page",
            None,
            "Rust is a systems language with memory safety",
            None,
            None,
            &[ghost_source],
            &now,
        )
        .await
        .unwrap();

        // fs_edit carrying the non-existent source id must succeed.
        let req = UpdatePageRequest {
            content: "Rust is a systems language with memory safety (user edited)".to_string(),
            source_memory_ids: vec![ghost_source.to_string()],
        };
        update_page(&db, page_id, req, "fs_edit", false, None)
            .await
            .unwrap();
        let page = db.get_page(page_id).await.unwrap().unwrap();
        assert_eq!(page.version, 2);
    }

    #[tokio::test]
    async fn update_page_warnings_carry_delta() {
        let (db, _dir) = test_db().await;
        let mem_id = "mem-warn-delta";
        let content = "Rust is a systems language";
        seed_memory(&db, mem_id, content).await;
        let page_id = seed_page(&db, mem_id, content).await;

        let new_content = "Rust is a systems language with memory safety and zero-cost abstractions for high performance systems programming";
        let req = UpdatePageRequest {
            content: new_content.to_string(),
            source_memory_ids: vec![mem_id.to_string()],
        };
        let result = update_page(&db, &page_id, req, "re_distill", false, None)
            .await
            .unwrap();

        assert!(
            !result.warnings.is_empty(),
            "warnings should be non-empty when content changes"
        );
        let warning = &result.warnings[0];
        assert!(
            warning.contains("v1") && warning.contains("v2"),
            "warning should reference version transition, got: {warning}"
        );
    }

    #[tokio::test]
    async fn update_page_idempotent_warnings_shape() {
        let (db, _dir) = test_db().await;
        let mem_id = "mem-idem-shape";
        let content_v1 = "Rust is a systems language with ownership model";
        seed_memory(&db, mem_id, content_v1).await;
        let page_id = seed_page(&db, mem_id, content_v1).await;

        // First call: v1 → v2
        let r1 = update_page(
            &db,
            &page_id,
            UpdatePageRequest {
                content: "Rust is a systems language with ownership model and borrow checker"
                    .to_string(),
                source_memory_ids: vec![mem_id.to_string()],
            },
            "re_distill",
            false,
            None,
        )
        .await
        .unwrap();
        assert!(r1.wrote, "first call should write");
        assert_eq!(
            r1.warnings.len(),
            1,
            "first call should produce exactly one warning"
        );
        let w1 = &r1.warnings[0];
        assert!(w1.starts_with('v'), "warning must start with 'v': {w1}");
        assert!(w1.contains('→'), "warning must contain '→': {w1}");
        assert!(
            w1.contains("v1") && w1.contains("v2"),
            "first warning should show v1 → v2: {w1}"
        );

        // Second call with different content: v2 → v3
        let r2 = update_page(
            &db,
            &page_id,
            UpdatePageRequest {
                content:
                    "Rust is a systems language with ownership model, borrow checker, and lifetimes"
                        .to_string(),
                source_memory_ids: vec![mem_id.to_string()],
            },
            "re_distill",
            false,
            None,
        )
        .await
        .unwrap();
        assert!(r2.wrote, "second call should write");
        assert_eq!(
            r2.warnings.len(),
            1,
            "second call should produce exactly one warning"
        );
        let w2 = &r2.warnings[0];
        assert!(w2.starts_with('v'), "warning must start with 'v': {w2}");
        assert!(w2.contains('→'), "warning must contain '→': {w2}");
        assert!(
            w2.contains("v2") && w2.contains("v3"),
            "second warning should show v2 → v3: {w2}"
        );
    }

    #[tokio::test]
    async fn update_page_noop_returns_wrote_false() {
        let (db, _dir) = test_db().await;
        let mem_id = "mem-noop-1";
        let content = "Rust is a systems language with memory safety";
        seed_memory(&db, mem_id, content).await;
        let page_id = seed_page(&db, mem_id, content).await;

        // Fetch baseline version before no-op call
        let page_before = db.get_page(&page_id).await.unwrap().unwrap();
        let version_before = page_before.version;

        // Call update_page with identical content and identical sources
        let req = UpdatePageRequest {
            content: content.to_string(),
            source_memory_ids: vec![mem_id.to_string()],
        };
        let result = update_page(&db, &page_id, req, "re_distill", false, None)
            .await
            .unwrap();

        assert!(!result.wrote, "identical-content call must not write");
        assert!(result.warnings.is_empty(), "no-op must produce no warnings");

        // Version must be unchanged
        let page_after = db.get_page(&page_id).await.unwrap().unwrap();
        assert_eq!(
            page_after.version, version_before,
            "version must not bump on no-op"
        );
    }

    // ── accept_pending_revision ──────────────────────────────────────────────

    async fn seed_pending_revision(db: &MemoryDB, target: &str, revision: &str) {
        let now = chrono::Utc::now().timestamp();
        let conn = db.conn.lock().await;
        conn.execute(
            "INSERT INTO memories (id, source_id, title, content, chunk_index, chunk_type, memory_type, space, source_agent, created_at, last_modified, confirmed, stability, source) VALUES (?1, ?1, ?1, 'original content', 0, 'text', 'fact', 'test', 'claude-code', ?2, ?2, 1, 'confirmed', 'memory')",
            libsql::params![target.to_string(), now],
        )
        .await
        .unwrap();
        conn.execute(
            "INSERT INTO memories (id, source_id, title, content, chunk_index, chunk_type, memory_type, space, source_agent, created_at, last_modified, confirmed, stability, source, supersedes, pending_revision) VALUES (?1, ?1, ?1, 'revised content', 0, 'text', 'fact', 'test', 'claude-code', ?2, ?2, 0, 'new', 'memory', ?3, 1)",
            libsql::params![revision.to_string(), now, target.to_string()],
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn accept_pending_revision_writes_and_logs_on_first_call() {
        let (db, _tmp) = crate::db::tests::test_db().await;
        seed_pending_revision(&db, "mem_apr_target", "mem_apr_rev").await;
        let result = accept_pending_revision(&db, "mem_apr_target", "test-agent")
            .await
            .unwrap();
        assert_eq!(result.target_source_id, "mem_apr_target");
        assert_eq!(result.revision_source_id, "mem_apr_rev");
        assert!(result.wrote);
    }

    #[tokio::test]
    async fn accept_pending_revision_returns_not_found_on_missing_id() {
        let (db, _tmp) = crate::db::tests::test_db().await;
        let err = accept_pending_revision(&db, "mem_nope", "test-agent")
            .await
            .unwrap_err();
        assert!(matches!(err, WenlanError::NotFound(_)));
    }

    #[tokio::test]
    async fn accept_pending_revision_returns_not_found_on_re_call_after_success() {
        let (db, _tmp) = crate::db::tests::test_db().await;
        seed_pending_revision(&db, "mem_arr_target", "mem_arr_rev").await;
        accept_pending_revision(&db, "mem_arr_target", "test-agent")
            .await
            .unwrap();
        let err = accept_pending_revision(&db, "mem_arr_target", "test-agent")
            .await
            .unwrap_err();
        assert!(matches!(err, WenlanError::NotFound(_)));
    }

    // ── dismiss_pending_revision ─────────────────────────────────────────────

    #[tokio::test]
    async fn dismiss_pending_revision_writes_and_logs_on_first_call() {
        let (db, _tmp) = crate::db::tests::test_db().await;
        seed_pending_revision(&db, "mem_dpr_target", "mem_dpr_rev").await;
        let result = dismiss_pending_revision(&db, "mem_dpr_target", "test-agent")
            .await
            .unwrap();
        assert_eq!(result.target_source_id, "mem_dpr_target");
        assert!(result.wrote);
    }

    #[tokio::test]
    async fn dismiss_pending_revision_returns_not_found_on_missing_id() {
        let (db, _tmp) = crate::db::tests::test_db().await;
        let err = dismiss_pending_revision(&db, "mem_nope", "test-agent")
            .await
            .unwrap_err();
        assert!(matches!(err, WenlanError::NotFound(_)));
    }

    #[tokio::test]
    async fn dismiss_pending_revision_returns_not_found_on_re_call() {
        let (db, _tmp) = crate::db::tests::test_db().await;
        seed_pending_revision(&db, "mem_dpr2_target", "mem_dpr2_rev").await;
        dismiss_pending_revision(&db, "mem_dpr2_target", "test-agent")
            .await
            .unwrap();
        let err = dismiss_pending_revision(&db, "mem_dpr2_target", "test-agent")
            .await
            .unwrap_err();
        assert!(matches!(err, WenlanError::NotFound(_)));
    }

    // ── dismiss_contradiction ────────────────────────────────────────────────

    #[tokio::test]
    async fn dismiss_contradiction_writes_and_returns_wrote_true() {
        let (db, _tmp) = crate::db::tests::test_db().await;
        let result = dismiss_contradiction(&db, "mem_any_source_id", "test-agent")
            .await
            .unwrap();
        assert_eq!(result.source_id, "mem_any_source_id");
        assert!(result.wrote);
    }

    #[tokio::test]
    async fn dismiss_contradiction_logs_activity_once_per_call() {
        let (db, _tmp) = crate::db::tests::test_db().await;
        dismiss_contradiction(&db, "mem_one", "test-agent")
            .await
            .unwrap();
        let conn = db.conn.lock().await;
        let mut rows = conn
            .query(
                "SELECT COUNT(*) FROM agent_activity WHERE action = 'contradiction_dismiss' AND memory_ids = 'mem_one'",
                libsql::params![],
            )
            .await
            .unwrap();
        let row = rows.next().await.unwrap().unwrap();
        let count: i64 = row.get(0).unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn dismiss_contradiction_swallows_no_rows_matched() {
        let (db, _tmp) = crate::db::tests::test_db().await;
        // No contradiction rows seeded — DB method is silent-idempotent
        let result = dismiss_contradiction(&db, "mem_no_contradictions", "test-agent")
            .await
            .unwrap();
        assert!(
            result.wrote,
            "wrote=true even with no rows matched (best-effort signal per §3 caveat)"
        );
    }

    // ── T16: MinHash/LSH entity near-dedup cascade (Step 2.5) ────────────────
    //
    // Test pair "Vorpalblade Jabberwock Inc" / "Vorpalblade Jabberwock Ino" is chosen so the char-trigram
    // Jaccard is >= 0.9 (MinHash auto-merges) while the BGE vector distance is
    // ~0.13 (> 0.1, so the existing vector step does NOT merge them). That
    // separation is what lets the flag-OFF noop test prove byte-identity:
    // without MinHash these stay two distinct entities.

    fn entity_req(name: &str, etype: &str) -> CreateEntityRequest {
        CreateEntityRequest {
            name: name.to_string(),
            entity_type: etype.to_string(),
            space: None,
            source_agent: Some("test".to_string()),
            confidence: None,
        }
    }

    #[tokio::test]
    async fn create_entity_minhash_merges_abbreviation() {
        temp_env::async_with_vars([("WENLAN_ENABLE_ENTITY_MINHASH", Some("1"))], async {
            let (db, _dir) = test_db().await;
            let first = create_entity(
                &db,
                entity_req("Vorpalblade Jabberwock Inc", "project"),
                "test",
            )
            .await
            .unwrap();
            let second = create_entity(
                &db,
                entity_req("Vorpalblade Jabberwock Ino", "project"),
                "test",
            )
            .await
            .unwrap();
            assert_eq!(
                first.id, second.id,
                "near-dup must resolve to the first entity id"
            );
            assert!(
                !second.wrote,
                "resolved-existing must not write a new entity"
            );
            // A "minhash" alias must have been recorded for the second name.
            let resolved = db
                .resolve_entity_by_alias(&"Vorpalblade Jabberwock Ino".to_lowercase())
                .await
                .unwrap();
            assert_eq!(resolved, Some(first.id));
        })
        .await;
    }

    #[tokio::test]
    async fn create_entity_minhash_respects_type_guard() {
        temp_env::async_with_vars([("WENLAN_ENABLE_ENTITY_MINHASH", Some("1"))], async {
            let (db, _dir) = test_db().await;
            let first = create_entity(
                &db,
                entity_req("Vorpalblade Jabberwock Inc", "project"),
                "test",
            )
            .await
            .unwrap();
            // Same near-dup name but a DIFFERENT entity type must not auto-merge.
            let second = create_entity(
                &db,
                entity_req("Vorpalblade Jabberwock Ino", "person"),
                "test",
            )
            .await
            .unwrap();
            assert_ne!(
                first.id, second.id,
                "cross-type near-dup must NOT auto-merge (same-type guard)"
            );
            assert!(second.wrote, "a new entity should have been created");
        })
        .await;
    }

    #[tokio::test]
    async fn create_entity_minhash_short_name_skips_fuzzy() {
        temp_env::async_with_vars([("WENLAN_ENABLE_ENTITY_MINHASH", Some("1"))], async {
            let (db, _dir) = test_db().await;
            // "API"/"APIs" are below the entropy gate, so Step 2.5 must punt them
            // to the vector step and never record a "minhash" alias.
            let _ = create_entity(&db, entity_req("API", "concept"), "test")
                .await
                .unwrap();
            let _ = create_entity(&db, entity_req("APIs", "concept"), "test")
                .await
                .unwrap();
            // No band rows are written for low-entropy names, regardless of how
            // the vector step resolved them.
            let conn = db.conn.lock().await;
            let mut rows = conn
                .query("SELECT COUNT(*) FROM entity_minhash_bands", ())
                .await
                .unwrap();
            let band_count: i64 = rows.next().await.unwrap().unwrap().get(0).unwrap();
            assert_eq!(
                band_count, 0,
                "low-entropy names must not be indexed into entity_minhash_bands"
            );
            drop(rows);
            let mut arows = conn
                .query(
                    "SELECT COUNT(*) FROM entity_aliases WHERE source = 'minhash'",
                    (),
                )
                .await
                .unwrap();
            let minhash_aliases: i64 = arows.next().await.unwrap().unwrap().get(0).unwrap();
            assert_eq!(
                minhash_aliases, 0,
                "short names must not produce a minhash alias"
            );
        })
        .await;
    }

    #[tokio::test]
    async fn create_entity_minhash_disabled_is_noop() {
        // CRITICAL regression guard: with the flag OFF, the near-dup pair must
        // stay TWO separate entities (vector distance ~0.13 > 0.1 so the vector
        // step does not merge them), and NO minhash alias / band row is written.
        temp_env::async_with_vars([("WENLAN_ENABLE_ENTITY_MINHASH", None::<&str>)], async {
            let (db, _dir) = test_db().await;
            let first = create_entity(
                &db,
                entity_req("Vorpalblade Jabberwock Inc", "project"),
                "test",
            )
            .await
            .unwrap();
            let second = create_entity(
                &db,
                entity_req("Vorpalblade Jabberwock Ino", "project"),
                "test",
            )
            .await
            .unwrap();
            assert_ne!(
                first.id, second.id,
                "flag OFF must leave near-dups as distinct entities (byte-identity)"
            );
            assert!(second.wrote, "flag OFF must create a second entity");
            let conn = db.conn.lock().await;
            let mut rows = conn
                .query("SELECT COUNT(*) FROM entity_minhash_bands", ())
                .await
                .unwrap();
            let band_count: i64 = rows.next().await.unwrap().unwrap().get(0).unwrap();
            assert_eq!(band_count, 0, "flag OFF must write zero band rows");
            drop(rows);
            let mut arows = conn
                .query(
                    "SELECT COUNT(*) FROM entity_aliases WHERE source = 'minhash'",
                    (),
                )
                .await
                .unwrap();
            let minhash_aliases: i64 = arows.next().await.unwrap().unwrap().get(0).unwrap();
            assert_eq!(
                minhash_aliases, 0,
                "flag OFF must write zero minhash aliases"
            );
        })
        .await;
    }

    #[tokio::test]
    async fn resolve_entity_bulk_minhash_mirrors_create_entity() {
        use crate::extract::ExtractedEntity;
        use std::collections::HashMap;
        temp_env::async_with_vars([("WENLAN_ENABLE_ENTITY_MINHASH", Some("1"))], async {
            let (db, _dir) = test_db().await;
            let mut cache: HashMap<String, String> = HashMap::new();
            let e1 = ExtractedEntity {
                name: "Vorpalblade Jabberwock Inc".to_string(),
                entity_type: "project".to_string(),
            };
            let (id1, new1) = crate::importer::resolve_entity_bulk(&db, &mut cache, &e1, "test")
                .await
                .unwrap();
            assert!(new1, "first bulk entity is newly created");
            // Fresh cache so the in-batch shortcut does not mask Step 2.5.
            let mut cache2: HashMap<String, String> = HashMap::new();
            let e2 = ExtractedEntity {
                name: "Vorpalblade Jabberwock Ino".to_string(),
                entity_type: "project".to_string(),
            };
            let (id2, new2) = crate::importer::resolve_entity_bulk(&db, &mut cache2, &e2, "test")
                .await
                .unwrap();
            assert_eq!(id1, id2, "bulk path must mirror create_entity merge");
            assert!(!new2, "bulk near-dup must resolve to existing, not create");
        })
        .await;
    }

    // Integration tests: update_page shrink-guard

    #[tokio::test]
    async fn update_page_shrink_guard_rejects_truncation() {
        let _lock = env_lock().await;
        // Guard ON + LLM-rewrite caller + body shrinks below threshold -> Err + DB unchanged
        std::env::set_var("WENLAN_MERGE_SHRINK_GUARD", "0.7");
        let (db, _dir) = test_db().await;
        let mem_id = "mem-sg-reject";
        // 100-char body
        let old_body = "a".repeat(100);
        seed_memory(&db, mem_id, &old_body).await;
        let page_id = seed_page(&db, mem_id, &old_body).await;

        // New body is only 60 chars: 60 < 100 * 0.7 = 70 -> should reject
        let short_body = "a".repeat(60);
        let req = UpdatePageRequest {
            content: short_body,
            source_memory_ids: vec![mem_id.to_string()],
        };
        let result = update_page(&db, &page_id, req, "distill", false, None).await;
        assert!(
            matches!(result, Err(WenlanError::Validation(_))),
            "shrink-guard must reject truncated LLM rewrite"
        );

        // DB must still have the ORIGINAL body
        let page = db.get_page(&page_id).await.unwrap().unwrap();
        assert_eq!(
            page.content,
            "a".repeat(100),
            "body must be unchanged after rejection"
        );
        assert_eq!(page.version, 1, "version must not bump on rejection");
        std::env::remove_var("WENLAN_MERGE_SHRINK_GUARD");
    }

    #[tokio::test]
    async fn update_page_shrink_guard_allows_growth() {
        let _lock = env_lock().await;
        // Guard ON + LLM-rewrite caller + body grows -> Ok
        std::env::set_var("WENLAN_MERGE_SHRINK_GUARD", "0.7");
        let (db, _dir) = test_db().await;
        let mem_id = "mem-sg-grow";
        let old_body = "a".repeat(50);
        seed_memory(&db, mem_id, &old_body).await;
        let page_id = seed_page(&db, mem_id, &old_body).await;

        let long_body = "a".repeat(200);
        let req = UpdatePageRequest {
            content: long_body.clone(),
            source_memory_ids: vec![mem_id.to_string()],
        };
        let result = update_page(&db, &page_id, req, "page_growth", false, None).await;
        assert!(result.is_ok(), "shrink-guard must allow growing body");
        let page = db.get_page(&page_id).await.unwrap().unwrap();
        assert_eq!(page.content, long_body);
        std::env::remove_var("WENLAN_MERGE_SHRINK_GUARD");
    }

    #[tokio::test]
    async fn update_page_shrink_guard_off_by_default() {
        let _lock = env_lock().await;
        // Guard UNSET: even extreme truncation must succeed (zero regression)
        std::env::remove_var("WENLAN_MERGE_SHRINK_GUARD");
        let (db, _dir) = test_db().await;
        let mem_id = "mem-sg-off";
        let old_body = "a".repeat(100);
        seed_memory(&db, mem_id, &old_body).await;
        let page_id = seed_page(&db, mem_id, &old_body).await;

        let tiny_body = "a".repeat(5); // 5 < 100 * 0.7 = 70, would fail if guard were ON
        let req = UpdatePageRequest {
            content: tiny_body.clone(),
            source_memory_ids: vec![mem_id.to_string()],
        };
        let result = update_page(&db, &page_id, req, "distill", false, None)
            .await
            .unwrap();
        assert!(result.wrote, "guard OFF must allow any size update");
        let page = db.get_page(&page_id).await.unwrap().unwrap();
        assert_eq!(
            page.content, tiny_body,
            "content must update when guard is OFF"
        );
    }

    #[tokio::test]
    async fn update_page_shrink_guard_skips_human_edits() {
        let _lock = env_lock().await;
        // Guard ON + human edited_by: guard never fires, update goes through
        std::env::set_var("WENLAN_MERGE_SHRINK_GUARD", "0.7");
        let (db, _dir) = test_db().await;
        let mem_id = "mem-sg-human";
        let old_body = "a".repeat(100);
        seed_memory(&db, mem_id, &old_body).await;
        let page_id = seed_page(&db, mem_id, &old_body).await;

        // 5 chars: would fail guard if LLM rewrite, but "manual_edit" is human
        let tiny_body = "a".repeat(5);
        let req = UpdatePageRequest {
            content: tiny_body.clone(),
            source_memory_ids: vec![mem_id.to_string()],
        };
        // manual_edit bypasses hallucination guard AND is NOT an LLM rewrite
        // so shrink-guard must NOT fire even though the body shrinks drastically
        // (hallucination guard WILL fire for manual_edit -- seed with real-ish content)
        // Actually manual_edit triggers hallucination guard, so use fs_edit instead
        let result = update_page(&db, &page_id, req, "fs_edit", false, None).await;
        // fs_edit IS guarded by hallucination guard and will likely fail cos-sim check.
        // The key assertion: if it fails, it must NOT be a shrink-guard Validation error.
        // If it succeeds, the body must be updated.
        match result {
            Ok(wr) => {
                // Succeeded: body updated (hallucination guard passed)
                if wr.wrote {
                    let page = db.get_page(&page_id).await.unwrap().unwrap();
                    assert_eq!(page.content, tiny_body);
                }
            }
            Err(WenlanError::Validation(msg)) => {
                // Hallucination guard may reject: ensure it is NOT a shrink-guard message
                assert!(
                    !msg.contains("shrink-guard"),
                    "human edit must not be rejected by shrink-guard; got: {msg}"
                );
            }
            Err(e) => panic!("unexpected error: {e:?}"),
        }
        std::env::remove_var("WENLAN_MERGE_SHRINK_GUARD");
    }

    // merge_shrink_threshold parse tests

    #[test]
    fn merge_shrink_threshold_unset_returns_none() {
        std::env::remove_var("WENLAN_MERGE_SHRINK_GUARD");
        assert!(merge_shrink_threshold().is_none());
    }

    #[test]
    fn merge_shrink_threshold_valid_float() {
        std::env::set_var("WENLAN_MERGE_SHRINK_GUARD", "0.7");
        assert_eq!(merge_shrink_threshold(), Some(0.7));
        std::env::remove_var("WENLAN_MERGE_SHRINK_GUARD");
    }

    #[test]
    fn merge_shrink_threshold_garbage_returns_none() {
        std::env::set_var("WENLAN_MERGE_SHRINK_GUARD", "garbage");
        assert!(merge_shrink_threshold().is_none());
        std::env::remove_var("WENLAN_MERGE_SHRINK_GUARD");
    }

    // is_llm_rewrite tests

    #[test]
    fn is_llm_rewrite_distill_true() {
        assert!(is_llm_rewrite("distill"));
        assert!(is_llm_rewrite("re_distill"));
        assert!(is_llm_rewrite("page_growth"));
        assert!(is_llm_rewrite("refinery_merge"));
    }

    #[test]
    fn is_llm_rewrite_user_false() {
        assert!(!is_llm_rewrite("user"));
        assert!(!is_llm_rewrite("manual_edit"));
        assert!(!is_llm_rewrite("fs_edit"));
        assert!(!is_llm_rewrite("api"));
        assert!(!is_llm_rewrite(""));
    }
}
