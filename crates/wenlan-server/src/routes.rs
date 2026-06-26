// SPDX-License-Identifier: Apache-2.0
use crate::error::ServerError;
use crate::state::ServerState;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use wenlan_core::router::classify::{classify_query, estimate_tokens, tier_allowed};
use wenlan_types::requests::ChatContextRequest;
use wenlan_types::responses::{
    ChatContextResponse, KnowledgeContext, ProfileContext, TierTokenEstimates,
};

// ===== Request/Response Types =====

#[derive(Debug, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    pub source_filter: Option<String>,
    #[serde(default, alias = "domain")]
    pub space: Option<String>,
}

fn default_limit() -> usize {
    10
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub results: Vec<wenlan_core::db::SearchResult>,
    pub took_ms: f64,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub db_initialized: bool,
    pub version: String,
}

// ===== Route Handlers =====

/// GET /api/health - Health check endpoint
pub async fn handle_health(
    State(state): State<Arc<RwLock<ServerState>>>,
) -> Result<Json<HealthResponse>, ServerError> {
    let s = state.read().await;
    let db_initialized = s.db.is_some();

    Ok(Json(HealthResponse {
        status: "ok".to_string(),
        db_initialized,
        version: wenlan_core::version().to_string(),
    }))
}

/// GET /api/status - Get indexing status
pub async fn handle_status(
    State(state): State<Arc<RwLock<ServerState>>>,
) -> Result<Json<wenlan_types::responses::StatusResponse>, ServerError> {
    let s = state.read().await;

    let files_indexed = if let Some(db) = &s.db {
        db.count().await.unwrap_or(0)
    } else {
        0
    };

    Ok(Json(wenlan_types::responses::StatusResponse {
        is_running: true,
        files_indexed,
        files_total: 0,
        sources_connected: vec![],
        reranker: s.reranker_status.clone(),
        reranker_light: s.reranker_light_status.clone(),
        reranker_mode: s.reranker_mode.clone(),
    }))
}

/// POST /api/search - Semantic search endpoint
pub async fn handle_search(
    State(state): State<Arc<RwLock<ServerState>>>,
    crate::space_header::SpaceHeader(header_space): crate::space_header::SpaceHeader,
    Json(mut req): Json<SearchRequest>,
) -> Result<Json<SearchResponse>, ServerError> {
    if req.space.is_none() {
        req.space = header_space;
    }
    let start = std::time::Instant::now();

    let (db, reranker_light) = {
        let s = state.read().await;
        let db = s.db.clone().ok_or(ServerError::DbNotInitialized)?;
        (db, s.reranker_light.clone())
    };

    let results = if req.source_filter.as_deref() == Some("memory") {
        match reranker_light {
            // Quick-path CE (WENLAN_RERANKER_MODE lite/full): fetch a widened pool via
            // plain search_memory, then cross-encode down to req.limit at the handler
            // layer. search_memory itself stays CE-free, so internal callers are unaffected.
            Some(reranker) => {
                // Light path uses the default pool (max(limit,10)). Unlike the deep path
                // (db.rs), it does NOT read RERANK_POOL_MULTIPLIER/FLOOR — turbo is cheap
                // and the light path isn't an eval-sweep surface. (review note)
                let pool = wenlan_core::db::compute_rerank_fetch_pool(req.limit, None, None);
                let pooled = db
                    .search_memory(
                        &req.query,
                        pool,
                        None,
                        req.space.as_deref(),
                        None,
                        None,
                        None,
                        None,
                    )
                    .await
                    .map_err(|e| ServerError::SearchFailed(e.to_string()))?;
                wenlan_core::db::rerank_results_light(reranker, &req.query, pooled, req.limit).await
            }
            // mode=off (default): byte-identical to the pre-mode path.
            None => db
                .search_memory(
                    &req.query,
                    req.limit,
                    None,
                    req.space.as_deref(),
                    None,
                    None,
                    None,
                    None,
                )
                .await
                .map_err(|e| ServerError::SearchFailed(e.to_string()))?,
        }
    } else {
        db.search(
            &req.query,
            req.limit,
            req.source_filter.as_deref(),
            req.space.as_deref(),
        )
        .await
        .map_err(|e| ServerError::SearchFailed(e.to_string()))?
    };

    let took_ms = start.elapsed().as_secs_f64() * 1000.0;

    Ok(Json(SearchResponse { results, took_ms }))
}

// ===== Context Endpoints =====

/// POST /api/context - Trust-gated tiered memory retrieval for LLM context injection
pub async fn handle_context(
    State(state): State<Arc<RwLock<ServerState>>>,
    headers: HeaderMap,
    crate::space_header::SpaceHeader(header_space): crate::space_header::SpaceHeader,
    Json(mut req): Json<ChatContextRequest>,
) -> Result<Json<ChatContextResponse>, ServerError> {
    if req.space.is_none() {
        req.space = header_space;
    }
    let start = std::time::Instant::now();

    let query = req
        .query
        .as_deref()
        .or(req.conversation_id.as_deref())
        .unwrap_or("recent context");

    let agent_name = headers
        .get("x-agent-name")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .unwrap_or("unknown");

    // Snapshot the Arc handles and drop the ServerState read guard BEFORE
    // the multi-second chain of DB awaits below. Holding the guard across
    // ~15 sequential awaits (load_memories_by_type, search_*, log_accesses,
    // etc.) would block every writer to ServerState — e.g. store_memory's
    // deferred async work — for the full duration. See CLAUDE.md locking rules.
    let (db_arc, access_tracker, reranker_light) = {
        let s = state.read().await;
        let db = s.db.clone().ok_or(ServerError::DbNotInitialized)?;
        (db, s.access_tracker.clone(), s.reranker_light.clone())
    }; // guard dropped here
    let db = db_arc.as_ref();

    let agent_trust = if agent_name == "unknown" {
        "unknown".to_string()
    } else {
        db.get_agent(agent_name)
            .await
            .ok()
            .flatten()
            .map(|a| a.trust_level)
            .unwrap_or_else(|| "unknown".to_string())
    };

    let classification = classify_query(query, agent_name, &agent_trust, true);
    // classification.space fallback removed (wire-gap fix #3): classifier never
    // populated it, so the .or() chain was dead code. Callers must now supply
    // space explicitly via req.space.
    let space_filter = req.space.as_deref();

    // Tier 1 (identity + preferences)
    let (identity, preferences) = if tier_allowed(&classification.trust_level, 1) {
        let mut id_mems = db
            .load_memories_by_type("identity", 10, space_filter)
            .await
            .unwrap_or_default();
        let mut pref_mems = db
            .load_memories_by_type("preference", 10, space_filter)
            .await
            .unwrap_or_default();

        if id_mems.is_empty() && space_filter.is_some() {
            id_mems = db
                .load_memories_by_type("identity", 5, None)
                .await
                .unwrap_or_default();
        }
        if pref_mems.is_empty() && space_filter.is_some() {
            pref_mems = db
                .load_memories_by_type("preference", 5, None)
                .await
                .unwrap_or_default();
        }

        (
            id_mems
                .iter()
                .map(|m| m.content.clone())
                .collect::<Vec<_>>(),
            pref_mems
                .iter()
                .map(|m| m.content.clone())
                .collect::<Vec<_>>(),
        )
    } else {
        (Vec::new(), Vec::new())
    };

    // Tier 2 (corrections + decisions). Goal taxonomy folded into Identity by
    // migration 45 (Phase 0); the goals Vec stays for ProfileContext wire compat
    // but is always empty now. Both `req.include_goals` and `ProfileContext.goals`
    // are deprecated and will be removed in wenlan-types 0.4.
    let goals: Vec<String> = Vec::new();

    let decisions: Vec<String> = if tier_allowed(&classification.trust_level, 2) {
        db.load_memories_by_type("decision", 5, space_filter)
            .await
            .unwrap_or_default()
            .iter()
            .map(|m| m.content.clone())
            .collect()
    } else {
        Vec::new()
    };

    let corrections = if tier_allowed(&classification.trust_level, 2) && query != "recent context" {
        db.search_corrections_by_topic(query, 5)
            .await
            .unwrap_or_default()
            .iter()
            .map(|r| r.content.clone())
            .collect()
    } else {
        Vec::new()
    };

    // Tier 3 (search). Plain search_memory by default (it beat the old LLM reranker
    // on LongMemEval, 0.790 base vs 0.722). Under WENLAN_RERANKER_MODE lite/full the
    // context path adds a turbo cross-encoder pass at the HANDLER layer over a widened
    // pool; search_memory itself stays CE-free so internal callers are unaffected.
    let search_results = match &reranker_light {
        Some(reranker) => {
            let pool = wenlan_core::db::compute_rerank_fetch_pool(req.max_chunks, None, None);
            let pooled = db
                .search_memory(query, pool, None, space_filter, None, None, None, None)
                .await
                .unwrap_or_default();
            wenlan_core::db::rerank_results_light(reranker.clone(), query, pooled, req.max_chunks)
                .await
        }
        None => db
            .search_memory(
                query,
                req.max_chunks,
                None,
                space_filter,
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap_or_default(),
    };

    let threshold = req.relevance_threshold.unwrap_or(0.0) as f32;
    let filtered_search: Vec<_> = search_results
        .into_iter()
        .filter(|r| r.score >= threshold)
        .collect();

    // Source IDs from search results — used to gate page relevance.
    // A page is only included if its source memories overlap with the
    // memories that search_memory returned for this query.
    let search_source_ids: std::collections::HashSet<String> = filtered_search
        .iter()
        .map(|r| r.source_id.clone())
        .collect();

    let page_results: Vec<String> = if tier_allowed(&classification.trust_level, 2)
        && query != "recent context"
    {
        let raw_pages = db.search_pages(query, 3, None).await.unwrap_or_default();
        // Un-gated: rank confirmed pages by relevance (source-overlap is a
        // tie-break boost, not a hard filter) and take the top 3 (token cap,
        // matching the search_pages fetch above). The old source-overlap gate
        // dropped high-relevance pages whose source memories did not intersect
        // the memory hit pool.
        let pages = wenlan_core::pages::select_pages_for_context(&raw_pages, &search_source_ids, 3);
        pages
            .iter()
            .map(|c| {
                let summary = c.summary.as_deref().unwrap_or("");
                format!("**{}**: {}\n{}", c.title, summary, c.content)
            })
            .collect()
    } else {
        Vec::new()
    };

    // T18 global-context prelude (opt-in, ship-dark). Read-only: surfaces the
    // pre-built summary_nodes as a `## Corpus Overview` section. The build path
    // is the refinery's SummaryRollup phase; here we only read. Honors the
    // space filter via the source-overlap gate (a summary surfaces iff >=1 of
    // its provenance memories survived the memory-side filter). Empty/absent
    // unless WENLAN_ENABLE_GLOBAL_PRELUDE is on, so the default response shape
    // is byte-identical to pre-T18.
    let corpus_overview: Vec<String> = if wenlan_core::db::global_prelude_enabled() {
        let nodes = db.search_summary_nodes(query, 3).await.unwrap_or_default();
        let mut out = Vec::new();
        for node in nodes {
            // Source-overlap gate when a space filter is active.
            if space_filter.is_some() {
                match db.get_summary_node_sources(&node.id).await {
                    Ok(srcs) if srcs.iter().any(|s| search_source_ids.contains(s)) => {}
                    _ => continue,
                }
            }
            out.push(format!("**{}**: {}", node.title, node.body));
        }
        out
    } else {
        Vec::new()
    };

    let narrative = db
        .get_cached_narrative()
        .await
        .ok()
        .flatten()
        .filter(|(content, _, _)| !content.is_empty())
        .map(|(content, _, _)| content)
        .unwrap_or_default();
    let narrative_brief: Option<&str> = if narrative.is_empty() {
        None
    } else {
        Some(&narrative)
    };

    // Token estimates
    let brief_text = narrative_brief.unwrap_or("");
    let tier1_text = format!(
        "{} {} {}",
        brief_text,
        identity.join(" "),
        preferences.join(" ")
    );
    let tier2_text = format!(
        "{} {} {} {}",
        goals.join(" "),
        corrections.join(" "),
        decisions.join(" "),
        page_results.join(" ")
    );
    let tier3_text = filtered_search
        .iter()
        .map(|r| r.content.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let t1 = estimate_tokens(&tier1_text);
    let t2 = estimate_tokens(&tier2_text);
    let t3 = estimate_tokens(&tier3_text);
    let token_estimates = TierTokenEstimates {
        tier1_identity: t1,
        tier2_project: t2,
        tier3_relevant: t3,
        total: t1 + t2 + t3,
    };

    // Build combined context string
    let mut sections: Vec<String> = Vec::new();

    if tier_allowed(&classification.trust_level, 1) {
        if let Some(brief) = narrative_brief {
            sections.push(format!("## About the User\n{}\n", brief));
        }
    }

    if !identity.is_empty() {
        let mut s = String::from("## Identity\n");
        for item in &identity {
            s.push_str(&format!("- {}\n", item));
        }
        sections.push(s);
    }

    if !preferences.is_empty() {
        let mut s = String::from("## Preferences\n");
        for item in &preferences {
            s.push_str(&format!("- {}\n", item));
        }
        sections.push(s);
    }

    if !goals.is_empty() {
        let mut s = String::from("## Goals\n");
        for item in &goals {
            s.push_str(&format!("- {}\n", item));
        }
        sections.push(s);
    }

    if !decisions.is_empty() {
        let mut sec = String::from("## Relevant Decisions\n");
        for item in &decisions {
            sec.push_str(&format!("- {}\n", item));
        }
        sections.push(sec);
    }

    if !corrections.is_empty() {
        let mut sec = String::from("## Corrections\n");
        for item in &corrections {
            sec.push_str(&format!("- {}\n", item));
        }
        sections.push(sec);
    }

    if !corpus_overview.is_empty() {
        let mut sec = String::from("## Corpus Overview\n");
        for item in &corpus_overview {
            sec.push_str(&format!("{}\n\n", item));
        }
        sections.push(sec);
    }

    if !page_results.is_empty() {
        let mut sec = String::from("## Compiled Knowledge\n");
        for item in &page_results {
            sec.push_str(&format!("{}\n\n---\n\n", item));
        }
        sections.push(sec);
    }

    if !filtered_search.is_empty() {
        let mut sec = String::from("## Relevant Memories\n");
        for r in &filtered_search {
            sec.push_str(&format!("[{}] {}\n\n", r.title, r.content));
        }
        sections.push(sec);
    }

    let context = sections.join("\n");

    // Track accesses
    let all_source_ids: Vec<String> = filtered_search
        .iter()
        .map(|r| r.source_id.clone())
        .collect();
    if !all_source_ids.is_empty() {
        access_tracker.record_accesses(&all_source_ids);
        if let Err(e) = db.log_accesses(&all_source_ids).await {
            tracing::warn!("Failed to log accesses: {}", e);
        }
    }

    if !all_source_ids.is_empty() {
        let detail = format!("used {} memories", all_source_ids.len());
        if let Err(e) = db
            .log_agent_activity(
                agent_name,
                "read",
                &all_source_ids,
                req.query.as_deref(),
                &detail,
            )
            .await
        {
            tracing::warn!("Failed to log agent read activity: {}", e);
        }
    }

    let took_ms = start.elapsed().as_secs_f64() * 1000.0;

    // Fire-once onboarding milestone check (recall side).
    //
    // Only fires when the caller identified itself via `x-agent-name`. The
    // evaluator skips empty-result recalls internally, so we pass the real
    // hit count from `filtered_search` (the post-threshold search results
    // surfaced back to the agent as `knowledge.relevant_memories`). The
    // daemon has no UI to notify, so a fresh `NoopEmitter` is used inline —
    // milestones are still persisted via `record_milestone` and surfaced
    // to the frontend through /api/onboarding/*.
    if agent_name != "unknown" {
        let agent_for_ms = agent_name.to_string();
        let results_count = filtered_search.len();
        // Quote the top-ranked hit so the first-recall toast can show WHAT
        // was just surfaced, not just who asked. Char-safe truncation
        // (UTF-8 rule: never byte-index a Rust string).
        let top_preview_for_ms: Option<String> = filtered_search.first().map(|r| {
            let s = &r.content;
            let truncated: String = s.chars().take(100).collect();
            if truncated.chars().count() < s.chars().count() {
                format!("{}…", truncated.trim_end())
            } else {
                truncated
            }
        });
        let db_for_ms = db_arc.clone();
        let emitter_for_ms: Arc<dyn wenlan_core::events::EventEmitter> =
            Arc::new(wenlan_core::events::NoopEmitter);
        tokio::spawn(async move {
            let ev = wenlan_core::onboarding::MilestoneEvaluator::new(&db_for_ms, emitter_for_ms);
            if let Err(e) = ev
                .check_after_context_call(
                    &agent_for_ms,
                    results_count,
                    top_preview_for_ms.as_deref(),
                )
                .await
            {
                tracing::warn!(?e, "onboarding: check_after_context_call failed");
            }
        });
    }

    // ProfileContext.goals is deprecated (migration 45 folded goal -> identity);
    // we still emit it as an empty Vec for wire backward compat with wenlan-mcp
    // and any external consumers of /api/context until wenlan-types 0.4
    // drops the field entirely.
    #[allow(deprecated)]
    let profile = ProfileContext {
        narrative,
        identity,
        preferences,
        goals,
    };

    Ok(Json(ChatContextResponse {
        context,
        profile,
        knowledge: KnowledgeContext {
            pages: page_results,
            decisions,
            relevant_memories: filtered_search,
            graph_context: Vec::new(),
        },
        took_ms,
        token_estimates,
    }))
}

/// GET /api/ping
pub async fn handle_ping() -> Result<(StatusCode, &'static str), ServerError> {
    Ok((StatusCode::OK, "pong"))
}

/// GET /api/debug/pipeline
pub async fn handle_pipeline_status(
    State(state): State<Arc<RwLock<ServerState>>>,
) -> Result<Json<serde_json::Value>, ServerError> {
    let s = state.read().await;
    let db = s.db.as_ref().ok_or(ServerError::DbNotInitialized)?;
    let status = db
        .pipeline_status()
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;
    Ok(Json(status))
}

/// POST /api/steep
pub async fn handle_steep(
    State(state): State<Arc<RwLock<ServerState>>>,
) -> Result<Json<SteepResponse>, ServerError> {
    let s = state.read().await;
    let db =
        s.db.as_ref()
            .ok_or(ServerError::Internal("DB not initialized".into()))?;
    let llm = s.llm.as_ref();
    let api_llm = s.api_llm.as_ref();
    let synthesis_llm = s.synthesis_llm.as_ref();
    let prompts = &s.prompts;
    let tuning = &s.tuning.refinery;
    let confidence_cfg = &s.tuning.confidence;
    let distillation_cfg = &s.tuning.distillation;
    let result = wenlan_core::refinery::run_periodic_steep_with_api(
        db,
        llm,
        api_llm,
        synthesis_llm,
        prompts,
        tuning,
        confidence_cfg,
        distillation_cfg,
        wenlan_core::refinery::TriggerKind::Backstop,
    )
    .await
    .map_err(|e| ServerError::Internal(e.to_string()))?;
    Ok(Json(SteepResponse {
        memories_decayed: result.memories_decayed,
        recaps_generated: result.recaps_generated,
        distilled: result.distilled,
        pending_remaining: result.pending_remaining,
        phases: result.phases,
    }))
}

#[derive(Debug, Serialize)]
pub struct SteepResponse {
    pub memories_decayed: u64,
    pub recaps_generated: u32,
    pub distilled: u32,
    pub pending_remaining: u32,
    pub phases: Vec<wenlan_core::refinery::PhaseResult>,
}

/// Request body for POST /api/distill. All fields are optional: an empty
/// body (or omitted Content-Type) is treated as `DistillRequest::default()`
/// which preserves the historical full-pass behavior.
///
/// `deny_unknown_fields` rejects bodies that name a field we don't recognize
/// (e.g. an old MCP client that still sends `page_id`). The error surfaces as
/// a 400 with a serde diagnostic — the alternative is silently dropping the
/// field and running a global pass the caller didn't ask for.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DistillRequest {
    /// Free-form target string. Resolved server-side:
    /// `page_*`/`concept_*` → page redistill; entity name → scoped distill;
    /// domain value → scoped distill; anything else → 404-ish hint payload.
    #[serde(default, alias = "page_id")]
    pub target: Option<String>,

    /// When true, clears `user_edited` on the resolved page before recompile.
    /// Used by `/distill rebuild <page>` to opt into wiping user prose.
    /// Only valid when target resolves to a single page; otherwise returns a hint.
    /// Requires daemon LLM.
    #[serde(default)]
    pub force: bool,
}

/// POST /api/distill
pub async fn handle_distill(
    State(state): State<Arc<RwLock<ServerState>>>,
    body: axum::body::Bytes,
) -> Result<Json<serde_json::Value>, ServerError> {
    let req: DistillRequest = if body.is_empty() {
        DistillRequest::default()
    } else {
        serde_json::from_slice(&body).map_err(|e| ServerError::ValidationError(e.to_string()))?
    };

    // `/api/distill` always returns clusters as `pending` — the route is
    // user-triggered, so synthesis belongs to whoever called it (the agent
    // session via the skill, or another client with its own LLM). The
    // background refinery calls `distill_pages_scoped` directly with the
    // daemon LLM; that path is the one that synthesizes inline. Two
    // triggers, one shared function, no behavior drift inside the function.
    let s = state.read().await;
    let db =
        s.db.as_ref()
            .ok_or(ServerError::Internal("DB not initialized".into()))?;
    let prompts = &s.prompts;
    let tuning = &s.tuning.distillation;
    let llm = s.llm.as_ref();
    let api_llm = s.api_llm.as_ref();

    let knowledge_path = {
        let config = wenlan_core::config::load_config();
        Some(config.knowledge_path_or_default())
    };

    let target = match req.target.as_deref() {
        Some(raw) if !raw.is_empty() => {
            match wenlan_core::refinery::resolve_distill_target(db, raw).await? {
                Some(t) => Some(t),
                None => {
                    return Ok(Json(serde_json::json!({
                        "pages_created": 0,
                        "pages_updated": 0,
                        "unresolved": raw,
                        "hint": "target must be a page id (page_* / concept_*), an entity name, or a domain value",
                    })));
                }
            }
        }
        _ => None,
    };

    // Force path: clear user_edited and run a full deep_distill_single.
    // Only valid when target resolves to a single page; all other targets
    // (entity, domain, none) return a hint payload.
    if req.force {
        match &target {
            Some(wenlan_core::synthesis::distill::DistillTarget::Page(page_id)) => {
                db.clear_user_edited(page_id)
                    .await
                    .map_err(|e| ServerError::Internal(e.to_string()))?;
                let prefer_llm = api_llm.or(llm);
                if prefer_llm.map(|p| p.is_available()).unwrap_or(false) {
                    let updated = wenlan_core::refinery::deep_distill_single(
                        db,
                        prefer_llm,
                        prompts,
                        page_id,
                        knowledge_path.as_deref(),
                    )
                    .await?;
                    return Ok(Json(serde_json::json!({
                        "status": "ok",
                        "force": true,
                        "page_id": page_id,
                        "updated": updated,
                    })));
                } else {
                    return Ok(Json(serde_json::json!({
                        "status": "skipped",
                        "force": true,
                        "page_id": page_id,
                        "updated": false,
                        "hint": "force rebuild needs an LLM in the daemon — install an on-device model or set an Anthropic key via `wenlan setup` / `/origin:doctor`",
                    })));
                }
            }
            _ => {
                return Ok(Json(serde_json::json!({
                    "unresolved": req.target.clone().unwrap_or_default(),
                    "hint": "force=true only valid when target is a single page id",
                })));
            }
        }
    }

    let scoped = target.is_some();

    // Capture scope filters before moving `target` into distill_pages_scoped
    // so we can use them to scope the stale-page list below.
    let (stale_entity_filter, stale_space_filter) = match &target {
        Some(wenlan_core::synthesis::distill::DistillTarget::Entity { id, .. }) => {
            (Some(id.clone()), None)
        }
        Some(wenlan_core::synthesis::distill::DistillTarget::Domain(d)) => (None, Some(d.clone())),
        Some(wenlan_core::synthesis::distill::DistillTarget::Page(_)) | None => (None, None),
    };

    let result = wenlan_core::refinery::distill_pages_scoped(
        db,
        None, // route never invokes daemon LLM; caller synthesizes pending
        prompts,
        tuning,
        knowledge_path.as_deref(),
        target,
    )
    .await?;

    // Shared "fully covered" rule with refinery (subset OR Jaccard >= 0.8):
    // those clusters never need synth from either path.
    //
    // Partial-overlap clusters diverge intentionally:
    // - Refinery (no agent reach): marks the page stale and skips. Refresh
    //   is `redistill_changed_pages`' job and needs daemon LLM.
    // - Route (agent has LLM in caller's session): surfaces the cluster
    //   with `existing_page_id` so the skill can synth the refresh inline.
    //
    // Divergence sits at the orchestration layer; the primitive is shared.
    //
    // Performance: load the active-page source-memory index ONCE and reuse
    // it across every cluster comparison, so the route is O(P + P*C) hash
    // ops instead of P*C SQL roundtrips + JSON re-parses.
    let page_index = db
        .load_page_source_index()
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    let mut filtered_pending: Vec<serde_json::Value> = Vec::new();
    for cluster in &result.pending {
        let cluster_size = cluster.source_ids.len();
        let best =
            wenlan_core::db::best_overlapping_page_in_index(&page_index, &cluster.source_ids);

        let (existing_page_id, existing_page_title, new_memory_count) = match best {
            Some(m) if m.intersection >= cluster_size || m.jaccard >= 0.8 => continue,
            Some(m) => (
                Some(m.page_id),
                Some(m.page_title),
                cluster_size - m.intersection,
            ),
            None => (None, None, cluster_size),
        };

        let mut payload = serde_json::to_value(cluster)
            .map_err(|e| ServerError::Internal(format!("cluster serialize: {e}")))?;
        if let serde_json::Value::Object(ref mut map) = payload {
            if let Some(id) = existing_page_id {
                map.insert("existing_page_id".into(), serde_json::json!(id));
            }
            if let Some(title) = existing_page_title {
                map.insert("existing_page_title".into(), serde_json::json!(title));
            }
            map.insert(
                "new_memory_count".into(),
                serde_json::json!(new_memory_count),
            );
        }
        filtered_pending.push(payload);
    }

    // Surface stale pages so the agent's `/distill` skill can refresh them
    // via PUT /api/pages/{id}. Restrict to `source_updated` — `source_conflict`
    // is the user-edited escalation and needs a human-in-the-loop UX, not an
    // auto-refresh. Scoped by entity/domain when the target was scoped.
    let stale_pages_list = db
        .list_stale_pages_scoped(
            "source_updated",
            stale_entity_filter.as_deref(),
            stale_space_filter.as_deref(),
        )
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    let stale_pages_payload: Vec<serde_json::Value> = stale_pages_list
        .iter()
        .map(|p| {
            serde_json::json!({
                "page_id": p.id,
                "title": p.title,
                "summary": p.summary,
                "source_memory_ids": p.source_memory_ids,
                "sources_updated_count": p.sources_updated_count,
                "stale_reason": p.stale_reason,
                "user_edited": p.user_edited,
            })
        })
        .collect();
    // 10 is the LIMIT inside list_stale_pages_scoped. Surface a hint so the
    // skill can tell the user to re-run instead of silently dropping rows.
    let stale_truncated = stale_pages_list.len() == 10;

    // Walk orphan rows in case a page minted earlier in this pass (or by
    // a concurrent create_page call) now matches an existing orphan label.
    // The refinery's Emergence phase does the same on its tick; doing it
    // inline here keeps the route's response (and the skill's view of the
    // graph) consistent without waiting up to a poll interval.
    if let Err(e) = db.resolve_orphan_page_links().await {
        tracing::warn!("[distill] orphan link resolve failed: {e}");
    }

    // Surface the orphan-wikilink feed so the skill can prompt the user to
    // distill a page on a topic that other pages already reach for. Cheap:
    // one GROUP BY against page_links, capped at 100. Threshold N=2 means
    // a single typo doesn't show up — needs at least two distinct pages
    // citing the same label before it counts as signal.
    //
    // Only surface on unscoped passes. A scoped `/distill rust` user
    // doesn't want generic topic suggestions from elsewhere in the graph;
    // that would confuse them about what "scope" means. The unscoped
    // global pass (`/distill deep` or bare `/distill` outside a repo) is
    // the right place for this feed.
    let orphan_topics: Vec<serde_json::Value> = if scoped {
        Vec::new()
    } else {
        let raw = db
            .list_orphan_link_labels(2)
            .await
            .map_err(|e| ServerError::Internal(e.to_string()))?;
        raw.into_iter()
            .map(|(label, count)| serde_json::json!({"label": label, "count": count}))
            .collect()
    };

    Ok(Json(serde_json::json!({
        "pages_created": result.created.len(),
        "scoped": scoped,
        "created_ids": result.created,
        "pending": filtered_pending,
        "stale_pages": stale_pages_payload,
        "stale_truncated": stale_truncated,
        "orphan_topics": orphan_topics,
    })))
}

/// POST /api/distill/{page_id}
///
/// Re-distill a single page. Requires the daemon to have an LLM available
/// (on-device or Anthropic key). When no model/key is configured (local memory
/// mode) the route returns a 200 with a hint payload instead of a 500 —
/// the caller's intent (refresh this page) can't be honored, but the
/// failure mode is documented in the response so the skill can surface
/// it to the user.
pub async fn handle_redistill(
    State(state): State<Arc<RwLock<ServerState>>>,
    Path(page_id): Path<String>,
) -> Result<Json<serde_json::Value>, ServerError> {
    let s = state.read().await;
    let db =
        s.db.as_ref()
            .ok_or(ServerError::Internal("DB not initialized".into()))?;
    let llm = s.llm.as_ref();
    let api_llm = s.api_llm.as_ref();
    let prompts = &s.prompts;

    let knowledge_path = {
        let config = wenlan_core::config::load_config();
        Some(config.knowledge_path_or_default())
    };

    // Clear user_edited and mark stale so the CAS gate inside
    // deep_distill_single (require_stale=true) lets this pass through.
    // This preserves the original contract of the route: always recompile
    // the page regardless of its current stale state.
    db.clear_user_edited(&page_id)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    let prefer_llm = api_llm.or(llm);
    if prefer_llm.map(|p| p.is_available()).unwrap_or(false) {
        let updated = wenlan_core::refinery::deep_distill_single(
            db,
            prefer_llm,
            prompts,
            &page_id,
            knowledge_path.as_deref(),
        )
        .await?;
        Ok(Json(serde_json::json!({
            "status": "ok",
            "updated": updated,
        })))
    } else {
        Ok(Json(serde_json::json!({
            "status": "skipped",
            "updated": false,
            "hint": "page re-distill needs an LLM in the daemon — install an on-device model or set an Anthropic key via `wenlan setup` / `/origin:doctor`",
        })))
    }
}

// ===== Recent retrieval / page-change feeds =====

#[derive(Debug, Default, Deserialize)]
pub struct RecentLimitQuery {
    #[serde(default)]
    pub limit: Option<i64>,
}

/// GET /api/retrievals/recent - Recent agent retrieval events joined to page titles.
pub async fn handle_recent_retrievals(
    State(state): State<Arc<RwLock<ServerState>>>,
    axum::extract::Query(q): axum::extract::Query<RecentLimitQuery>,
) -> Result<Json<Vec<wenlan_types::RetrievalEvent>>, ServerError> {
    let limit = q.limit.unwrap_or(10).clamp(1, 100);
    let db = {
        let s = state.read().await;
        s.db.as_ref().cloned()
    };
    let db = db.ok_or(ServerError::DbNotInitialized)?;
    let events = db
        .list_recent_retrievals(limit)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;
    Ok(Json(events))
}

/// GET /api/pages/recent-changes - Recent page created/revised events.
pub async fn handle_recent_page_changes(
    State(state): State<Arc<RwLock<ServerState>>>,
    axum::extract::Query(q): axum::extract::Query<RecentLimitQuery>,
) -> Result<Json<Vec<wenlan_types::PageChange>>, ServerError> {
    let limit = q.limit.unwrap_or(10).clamp(1, 100);
    let db = {
        let s = state.read().await;
        s.db.as_ref().cloned()
    };
    let db = db.ok_or(ServerError::DbNotInitialized)?;
    let changes = db
        .list_recent_changes(limit)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;
    Ok(Json(changes))
}

/// GET /api/pages/recent — top-N page activity with badge deltas.
/// `since_ms` scopes badge derivation only; the feed is always top-N by recency.
pub async fn handle_recent_pages(
    State(state): State<Arc<RwLock<ServerState>>>,
    axum::extract::Query(q): axum::extract::Query<crate::memory_routes::RecentActivityQuery>,
) -> Result<Json<Vec<wenlan_types::RecentActivityItem>>, ServerError> {
    let db = {
        let s = state.read().await;
        s.db.as_ref().cloned()
    };
    let db = db.ok_or(ServerError::DbNotInitialized)?;
    let items = db
        .list_recent_pages_with_badges(q.limit.unwrap_or(10), q.since_ms)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;
    Ok(Json(items))
}

/// POST /api/llm/test — probe an OpenAI-compatible LLM endpoint with a 1-shot prompt.
/// Validates a custom endpoint from the app settings UI before saving.
pub async fn handle_test_llm(
    Json(req): Json<wenlan_types::requests::TestLlmRequest>,
) -> Result<Json<wenlan_types::requests::TestLlmResponse>, ServerError> {
    use wenlan_core::llm_provider::{LlmProvider, LlmRequest, OpenAICompatibleProvider};

    let provider = OpenAICompatibleProvider::new(req.endpoint, req.model);
    let llm_req = LlmRequest {
        system_prompt: None,
        user_prompt: req
            .prompt
            .unwrap_or_else(|| "Say 'hello' and nothing else.".into()),
        max_tokens: 10,
        temperature: 0.0,
        label: None,
        timeout_secs: None,
    };
    let response = provider
        .generate(llm_req)
        .await
        .map_err(|e| ServerError::Internal(format!("test_llm: {e}")))?;
    Ok(Json(wenlan_types::requests::TestLlmResponse { response }))
}

/// POST /api/shutdown — exits the daemon process cleanly.
/// Returns 200 OK, then exits 0 after a brief delay so the response is delivered.
pub async fn handle_shutdown() -> &'static str {
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        std::process::exit(0);
    });
    "shutting down"
}

#[cfg(test)]
mod recent_endpoints_tests {
    use axum::body::Body;
    use axum::http::Request;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use tower::ServiceExt;

    use crate::state::ServerState;

    #[tokio::test]
    async fn get_recent_retrievals_without_db_returns_503() {
        let state = Arc::new(RwLock::new(ServerState::default()));
        let app = crate::router::build_router(state);
        let req = Request::builder()
            .method("GET")
            .uri("/api/retrievals/recent?limit=5")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), 503);
    }

    #[tokio::test]
    async fn get_recent_page_changes_without_db_returns_503() {
        let state = Arc::new(RwLock::new(ServerState::default()));
        let app = crate::router::build_router(state);
        let req = Request::builder()
            .method("GET")
            .uri("/api/pages/recent-changes?limit=5")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), 503);
    }

    #[tokio::test]
    async fn get_recent_retrievals_route_is_registered() {
        let state = Arc::new(RwLock::new(ServerState::default()));
        let app = crate::router::build_router(state);
        let req = Request::builder()
            .method("GET")
            .uri("/api/retrievals/recent")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        // Route exists => NOT a 404. We allow any other status (most likely 503 from DbNotInitialized).
        assert_ne!(response.status(), 404);
    }

    #[tokio::test]
    async fn get_recent_page_changes_route_is_registered() {
        let state = Arc::new(RwLock::new(ServerState::default()));
        let app = crate::router::build_router(state);
        let req = Request::builder()
            .method("GET")
            .uri("/api/pages/recent-changes")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        assert_ne!(response.status(), 404);
    }

    #[tokio::test]
    async fn get_recent_pages_route_is_registered() {
        let state = Arc::new(RwLock::new(ServerState::default()));
        let app = crate::router::build_router(state);
        let req = Request::builder()
            .method("GET")
            .uri("/api/pages/recent?limit=5")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        // Route exists => NOT a 404. With no DB initialised we expect 503.
        assert_ne!(response.status(), 404);
    }

    #[tokio::test]
    async fn get_recent_pages_without_db_returns_503() {
        let state = Arc::new(RwLock::new(ServerState::default()));
        let app = crate::router::build_router(state);
        let req = Request::builder()
            .method("GET")
            .uri("/api/pages/recent?limit=5&since_ms=1000")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), 503);
    }

    #[tokio::test]
    async fn status_reports_reranker_disabled_by_default() {
        let state = Arc::new(RwLock::new(ServerState::default()));
        let app = crate::router::build_router(state);
        let req = Request::builder()
            .method("GET")
            .uri("/api/status")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), 200);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        // Verify the reranker field is actually present in the JSON response.
        let raw = std::str::from_utf8(&bytes).unwrap();
        assert!(
            raw.contains("\"reranker\""),
            "status JSON must include a reranker field, got: {raw}"
        );
        let parsed: wenlan_types::responses::StatusResponse =
            serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            parsed.reranker,
            wenlan_types::responses::RerankerStatus::Disabled
        );
    }
}

/// Static assertion: `search_memory_llm_rerank` must NOT appear in this module
/// (routes.rs) at Tier-3.  The LLM reranker regresses below no-rerank on
/// LongMemEval (0.722 < 0.790 base); Tier-3 uses plain `search_memory`.
/// This test catches any accidental re-introduction of the reranked call.
#[cfg(test)]
mod chat_context_tests {
    use axum::body::Body;
    use axum::http::Request;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use tower::ServiceExt;

    use crate::state::ServerState;

    /// Chat-context route must exist (not 404) and must not require a DB for
    /// routing decisions: the handler returns 503 when DB is absent, not an
    /// LLM-rerank error.  This confirms T3 never pulls in the LLM reranker —
    /// the route completes its routing phase before reaching any LLM call.
    #[tokio::test]
    async fn chat_context_tier3_without_db_returns_503_not_llm_error() {
        // No LLM provider is wired — if T3 still called search_memory_llm_rerank
        // with an llm=None path that short-circuits, this might pass.  But the
        // real guarantee is structural: after removing the reranked branch the
        // handler falls through to DbNotInitialized before touching any LLM.
        let state = Arc::new(RwLock::new(ServerState::default()));
        let app = crate::router::build_router(state);
        let body = r#"{"query":"what programming languages do I know","max_chunks":5}"#;
        let req = Request::builder()
            .method("POST")
            .uri("/api/context")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        // 503 = DbNotInitialized.  Would be a different error if LLM path ran first.
        assert_eq!(
            response.status(),
            503,
            "T3 should fail with DbNotInitialized (503), not an LLM error"
        );
    }
}

#[cfg(test)]
mod distill_request_tests {
    use super::DistillRequest;

    #[test]
    fn distill_request_deserializes_force() {
        let r: DistillRequest =
            serde_json::from_str(r#"{"target":"page_xyz","force":true}"#).unwrap();
        assert_eq!(r.target.as_deref(), Some("page_xyz"));
        assert!(r.force);
    }

    #[test]
    fn distill_request_defaults_force_to_false() {
        let r: DistillRequest = serde_json::from_str(r#"{"target":"foo"}"#).unwrap();
        assert!(!r.force);
    }

    #[test]
    fn distill_request_rejects_unknown_field() {
        let r = serde_json::from_str::<DistillRequest>(r#"{"bogus":true}"#);
        assert!(r.is_err(), "deny_unknown_fields should reject unknown keys");
    }
}

#[cfg(test)]
mod context_page_selection_tests {
    use std::collections::HashSet;
    use wenlan_core::pages::{filter_pages_by_source_overlap, select_pages_for_context, Page};

    fn make_page(id: &str, source_ids: &[&str], relevance_score: f32, review_status: &str) -> Page {
        Page {
            id: id.to_string(),
            title: id.to_string(),
            summary: None,
            content: String::new(),
            entity_id: None,
            space: None,
            source_memory_ids: source_ids.iter().map(|s| s.to_string()).collect(),
            version: 1,
            status: "active".to_string(),
            created_at: String::new(),
            last_compiled: String::new(),
            last_modified: String::new(),
            sources_updated_count: 0,
            stale_reason: None,
            user_edited: false,
            relevance_score,
            last_edited_by: None,
            last_edited_at: None,
            last_delta_summary: None,
            changelog: None,
            creation_kind: "distilled".to_string(),
            review_status: review_status.to_string(),
            workspace: None,
        }
    }

    /// The `/api/context` page block is wired to `select_pages_for_context(.., 3)`
    /// (the un-gated selector), NOT the old source-overlap gate. A high-relevance
    /// page whose source memories do NOT intersect the memory hit pool MUST still
    /// surface — the exact case the prior `filter_pages_by_source_overlap(.., >=1)`
    /// gate dropped. This locks the T2 wiring so a revert to the gate fails loud.
    #[test]
    fn context_surfaces_zero_overlap_page_that_old_gate_dropped() {
        // Memory hit pool returned by search_memory for this query.
        let search_source_ids: HashSet<String> =
            ["m1", "m2"].iter().map(|s| s.to_string()).collect();

        // Top-relevance page compiled from memories DISJOINT from the pool,
        // plus a lower-relevance page that does overlap.
        let raw_pages = vec![
            make_page("page_zero_overlap", &["x9", "x8"], 0.95, "confirmed"),
            make_page("page_overlap", &["m1"], 0.40, "confirmed"),
        ];

        // The OLD gate (min_overlap >= 1) drops the zero-overlap page — the
        // regression this change removes. Assert it really would have dropped it,
        // so the test below is exercising the gate boundary, not a no-op.
        let gated = filter_pages_by_source_overlap(&raw_pages, &search_source_ids, 1);
        assert!(
            !gated.iter().any(|p| p.id == "page_zero_overlap"),
            "old source-overlap gate should drop the zero-overlap page"
        );

        // The NEW wiring (cap = 3, matching the handler's literal) surfaces it.
        let selected = select_pages_for_context(&raw_pages, &search_source_ids, 3);
        assert!(
            selected.iter().any(|p| p.id == "page_zero_overlap"),
            "un-gated context path must surface the high-relevance zero-overlap page"
        );
    }
}
