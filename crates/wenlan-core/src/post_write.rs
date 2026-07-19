// SPDX-License-Identifier: Apache-2.0
//! Canonical write-path capability functions. Each fn owns the full create
//! flow for one kind: validation, resolve-or-create (where applicable),
//! storage primitive call, post-write enrichment (verify, log, refinery
//! enqueue). Both HTTP route handlers and daemon-internal extractors call
//! these -- eliminating drift between agent-LLM and daemon-LLM trigger paths.

use crate::db::MemoryDB;
use crate::error::WenlanError;
use std::{collections::HashSet, path::Path, str::FromStr};
use wenlan_types::{
    requests::{
        AddObservationRequest, CreateConceptRequest, CreateEntityRequest, CreateRelationRequest,
        UpdatePageRequest,
    },
    MemoryType, RawDocument,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WriteResult {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attached_to: Option<String>,
    pub warnings: Vec<String>,
    pub wrote: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision_card_id: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub gated: bool,
    /// Why the write did or did not land. `wrote` says whether the page moved;
    /// this says why not, which is what a caller needs to answer the user.
    ///
    /// Branch on this, never on `warnings` — the strings are for humans reading
    /// logs and will get reworded.
    #[serde(default)]
    pub outcome: WriteOutcome,
}

/// The distinguishable ends of a page write.
///
/// Without this, every unsuccessful write looked the same to a caller: "content
/// was already correct" and "somebody else holds the page" both arrived as
/// `wrote: false` and an empty warning list. The manual-edit route guessed
/// conflict for both and told users their routine no-op save had been
/// overwritten by someone else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteOutcome {
    /// The page was updated.
    ///
    /// Default because the only `WriteResult`s that predate this field are the
    /// ones stored in idempotency receipts, and a receipt is only ever written
    /// inside a transaction that committed — so a receipt without an outcome
    /// describes a write that landed.
    #[default]
    Wrote,
    /// The page already said what the caller asked for, or the caller asked to
    /// write only if stale and it wasn't. Nothing to do, and nothing wrong.
    Unchanged,
    /// The caller declared an `expected_version` that no longer matches. Its
    /// content was computed against a version that has moved on.
    Refused,
    /// The page kept moving and the write lost every CAS attempt. The caller's
    /// content was discarded.
    Contended,
    /// A machine write to a human-owned page, preserved as a revision card
    /// rather than applied. See `revision_card_id`.
    Gated,
}

#[derive(Debug, Clone, Copy)]
pub struct MemoryUpdate<'a> {
    pub content: Option<&'a str>,
    pub space: Option<Option<&'a str>>,
    pub confirm: bool,
    pub memory_type: Option<&'a str>,
}

pub async fn update_memory(
    db: &MemoryDB,
    source_id: &str,
    update: MemoryUpdate<'_>,
) -> Result<(), WenlanError> {
    let parsed_memory_type = update
        .memory_type
        .map(MemoryType::from_str)
        .transpose()
        .map_err(WenlanError::Validation)?;
    let normalized_memory_type = parsed_memory_type.map(|memory_type| memory_type.to_string());

    db.apply_memory_update(
        source_id,
        update.content,
        update.space,
        update.confirm,
        normalized_memory_type.as_deref(),
    )
    .await
}

fn is_false(value: &bool) -> bool {
    !*value
}

const VALID_PAGE_CREATION_KINDS: [&str; 5] =
    ["distilled", "authored", "research", "imported", "source"];
const PAGE_BIRTH_REVIEW_STATUS: &str = "unconfirmed";

pub enum PageWrite<'a> {
    Attach {
        page_id: &'a str,
        source_memory_ids: &'a [String],
        link_reason: &'a str,
        agent: &'a str,
    },
    Create {
        page_id: Option<&'a str>,
        req: CreateConceptRequest,
        agent: &'a str,
        knowledge_path: Option<&'a Path>,
        page_min_cluster_size: usize,
        page_match_threshold: f64,
        citations_json: Option<String>,
    },
    Update {
        page_id: &'a str,
        req: UpdatePageRequest,
        edited_by: &'a str,
        require_stale: bool,
        knowledge_path: Option<&'a Path>,
        citations: Option<(String, String)>,
    },
    ReplaceSource {
        page_id: &'a str,
        title: &'a str,
        summary: Option<&'a str>,
        content: &'a str,
        source_memory_ids: &'a [String],
        agent: &'a str,
    },
}

pub async fn page_write(db: &MemoryDB, write: PageWrite<'_>) -> Result<WriteResult, WenlanError> {
    match write {
        PageWrite::Attach {
            page_id,
            source_memory_ids,
            link_reason,
            agent,
        } => attach_page_sources_impl(db, page_id, source_memory_ids, link_reason, agent).await,
        PageWrite::Create {
            page_id,
            req,
            agent,
            knowledge_path,
            page_min_cluster_size,
            page_match_threshold,
            citations_json,
        } => {
            create_page_impl(
                db,
                CreatePageInput {
                    page_id,
                    req,
                    agent,
                    knowledge_path,
                    page_min_cluster_size,
                    page_match_threshold,
                    citations_json: citations_json.as_deref(),
                },
            )
            .await
        }
        PageWrite::Update {
            page_id,
            req,
            edited_by,
            require_stale,
            knowledge_path,
            citations,
        } => {
            update_page_impl(
                db,
                page_id,
                req,
                edited_by,
                require_stale,
                knowledge_path,
                citations,
            )
            .await
        }
        PageWrite::ReplaceSource {
            page_id,
            title,
            summary,
            content,
            source_memory_ids,
            agent,
        } => {
            replace_source_page_impl(
                db,
                page_id,
                title,
                summary,
                content,
                source_memory_ids,
                agent,
            )
            .await
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn replace_source_page_impl(
    db: &MemoryDB,
    page_id: &str,
    title: &str,
    summary: Option<&str>,
    content: &str,
    source_memory_ids: &[String],
    agent: &str,
) -> Result<WriteResult, WenlanError> {
    if title.trim().is_empty() || content.trim().is_empty() || source_memory_ids.is_empty() {
        return Err(WenlanError::Validation(
            "source Page replacement requires title, content, and source ids".into(),
        ));
    }
    let current = db
        .get_page(page_id)
        .await?
        .ok_or_else(|| WenlanError::NotFound(format!("Page not found: {page_id}")))?;
    if current.creation_kind != "source" || current.user_edited {
        return Err(WenlanError::Conflict(format!(
            "source Page {page_id} is not machine-owned"
        )));
    }

    // Nothing to write: this enrichment recomputed exactly what the page
    // already says. Mirrors the same early return on the Update path, and
    // matters more here — a document whose analysis LLM is unreachable rebuilds
    // a byte-identical stub body on every retry, and without this each retry
    // would bump the version and append another identical history row.
    //
    // Compared against the *sanitized* body because that is the form the row
    // holds; comparing the raw one would report a spurious change forever.
    let sanitized = crate::export::provenance::sanitize_ingress_content(content);
    let stored_sources: HashSet<&str> = current
        .source_memory_ids
        .iter()
        .map(String::as_str)
        .collect();
    let incoming_sources: HashSet<&str> = source_memory_ids.iter().map(String::as_str).collect();
    if current.title == title
        && current.summary.as_deref() == summary
        && current.content == sanitized
        && stored_sources == incoming_sources
    {
        return Ok(WriteResult {
            id: page_id.to_string(),
            attached_to: None,
            warnings: Vec::new(),
            wrote: false,
            revision_card_id: None,
            gated: false,
            outcome: WriteOutcome::Unchanged,
        });
    }

    let source_refs: Vec<&str> = source_memory_ids.iter().map(String::as_str).collect();
    if !db
        .replace_source_page(page_id, title, summary, content, &source_refs, agent)
        .await?
    {
        return Err(WenlanError::Conflict(format!(
            "source Page {page_id} changed ownership before replacement"
        )));
    }
    let detail = format!("title={title}, sources={}", source_memory_ids.len());
    if let Err(error) = db
        .log_agent_activity(agent, "page_update", source_memory_ids, None, &detail)
        .await
    {
        log::warn!("[replace_source_page] activity log failed: {error}");
    }
    Ok(WriteResult {
        id: page_id.to_string(),
        attached_to: None,
        warnings: Vec::new(),
        wrote: true,
        revision_card_id: None,
        gated: false,
        outcome: WriteOutcome::Wrote,
    })
}

async fn attach_page_sources_impl(
    db: &MemoryDB,
    page_id: &str,
    source_memory_ids: &[String],
    link_reason: &str,
    agent: &str,
) -> Result<WriteResult, WenlanError> {
    for sid in source_memory_ids {
        db.link_page_source(page_id, sid, link_reason).await?;
    }
    log_activity_best_effort(db, agent, "page_attach", page_id).await;
    Ok(WriteResult {
        id: page_id.to_string(),
        attached_to: Some(page_id.to_string()),
        warnings: vec![],
        wrote: true,
        revision_card_id: None,
        gated: false,
        outcome: WriteOutcome::Wrote,
    })
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
            attached_to: None,
            warnings: vec![],
            wrote: false,
            revision_card_id: None,
            gated: false,
            outcome: WriteOutcome::Unchanged,
        });
    }

    // Step 2: Exact name search
    let name_results = db.search_entities_by_name(name).await?;
    if let Some(existing) = name_results.first() {
        let _ = db.add_entity_alias(&name_lower, &existing.id, "auto").await;
        return Ok(WriteResult {
            id: existing.id.clone(),
            attached_to: None,
            warnings: vec![],
            wrote: false,
            revision_card_id: None,
            gated: false,
            outcome: WriteOutcome::Unchanged,
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
                attached_to: None,
                warnings: vec![],
                wrote: false,
                revision_card_id: None,
                gated: false,
                outcome: WriteOutcome::Unchanged,
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
                attached_to: None,
                warnings: vec![],
                wrote: false,
                revision_card_id: None,
                gated: false,
                outcome: WriteOutcome::Unchanged,
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
        attached_to: None,
        warnings,
        wrote: true,
        revision_card_id: None,
        gated: false,
        outcome: WriteOutcome::Wrote,
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
                attached_to: None,
                warnings: vec![],
                wrote: false,
                revision_card_id: None,
                gated: false,
                outcome: WriteOutcome::Unchanged,
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
        attached_to: None,
        warnings,
        wrote: true,
        revision_card_id: None,
        gated: false,
        outcome: WriteOutcome::Wrote,
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
        attached_to: None,
        warnings: vec![],
        wrote: true,
        revision_card_id: None,
        gated: false,
        outcome: WriteOutcome::Wrote,
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
    let distillation = crate::tuning::DistillationConfig::default();
    create_page_with_tuning(
        db,
        req,
        agent,
        knowledge_path,
        distillation.page_min_cluster_size,
        distillation.page_match_threshold,
    )
    .await
}

pub async fn create_page_with_floor(
    db: &MemoryDB,
    req: CreateConceptRequest,
    agent: &str,
    knowledge_path: Option<&Path>,
    page_min_cluster_size: usize,
) -> Result<WriteResult, WenlanError> {
    create_page_with_tuning(
        db,
        req,
        agent,
        knowledge_path,
        page_min_cluster_size,
        crate::tuning::DistillationConfig::default().page_match_threshold,
    )
    .await
}

pub async fn create_page_with_tuning(
    db: &MemoryDB,
    req: CreateConceptRequest,
    agent: &str,
    knowledge_path: Option<&Path>,
    page_min_cluster_size: usize,
    page_match_threshold: f64,
) -> Result<WriteResult, WenlanError> {
    page_write(
        db,
        PageWrite::Create {
            page_id: None,
            req,
            agent,
            knowledge_path,
            page_min_cluster_size,
            page_match_threshold,
            citations_json: None,
        },
    )
    .await
}

async fn page_source_reference_exists(
    db: &MemoryDB,
    creation_kind: &str,
    source_id: &str,
) -> Result<bool, WenlanError> {
    if db.get_memory_detail(source_id).await?.is_some() {
        return Ok(true);
    }
    if creation_kind != "source" {
        return Ok(false);
    }
    let conn = db.conn.lock().await;
    let mut rows = conn
        .query(
            "SELECT 1 FROM memories WHERE id = ?1 AND pending_revision = 0 AND source != 'episode' LIMIT 1",
            libsql::params![source_id],
        )
        .await
        .map_err(|e| WenlanError::VectorDb(format!("source page chunk lookup: {e}")))?;
    rows.next()
        .await
        .map(|row| row.is_some())
        .map_err(|e| WenlanError::VectorDb(format!("source page chunk lookup row: {e}")))
}

struct CreatePageInput<'a> {
    page_id: Option<&'a str>,
    req: CreateConceptRequest,
    agent: &'a str,
    knowledge_path: Option<&'a Path>,
    page_min_cluster_size: usize,
    page_match_threshold: f64,
    citations_json: Option<&'a str>,
}

async fn create_page_impl(
    db: &MemoryDB,
    input: CreatePageInput<'_>,
) -> Result<WriteResult, WenlanError> {
    let CreatePageInput {
        page_id,
        req,
        agent,
        knowledge_path,
        page_min_cluster_size,
        page_match_threshold,
        citations_json,
    } = input;

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
    if !VALID_PAGE_CREATION_KINDS.contains(&creation_kind) {
        return Err(WenlanError::Validation(format!(
            "invalid creation_kind '{creation_kind}' (expected one of: distilled, authored, research, imported, source)"
        )));
    }
    if creation_kind == "source" && page_id.is_none() {
        return Err(WenlanError::Validation(
            "source page requires a deterministic page id".into(),
        ));
    }
    if creation_kind == "source" && req.source_memory_ids.is_empty() {
        return Err(WenlanError::Validation(
            "source page must cite at least one source memory".into(),
        ));
    }
    let distinct_source_count = req
        .source_memory_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>()
        .len();
    if creation_kind == "distilled" && distinct_source_count < page_min_cluster_size {
        return Err(WenlanError::Validation(format!(
            "distilled page requires at least {page_min_cluster_size} distinct source memories (got {distinct_source_count})"
        )));
    }
    let review_status = PAGE_BIRTH_REVIEW_STATUS;
    // Resolution check: every source id must exist
    for sid in &req.source_memory_ids {
        if !page_source_reference_exists(db, creation_kind, sid).await? {
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

    if creation_kind == "distilled" {
        let embed_text =
            crate::pages::page_embedding_text(&req.title, req.summary.as_deref(), &req.content);
        let embedding = match db.generate_embeddings(&[embed_text]) {
            Ok(mut embeddings) => embeddings.pop(),
            Err(e) => {
                log::warn!("[create_page] dedup embedding failed: {e}");
                None
            }
        };
        let workspace = req.workspace.as_deref().or(req.space.as_deref());
        if let (Some(embedding), Some(workspace)) = (embedding, workspace) {
            if let Some(matched) = db
                .find_matching_page_scoped(
                    req.entity_id.as_deref(),
                    &embedding,
                    page_match_threshold,
                    Some(workspace),
                    false,
                )
                .await?
            {
                let matched_id = matched.id;
                return attach_page_sources_impl(
                    db,
                    &matched_id,
                    &req.source_memory_ids,
                    "page_write_attach",
                    agent,
                )
                .await;
            }
        }
    }

    // Build page
    let id = page_id
        .map(str::to_string)
        .unwrap_or_else(crate::pages::new_page_id);
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
        pending_rebuild: None,
        user_edited: false,
        relevance_score: 0.0,
        last_edited_by: None,
        last_edited_at: None,
        last_delta_summary: None,
        changelog: None,
        creation_kind: creation_kind.to_string(),
        review_status: review_status.to_string(),
        workspace: req.workspace.clone(),
        citations: citations_json
            .and_then(|raw| serde_json::from_str(raw).ok())
            .unwrap_or_default(),
    };

    // md-first write (only if a knowledge_path was provided)
    let projection = knowledge_path.map(|path| {
        crate::export::knowledge::KnowledgeProjectionWrite::new(path.to_path_buf(), db)
    });
    if let Some(ref projection) = projection {
        projection
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
            citations_json,
        )
        .await
    {
        // Rollback md if it was written
        if let Some(ref projection) = projection {
            if let Err(rb) = projection.remove_page(&id) {
                log::warn!(
                    "[create_page] DB insert failed and md rollback also failed for {id}: db_err={e}, rollback_err={rb}"
                );
            }
        }
        return Err(WenlanError::VectorDb(e.to_string()));
    }
    drop(projection);

    // Post-write enrichment
    let mut warnings: Vec<String> = Vec::new();

    if creation_kind == "distilled" {
        if let Err(e) =
            crate::maintenance::emit_keep_or_archive_card(db, &id, distinct_source_count).await
        {
            log::warn!("[create_page] keep/archive card failed for {id}: {e}");
            warnings.push(format!("keep/archive card failed: {e}"));
        }
    }

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
        attached_to: None,
        warnings,
        wrote: true,
        revision_card_id: None,
        gated: false,
        outcome: WriteOutcome::Wrote,
    })
}

/// A daemon pipeline stage that rewrites page prose with an LLM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineStage<'a> {
    Distill,
    ReDistill,
    PageGrowth,
    RefineryMerge,
    /// A writer string the gate does not recognize, carried verbatim.
    ///
    /// Two kinds of string land here. Legitimate ones: `edited_by` values that
    /// reach `page_history` without passing this gate (`create` and
    /// `migration_73` are SQL literals in db.rs; `citation_backfill` and
    /// `revision_accept` write via the db layer). Illegitimate ones: a typo in
    /// a caller's literal. The gate cannot tell them apart, so both get the
    /// conservative answer — see `Writer::is_machine`.
    Unknown(&'a str),
}

/// Who is writing a page, as the spec's `human | agent | pipeline(stage)`.
///
/// This is a **lens over the persisted string, not a replacement for it**.
/// Every page write records provenance as text in `pages.changelog` and
/// `page_history.edited_by`, and those bytes are already in users' databases,
/// so `as_str` round-trips the exact literal that was classified.
///
/// Its job is to put the write gate's three authority questions in one place
/// with one vocabulary. They used to be three independent `matches!` lists over
/// `&str`, which could drift apart silently and had no name for the fallthrough
/// case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Writer<'a> {
    /// A person edited the page directly — in the app editor, or in the vault
    /// on disk. The only identity that may overwrite a human-owned page.
    Human(&'a str),
    /// An agent asked for a faithful re-synth of a page it already owns.
    Agent(&'a str),
    /// A daemon pipeline stage.
    Pipeline(PipelineStage<'a>),
}

impl<'a> Writer<'a> {
    /// Total over every `&str` — there is no failure mode, only `Unknown`.
    pub fn classify(edited_by: &'a str) -> Self {
        match edited_by {
            "manual_edit" | "fs_edit" => Writer::Human(edited_by),
            "agent_refresh" => Writer::Agent(edited_by),
            "distill" => Writer::Pipeline(PipelineStage::Distill),
            "re_distill" => Writer::Pipeline(PipelineStage::ReDistill),
            "page_growth" => Writer::Pipeline(PipelineStage::PageGrowth),
            "refinery_merge" => Writer::Pipeline(PipelineStage::RefineryMerge),
            other => Writer::Pipeline(PipelineStage::Unknown(other)),
        }
    }

    /// The persisted `edited_by` literal, byte-identical to what was classified.
    pub fn as_str(&self) -> &'a str {
        match self {
            Writer::Human(s) | Writer::Agent(s) => s,
            Writer::Pipeline(stage) => match stage {
                PipelineStage::Distill => "distill",
                PipelineStage::ReDistill => "re_distill",
                PipelineStage::PageGrowth => "page_growth",
                PipelineStage::RefineryMerge => "refinery_merge",
                PipelineStage::Unknown(s) => s,
            },
        }
    }

    /// Everything that is not a human. **Unknown counts as machine on purpose**:
    /// that is the fail-safe direction, because a machine write to a
    /// human-owned page is staged as a revision card rather than overwriting
    /// the human's prose. Trusting an unrecognized string as human would let it
    /// clobber that prose instead.
    fn is_machine(&self) -> bool {
        !matches!(self, Writer::Human(_))
    }

    /// Writers that bypass the hallucination guard. Incremental updates can push
    /// aggregate cosine sim below 0.6; the HTTP/MCP `agent_refresh` route
    /// historically accepted agent-provided refreshes without this guard, so
    /// routing it through PageWrite preserves that behavior.
    ///
    /// This is `is_llm_rewrite` plus `Agent` — the two used to be parallel
    /// `matches!` lists whose only difference was the `agent_refresh` arm, with
    /// a comment warning not to merge them. Here the difference is structural.
    fn skips_hallucination_guard(&self) -> bool {
        matches!(self, Writer::Agent(_)) || self.is_llm_rewrite()
    }

    /// A recognized LLM-rewrite stage, checked by the shrink-guard. `Unknown` is
    /// excluded: the shrink-guard rejects a write outright, so an unrecognized
    /// writer must not be able to opt itself into being rejected.
    fn is_llm_rewrite(&self) -> bool {
        matches!(
            self,
            Writer::Pipeline(
                PipelineStage::Distill
                    | PipelineStage::ReDistill
                    | PipelineStage::PageGrowth
                    | PipelineStage::RefineryMerge
            )
        )
    }
}

pub fn page_is_human_owned(page: &crate::pages::Page) -> bool {
    page.user_edited || page.creation_kind == "authored"
}

/// Stage a machine write to a human-owned page as a pending revision card
/// instead of overwriting the page's prose. Uses the same grammar as L3
/// doc-grounded revisions (`crate::reconcile::write_revision`): a
/// `source='memory'`, `pending_revision=1`, `supersedes=<page id>` row that
/// `list_pending_revisions` surfaces on the `/curate revisions` queue. The page
/// itself is never mutated here — the human accepts or dismisses the card.
/// Returns a gated `WriteResult` carrying the new card id.
pub async fn stage_page_revision_card(
    db: &MemoryDB,
    page: &crate::pages::Page,
    content: &str,
    source_memory_ids: &[String],
    edited_by: &str,
) -> Result<WriteResult, WenlanError> {
    let revision_card_id = format!(
        "mem_{}",
        uuid::Uuid::new_v4()
            .to_string()
            .replace('-', "")
            .chars()
            .take(12)
            .collect::<String>()
    );
    let structured = serde_json::json!({
        "revision_kind": "page_write",
        "target_kind": "page",
        "revises_page": page.id,
        "page_version": page.version,
        "edited_by": edited_by,
        "source_memory_ids": source_memory_ids,
    })
    .to_string();
    let title: String = format!("Revision: {}", page.title)
        .chars()
        .take(80)
        .collect();
    let row = RawDocument {
        source: "memory".to_string(),
        source_id: revision_card_id.clone(),
        title,
        content: content.to_string(),
        last_modified: chrono::Utc::now().timestamp(),
        memory_type: Some("fact".to_string()),
        space: page.space.clone().or_else(|| page.workspace.clone()),
        source_agent: Some("page_write".to_string()),
        confidence: Some(0.9),
        confirmed: Some(false),
        stability: Some("new".to_string()),
        supersedes: Some(page.id.clone()),
        pending_revision: true,
        structured_fields: Some(structured.clone()),
        source_text: Some(content.to_string()),
        ..Default::default()
    };
    db.upsert_documents(vec![row]).await?;
    if let Err(e) = db
        .log_agent_activity(
            edited_by,
            "page_revision_card",
            &[page.id.clone(), revision_card_id.clone()],
            None,
            &structured,
        )
        .await
    {
        log::warn!("[page_revision_card] activity log failed: {e}");
    }

    Ok(WriteResult {
        id: page.id.clone(),
        attached_to: None,
        warnings: vec![
            "human-owned page; staged revision card instead of overwriting content".to_string(),
        ],
        wrote: false,
        revision_card_id: Some(revision_card_id),
        gated: true,
        outcome: WriteOutcome::Gated,
    })
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
/// Hallucination guard runs for human writers (`manual_edit`, `fs_edit`) and
/// for any writer the gate does not recognize. `agent_refresh` and the LLM
/// rewrite stages (`distill`, `re_distill`, `page_growth`, `refinery_merge`)
/// skip it — incremental updates may push aggregate cosine sim below 0.6 and
/// would silently drop legitimate writes. See `Writer::skips_hallucination_guard`,
/// which is the single source of truth; `WRITER_TABLE` pins it per writer.
///
/// `citations`: `Some((citations_json, stats_summary))` when the caller has
/// freshly verified [N] markers against a numbered source list for this
/// exact `req.content` — persisted atomically with the content, and
/// `stats_summary` is recorded on the changelog entry. `None` always resets
/// `citations` to `'[]'` (a stale marker-to-source map must not survive a
/// content change without fresh verification).
#[allow(clippy::too_many_arguments)]
pub async fn update_page(
    db: &MemoryDB,
    page_id: &str,
    req: UpdatePageRequest,
    edited_by: &str,
    require_stale: bool,
    knowledge_path: Option<&Path>,
    citations: Option<(String, String)>,
) -> Result<WriteResult, WenlanError> {
    page_write(
        db,
        PageWrite::Update {
            page_id,
            req,
            edited_by,
            require_stale,
            knowledge_path,
            citations,
        },
    )
    .await
}

/// Test-only seam. A test installs a `(parked, go)` handshake here and
/// `update_page_impl` uses it once, *after* deciding ownership and *before*
/// writing — i.e. in the exact window a competing edit has to land in. It
/// announces that it is parked, then blocks until released, so the test can
/// land a full competing write in between with no ordering guesswork.
///
/// This is the only way to deterministically exercise the version CAS: with no
/// interleaving edit, a guarded write and an unguarded one behave identically.
///
/// One-shot (`take`), so the retry attempt after a CAS miss runs unblocked.
/// Compiled out entirely in non-test builds.
#[cfg(test)]
type PreWriteGate = (
    tokio::sync::oneshot::Sender<()>,
    tokio::sync::oneshot::Receiver<()>,
);

#[cfg(test)]
pub(crate) static PRE_WRITE_GATE: std::sync::Mutex<Option<PreWriteGate>> =
    std::sync::Mutex::new(None);

#[cfg(test)]
async fn pre_write_pause() {
    let gate = PRE_WRITE_GATE.lock().unwrap().take();
    if let Some((parked, go)) = gate {
        let _ = parked.send(());
        let _ = go.await;
    }
}

#[cfg(not(test))]
#[inline(always)]
async fn pre_write_pause() {}

#[allow(clippy::too_many_arguments)]
/// The advisory line a successful page update returns. Shared between the
/// receipt and the return value so a replay hands back exactly what the
/// original call did, rather than a lookalike rebuilt at replay time.
fn write_warnings(delta_summary: &Option<String>, from: i64, to: i64) -> Vec<String> {
    match delta_summary {
        Some(summary) => vec![format!("v{from} → v{to}: {summary}")],
        None => vec![],
    }
}

/// Fingerprint of a page write, used to tell an honest retry (same request,
/// replay the stored response) from an operation id being reused for a
/// different write (a conflict).
///
/// Covers everything that decides what the write does: which page, the body,
/// the sources, who is writing, and the version precondition. Field lengths
/// are hashed alongside the values so two different requests cannot collide by
/// shifting a boundary — `["ab","c"]` and `["a","bc"]` must not agree.
fn page_write_digest(page_id: &str, req: &UpdatePageRequest, edited_by: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    let mut field = |bytes: &[u8]| {
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    };
    field(page_id.as_bytes());
    field(req.content.as_bytes());
    field(edited_by.as_bytes());
    field(&(req.source_memory_ids.len() as u64).to_le_bytes());
    for sid in &req.source_memory_ids {
        field(sid.as_bytes());
    }
    match req.expected_version {
        Some(v) => {
            field(b"v");
            field(&v.to_le_bytes());
        }
        None => field(b"-"),
    }
    format!("{:x}", hasher.finalize())
}

async fn update_page_impl(
    db: &MemoryDB,
    page_id: &str,
    req: UpdatePageRequest,
    edited_by: &str,
    require_stale: bool,
    knowledge_path: Option<&Path>,
    citations: Option<(String, String)>,
) -> Result<WriteResult, WenlanError> {
    // ── Pre-write validation ────────────────────────────────────────────────
    if req.content.trim().is_empty() {
        return Err(WenlanError::Validation(
            "page content must not be empty".into(),
        ));
    }
    // A machine write must carry provenance. A human write need not: an
    // authored page is legitimately born with zero sources (create_page
    // allows exactly that), so demanding one here would reject every later
    // human edit of that page — in the app and in the vault alike.
    let writer = Writer::classify(edited_by);
    if req.source_memory_ids.is_empty() && writer.is_machine() {
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
    if !writer.skips_hallucination_guard() {
        let passed =
            crate::kg_quality::hallucination_guard(db, &req.content, &req.source_memory_ids)
                .await?;
        if !passed {
            return Err(WenlanError::Validation(
                "page body diverges from cited sources (cos sim < 0.6)".into(),
            ));
        }
    }

    let source_refs: Vec<&str> = req.source_memory_ids.iter().map(|s| s.as_str()).collect();
    let projection = knowledge_path.map(|path| {
        crate::export::knowledge::KnowledgeProjectionWrite::new(path.to_path_buf(), db)
    });

    // ── Retry identity ──────────────────────────────────────────────────────
    // A caller that sends both ids gets exactly-once semantics. The same pair
    // with the same request replays the recorded response without writing
    // again; the same pair with a different request is refused rather than
    // quietly becoming a second version. Either id alone is ignored — an
    // operation id only means anything within the caller that minted it.
    let retry = match (req.caller_id.as_deref(), req.operation_id.as_deref()) {
        (Some(caller), Some(operation)) if !caller.is_empty() && !operation.is_empty() => Some((
            caller.to_string(),
            operation.to_string(),
            page_write_digest(page_id, &req, edited_by),
        )),
        _ => None,
    };
    if let Some((caller, operation, digest)) = retry.as_ref() {
        if let Some(stored) = db.get_operation_receipt(caller, operation).await? {
            if stored.request_digest != *digest {
                return Err(WenlanError::Conflict(format!(
                    "operation id '{operation}' was already used by '{caller}' for a \
                     different page write"
                )));
            }
            log::debug!("[update_page] {page_id}: replaying receipt for {caller}/{operation}");
            return serde_json::from_str::<WriteResult>(&stored.response).map_err(|e| {
                WenlanError::VectorDb(format!(
                    "receipt for {caller}/{operation} is unreadable: {e}"
                ))
            });
        }
    }

    // ── Load, decide ownership, and write under one version CAS ─────────────
    // The ownership decision is made from a loaded row, and the write CASes on
    // *that row's* version — so the row we decided from is provably the row we
    // wrote. An edit landing in the gap fails the CAS instead of being
    // clobbered, and we reload and re-decide rather than forcing the write
    // through: a page that became human-owned in the gap gets a revision card.
    //
    // Bounded because each retry only re-runs on a version that actually moved;
    // a caller losing three races in a row is a write storm, not a lost update,
    // and yielding is the safe answer.
    const MAX_CAS_ATTEMPTS: usize = 3;
    let no_op = |outcome: WriteOutcome, warnings: Vec<String>| WriteResult {
        id: page_id.to_string(),
        attached_to: None,
        warnings,
        wrote: false,
        revision_card_id: None,
        gated: false,
        outcome,
    };

    let (delta_summary, current_version, new_version) = 'cas: {
        for attempt in 1..=MAX_CAS_ATTEMPTS {
            let current = db.get_page(page_id).await?.ok_or_else(|| {
                WenlanError::Validation(format!("page '{page_id}' does not exist"))
            })?;

            let current_version = current.version;
            let new_version = current_version + 1;

            // A caller-supplied `expected_version` is a precondition, not a retry
            // hint: once it stops matching, the write is refused outright rather
            // than re-aimed at a row the caller never saw.
            //
            // This has to come BEFORE the ownership gate, not after. Staging a
            // card first would take content the agent computed against an old
            // base and bind it to the version we just loaded — so accepting that
            // card silently reverts whatever the human wrote in between. Refusing
            // first drops no agent work either: the caller re-reads the fresh
            // content and stages a better card against it.
            if let Some(expected) = req.expected_version {
                if expected != current_version {
                    log::debug!(
                        "[update_page] {page_id}: expected_version {expected} != current {current_version}; refusing write"
                    );
                    return Ok(no_op(
                        WriteOutcome::Refused,
                        vec![format!(
                            "page moved to v{current_version} (expected v{expected}); write refused"
                        )],
                    ));
                }
            }

            // Ownership gate, re-evaluated on every attempt. Inside the CAS loop
            // it is no longer advisory: whatever it decided is what the write
            // guards on.
            //
            // ponytail: staging is a durable write with a fresh id and records no
            // receipt, so a client retrying an operation whose first attempt was
            // gated stages one duplicate card per retry — "same pair replays" does
            // not hold on this branch. Harmless today (resolution takes the latest
            // card) and it costs a receipt write on a path that ends in human
            // review; fix when cards get deduped, not before.
            if writer.is_machine() && page_is_human_owned(&current) {
                return stage_page_revision_card(
                    db,
                    &current,
                    &req.content,
                    &req.source_memory_ids,
                    edited_by,
                )
                .await;
            }

            // Shrink-guard (T17): opt-in via WENLAN_MERGE_SHRINK_GUARD=<f64>.
            // OFF by default: unset/unparseable = None = zero regression.
            // Only fires for LLM-rewrite edited_by; human edits are never blocked.
            // Placed AFTER current page load (needs old body), BEFORE early-return.
            // NOT inside the skips_hallucination_guard block: that skips page_growth/re_distill.
            if writer.is_llm_rewrite() {
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

            // ── Build changelog entry ───────────────────────────────────────
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
                return Ok(no_op(WriteOutcome::Unchanged, vec![]));
            }

            let mut added_sources: Vec<&str> = new_set.difference(&old_set).copied().collect();
            added_sources.sort_unstable();
            let added_sources_json = serde_json::Value::Array(
                added_sources
                    .iter()
                    .map(|s| serde_json::Value::String(s.to_string()))
                    .collect(),
            );

            let mut entry = serde_json::json!({
                "version": new_version,
                "at": chrono::Utc::now().timestamp(),
                "edited_by": edited_by,
                "delta_summary": delta_summary,
                "incoming_source_ids": added_sources_json,
            });
            if let Some((_, ref stats_summary)) = citations {
                entry["citations_summary"] = serde_json::Value::String(stats_summary.clone());
            }

            // Read existing changelog and append the new entry
            let existing_cl = db.get_page_changelog(page_id).await?;
            const DEFAULT_CHANGELOG_CAP: usize = 20;
            let new_changelog =
                crate::db::append_changelog_entry(&existing_cl, entry, DEFAULT_CHANGELOG_CAP)?;

            // ── Apply DB update ─────────────────────────────────────────────
            // The receipt records the response this call is about to return,
            // so a replay hands back the identical envelope rather than a
            // reconstruction. It commits inside the write's own transaction.
            let receipt_response = match retry {
                Some(_) => Some(serde_json::to_string(&WriteResult {
                    id: page_id.to_string(),
                    attached_to: None,
                    warnings: write_warnings(&delta_summary, current_version, new_version),
                    wrote: true,
                    revision_card_id: None,
                    gated: false,
                    outcome: WriteOutcome::Wrote,
                })?),
                None => None,
            };
            let receipt = match (retry.as_ref(), receipt_response.as_deref()) {
                (Some((caller, operation, digest)), Some(response)) => {
                    Some(crate::db::OperationReceipt {
                        caller_id: caller,
                        operation_id: operation,
                        request_digest: digest,
                        response,
                    })
                }
                _ => None,
            };

            pre_write_pause().await;
            // citations: None -> resets `citations` to '[]' (no fresh citation source
            // for this write; a stale claim-map must not survive a content change).
            let wrote = db
                .try_update_page_content_with_changelog(
                    page_id,
                    &req.content,
                    &source_refs,
                    edited_by,
                    require_stale,
                    &new_changelog,
                    citations.as_ref().map(|(json, _)| json.as_str()),
                    Some(current_version),
                    receipt,
                )
                .await?;

            if wrote {
                break 'cas (delta_summary, current_version, new_version);
            }

            // Nothing was written. Two distinguishable causes share this branch:
            // the `require_stale` gate (row untouched) and a version conflict
            // (row moved under us). Only the latter is worth retrying.
            //
            // Losing every CAS attempt used to return a bare no-op — byte-identical
            // to the "content is already what you asked for" return above. A caller
            // whose work was thrown away under contention could not tell that apart
            // from having had nothing to do, so it now says so.
            //
            // The `require_stale` skip stays silent on purpose: the caller asked to
            // write only if the page was stale, and it wasn't. That is the answer to
            // the question it asked, not a discarded write, and it is the common
            // path on every re-distill sweep.
            let landed_version = db.get_page(page_id).await?.map(|p| p.version);
            if landed_version != Some(current_version) {
                if attempt < MAX_CAS_ATTEMPTS {
                    log::debug!(
                        "[update_page] {page_id}: version moved {current_version} -> {landed_version:?} mid-write; reloading (attempt {attempt})"
                    );
                    continue;
                }
                log::warn!(
                    "[update_page] {page_id}: gave up after {MAX_CAS_ATTEMPTS} attempts; page still moving"
                );
                return Ok(no_op(
                    WriteOutcome::Contended,
                    vec![format!(
                        "page kept moving under this write ({MAX_CAS_ATTEMPTS} attempts); nothing was written"
                    )],
                ));
            }
            return Ok(no_op(WriteOutcome::Unchanged, vec![]));
        }
        // Unreachable: every path through the loop returns, continues, or breaks
        // with a value. The compiler cannot prove that, so the block needs a
        // tail value.
        return Ok(no_op(WriteOutcome::Unchanged, vec![]));
    };

    // ── md re-write ─────────────────────────────────────────────────────────
    if let Some(ref projection) = projection {
        if let Ok(Some(updated_page)) = db.get_page(page_id).await {
            if let Err(e) = projection.write_page(&updated_page) {
                log::warn!("[update_page] md re-write failed for {page_id}: {e}");
            }
        }
    }
    drop(projection);

    Ok(WriteResult {
        id: page_id.to_string(),
        attached_to: None,
        warnings: write_warnings(&delta_summary, current_version, new_version),
        wrote: true,
        revision_card_id: None,
        gated: false,
        outcome: WriteOutcome::Wrote,
    })
}

struct PageRevisionCard {
    page_id: String,
    revision_id: String,
    page_version: Option<i64>,
    content: String,
    source_memory_ids: Vec<String>,
}

async fn resolve_page_revision_card(
    db: &MemoryDB,
    id: &str,
) -> Result<Option<PageRevisionCard>, WenlanError> {
    let conn = db.conn.lock().await;
    let mut rows = conn
        .query(
            "SELECT source_id, supersedes, content, structured_fields \
             FROM memories \
             WHERE pending_revision = 1 \
               AND source = 'memory' \
               AND (source_id = ?1 OR supersedes = ?1) \
             ORDER BY CASE WHEN source_id = ?1 THEN 0 ELSE 1 END, last_modified DESC \
             LIMIT 1",
            libsql::params![id.to_string()],
        )
        .await
        .map_err(|e| WenlanError::VectorDb(format!("resolve_page_revision_card: {e}")))?;

    let Some(row) = rows
        .next()
        .await
        .map_err(|e| WenlanError::VectorDb(format!("resolve_page_revision_card row: {e}")))?
    else {
        return Ok(None);
    };
    let revision_id = row
        .get::<String>(0)
        .map_err(|e| WenlanError::VectorDb(format!("revision source_id: {e}")))?;
    let supersedes = row
        .get::<String>(1)
        .map_err(|e| WenlanError::VectorDb(format!("revision supersedes: {e}")))?;
    let content = row
        .get::<String>(2)
        .map_err(|e| WenlanError::VectorDb(format!("revision content: {e}")))?;
    let structured = row
        .get::<Option<String>>(3)
        .unwrap_or(None)
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
    drop(rows);
    drop(conn);

    let Some(structured) = structured else {
        return Ok(None);
    };
    if structured.get("revision_kind").and_then(|v| v.as_str()) != Some("page_write")
        || structured.get("target_kind").and_then(|v| v.as_str()) != Some("page")
    {
        return Ok(None);
    }

    let page_id = structured
        .get("revises_page")
        .and_then(|v| v.as_str())
        .unwrap_or(&supersedes)
        .to_string();
    let source_memory_ids = structured
        .get("source_memory_ids")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let page_version = structured.get("page_version").and_then(|v| v.as_i64());

    Ok(Some(PageRevisionCard {
        page_id,
        revision_id,
        page_version,
        content,
        source_memory_ids,
    }))
}

// ponytail: a card records no base version, so accept writes its body over
// whatever the page holds now. If the page moved after the card was staged, the
// accepted body silently reverts that change. M0 closes this for writers that
// declare a base (they are refused instead of carded); a writer that passed
// `expected_version: None` can still stage a stale-based card. Closing it needs
// the base version stored on the card and re-checked here — that is the M5
// claim-revision problem, not M0's.
async fn accept_page_revision_card(
    db: &MemoryDB,
    card: PageRevisionCard,
    knowledge_path: Option<&Path>,
) -> Result<wenlan_types::RevisionAcceptResponse, WenlanError> {
    let current = db
        .get_page(&card.page_id)
        .await?
        .ok_or_else(|| WenlanError::NotFound(format!("Page not found: {}", card.page_id)))?;
    let source_memory_ids = if card.source_memory_ids.is_empty() {
        current.source_memory_ids.clone()
    } else {
        card.source_memory_ids.clone()
    };
    let source_refs: Vec<&str> = source_memory_ids.iter().map(String::as_str).collect();
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
            .map(|s| serde_json::Value::String((*s).to_string()))
            .collect(),
    );
    let new_version = current.version + 1;
    let entry = serde_json::json!({
        "version": new_version,
        "at": chrono::Utc::now().timestamp(),
        "edited_by": "revision_accept",
        "delta_summary": crate::db::compute_page_delta_summary(
            &current.content,
            &current.source_memory_ids,
            &card.content,
            &source_refs,
            "revision_accept",
        ),
        "incoming_source_ids": added_sources_json,
    });
    let existing_cl = db.get_page_changelog(&card.page_id).await?;
    const DEFAULT_CHANGELOG_CAP: usize = 20;
    let new_changelog =
        crate::db::append_changelog_entry(&existing_cl, entry, DEFAULT_CHANGELOG_CAP)?;

    let projection = knowledge_path.map(|path| {
        crate::export::knowledge::KnowledgeProjectionWrite::new(path.to_path_buf(), db)
    });
    let wrote = db
        .try_accept_page_revision(
            &card.page_id,
            &card.content,
            &source_refs,
            &new_changelog,
            card.page_version,
            &card.revision_id,
        )
        .await?;

    if !wrote {
        let current_version = db
            .get_page(&card.page_id)
            .await?
            .ok_or_else(|| WenlanError::NotFound(format!("Page not found: {}", card.page_id)))?
            .version;
        let Some(staged_version) = card.page_version else {
            return Err(WenlanError::Conflict(format!(
                "page revision card {} for page {} did not write",
                card.revision_id, card.page_id
            )));
        };
        return Err(WenlanError::Conflict(format!(
            "page revision card {} was staged for page {} at staged version {}, but current version {} no longer matches",
            card.revision_id, card.page_id, staged_version, current_version
        )));
    }

    if let Some(ref projection) = projection {
        if let Ok(Some(updated_page)) = db.get_page(&card.page_id).await {
            if let Err(e) = projection.write_page(&updated_page) {
                log::warn!(
                    "[accept_page_revision_card] md re-write failed for {}: {e}",
                    card.page_id
                );
            }
        }
    }
    drop(projection);

    Ok(wenlan_types::RevisionAcceptResponse {
        target_source_id: card.page_id,
        revision_source_id: card.revision_id,
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
    accept_pending_revision_with_knowledge_path(db, id, agent, None).await
}

pub async fn accept_pending_revision_with_knowledge_path(
    db: &MemoryDB,
    id: &str,
    agent: &str,
    knowledge_path: Option<&Path>,
) -> Result<wenlan_types::RevisionAcceptResponse, WenlanError> {
    if let Some(card) = resolve_page_revision_card(db, id).await? {
        let result = accept_page_revision_card(db, card, knowledge_path).await?;
        log_activity_best_effort(db, agent, "revision_accept", &result.target_source_id).await;
        return Ok(result);
    }

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
            creation_kind: Some("authored".to_string()),
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
        seed_memory(&db, "mem-rust-a", "Rust is a systems programming language").await;
        seed_memory(&db, "mem-rust-b", "Rust has ownership and borrowing").await;
        seed_memory(&db, "mem-rust-c", "Rust supports memory-safe concurrency").await;
        let req = CreateConceptRequest {
            title: "Cooking".to_string(),
            content: "Pasta carbonara needs eggs and pancetta".to_string(),
            summary: None,
            entity_id: None,
            space: None,
            source_memory_ids: vec![
                "mem-rust-a".to_string(),
                "mem-rust-b".to_string(),
                "mem-rust-c".to_string(),
            ],
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
        seed_memory(
            &db,
            "mem-rust-happy-a",
            "Rust is a systems programming language with memory safety guarantees",
        )
        .await;
        seed_memory(
            &db,
            "mem-rust-happy-b",
            "Rust provides ownership and borrowing for memory safety",
        )
        .await;
        seed_memory(
            &db,
            "mem-rust-happy-c",
            "Rust supports systems programming with safe concurrency",
        )
        .await;
        let req = CreateConceptRequest {
            title: "Rust".to_string(),
            content: "Rust is a systems programming language providing memory safety guarantees"
                .to_string(),
            summary: Some("memory-safe systems language".to_string()),
            entity_id: None,
            space: None,
            source_memory_ids: vec![
                "mem-rust-happy-a".to_string(),
                "mem-rust-happy-b".to_string(),
                "mem-rust-happy-c".to_string(),
            ],
            creation_kind: None,
            workspace: None,
        };
        let result = create_page(&db, req, "test", None).await.unwrap();
        assert!(result.id.starts_with("page_"));
    }

    #[tokio::test]
    async fn create_page_with_floor_rejects_distilled_below_configured_floor() {
        let (db, _dir) = test_db().await;
        seed_memory(
            &db,
            "mem-rust-floor-a",
            "Rust has ownership and borrowing for memory safety",
        )
        .await;
        seed_memory(
            &db,
            "mem-rust-floor-b",
            "Rust uses lifetimes to validate borrowed references",
        )
        .await;
        seed_memory(
            &db,
            "mem-rust-floor-c",
            "Rust tracks reference validity through lifetimes",
        )
        .await;
        let req = CreateConceptRequest {
            title: "Rust Memory Safety".to_string(),
            content:
                "Rust has ownership, borrowing, lifetimes, reference validity, and memory safety"
                    .to_string(),
            summary: Some("Rust memory safety".to_string()),
            entity_id: None,
            space: None,
            source_memory_ids: vec![
                "mem-rust-floor-a".to_string(),
                "mem-rust-floor-b".to_string(),
                "mem-rust-floor-c".to_string(),
            ],
            creation_kind: Some("distilled".to_string()),
            workspace: None,
        };

        let result = create_page_with_floor(&db, req, "test", None, 4).await;

        match result {
            Err(WenlanError::Validation(message)) => assert_eq!(
                message,
                "distilled page requires at least 4 distinct source memories (got 3)"
            ),
            other => panic!("expected distinct-source floor validation error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn create_page_counts_distinct_sources_for_distilled_floor() {
        let (db, _dir) = test_db().await;
        seed_memory(
            &db,
            "mem-rust-distinct-a",
            "Rust ownership prevents memory safety bugs",
        )
        .await;
        seed_memory(
            &db,
            "mem-rust-distinct-b",
            "Rust borrowing validates references at compile time",
        )
        .await;
        let req = CreateConceptRequest {
            title: "Rust Safety".to_string(),
            content: "Rust ownership and borrowing validate memory-safe references".to_string(),
            summary: Some("Rust source floor".to_string()),
            entity_id: None,
            space: None,
            source_memory_ids: vec![
                "mem-rust-distinct-a".to_string(),
                "mem-rust-distinct-a".to_string(),
                "mem-rust-distinct-b".to_string(),
            ],
            creation_kind: Some("distilled".to_string()),
            workspace: None,
        };

        let result = create_page(&db, req, "test", None).await;

        match result {
            Err(WenlanError::Validation(message)) => assert_eq!(
                message,
                "distilled page requires at least 3 distinct source memories (got 2)"
            ),
            other => panic!("expected distinct-source floor validation error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn create_page_allows_authored_below_distilled_floor() {
        let (db, _dir) = test_db().await;
        seed_memory(
            &db,
            "mem-rust-authored-a",
            "Rust ownership prevents memory safety bugs",
        )
        .await;
        let req = CreateConceptRequest {
            title: "Rust Authored Note".to_string(),
            content: "Rust ownership prevents memory safety bugs".to_string(),
            summary: Some("Rust authored page".to_string()),
            entity_id: None,
            space: None,
            source_memory_ids: vec!["mem-rust-authored-a".to_string()],
            creation_kind: Some("authored".to_string()),
            workspace: None,
        };

        let result = create_page(&db, req, "test", None).await.unwrap();

        assert!(result.id.starts_with("page_"));
    }

    #[tokio::test]
    async fn create_page_rejects_zero_source_distilled_with_preexisting_message() {
        let (db, _dir) = test_db().await;
        let req = CreateConceptRequest {
            title: "Rust".to_string(),
            content: "Rust is a systems programming language".to_string(),
            summary: None,
            entity_id: None,
            space: None,
            source_memory_ids: vec![],
            creation_kind: Some("distilled".to_string()),
            workspace: None,
        };

        let result = create_page(&db, req, "test", None).await;

        match result {
            Err(WenlanError::Validation(message)) => assert_eq!(
                message, "distilled page must cite at least one source memory",
                "zero-source distilled must keep the pre-existing message, not the distinct-source floor message"
            ),
            other => panic!("expected zero-source validation error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn create_page_allows_authored_with_zero_sources() {
        let (db, _dir) = test_db().await;
        let req = CreateConceptRequest {
            title: "Rust Authored Note".to_string(),
            content: "Rust ownership prevents memory safety bugs".to_string(),
            summary: None,
            entity_id: None,
            space: None,
            source_memory_ids: vec![],
            creation_kind: Some("authored".to_string()),
            workspace: None,
        };

        let result = create_page(&db, req, "test", None).await.unwrap();

        assert!(result.id.starts_with("page_"));
    }

    #[tokio::test]
    async fn create_page_borns_distilled_unconfirmed() {
        let (db, _dir) = test_db().await;
        let docs = [
            (
                "mem-rust-birth-a",
                "Rust ownership helps prevent memory safety bugs",
            ),
            (
                "mem-rust-birth-b",
                "Rust borrowing validates references at compile time",
            ),
            (
                "mem-rust-birth-c",
                "Rust lifetimes describe how long references remain valid",
            ),
        ]
        .into_iter()
        .map(|(source_id, content)| crate::sources::RawDocument {
            source: "memory".to_string(),
            source_id: source_id.to_string(),
            title: source_id.to_string(),
            content: content.to_string(),
            last_modified: chrono::Utc::now().timestamp(),
            memory_type: Some("fact".to_string()),
            source_agent: Some("test".to_string()),
            confidence: Some(0.9),
            ..Default::default()
        })
        .collect::<Vec<_>>();
        db.upsert_documents(docs).await.unwrap();
        let req = CreateConceptRequest {
            title: "Rust References".to_string(),
            content: "Rust ownership, borrowing, and lifetimes keep references memory safe"
                .to_string(),
            summary: Some("Rust reference safety".to_string()),
            entity_id: None,
            space: None,
            source_memory_ids: vec![
                "mem-rust-birth-a".to_string(),
                "mem-rust-birth-b".to_string(),
                "mem-rust-birth-c".to_string(),
            ],
            creation_kind: Some("distilled".to_string()),
            workspace: None,
        };

        let result = create_page(&db, req, "test", None).await.unwrap();
        let page = db.get_page(&result.id).await.unwrap().unwrap();

        assert_eq!(page.review_status, "unconfirmed");
        let keep_cards: Vec<_> = db
            .get_pending_refinements()
            .await
            .unwrap()
            .into_iter()
            .filter(|proposal| {
                proposal.action == "page_keep_or_archive"
                    && proposal.source_ids.iter().any(|id| id == &result.id)
            })
            .collect();
        assert_eq!(
            keep_cards.len(),
            1,
            "distilled page birth must mint exactly one keep/archive card"
        );
        let payload = keep_cards[0].payload.as_deref().unwrap_or_default();
        assert!(
            payload.contains("\"source_count\":3"),
            "keep/archive card should preserve source count, got {payload}"
        );
    }

    #[tokio::test]
    async fn create_page_borns_authored_without_keep_card() {
        let (db, _dir) = test_db().await;
        let req = CreateConceptRequest {
            title: "Authored Rust Notes".to_string(),
            content: "Authored notes about Rust references and workspace conventions.".to_string(),
            summary: None,
            entity_id: None,
            space: None,
            source_memory_ids: vec![],
            creation_kind: Some("authored".to_string()),
            workspace: None,
        };

        let result = create_page(&db, req, "test", None).await.unwrap();

        let keep_cards: Vec<_> = db
            .get_pending_refinements()
            .await
            .unwrap()
            .into_iter()
            .filter(|proposal| {
                proposal.action == "page_keep_or_archive"
                    && proposal.source_ids.iter().any(|id| id == &result.id)
            })
            .collect();
        assert!(
            keep_cards.is_empty(),
            "authored page birth must not mint a keep/archive card"
        );
    }

    #[tokio::test]
    async fn create_page_attaches_same_workspace_near_duplicate_without_new_page() {
        let (db, _dir) = test_db().await;
        let existing_sources = [
            (
                "mem-pagewrite-existing-a",
                "Rust workspaces can share a single Cargo lockfile across related crates",
            ),
            (
                "mem-pagewrite-existing-b",
                "Rust workspace members inherit shared package metadata from the root",
            ),
            (
                "mem-pagewrite-existing-c",
                "Rust workspace builds can check all member crates together",
            ),
        ];
        for (source_id, content) in existing_sources {
            seed_memory(&db, source_id, content).await;
        }
        let now = chrono::Utc::now().to_rfc3339();
        db.insert_page_with_kind(
            "page_pagewrite_existing",
            "Rust Workspace Operations",
            Some("Rust workspace operations"),
            "Rust workspaces share Cargo lockfiles, inherited metadata, and all-crate checks",
            None,
            Some("recap"),
            &[
                "mem-pagewrite-existing-a",
                "mem-pagewrite-existing-b",
                "mem-pagewrite-existing-c",
            ],
            &now,
            "distilled",
            "confirmed",
            Some("work"),
            None,
        )
        .await
        .unwrap();

        for (source_id, content) in [
            (
                "mem-pagewrite-candidate-a",
                "Rust workspaces share one Cargo lockfile for related crates",
            ),
            (
                "mem-pagewrite-candidate-b",
                "Rust workspace members can inherit shared package metadata",
            ),
            (
                "mem-pagewrite-candidate-c",
                "Rust workspace checks can validate every member crate together",
            ),
        ] {
            seed_memory(&db, source_id, content).await;
        }
        let before_pages = db.list_pages("active", 10, 0).await.unwrap();
        assert_eq!(before_pages.len(), 1, "precondition: one active page");
        let req = CreateConceptRequest {
            title: "Rust Workspace Operations".to_string(),
            content:
                "Rust workspaces share Cargo lockfiles, inherited metadata, and all-crate checks"
                    .to_string(),
            summary: Some("Rust workspace operations".to_string()),
            entity_id: None,
            space: Some("recap".to_string()),
            source_memory_ids: vec![
                "mem-pagewrite-candidate-a".to_string(),
                "mem-pagewrite-candidate-b".to_string(),
                "mem-pagewrite-candidate-c".to_string(),
            ],
            creation_kind: Some("distilled".to_string()),
            workspace: Some("work".to_string()),
        };

        let result = create_page(&db, req, "test", None).await.unwrap();

        assert_eq!(
            result.id, "page_pagewrite_existing",
            "near-duplicate create must resolve to the existing page id"
        );
        let result_json = serde_json::to_value(&result).unwrap();
        assert_eq!(
            result_json.get("attached_to").and_then(|v| v.as_str()),
            Some("page_pagewrite_existing"),
            "response must expose the attach target"
        );
        let after_pages = db.list_pages("active", 10, 0).await.unwrap();
        assert_eq!(
            after_pages.len(),
            1,
            "same-workspace near-duplicate must not mint a second page"
        );
        let evidence = db
            .get_page_evidence("page_pagewrite_existing")
            .await
            .unwrap();
        let locators = evidence
            .iter()
            .filter(|ev| ev.source_kind == "memory")
            .filter_map(|ev| ev.locator.as_deref())
            .collect::<HashSet<_>>();
        for expected in [
            "mem-pagewrite-existing-a",
            "mem-pagewrite-existing-b",
            "mem-pagewrite-existing-c",
            "mem-pagewrite-candidate-a",
            "mem-pagewrite-candidate-b",
            "mem-pagewrite-candidate-c",
        ] {
            assert!(
                locators.contains(expected),
                "page_evidence must include {expected}; got {locators:?}"
            );
        }
    }

    #[tokio::test]
    async fn create_page_does_not_attach_no_space_candidate_to_workspace_page() {
        let (db, _dir) = test_db().await;
        for (source_id, content) in [
            (
                "mem-pagewrite-cross-existing-a",
                "Rust workspaces can share a single Cargo lockfile across related crates",
            ),
            (
                "mem-pagewrite-cross-existing-b",
                "Rust workspace members inherit shared package metadata from the root",
            ),
            (
                "mem-pagewrite-cross-existing-c",
                "Rust workspace builds can check all member crates together",
            ),
            (
                "mem-pagewrite-cross-candidate-a",
                "Rust workspaces share one Cargo lockfile for related crates",
            ),
            (
                "mem-pagewrite-cross-candidate-b",
                "Rust workspace members can inherit shared package metadata",
            ),
            (
                "mem-pagewrite-cross-candidate-c",
                "Rust workspace checks can validate every member crate together",
            ),
        ] {
            seed_memory(&db, source_id, content).await;
        }
        let now = chrono::Utc::now().to_rfc3339();
        db.insert_page_with_kind(
            "page_pagewrite_cross_existing",
            "Rust Workspace Operations",
            Some("Rust workspace operations"),
            "Rust workspaces share Cargo lockfiles, inherited metadata, and all-crate checks",
            None,
            Some("recap"),
            &[
                "mem-pagewrite-cross-existing-a",
                "mem-pagewrite-cross-existing-b",
                "mem-pagewrite-cross-existing-c",
            ],
            &now,
            "distilled",
            "confirmed",
            Some("work"),
            None,
        )
        .await
        .unwrap();
        let req = CreateConceptRequest {
            title: "Rust Workspace Operations".to_string(),
            content:
                "Rust workspaces share Cargo lockfiles, inherited metadata, and all-crate checks"
                    .to_string(),
            summary: Some("Rust workspace operations".to_string()),
            entity_id: None,
            space: None,
            source_memory_ids: vec![
                "mem-pagewrite-cross-candidate-a".to_string(),
                "mem-pagewrite-cross-candidate-b".to_string(),
                "mem-pagewrite-cross-candidate-c".to_string(),
            ],
            creation_kind: Some("distilled".to_string()),
            workspace: None,
        };

        let result = create_page(&db, req, "test", None).await.unwrap();

        assert_ne!(
            result.id, "page_pagewrite_cross_existing",
            "space-scoped dedup must not attach a no-space candidate to a workspace page"
        );
        assert_eq!(
            result.attached_to, None,
            "cross-space create must report a new page, not an attachment"
        );
        let pages = db.list_pages("active", 10, 0).await.unwrap();
        assert_eq!(
            pages.len(),
            2,
            "cross-space near-duplicate must mint a second page"
        );
    }

    #[tokio::test]
    async fn create_page_does_not_attach_different_space_candidate() {
        let (db, _dir) = test_db().await;
        for (source_id, content) in [
            (
                "mem-pagewrite-diffspace-existing-a",
                "Rust workspaces can share a single Cargo lockfile across related crates",
            ),
            (
                "mem-pagewrite-diffspace-existing-b",
                "Rust workspace members inherit shared package metadata from the root",
            ),
            (
                "mem-pagewrite-diffspace-existing-c",
                "Rust workspace builds can check all member crates together",
            ),
            (
                "mem-pagewrite-diffspace-candidate-a",
                "Rust workspaces share one Cargo lockfile for related crates",
            ),
            (
                "mem-pagewrite-diffspace-candidate-b",
                "Rust workspace members can inherit shared package metadata",
            ),
            (
                "mem-pagewrite-diffspace-candidate-c",
                "Rust workspace checks can validate every member crate together",
            ),
        ] {
            seed_memory(&db, source_id, content).await;
        }
        let now = chrono::Utc::now().to_rfc3339();
        db.insert_page_with_kind(
            "page_pagewrite_diffspace_existing",
            "Rust Workspace Operations",
            Some("Rust workspace operations"),
            "Rust workspaces share Cargo lockfiles, inherited metadata, and all-crate checks",
            None,
            Some("recap"),
            &[
                "mem-pagewrite-diffspace-existing-a",
                "mem-pagewrite-diffspace-existing-b",
                "mem-pagewrite-diffspace-existing-c",
            ],
            &now,
            "distilled",
            "confirmed",
            Some("work"),
            None,
        )
        .await
        .unwrap();
        // Same content, but scoped to a DIFFERENT workspace ("personal") — the
        // scoped matcher's `COALESCE(workspace, space) = ?` filter must exclude
        // the "work" page, so this mints a new page rather than attaching.
        let req = CreateConceptRequest {
            title: "Rust Workspace Operations".to_string(),
            content:
                "Rust workspaces share Cargo lockfiles, inherited metadata, and all-crate checks"
                    .to_string(),
            summary: Some("Rust workspace operations".to_string()),
            entity_id: None,
            space: Some("recap".to_string()),
            source_memory_ids: vec![
                "mem-pagewrite-diffspace-candidate-a".to_string(),
                "mem-pagewrite-diffspace-candidate-b".to_string(),
                "mem-pagewrite-diffspace-candidate-c".to_string(),
            ],
            creation_kind: Some("distilled".to_string()),
            workspace: Some("personal".to_string()),
        };

        let result = create_page(&db, req, "test", None).await.unwrap();

        assert_ne!(
            result.id, "page_pagewrite_diffspace_existing",
            "space-scoped dedup must not attach a different-space candidate to a work page"
        );
        assert_eq!(
            result.attached_to, None,
            "different-space create must report a new page, not an attachment"
        );
        let pages = db.list_pages("active", 10, 0).await.unwrap();
        assert_eq!(
            pages.len(),
            2,
            "different-space near-duplicate must mint a second page"
        );
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
            creation_kind: Some("research".to_string()),
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
            expected_version: None,
            caller_id: None,
            operation_id: None,
        };
        let r2 = update_page(&db, &page_id, req2, "re_distill", false, None, None)
            .await
            .unwrap();
        assert_eq!(r2.id, page_id);

        // Second update → version=3
        let content_v3 = "Rust is a systems language with memory safety, zero-cost abstractions and concurrency without data races";
        let req3 = UpdatePageRequest {
            content: content_v3.to_string(),
            source_memory_ids: vec![mem_id.to_string()],
            expected_version: None,
            caller_id: None,
            operation_id: None,
        };
        let r3 = update_page(&db, &page_id, req3, "re_distill", false, None, None)
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
            expected_version: None,
            caller_id: None,
            operation_id: None,
        };
        let result = update_page(&db, &page_id, req, "re_distill", true, None, None)
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
            expected_version: None,
            caller_id: None,
            operation_id: None,
        };
        let result = update_page(&db, &page_id, req, "re_distill", true, None, None)
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
            expected_version: None,
            caller_id: None,
            operation_id: None,
        };
        let result = update_page(&db, &page_id, req, "manual_edit", false, None, None).await;
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
            expected_version: None,
            caller_id: None,
            operation_id: None,
        };
        // Should succeed without hallucination check
        update_page(&db, &page_id, req, "re_distill", false, None, None)
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
            expected_version: None,
            caller_id: None,
            operation_id: None,
        };
        update_page(&db, &page_id, req, "fs_edit", false, None, None)
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
            expected_version: None,
            caller_id: None,
            operation_id: None,
        };
        update_page(&db, page_id, req, "fs_edit", false, None, None)
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
            expected_version: None,
            caller_id: None,
            operation_id: None,
        };
        let result = update_page(&db, &page_id, req, "re_distill", false, None, None)
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
                expected_version: None,
                caller_id: None,
                operation_id: None,
            },
            "re_distill",
            false,
            None,
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
                expected_version: None,
                caller_id: None,
                operation_id: None,
            },
            "re_distill",
            false,
            None,
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
            expected_version: None,
            caller_id: None,
            operation_id: None,
        };
        let result = update_page(&db, &page_id, req, "re_distill", false, None, None)
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

    #[tokio::test]
    async fn page_write_update_user_edited_machine_write_creates_revision_card_without_overwrite() {
        let (db, _dir) = test_db().await;
        let mem_id = "mem-pagewrite-owned";
        let source_content = "Rust ownership keeps memory safety rules explicit in systems code";
        seed_memory(&db, mem_id, source_content).await;
        let now = chrono::Utc::now().to_rfc3339();
        let page_id = "page_pagewrite_owned";
        db.insert_page(
            page_id,
            "Rust Ownership",
            None,
            source_content,
            None,
            None,
            &[mem_id],
            &now,
        )
        .await
        .unwrap();

        let human_content =
            "Rust ownership keeps memory safety rules explicit in systems code, with human notes";
        page_write(
            &db,
            PageWrite::Update {
                page_id,
                req: UpdatePageRequest {
                    content: human_content.to_string(),
                    source_memory_ids: vec![mem_id.to_string()],
                    expected_version: None,
                    caller_id: None,
                    operation_id: None,
                },
                edited_by: "fs_edit",
                require_stale: false,
                knowledge_path: None,
                citations: None,
            },
        )
        .await
        .unwrap();

        let before = db.get_page(page_id).await.unwrap().unwrap();
        assert!(
            before.user_edited,
            "precondition: fs_edit marks human ownership"
        );

        let machine_content =
            "Rust ownership lets the compiler enforce memory safety during page refresh";
        let result = page_write(
            &db,
            PageWrite::Update {
                page_id,
                req: UpdatePageRequest {
                    content: machine_content.to_string(),
                    source_memory_ids: vec![mem_id.to_string()],
                    expected_version: None,
                    caller_id: None,
                    operation_id: None,
                },
                edited_by: "re_distill",
                require_stale: false,
                knowledge_path: None,
                citations: None,
            },
        )
        .await
        .unwrap();

        let after = db.get_page(page_id).await.unwrap().unwrap();
        assert_eq!(result.id, page_id);
        assert!(!result.wrote, "gated PageWrite must report wrote=false");
        assert!(result.gated, "gated PageWrite must expose gated=true");
        assert_eq!(result.attached_to, None);
        assert_eq!(
            result.warnings,
            vec!["human-owned page; staged revision card instead of overwriting content"],
            "gated PageWrite must explain that the page prose was not overwritten"
        );
        assert_eq!(
            after.content, before.content,
            "machine PageWrite must not overwrite human-owned page prose"
        );
        assert_eq!(
            after.content, human_content,
            "machine PageWrite must leave the human-authored bytes unchanged"
        );
        assert_eq!(
            after.source_memory_ids, before.source_memory_ids,
            "gated PageWrite must not mutate the protected page source set"
        );
        assert_eq!(
            after.version, before.version,
            "gated PageWrite must not bump the protected page version"
        );
        assert!(
            after.user_edited,
            "gated PageWrite must preserve the human ownership marker"
        );

        let result_json = serde_json::to_value(&result).unwrap();
        assert_eq!(result_json.get("gated"), Some(&serde_json::json!(true)));
        let revision_card_id = result_json
            .get("revision_card_id")
            .and_then(|v| v.as_str())
            .expect("gated response must include revision_card_id");

        let revisions = db.list_pending_revisions(10).await.unwrap();
        assert_eq!(
            revisions.len(),
            1,
            "gated PageWrite must stage exactly one pending revision card"
        );
        let card = revisions
            .iter()
            .find(|r| r.revision_source_id == revision_card_id)
            .expect("revision card must be visible in pending revisions");
        assert_eq!(card.target_source_id, page_id);
        assert_eq!(card.revision_content, machine_content);
        assert_eq!(card.source_agent.as_deref(), Some("page_write"));

        let conn = db.conn.lock().await;
        let mut rows = conn
            .query(
                "SELECT source, supersedes, pending_revision, confirmed, stability, \
                        structured_fields, source_text, memory_type \
                 FROM memories WHERE source_id = ?1",
                libsql::params![revision_card_id.to_string()],
            )
            .await
            .unwrap();
        let row = rows
            .next()
            .await
            .unwrap()
            .expect("revision card row must be persisted");
        assert_eq!(row.get::<String>(0).unwrap(), "memory");
        assert_eq!(row.get::<String>(1).unwrap(), page_id);
        assert_eq!(row.get::<i64>(2).unwrap(), 1);
        assert_eq!(row.get::<i64>(3).unwrap(), 0);
        assert_eq!(row.get::<String>(4).unwrap(), "new");
        let structured_fields = row.get::<String>(5).unwrap();
        assert_eq!(
            row.get::<Option<String>>(6).unwrap().as_deref(),
            Some(machine_content)
        );
        assert_eq!(row.get::<String>(7).unwrap(), "fact");
        assert!(
            rows.next().await.unwrap().is_none(),
            "revision_card_id must identify one persisted card row"
        );
        drop(rows);
        drop(conn);

        let structured: serde_json::Value = serde_json::from_str(&structured_fields).unwrap();
        assert_eq!(structured["revision_kind"], "page_write");
        assert_eq!(structured["target_kind"], "page");
        assert_eq!(structured["revises_page"], page_id);
        assert_eq!(structured["page_version"], before.version);
        assert_eq!(structured["edited_by"], "re_distill");
        assert_eq!(structured["source_memory_ids"], serde_json::json!([mem_id]));
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

    /// M0 write gate: a content write whose `expected_version` no longer matches
    /// the stored row must not land. Without the guard, a writer that loaded v1
    /// and decided its ownership branch there overwrites whatever landed at v2.
    #[tokio::test]
    async fn page_write_update_with_stale_expected_version_does_not_clobber() {
        let (db, _dir) = test_db().await;
        let mem_id = "mem-cas-stale";
        let source_content = "Version guards keep concurrent page writers from losing updates";
        seed_memory(&db, mem_id, source_content).await;
        let now = chrono::Utc::now().to_rfc3339();
        let page_id = "page_cas_stale";
        db.insert_page(
            page_id,
            "Version Guards",
            None,
            source_content,
            None,
            None,
            &[mem_id],
            &now,
        )
        .await
        .unwrap();

        let loaded = db.get_page(page_id).await.unwrap().unwrap();
        let stale_version = loaded.version;

        // A concurrent writer lands first, advancing the stored version.
        let winner_content =
            "Version guards keep concurrent page writers from losing updates, landed first";
        page_write(
            &db,
            PageWrite::Update {
                page_id,
                req: UpdatePageRequest {
                    content: winner_content.to_string(),
                    source_memory_ids: vec![mem_id.to_string()],
                    expected_version: Some(stale_version),
                    caller_id: None,
                    operation_id: None,
                },
                edited_by: "re_distill",
                require_stale: false,
                knowledge_path: None,
                citations: None,
            },
        )
        .await
        .unwrap();

        let after_winner = db.get_page(page_id).await.unwrap().unwrap();
        assert!(
            after_winner.version > stale_version,
            "precondition: the first writer advanced the page version"
        );

        // The writer still holding the pre-write version must be refused.
        let result = page_write(
            &db,
            PageWrite::Update {
                page_id,
                req: UpdatePageRequest {
                    content: "Stale writer body that must never land".to_string(),
                    source_memory_ids: vec![mem_id.to_string()],
                    expected_version: Some(stale_version),
                    caller_id: None,
                    operation_id: None,
                },
                edited_by: "re_distill",
                require_stale: false,
                knowledge_path: None,
                citations: None,
            },
        )
        .await
        .unwrap();

        assert!(
            !result.wrote,
            "a write carrying a stale expected_version must report wrote=false"
        );
        let after = db.get_page(page_id).await.unwrap().unwrap();
        assert_eq!(
            after.content, winner_content,
            "the stale writer must not overwrite the content that landed first"
        );
        assert_eq!(
            after.version, after_winner.version,
            "a refused write must not bump the page version"
        );
    }

    /// Every page write appends exactly one immutable `page_history` row, and
    /// the snapshot it stores is the body at *that* version — so a page's
    /// evolution is reconstructable rather than inferred.
    ///
    /// Mutation check: drop the history INSERT from `try_update_page_content`
    /// and the version sequence collapses to `[1]`; store the pre-write body
    /// instead of the post-write one and the content assertions invert.
    #[tokio::test]
    async fn page_write_records_one_history_row_per_version() {
        let (db, _dir) = test_db().await;
        let mem_id = "mem-history";
        let source_content = "Every version of a page is recorded as its own history row";
        seed_memory(&db, mem_id, source_content).await;
        let now = chrono::Utc::now().to_rfc3339();
        let page_id = "page_history_seq";
        db.insert_page(
            page_id,
            "History",
            None,
            source_content,
            None,
            None,
            &[mem_id],
            &now,
        )
        .await
        .unwrap();

        // Creation is itself a version: the timeline is never empty.
        let at_create = db.list_page_history(page_id, 10).await.unwrap();
        assert_eq!(
            at_create.len(),
            1,
            "a newly created page must already have its v1 history row"
        );
        assert_eq!(at_create[0].content, source_content);

        let v2 =
            "Every version of a page is recorded as its own history row, appended in the write";
        let v3 = "Every version of a page is recorded as its own history row, appended in the write transaction";
        for body in [v2, v3] {
            let result = page_write(
                &db,
                PageWrite::Update {
                    page_id,
                    req: UpdatePageRequest {
                        content: body.to_string(),
                        source_memory_ids: vec![mem_id.to_string()],
                        expected_version: None,
                        caller_id: None,
                        operation_id: None,
                    },
                    edited_by: "re_distill",
                    require_stale: false,
                    knowledge_path: None,
                    citations: None,
                },
            )
            .await
            .unwrap();
            assert!(result.wrote, "each update must land");
        }

        let history = db.list_page_history(page_id, 10).await.unwrap();
        let versions: Vec<i64> = history.iter().map(|h| h.version).collect();
        assert_eq!(
            versions,
            vec![3, 2, 1],
            "one row per version, newest first, no gaps and no duplicates"
        );

        // Each row holds the body *at* its version, not the body it replaced.
        assert_eq!(history[0].content, v3);
        assert_eq!(history[1].content, v2);
        assert_eq!(history[2].content, source_content);
        assert_eq!(history[0].edited_by, "re_distill");
        assert_eq!(history[2].edited_by, "create");

        // The history head must agree with the page itself — that is the whole
        // point of writing them in one transaction.
        let page = db.get_page(page_id).await.unwrap().unwrap();
        assert_eq!(page.version, history[0].version);
        assert_eq!(page.content, history[0].content);
    }

    /// Seed the `creation_kind='source'` page that a re-enriched document
    /// replaces, and return `(page_id, memory_id, body)`.
    async fn seed_source_page(db: &MemoryDB, page_id: &'static str) -> (&'static str, String) {
        let mem_id = "mem-source-page";
        let body = "A source page is the machine-owned projection of one ingested document";
        seed_memory(db, mem_id, body).await;
        let req = CreateConceptRequest {
            title: "Ingested Document".to_string(),
            content: body.to_string(),
            summary: None,
            entity_id: None,
            space: None,
            source_memory_ids: vec![mem_id.to_string()],
            creation_kind: Some("source".to_string()),
            workspace: None,
        };
        page_write(
            db,
            PageWrite::Create {
                page_id: Some(page_id),
                req,
                agent: "doc-enrich",
                knowledge_path: None,
                page_min_cluster_size: 1,
                page_match_threshold: 0.0,
                citations_json: None,
            },
        )
        .await
        .unwrap();
        (mem_id, body.to_string())
    }

    /// `ReplaceSource` is a page write like any other: re-enriching a document
    /// bumps the page version, so it must leave the same immutable `page_history`
    /// row behind. Without this the M0-B invariant — one durable row per page
    /// version — is simply false for every source page in the corpus.
    ///
    /// Mutation check: drop the history INSERT from `replace_source_page` and the
    /// version sequence collapses to `[1]`.
    #[tokio::test]
    async fn replace_source_page_records_history_row_for_new_version() {
        let (db, _dir) = test_db().await;
        let page_id = "page_source_history";
        let (mem_id, v1) = seed_source_page(&db, page_id).await;

        let at_create = db.list_page_history(page_id, 10).await.unwrap();
        assert_eq!(
            at_create.len(),
            1,
            "a freshly created source page must already have its v1 history row"
        );

        let v2 = "A source page is the machine-owned projection of one ingested document, \
                  rewritten whenever that document is enriched again";
        let result = page_write(
            &db,
            PageWrite::ReplaceSource {
                page_id,
                title: "Ingested Document",
                summary: Some("the second enrichment"),
                content: v2,
                source_memory_ids: &[mem_id.to_string()],
                agent: "doc-enrich",
            },
        )
        .await
        .unwrap();
        assert!(result.wrote, "replacing a source page must land");

        let history = db.list_page_history(page_id, 10).await.unwrap();
        let versions: Vec<i64> = history.iter().map(|h| h.version).collect();
        assert_eq!(
            versions,
            vec![2, 1],
            "a source-page replacement must append one history row for the version it created"
        );
        assert_eq!(
            history[0].content, v2,
            "the row holds the body at its version"
        );
        assert_eq!(history[1].content, v1);
        assert_eq!(history[0].edited_by, "doc-enrich");

        // The head of the history must agree with the page — the reason both are
        // written in one transaction.
        let page = db.get_page(page_id).await.unwrap().unwrap();
        assert_eq!(page.version, history[0].version);
        assert_eq!(page.content, history[0].content);
    }

    /// A re-enrichment that recomputes the body it already wrote must not
    /// become a second version.
    ///
    /// This is not hypothetical: when the analysis LLM is unreachable,
    /// `document_enrichment` writes a deterministic stub page and pauses for
    /// retry, and every retry rebuilds the same stub from the same stored
    /// chunks. Without this guard that document's page version climbs once per
    /// retry forever — and now that a version also appends a `page_history`
    /// row, it would grow an uncapped history of byte-identical snapshots.
    ///
    /// Mutation check: drop the unchanged-guard and the version reaches 3.
    #[tokio::test]
    async fn replace_source_page_with_identical_content_writes_no_new_version() {
        let (db, _dir) = test_db().await;
        let page_id = "page_source_unchanged";
        let (mem_id, _v1) = seed_source_page(&db, page_id).await;
        let sources = [mem_id.to_string()];

        let body = "A source page is the machine-owned projection of one ingested document, \
                    rebuilt identically on every retry while the analysis LLM stays down";
        let replace = |content: &'static str| {
            page_write(
                &db,
                PageWrite::ReplaceSource {
                    page_id,
                    title: "Ingested Document",
                    summary: Some("stub"),
                    content,
                    source_memory_ids: &sources,
                    agent: "doc-enrich",
                },
            )
        };

        let first = replace(body).await.unwrap();
        assert!(first.wrote, "the first enrichment must land");
        let after_first = db.get_page(page_id).await.unwrap().unwrap();

        let retry = replace(body).await.unwrap();
        assert!(
            !retry.wrote,
            "recomputing the same body must not be reported as a write"
        );
        assert_eq!(
            retry.outcome,
            WriteOutcome::Unchanged,
            "a no-op retry is Unchanged, not a conflict and not a write"
        );

        let after_retry = db.get_page(page_id).await.unwrap().unwrap();
        assert_eq!(
            after_retry.version, after_first.version,
            "an identical replacement must not bump the version"
        );
        let versions: Vec<i64> = db
            .list_page_history(page_id, 10)
            .await
            .unwrap()
            .iter()
            .map(|h| h.version)
            .collect();
        assert_eq!(
            versions,
            vec![2, 1],
            "no version means no history row — the retry appends nothing"
        );
    }

    /// Seed a page and return `(db, tempdir, page_id, memory_id)` for the
    /// retry-receipt tests below.
    async fn receipt_fixture(
        page_id: &'static str,
    ) -> (MemoryDB, tempfile::TempDir, &'static str, &'static str) {
        let (db, dir) = test_db().await;
        let mem_id = "mem-receipt";
        let body = "A retried write must not become a second version of the page";
        seed_memory(&db, mem_id, body).await;
        let now = chrono::Utc::now().to_rfc3339();
        db.insert_page(page_id, "Receipts", None, body, None, None, &[mem_id], &now)
            .await
            .unwrap();
        (db, dir, page_id, mem_id)
    }

    fn retry_req(content: &str, mem_id: &str, operation_id: &str) -> UpdatePageRequest {
        UpdatePageRequest {
            content: content.to_string(),
            source_memory_ids: vec![mem_id.to_string()],
            expected_version: None,
            caller_id: Some("app".to_string()),
            operation_id: Some(operation_id.to_string()),
        }
    }

    /// The lost-response case: the client never saw the reply and sent the very
    /// same write again. It must get the original response back and the page
    /// must be untouched — one version, one history row, not two.
    #[tokio::test]
    async fn page_write_same_operation_id_replays_instead_of_writing_again() {
        let (db, _dir, page_id, mem_id) = receipt_fixture("page_receipt_replay").await;
        let body = "A retried write must not become a second version of the page, ever";

        let first = update_page(
            &db,
            page_id,
            retry_req(body, mem_id, "op-1"),
            "re_distill",
            false,
            None,
            None,
        )
        .await
        .unwrap();
        assert!(first.wrote);
        let after_first = db.get_page(page_id).await.unwrap().unwrap();

        let replay = update_page(
            &db,
            page_id,
            retry_req(body, mem_id, "op-1"),
            "re_distill",
            false,
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(
            serde_json::to_string(&replay).unwrap(),
            serde_json::to_string(&first).unwrap(),
            "a replay returns the recorded response verbatim"
        );
        let after_replay = db.get_page(page_id).await.unwrap().unwrap();
        assert_eq!(
            after_replay.version, after_first.version,
            "the replay must not bump the version"
        );
        let versions: Vec<i64> = db
            .list_page_history(page_id, 10)
            .await
            .unwrap()
            .iter()
            .map(|h| h.version)
            .collect();
        assert_eq!(
            versions,
            vec![2, 1],
            "the replay must not append a second history row"
        );
    }

    /// The same operation id carrying a *different* write is not a retry — it
    /// is an id being reused. Replaying the old response would tell the caller
    /// their new text was saved when it never was, so refuse instead.
    #[tokio::test]
    async fn page_write_same_operation_id_with_different_body_is_a_conflict() {
        let (db, _dir, page_id, mem_id) = receipt_fixture("page_receipt_conflict").await;

        update_page(
            &db,
            page_id,
            retry_req(
                "A retried write must not become a second version of the page, ever",
                mem_id,
                "op-1",
            ),
            "re_distill",
            false,
            None,
            None,
        )
        .await
        .unwrap();
        let after_first = db.get_page(page_id).await.unwrap().unwrap();

        let err = update_page(
            &db,
            page_id,
            retry_req(
                "A retried write must not become a second version of the page — different text",
                mem_id,
                "op-1",
            ),
            "re_distill",
            false,
            None,
            None,
        )
        .await
        .expect_err("reusing an operation id for a different write must be refused");
        assert!(
            matches!(err, WenlanError::Conflict(_)),
            "expected a conflict, got {err:?}"
        );

        let unchanged = db.get_page(page_id).await.unwrap().unwrap();
        assert_eq!(unchanged.content, after_first.content);
        assert_eq!(unchanged.version, after_first.version);
    }

    /// A distinct operation id from the same caller is a distinct write and
    /// must land normally — the receipt table must not become a write blocker.
    #[tokio::test]
    async fn page_write_different_operation_id_writes_normally() {
        let (db, _dir, page_id, mem_id) = receipt_fixture("page_receipt_distinct").await;

        for (op, body) in [
            (
                "op-1",
                "A retried write must not become a second version of the page, once",
            ),
            (
                "op-2",
                "A retried write must not become a second version of the page, twice",
            ),
        ] {
            let r = update_page(
                &db,
                page_id,
                retry_req(body, mem_id, op),
                "re_distill",
                false,
                None,
                None,
            )
            .await
            .unwrap();
            assert!(r.wrote, "operation {op} must land");
        }

        let versions: Vec<i64> = db
            .list_page_history(page_id, 10)
            .await
            .unwrap()
            .iter()
            .map(|h| h.version)
            .collect();
        assert_eq!(versions, vec![3, 2, 1]);
    }

    /// A write refused by the version precondition must leave no receipt, or
    /// the caller's next honest attempt would replay a response for a write
    /// that never happened. (This pins the refusal path only — the refusal
    /// returns before the receipt is ever written. The transaction claim is
    /// pinned by `page_write_crash_before_commit_leaves_no_receipt`.)
    #[tokio::test]
    async fn page_write_refused_by_cas_records_no_receipt() {
        let (db, _dir, page_id, mem_id) = receipt_fixture("page_receipt_rollback").await;
        let current = db.get_page(page_id).await.unwrap().unwrap();

        let mut req = retry_req(
            "A retried write must not become a second version of the page, refused",
            mem_id,
            "op-doomed",
        );
        req.expected_version = Some(current.version + 99);

        let result = update_page(&db, page_id, req, "re_distill", false, None, None)
            .await
            .unwrap();
        assert!(!result.wrote, "precondition: the write must be refused");

        assert!(
            db.get_operation_receipt("app", "op-doomed")
                .await
                .unwrap()
                .is_none(),
            "a refused write must leave no receipt behind"
        );
    }

    /// The crash window: the page row, the history row, and the receipt are all
    /// staged, and the process dies before COMMIT. Nothing may survive — least
    /// of all the receipt, which would answer the caller's retry with "already
    /// done" about a version that does not exist. This is the only test that
    /// can tell an in-transaction receipt from an after-COMMIT one.
    #[tokio::test]
    async fn page_write_crash_before_commit_leaves_no_receipt() {
        let (db, _dir, page_id, mem_id) = receipt_fixture("page_receipt_crash").await;
        let before = db.get_page(page_id).await.unwrap().unwrap();

        *crate::db::FAIL_BEFORE_COMMIT.lock().unwrap() = Some(page_id.to_string());
        let result = update_page(
            &db,
            page_id,
            retry_req(
                "A retried write must not become a second version of the page, crashed",
                mem_id,
                "op-crash",
            ),
            "re_distill",
            false,
            None,
            None,
        )
        .await;
        assert!(
            result.is_err(),
            "precondition: the injected fault must abort the write; got {result:?}"
        );

        let after = db.get_page(page_id).await.unwrap().unwrap();
        assert_eq!(
            (after.content, after.version),
            (before.content, before.version),
            "the aborted write must have rolled back entirely"
        );
        assert!(
            db.get_operation_receipt("app", "op-crash")
                .await
                .unwrap()
                .is_none(),
            "a receipt must not outlive the transaction that wrote it"
        );

        // The retry now behaves as if the first attempt never happened.
        let retry = update_page(
            &db,
            page_id,
            retry_req(
                "A retried write must not become a second version of the page, crashed",
                mem_id,
                "op-crash",
            ),
            "re_distill",
            false,
            None,
            None,
        )
        .await
        .unwrap();
        assert!(
            retry.wrote,
            "after a rolled-back attempt the same operation id must still be usable"
        );
    }

    /// One id without the other is not a retry identity, and must not silently
    /// behave like one — otherwise two callers could collide on "op-1".
    #[tokio::test]
    async fn page_write_partial_retry_identity_is_ignored() {
        let (db, _dir, page_id, mem_id) = receipt_fixture("page_receipt_partial").await;

        let mut req = retry_req(
            "A retried write must not become a second version of the page, partial",
            mem_id,
            "op-partial",
        );
        req.caller_id = None;

        update_page(&db, page_id, req, "re_distill", false, None, None)
            .await
            .unwrap();

        assert!(
            db.get_operation_receipt("", "op-partial")
                .await
                .unwrap()
                .is_none(),
            "an operation id with no caller must not be recorded"
        );
    }

    /// A write that loses the CAS must leave no trace: no version bump and no
    /// history row. Otherwise the timeline would record edits that never
    /// happened, which is worse than no timeline at all.
    #[tokio::test]
    async fn page_write_refused_by_cas_appends_no_history_row() {
        let (db, _dir) = test_db().await;
        let mem_id = "mem-history-refused";
        let source_content = "A refused write must not appear in the page timeline";
        seed_memory(&db, mem_id, source_content).await;
        let now = chrono::Utc::now().to_rfc3339();
        let page_id = "page_history_refused";
        db.insert_page(
            page_id,
            "Refused",
            None,
            source_content,
            None,
            None,
            &[mem_id],
            &now,
        )
        .await
        .unwrap();
        let stale_version = db.get_page(page_id).await.unwrap().unwrap().version;

        let result = page_write(
            &db,
            PageWrite::Update {
                page_id,
                req: UpdatePageRequest {
                    content: "A refused write must not appear in the page timeline at all"
                        .to_string(),
                    source_memory_ids: vec![mem_id.to_string()],
                    expected_version: Some(stale_version + 5),
                    caller_id: None,
                    operation_id: None,
                },
                edited_by: "re_distill",
                require_stale: false,
                knowledge_path: None,
                citations: None,
            },
        )
        .await
        .unwrap();

        assert!(!result.wrote, "precondition: the write must be refused");
        let history = db.list_page_history(page_id, 10).await.unwrap();
        assert_eq!(
            history.len(),
            1,
            "a refused write must not append a history row"
        );
        assert_eq!(history[0].version, stale_version);
    }

    /// The load-bearing test for the M0 write gate: an edit that lands *inside*
    /// the window between the ownership decision and the write must not be
    /// clobbered.
    ///
    /// This is the only test here that fails when `Some(current_version)` is
    /// dropped from the write — with no interleaving, a guarded write and an
    /// unguarded one are byte-identical. Mutation check: pass `None` as the
    /// guard and this test overwrites the human edit and reports v3.
    #[tokio::test]
    async fn page_write_update_edit_landing_mid_write_is_not_clobbered() {
        let (db, _dir) = test_db().await;
        let mem_id = "mem-cas-interleave";
        let source_content = "A write must land on the row its ownership decision was made from";
        seed_memory(&db, mem_id, source_content).await;
        let now = chrono::Utc::now().to_rfc3339();
        let page_id = "page_cas_interleave";
        db.insert_page(
            page_id,
            "Interleaved Write",
            None,
            source_content,
            None,
            None,
            &[mem_id],
            &now,
        )
        .await
        .unwrap();
        let start_version = db.get_page(page_id).await.unwrap().unwrap().version;

        // Arm the seam: the machine write below parks after deciding ownership
        // (page is still machine-owned) and before writing.
        let (parked_tx, parked_rx) = tokio::sync::oneshot::channel();
        let (go_tx, go_rx) = tokio::sync::oneshot::channel();
        *PRE_WRITE_GATE.lock().unwrap() = Some((parked_tx, go_rx));

        // Close to the seeded source: `fs_edit` is not exempt from the
        // hallucination guard, so an unrelated body would be rejected before
        // ever reaching the write this test is about.
        let human_content =
            "A write must land on the row its ownership decision was made from, typed by hand";
        let machine_write = page_write(
            &db,
            PageWrite::Update {
                page_id,
                req: UpdatePageRequest {
                    content: "Machine body computed from the pre-edit row".to_string(),
                    source_memory_ids: vec![mem_id.to_string()],
                    expected_version: None,
                    caller_id: None,
                    operation_id: None,
                },
                edited_by: "re_distill",
                require_stale: false,
                knowledge_path: None,
                citations: None,
            },
        );

        // Runs to completion while the machine write is parked — a genuine
        // interleaving, not a simulated one.
        let human_edit = async {
            parked_rx
                .await
                .expect("machine write must reach the pre-write gate");
            let result = page_write(
                &db,
                PageWrite::Update {
                    page_id,
                    req: UpdatePageRequest {
                        content: human_content.to_string(),
                        source_memory_ids: vec![mem_id.to_string()],
                        expected_version: None,
                        caller_id: None,
                        operation_id: None,
                    },
                    edited_by: "fs_edit",
                    require_stale: false,
                    knowledge_path: None,
                    citations: None,
                },
            )
            .await
            .unwrap();
            // Only now release the parked machine write, so it resumes against a
            // page that has definitively moved.
            go_tx.send(()).expect("machine write must still be parked");
            result
        };

        let (machine_result, human_result) = tokio::join!(machine_write, human_edit);
        let machine_result = machine_result.unwrap();

        assert!(human_result.wrote, "the human edit itself must land");
        assert!(
            !machine_result.wrote,
            "the machine write lost the CAS and must not report a write"
        );

        let after = db.get_page(page_id).await.unwrap().unwrap();
        assert_eq!(
            after.content, human_content,
            "the edit that landed mid-write must survive"
        );
        assert_eq!(
            after.version,
            start_version + 1,
            "only the human edit bumped the version; the losing write must not have applied"
        );
        assert!(
            machine_result.revision_card_id.is_some(),
            "on reload the page is human-owned, so the machine body is preserved as a card"
        );
    }

    /// A machine writer that DECLARED the version it read (`expected_version`)
    /// and lost the race to a human edit is refused outright — it does not get a
    /// revision card.
    ///
    /// This reverses what this test asserted when M0-C landed. Carding looked
    /// like the conservative choice ("never drop agent work"), but a card is not
    /// inert: `accept_page_revision_card` re-reads the page and writes the card's
    /// body over whatever it finds, without checking the base that body was
    /// computed from. So a card staged from a stale base silently reverts the
    /// human edit the moment someone accepts it — the exact loss M0 exists to
    /// prevent, just deferred to accept time. Refusing drops no agent work
    /// either: the caller sees the conflict, re-reads, and stages a card against
    /// the real content.
    ///
    /// Only writers that declare a base get this. A machine write with
    /// `expected_version: None` never told us what it read, so a card is still
    /// the best available answer — see
    /// `page_write_update_edit_landing_mid_write_is_not_clobbered`, which is
    /// unaffected by this and still stages one.
    #[tokio::test]
    async fn page_write_update_stale_version_on_human_owned_page_is_refused() {
        let (db, _dir) = test_db().await;
        let mem_id = "mem-cas-owned";
        let source_content = "Ownership is re-decided against the row the write lands on";
        seed_memory(&db, mem_id, source_content).await;
        let now = chrono::Utc::now().to_rfc3339();
        let page_id = "page_cas_owned";
        db.insert_page(
            page_id,
            "Ownership Recheck",
            None,
            source_content,
            None,
            None,
            &[mem_id],
            &now,
        )
        .await
        .unwrap();

        // The machine writer's view of the world: machine-owned, at this version.
        let machine_view = db.get_page(page_id).await.unwrap().unwrap();
        assert!(
            !machine_view.user_edited,
            "precondition: page starts machine-owned"
        );

        // A human edit lands underneath it, taking ownership and bumping version.
        let human_content = "Ownership is re-decided against the row the write lands on, by hand";
        page_write(
            &db,
            PageWrite::Update {
                page_id,
                req: UpdatePageRequest {
                    content: human_content.to_string(),
                    source_memory_ids: vec![mem_id.to_string()],
                    expected_version: Some(machine_view.version),
                    caller_id: None,
                    operation_id: None,
                },
                edited_by: "fs_edit",
                require_stale: false,
                knowledge_path: None,
                citations: None,
            },
        )
        .await
        .unwrap();
        let owned = db.get_page(page_id).await.unwrap().unwrap();
        assert!(
            owned.user_edited,
            "precondition: the human edit took ownership"
        );

        let pending_before = db.list_pending_revisions(10).await.unwrap().len();

        // The machine writer proceeds from its stale view.
        let result = page_write(
            &db,
            PageWrite::Update {
                page_id,
                req: UpdatePageRequest {
                    content: "Machine body that must never overwrite the human edit".to_string(),
                    source_memory_ids: vec![mem_id.to_string()],
                    expected_version: Some(machine_view.version),
                    caller_id: None,
                    operation_id: None,
                },
                edited_by: "re_distill",
                require_stale: false,
                knowledge_path: None,
                citations: None,
            },
        )
        .await
        .unwrap();

        assert!(
            !result.wrote,
            "machine write must not land on a human-owned page"
        );
        assert!(
            result.warnings.iter().any(|w| w.contains("write refused")),
            "the caller must be told its write was refused, not handed a silent no-op; got {:?}",
            result.warnings
        );
        let after = db.get_page(page_id).await.unwrap().unwrap();
        assert_eq!(
            after.content, human_content,
            "human prose must survive the losing machine write"
        );
        assert_eq!(
            after.version, owned.version,
            "the refused machine write must not bump the version"
        );
        let pending_after = db.list_pending_revisions(10).await.unwrap().len();
        assert_eq!(
            pending_after, pending_before,
            "a declared-base conflict is refused outright; staging a card here would \
             let accept-time silently revert the human edit"
        );
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
    async fn accept_pending_revision_page_write_card_updates_page_content() {
        let (db, _dir) = test_db().await;
        let knowledge_dir = tempfile::tempdir().unwrap();
        let mem_id = "mem_page_accept_original";
        let new_mem_id = "mem_page_accept_new";
        let original_content = "Rust ownership keeps memory safety rules explicit";
        let human_content = "Rust ownership keeps memory safety rules explicit, with human notes";
        let proposed_content =
            "Rust ownership lets the compiler enforce memory safety during page refresh";

        seed_memory(&db, mem_id, original_content).await;
        seed_memory(&db, new_mem_id, proposed_content).await;
        let page_id = seed_page(&db, mem_id, original_content).await;
        update_page(
            &db,
            &page_id,
            UpdatePageRequest {
                content: human_content.to_string(),
                source_memory_ids: vec![mem_id.to_string()],
                expected_version: None,
                caller_id: None,
                operation_id: None,
            },
            "fs_edit",
            false,
            None,
            None,
        )
        .await
        .unwrap();

        let before = db.get_page(&page_id).await.unwrap().unwrap();
        assert!(before.user_edited, "precondition: page is human-owned");

        let card = stage_page_revision_card(
            &db,
            &before,
            proposed_content,
            &[mem_id.to_string(), new_mem_id.to_string()],
            "page_growth",
        )
        .await
        .unwrap();
        let card_id = card
            .revision_card_id
            .as_deref()
            .expect("staged page card must return an id");

        let accepted = accept_pending_revision_with_knowledge_path(
            &db,
            card_id,
            "test-agent",
            Some(knowledge_dir.path()),
        )
        .await
        .unwrap();
        assert_eq!(accepted.target_source_id, page_id);
        assert_eq!(accepted.revision_source_id, card_id);
        assert!(accepted.wrote);

        let after = db.get_page(&page_id).await.unwrap().unwrap();
        assert_eq!(
            after.content, proposed_content,
            "accepting a page-write card must apply the proposed prose to the page"
        );
        assert_eq!(
            after.source_memory_ids,
            vec![mem_id.to_string(), new_mem_id.to_string()],
            "accepting a page-write card must apply its proposed source set"
        );
        assert_eq!(
            after.version,
            before.version + 1,
            "accepting a page-write card must bump the page version"
        );
        assert!(
            db.list_pending_revisions(10).await.unwrap().is_empty(),
            "accepted page-write card must leave the pending revision queue"
        );

        let writer =
            crate::export::knowledge::KnowledgeWriter::new(knowledge_dir.path().to_path_buf(), &db);
        let filename = writer
            .page_filename(&page_id)
            .expect("accepted page-write card must refresh the markdown projection");
        let markdown = std::fs::read_to_string(knowledge_dir.path().join(filename)).unwrap();
        assert!(
            markdown.contains(proposed_content),
            "markdown projection must contain the accepted page prose"
        );
        assert!(
            markdown.contains(&format!("origin_version: {}", after.version)),
            "markdown projection must carry the accepted page version"
        );
    }

    #[tokio::test]
    async fn accept_page_revision_consume_failure_keeps_page_retryable() {
        let (db, _dir) = test_db().await;
        let mem_id = "mem_page_accept_abort_original";
        let new_mem_id = "mem_page_accept_abort_new";
        let original_content = "original page content before failed revision acceptance";
        let proposed_content = "proposed page content must commit with card consumption";

        seed_memory(&db, mem_id, original_content).await;
        seed_memory(&db, new_mem_id, proposed_content).await;
        let page_id = seed_page(&db, mem_id, original_content).await;
        let before = db.get_page(&page_id).await.unwrap().unwrap();
        let card = stage_page_revision_card(
            &db,
            &before,
            proposed_content,
            &[mem_id.to_string(), new_mem_id.to_string()],
            "page_growth",
        )
        .await
        .unwrap();
        let card_id = card.revision_card_id.unwrap();

        {
            let conn = db.conn.lock().await;
            conn.execute_batch(&format!(
                "CREATE TRIGGER abort_page_revision_consume
                 BEFORE UPDATE OF pending_revision ON memories
                 WHEN OLD.source_id = '{}' AND OLD.pending_revision = 1
                 BEGIN SELECT RAISE(ABORT, 'blocked revision consume'); END;",
                card_id.replace('\'', "''")
            ))
            .await
            .unwrap();
        }

        let err = accept_pending_revision(&db, &card_id, "test-agent")
            .await
            .expect_err("consume fault must fail the acceptance");
        assert!(err.to_string().contains("blocked revision consume"));
        let after_failure = db.get_page(&page_id).await.unwrap().unwrap();
        let pending = db.list_pending_revisions(10).await.unwrap();
        assert!(pending
            .iter()
            .any(|revision| revision.revision_source_id == card_id));
        {
            let conn = db.conn.lock().await;
            conn.execute("DROP TRIGGER abort_page_revision_consume", ())
                .await
                .unwrap();
        }
        let retry = accept_pending_revision(&db, &card_id, "test-agent").await;
        assert_eq!(
            after_failure.content, before.content,
            "failed card consumption must not commit Page content first"
        );
        assert_eq!(
            after_failure.version, before.version,
            "failed card consumption must leave the Page version retryable"
        );
        assert!(
            retry.is_ok(),
            "retry after the fault is removed must converge"
        );
        assert!(db.list_pending_revisions(10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn accept_page_revision_source_failure_keeps_page_retryable() {
        let (db, _dir) = test_db().await;
        let mem_id = "mem_page_source_abort_original";
        let new_mem_id = "mem_page_source_abort_new";
        let original_content = "original page content before source attachment failure";
        let proposed_content = "proposed page content must commit with exact sources";

        seed_memory(&db, mem_id, original_content).await;
        seed_memory(&db, new_mem_id, proposed_content).await;
        let page_id = seed_page(&db, mem_id, original_content).await;
        let before = db.get_page(&page_id).await.unwrap().unwrap();
        let card = stage_page_revision_card(
            &db,
            &before,
            proposed_content,
            &[mem_id.to_string(), new_mem_id.to_string()],
            "page_growth",
        )
        .await
        .unwrap();
        let card_id = card.revision_card_id.unwrap();

        {
            let conn = db.conn.lock().await;
            conn.execute_batch(&format!(
                "CREATE TRIGGER abort_page_revision_source
                 BEFORE INSERT ON page_sources
                 WHEN NEW.memory_source_id = '{}'
                 BEGIN SELECT RAISE(ABORT, 'blocked revision source attachment'); END;",
                new_mem_id.replace('\'', "''")
            ))
            .await
            .unwrap();
        }

        let err = accept_pending_revision(&db, &card_id, "test-agent")
            .await
            .expect_err("source attachment fault must fail the acceptance");
        assert!(err
            .to_string()
            .contains("blocked revision source attachment"));
        let after_failure = db.get_page(&page_id).await.unwrap().unwrap();
        assert_eq!(after_failure.content, before.content);
        assert_eq!(after_failure.version, before.version);
        assert_eq!(after_failure.source_memory_ids, before.source_memory_ids);
        assert!(db
            .list_pending_revisions(10)
            .await
            .unwrap()
            .iter()
            .any(|revision| revision.revision_source_id == card_id));

        {
            let conn = db.conn.lock().await;
            conn.execute("DROP TRIGGER abort_page_revision_source", ())
                .await
                .unwrap();
        }
        accept_pending_revision(&db, &card_id, "test-agent")
            .await
            .expect("retry after the source fault must converge");
        assert!(db.list_pending_revisions(10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn accept_pending_revision_page_write_card_conflicts_when_page_version_changed() {
        let (db, _dir) = test_db().await;
        let mem_id = "mem_page_accept_conflict_original";
        let new_mem_id = "mem_page_accept_conflict_new";
        let original_content = "Rust ownership keeps memory safety rules explicit";
        let human_content = "Rust ownership keeps memory safety rules explicit, with human notes";
        let proposed_content =
            "Rust ownership lets the compiler enforce memory safety during page refresh";
        let newer_human_content =
            "Rust ownership keeps memory safety rules explicit, with newer human notes";

        seed_memory(&db, mem_id, original_content).await;
        seed_memory(&db, new_mem_id, proposed_content).await;
        let page_id = seed_page(&db, mem_id, original_content).await;
        update_page(
            &db,
            &page_id,
            UpdatePageRequest {
                content: human_content.to_string(),
                source_memory_ids: vec![mem_id.to_string()],
                expected_version: None,
                caller_id: None,
                operation_id: None,
            },
            "fs_edit",
            false,
            None,
            None,
        )
        .await
        .unwrap();

        let before = db.get_page(&page_id).await.unwrap().unwrap();
        let staged_version = before.version;
        let card = stage_page_revision_card(
            &db,
            &before,
            proposed_content,
            &[mem_id.to_string(), new_mem_id.to_string()],
            "page_growth",
        )
        .await
        .unwrap();
        let card_id = card
            .revision_card_id
            .as_deref()
            .expect("staged page card must return an id");

        update_page(
            &db,
            &page_id,
            UpdatePageRequest {
                content: newer_human_content.to_string(),
                source_memory_ids: vec![mem_id.to_string()],
                expected_version: None,
                caller_id: None,
                operation_id: None,
            },
            "fs_edit",
            false,
            None,
            None,
        )
        .await
        .unwrap();

        let err = accept_pending_revision(&db, card_id, "test-agent")
            .await
            .unwrap_err();
        match err {
            WenlanError::Conflict(msg) => {
                assert!(
                    msg.contains(&format!("staged version {staged_version}")),
                    "conflict message must name the staged version, got: {msg}"
                );
                assert!(
                    msg.contains(&format!("current version {}", staged_version + 1)),
                    "conflict message must name the current version, got: {msg}"
                );
            }
            other => panic!("expected version conflict, got {other:?}"),
        }

        let after = db.get_page(&page_id).await.unwrap().unwrap();
        assert_eq!(
            after.content, newer_human_content,
            "stale page-write card must not overwrite newer human prose"
        );
        assert!(
            db.list_pending_revisions(10)
                .await
                .unwrap()
                .iter()
                .any(|row| row.revision_source_id == card_id),
            "conflicted page-write card must remain pending"
        );
    }

    #[tokio::test]
    async fn accept_pending_revision_legacy_page_write_card_without_version_still_accepts() {
        let (db, _dir) = test_db().await;
        let mem_id = "mem_page_accept_legacy_original";
        let new_mem_id = "mem_page_accept_legacy_new";
        let original_content = "Rust ownership keeps memory safety rules explicit";
        let human_content = "Rust ownership keeps memory safety rules explicit, with human notes";
        let proposed_content =
            "Rust ownership lets the compiler enforce memory safety during page refresh";

        seed_memory(&db, mem_id, original_content).await;
        seed_memory(&db, new_mem_id, proposed_content).await;
        let page_id = seed_page(&db, mem_id, original_content).await;
        update_page(
            &db,
            &page_id,
            UpdatePageRequest {
                content: human_content.to_string(),
                source_memory_ids: vec![mem_id.to_string()],
                expected_version: None,
                caller_id: None,
                operation_id: None,
            },
            "fs_edit",
            false,
            None,
            None,
        )
        .await
        .unwrap();

        let before = db.get_page(&page_id).await.unwrap().unwrap();
        let card = stage_page_revision_card(
            &db,
            &before,
            proposed_content,
            &[mem_id.to_string(), new_mem_id.to_string()],
            "page_growth",
        )
        .await
        .unwrap();
        let card_id = card
            .revision_card_id
            .as_deref()
            .expect("staged page card must return an id");
        {
            let conn = db.conn.lock().await;
            let mut rows = conn
                .query(
                    "SELECT structured_fields FROM memories WHERE source_id = ?1",
                    libsql::params![card_id.to_string()],
                )
                .await
                .unwrap();
            let row = rows
                .next()
                .await
                .unwrap()
                .expect("revision card row must exist");
            let structured_fields = row.get::<String>(0).unwrap();
            drop(rows);

            let mut structured: serde_json::Value =
                serde_json::from_str(&structured_fields).unwrap();
            structured
                .as_object_mut()
                .expect("structured_fields must be an object")
                .remove("page_version");
            conn.execute(
                "UPDATE memories SET structured_fields = ?1 WHERE source_id = ?2",
                libsql::params![structured.to_string(), card_id.to_string()],
            )
            .await
            .unwrap();
        }

        let accepted = accept_pending_revision(&db, card_id, "test-agent")
            .await
            .unwrap();
        assert_eq!(accepted.target_source_id, page_id);
        assert_eq!(accepted.revision_source_id, card_id);
        assert!(accepted.wrote);

        let after = db.get_page(&page_id).await.unwrap().unwrap();
        assert_eq!(
            after.content, proposed_content,
            "legacy page-write cards without page_version must still accept"
        );
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
            expected_version: None,
            caller_id: None,
            operation_id: None,
        };
        let result = update_page(&db, &page_id, req, "distill", false, None, None).await;
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
            expected_version: None,
            caller_id: None,
            operation_id: None,
        };
        let result = update_page(&db, &page_id, req, "page_growth", false, None, None).await;
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
            expected_version: None,
            caller_id: None,
            operation_id: None,
        };
        let result = update_page(&db, &page_id, req, "distill", false, None, None)
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
            expected_version: None,
            caller_id: None,
            operation_id: None,
        };
        // manual_edit bypasses hallucination guard AND is NOT an LLM rewrite
        // so shrink-guard must NOT fire even though the body shrinks drastically
        // (hallucination guard WILL fire for manual_edit -- seed with real-ish content)
        // Actually manual_edit triggers hallucination guard, so use fs_edit instead
        let result = update_page(&db, &page_id, req, "fs_edit", false, None, None).await;
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
        assert!(Writer::classify("distill").is_llm_rewrite());
        assert!(Writer::classify("re_distill").is_llm_rewrite());
        assert!(Writer::classify("page_growth").is_llm_rewrite());
        assert!(Writer::classify("refinery_merge").is_llm_rewrite());
    }

    #[test]
    fn is_llm_rewrite_user_false() {
        assert!(!Writer::classify("user").is_llm_rewrite());
        assert!(!Writer::classify("manual_edit").is_llm_rewrite());
        assert!(!Writer::classify("fs_edit").is_llm_rewrite());
        assert!(!Writer::classify("api").is_llm_rewrite());
        assert!(!Writer::classify("").is_llm_rewrite());
    }

    // ── Writer classification ───────────────────────────────────────────────

    /// Every `edited_by` value this build persists, paired with the three
    /// authority answers the write gate derives from it. This table is the
    /// characterization pin: it is exactly what the string helpers
    /// (`is_machine_page_write` / `skip_hallucination_guard` /
    /// `is_llm_rewrite`) returned before `Writer` replaced them, so a drift in
    /// any single classification fails here rather than silently changing who
    /// wins an ownership decision inside the CAS.
    const WRITER_TABLE: &[(&str, bool, bool, bool)] = &[
        // edited_by            machine  skips_guard  llm_rewrite
        ("manual_edit", false, false, false),
        ("fs_edit", false, false, false),
        ("distill", true, true, true),
        ("re_distill", true, true, true),
        ("page_growth", true, true, true),
        ("refinery_merge", true, true, true),
        ("agent_refresh", true, true, false),
        // Persisted in `page_history.edited_by` / `pages.changelog` but never
        // routed through the write gate today — these paths write via the db
        // layer (db.rs `create`/`migration_73`, citations.rs, revision accept).
        // Unrecognized by the classifier, and unrecognized means machine.
        ("create", true, false, false),
        ("revision_accept", true, false, false),
        ("citation_backfill", true, false, false),
        ("migration_73", true, false, false),
    ];

    #[test]
    fn writer_table_pins_gate_classification() {
        for &(edited_by, machine, skips_guard, llm_rewrite) in WRITER_TABLE {
            let w = Writer::classify(edited_by);
            assert_eq!(w.is_machine(), machine, "is_machine for {edited_by:?}");
            assert_eq!(
                w.skips_hallucination_guard(),
                skips_guard,
                "skips_hallucination_guard for {edited_by:?}"
            );
            assert_eq!(
                w.is_llm_rewrite(),
                llm_rewrite,
                "is_llm_rewrite for {edited_by:?}"
            );
        }
    }

    /// The persisted-string contract: these bytes are already in users'
    /// databases, so classification must never rewrite them.
    #[test]
    fn writer_round_trips_persisted_string() {
        for &(edited_by, ..) in WRITER_TABLE {
            assert_eq!(
                Writer::classify(edited_by).as_str(),
                edited_by,
                "round-trip for {edited_by:?}"
            );
        }
        // Unrecognized strings round-trip too — the type is a lens over the
        // string, not a replacement for it.
        for odd in ["manual_edt", "", "Distill", "totally_new_stage"] {
            assert_eq!(
                Writer::classify(odd).as_str(),
                odd,
                "round-trip for {odd:?}"
            );
        }
    }

    /// The bug this type exists to bound: an unrecognized writer string used to
    /// fall through `!matches!(..)` into "machine" with no diagnostic. That
    /// direction is preserved deliberately — machine is the fail-safe answer,
    /// because a machine write to a human-owned page is staged as a revision
    /// card instead of overwriting the human's prose.
    #[test]
    fn unknown_writer_is_machine_and_guarded() {
        let typo = Writer::classify("manual_edt");
        assert!(
            typo.is_machine(),
            "a typo'd human writer must not be trusted"
        );
        assert!(
            !typo.skips_hallucination_guard(),
            "an unknown writer must not skip the hallucination guard"
        );
        assert!(!typo.is_llm_rewrite());
        assert!(matches!(typo, Writer::Pipeline(PipelineStage::Unknown(_))));
    }

    #[test]
    fn known_writers_are_not_unknown() {
        for &(edited_by, ..) in WRITER_TABLE {
            let is_unknown = matches!(
                Writer::classify(edited_by),
                Writer::Pipeline(PipelineStage::Unknown(_))
            );
            // Only the four db-layer writers are outside the gate's vocabulary.
            let expected_unknown = matches!(
                edited_by,
                "create" | "revision_accept" | "citation_backfill" | "migration_73"
            );
            assert_eq!(
                is_unknown, expected_unknown,
                "unknown-ness of {edited_by:?}"
            );
        }
    }
}
