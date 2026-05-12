// SPDX-License-Identifier: Apache-2.0
//! Canonical write-path capability functions. Each fn owns the full create
//! flow for one kind: validation, resolve-or-create (where applicable),
//! storage primitive call, post-write enrichment (verify, log, refinery
//! enqueue). Both HTTP route handlers and daemon-internal extractors call
//! these -- eliminating drift between agent-LLM and daemon-LLM trigger paths.

use crate::db::MemoryDB;
use crate::error::OriginError;
use origin_types::requests::{
    AddObservationRequest, CreateConceptRequest, CreateEntityRequest, CreateRelationRequest,
    UpdatePageRequest,
};
use std::path::Path;

#[derive(Debug, Clone, serde::Serialize)]
pub struct WriteResult {
    pub id: String,
    pub warnings: Vec<String>,
    pub wrote: bool,
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
) -> Result<WriteResult, OriginError> {
    // Pre-write validation
    let name = req.name.trim();
    if name.is_empty() {
        return Err(OriginError::Validation(
            "entity name must not be empty".into(),
        ));
    }
    let entity_type = req.entity_type.trim();
    if entity_type.is_empty() {
        return Err(OriginError::Validation(
            "entity_type must not be empty".into(),
        ));
    }
    if let Some(c) = req.confidence {
        if !(0.0..=1.0).contains(&c) {
            return Err(OriginError::Validation(format!(
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
            req.domain.as_deref(),
            req.source_agent.as_deref(),
            req.confidence,
        )
        .await?;

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
) -> Result<WriteResult, OriginError> {
    // Pre-write validation
    if !db.entity_exists(&req.from_entity).await? {
        return Err(OriginError::Validation(format!(
            "from_entity '{}' does not exist",
            req.from_entity
        )));
    }
    if !db.entity_exists(&req.to_entity).await? {
        return Err(OriginError::Validation(format!(
            "to_entity '{}' does not exist",
            req.to_entity
        )));
    }
    let rt = req.relation_type.trim();
    if !is_valid_snake_case_relation(rt) {
        return Err(OriginError::Validation(format!(
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

    // Conflict check: existing relation between same (from, to) with different type
    if let Ok(existing) = db
        .list_relations_between(&req.from_entity, &req.to_entity)
        .await
    {
        for (existing_id, existing_type) in &existing {
            if existing_id != &id && existing_type != rt {
                let id_short: String = id.chars().take(8).collect();
                let exid_short: String = existing_id.chars().take(8).collect();
                let proposal_id = format!("rel_conflict_{id_short}_{exid_short}");
                let payload = serde_json::json!({
                    "existing_id": existing_id,
                    "new_id": id,
                    "from": req.from_entity,
                    "to": req.to_entity,
                    "old_type": existing_type,
                    "new_type": rt,
                })
                .to_string();
                let _ = db
                    .insert_refinement_proposal(
                        &proposal_id,
                        "relation_conflict",
                        &[id.clone(), existing_id.clone()],
                        Some(&payload),
                        0.7,
                    )
                    .await;
                warnings.push(format!(
                    "conflicting relation exists ({}-{}-{}); enqueued for review",
                    req.from_entity, existing_type, req.to_entity
                ));
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
) -> Result<WriteResult, OriginError> {
    // Pre-write validation
    if !db.entity_exists(&req.entity_id).await? {
        return Err(OriginError::Validation(format!(
            "entity_id '{}' does not exist",
            req.entity_id
        )));
    }
    let content = req.content.trim();
    if content.chars().count() < 5 {
        return Err(OriginError::Validation(
            "observation content must be at least 5 characters".into(),
        ));
    }
    if let Some(c) = req.confidence {
        if !(0.0..=1.0).contains(&c) {
            return Err(OriginError::Validation(format!(
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
) -> Result<WriteResult, OriginError> {
    // Pre-write validation
    if req.title.trim().is_empty() {
        return Err(OriginError::Validation(
            "page title must not be empty".into(),
        ));
    }
    if req.content.trim().is_empty() {
        return Err(OriginError::Validation(
            "page content must not be empty".into(),
        ));
    }
    if req.source_memory_ids.is_empty() {
        return Err(OriginError::Validation(
            "page must cite at least one source memory".into(),
        ));
    }
    // Resolution check: every source id must exist
    for sid in &req.source_memory_ids {
        if db.get_memory_detail(sid).await?.is_none() {
            return Err(OriginError::Validation(format!(
                "source memory '{sid}' does not exist"
            )));
        }
    }
    // Hallucination guard
    let passed =
        crate::kg_quality::hallucination_guard(db, &req.content, &req.source_memory_ids).await?;
    if !passed {
        return Err(OriginError::Validation(
            "page body diverges from cited sources (cos sim < 0.6)".into(),
        ));
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
        domain: req.domain.clone(),
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
    };

    // md-first write (only if a knowledge_path was provided)
    let writer_opt =
        knowledge_path.map(|p| crate::export::knowledge::KnowledgeWriter::new(p.to_path_buf()));
    if let Some(ref writer) = writer_opt {
        writer
            .write_page(&page)
            .map_err(|e| OriginError::VectorDb(format!("write_page: {e}")))?;
    }

    // DB index
    let source_refs: Vec<&str> = req.source_memory_ids.iter().map(|s| s.as_str()).collect();
    if let Err(e) = db
        .insert_page(
            &id,
            &req.title,
            req.summary.as_deref(),
            &req.content,
            req.entity_id.as_deref(),
            req.domain.as_deref(),
            &source_refs,
            &now,
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
        return Err(OriginError::VectorDb(e.to_string()));
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
) -> Result<WriteResult, OriginError> {
    // ── Pre-write validation ────────────────────────────────────────────────
    if req.content.trim().is_empty() {
        return Err(OriginError::Validation(
            "page content must not be empty".into(),
        ));
    }
    if req.source_memory_ids.is_empty() {
        return Err(OriginError::Validation(
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
            return Err(OriginError::Validation(
                "page body diverges from cited sources (cos sim < 0.6)".into(),
            ));
        }
    }

    // ── Load current page for delta computation ─────────────────────────────
    let current = db
        .get_page(page_id)
        .await?
        .ok_or_else(|| OriginError::Validation(format!("page '{page_id}' does not exist")))?;
    let current_version = current.version;
    let new_version = current_version + 1;

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
            domain: None,
            source_agent: Some("test".to_string()),
            confidence: None,
        };
        assert!(matches!(
            create_entity(&db, req, "test").await,
            Err(OriginError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn create_entity_rejects_empty_type() {
        let (db, _dir) = test_db().await;
        let req = CreateEntityRequest {
            name: "Alice".to_string(),
            entity_type: "".to_string(),
            domain: None,
            source_agent: Some("test".to_string()),
            confidence: None,
        };
        assert!(matches!(
            create_entity(&db, req, "test").await,
            Err(OriginError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn create_entity_rejects_out_of_range_confidence() {
        let (db, _dir) = test_db().await;
        let req = CreateEntityRequest {
            name: "Alice".to_string(),
            entity_type: "person".to_string(),
            domain: None,
            source_agent: Some("test".to_string()),
            confidence: Some(1.5),
        };
        assert!(matches!(
            create_entity(&db, req, "test").await,
            Err(OriginError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn create_entity_happy_path_returns_id() {
        let (db, _dir) = test_db().await;
        let req = CreateEntityRequest {
            name: "Alice".to_string(),
            entity_type: "person".to_string(),
            domain: None,
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
            domain: None,
            source_agent: Some("test".to_string()),
            confidence: None,
        };
        let first = create_entity(&db, req1, "test").await.unwrap();
        let req2 = CreateEntityRequest {
            name: "Alice".to_string(),
            entity_type: "person".to_string(),
            domain: None,
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
            Err(OriginError::Validation(_))
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
            Err(OriginError::Validation(_))
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
            Err(OriginError::Validation(_))
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
            Err(OriginError::Validation(_))
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
            domain: None,
            source_memory_ids: vec!["mem_does_not_exist".to_string()],
        };
        assert!(matches!(
            create_page(&db, req, "test", None).await,
            Err(OriginError::Validation(_))
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
            domain: None,
            source_memory_ids: vec!["mem-rust".to_string()],
        };
        // Hallucination guard should reject (cos sim < 0.6)
        assert!(matches!(
            create_page(&db, req, "test", None).await,
            Err(OriginError::Validation(_))
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
            domain: None,
            source_memory_ids: vec!["mem-rust-happy".to_string()],
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
            domain: None,
            source_memory_ids: vec![source_id.to_string()],
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
            matches!(result, Err(OriginError::Validation(_))),
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
}
