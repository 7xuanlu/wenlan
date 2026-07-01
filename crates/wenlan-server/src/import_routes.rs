// SPDX-License-Identifier: Apache-2.0
use crate::error::ServerError;
use crate::state::ServerState;
use axum::{extract::State, response::Json};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;
use wenlan_types::import::{ImportChatExportRequest, ImportChatExportResponse};

#[derive(Debug, Deserialize)]
pub struct ImportMemoriesRequest {
    pub source: String,
    pub content: String,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ImportMemoriesResponse {
    pub imported: usize,
    pub skipped: usize,
    pub breakdown: HashMap<String, usize>,
    pub entities_created: usize,
    pub observations_added: usize,
    pub relations_created: usize,
    pub batch_id: String,
}

/// POST /api/import/memories
pub async fn handle_import_memories(
    State(state): State<Arc<RwLock<ServerState>>>,
    Json(req): Json<ImportMemoriesRequest>,
) -> Result<Json<ImportMemoriesResponse>, ServerError> {
    let valid_sources = ["chatgpt", "claude", "other"];
    if !valid_sources.contains(&req.source.as_str()) {
        return Err(ServerError::ValidationError(format!(
            "Invalid source '{}'. Must be one of: chatgpt, claude, other",
            req.source
        )));
    }

    let result = {
        let s = state.read().await;
        let db = s.db.as_ref().ok_or(ServerError::DbNotInitialized)?;
        let confidence_cfg = &s.tuning.confidence;
        wenlan_core::importer::import_memories_no_llm(
            db,
            &req.content,
            &req.source,
            req.label.as_deref(),
            confidence_cfg,
        )
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?
    };

    Ok(Json(ImportMemoriesResponse {
        imported: result.imported,
        skipped: result.skipped,
        breakdown: result.breakdown,
        entities_created: result.entities_created,
        observations_added: result.observations_added,
        relations_created: result.relations_created,
        batch_id: result.batch_id,
    }))
}

// ---------------------------------------------------------------------------
// POST /api/import/chat-export
// ---------------------------------------------------------------------------

/// Maximum ZIP file size we accept (512 MB).
const MAX_IMPORT_ZIP_SIZE: u64 = 512 * 1024 * 1024;

/// POST /api/import/chat-export
///
/// Reads a chat-export ZIP from disk, auto-detects the vendor, bulk-ingests
/// all new conversations as raw memories, and spawns a background refinery
/// steep to classify + enrich them.
pub async fn handle_chat_export_import(
    State(state): State<Arc<RwLock<ServerState>>>,
    Json(req): Json<ImportChatExportRequest>,
) -> Result<Json<ImportChatExportResponse>, ServerError> {
    // 1. Validate file size before reading (I4).
    let meta = tokio::fs::metadata(&req.path)
        .await
        .map_err(|e| ServerError::ChatImport(format!("failed to stat {}: {e}", req.path)))?;
    if meta.len() > MAX_IMPORT_ZIP_SIZE {
        return Err(ServerError::ChatImport(format!(
            "file too large ({} bytes, max {})",
            meta.len(),
            MAX_IMPORT_ZIP_SIZE
        )));
    }

    // 2. Read ZIP from disk (async I/O to avoid blocking Tokio worker — I1).
    let bytes = tokio::fs::read(&req.path)
        .await
        .map_err(|e| ServerError::ChatImport(format!("failed to read {}: {e}", req.path)))?;

    // 3. Dispatch parse — auto-detect vendor.
    let batch = wenlan_core::chat_import::dispatch_parse(&bytes)
        .map_err(|e| ServerError::ChatImport(format!("parse failed: {e}")))?;

    // 4. Snapshot Arc<MemoryDB> + LLM providers + enrichment config out of the
    //    RwLock guard before any awaits.
    let (db, llm, api_llm, prompts, tuning) = {
        let guard = state.read().await;
        let db = guard
            .db
            .as_ref()
            .ok_or_else(|| ServerError::ChatImport("database not initialized".into()))?
            .clone();
        (
            db,
            guard.llm.clone(),
            guard.api_llm.clone(),
            guard.prompts.clone(),
            guard.tuning.clone(),
        )
    };

    // 5. Create import_state row.
    let import_id = format!("imp_{}", Uuid::new_v4());
    db.start_import_state(&import_id, batch.vendor, &req.path)
        .await
        .map_err(|e| ServerError::ChatImport(format!("start_import_state: {e}")))?;
    db.update_import_state_stage(
        &import_id,
        wenlan_core::chat_import::bulk_ingest::ImportStage::StageA,
        Some(batch.conversations.len() as i64),
        Some(0),
    )
    .await
    .map_err(|e| ServerError::ChatImport(format!("update_import_state_stage: {e}")))?;

    // 6. Bulk ingest conversations. On failure, mark import_state as Error (I3).
    // TODO: Replace NoopEmitter with a broadcast-channel emitter that forwards
    // progress events to /ws/updates so the Tauri app can display live progress.
    let emitter: Arc<dyn wenlan_core::events::EventEmitter> =
        Arc::new(wenlan_core::events::NoopEmitter);
    let result = match wenlan_core::chat_import::bulk_ingest::bulk_import_conversations(
        db.clone(),
        &batch.conversations,
        emitter,
        &import_id,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            let msg = format!("bulk ingest: {e}");
            let _ = db
                .update_import_state_stage_with_error(
                    &import_id,
                    wenlan_core::chat_import::bulk_ingest::ImportStage::Error,
                    None,
                    None,
                    Some(&msg),
                )
                .await;
            return Err(ServerError::ChatImport(msg));
        }
    };

    // 7. Mark StageB and spawn background enrichment pass.
    db.update_import_state_stage(
        &import_id,
        wenlan_core::chat_import::bulk_ingest::ImportStage::StageB,
        None,
        Some(result.conversations_ingested as i64),
    )
    .await
    .map_err(|e| ServerError::ChatImport(format!("update_import_state_stage: {e}")))?;

    // Spawn background bulk enrichment pass for all imported memories that still
    // lack memory_type. Processes in batches of 100 with concurrency 8 until
    // none remain, then marks the import Done.
    //
    // Each iteration does TWO phases matching the canonical handle_store_memory
    // pattern (memory_routes.rs:850-1008):
    //   Phase 1: classify → apply_enrichment  (writes memory_type to DB)
    //   Phase 2: run_post_ingest_enrichment   (entity link, title, concept)
    //
    // Skipped from Phase 1: extract (structured_fields / retrieval_cue). Chat
    // imports are high-volume bulk data; title_enrich in Phase 2 covers the most
    // important enrichment. Extract can be added later if needed.
    //
    // Loop safety: after Phase 1 writes memory_type, the row is no longer
    // returned by get_unclassified_imports. Even when LLM is absent we write
    // memory_type = "fact" so the row exits the unclassified set. A hard cap
    // of MAX_STUCK_ITERS consecutive iterations without a count reduction
    // aborts with an error to cover any case where the row count stops decreasing.
    let db_for_enrich = db.clone();
    let import_id_for_enrich = import_id.clone();
    tokio::spawn(async move {
        use futures::stream::{self, StreamExt};
        const BATCH: usize = 100;
        const CONCURRENCY: usize = 8;
        // Abort if we see this many consecutive iterations without progress.
        const MAX_STUCK_ITERS: usize = 3;

        // Prefer API LLM for enrichment (faster for bulk); fall back to on-device.
        let prefer_llm: Option<std::sync::Arc<dyn wenlan_core::llm_provider::LlmProvider>> =
            api_llm.or(llm);

        let classify_prompt = prompts.classify_memory_quality.clone();
        let mut stuck_count: usize = 0;
        let mut last_batch_len: usize = usize::MAX;

        loop {
            let candidates = match db_for_enrich.get_unclassified_imports(BATCH).await {
                Ok(v) => v,
                Err(e) => {
                    let msg = format!("get_unclassified_imports failed: {e}");
                    tracing::error!("[chat-import-enrich] {msg}");
                    let _ = db_for_enrich
                        .update_import_state_stage_with_error(
                            &import_id_for_enrich,
                            wenlan_core::chat_import::bulk_ingest::ImportStage::Error,
                            None,
                            None,
                            Some(&msg),
                        )
                        .await;
                    return;
                }
            };
            if candidates.is_empty() {
                // All imports enriched — mark Done.
                let _ = db_for_enrich
                    .update_import_state_stage(
                        &import_id_for_enrich,
                        wenlan_core::chat_import::bulk_ingest::ImportStage::Done,
                        None,
                        None,
                    )
                    .await;
                return;
            }

            // Stuck-loop guard: abort if we're not making progress.
            if candidates.len() >= last_batch_len {
                stuck_count += 1;
                if stuck_count >= MAX_STUCK_ITERS {
                    let msg = format!(
                        "enrichment loop stuck after {MAX_STUCK_ITERS} non-progress iterations \
                         ({} candidates remaining); aborting",
                        candidates.len()
                    );
                    tracing::error!("[chat-import-enrich] {msg}");
                    let _ = db_for_enrich
                        .update_import_state_stage_with_error(
                            &import_id_for_enrich,
                            wenlan_core::chat_import::bulk_ingest::ImportStage::Error,
                            None,
                            None,
                            Some(&msg),
                        )
                        .await;
                    return;
                }
            } else {
                stuck_count = 0;
            }
            last_batch_len = candidates.len();

            // Process this batch with bounded concurrency.
            let knowledge_path =
                Some(wenlan_core::config::load_config().knowledge_path_or_default());
            stream::iter(candidates)
                .for_each_concurrent(CONCURRENCY, |(source_id, content)| {
                    let db_inner = db_for_enrich.clone();
                    let prompts = prompts.clone();
                    let tuning = tuning.clone();
                    let llm_inner = prefer_llm.clone();
                    let classify_prompt = classify_prompt.clone();
                    let knowledge_path = knowledge_path.clone();
                    async move {
                        // --- Phase 1: classify + apply_enrichment ---
                        // Mirrors memory_routes.rs handle_store_memory lines 859-986.
                        let mut memory_type = "fact".to_string();
                        let mut domain: Option<String> = None;
                        let mut quality: Option<String> = None;
                        let mut importance: Option<u8> = None;

                        if let Some(ref llm) = llm_inner {
                            let truncated: String = content.chars().take(1000).collect();
                            match tokio::time::timeout(
                                std::time::Duration::from_secs(30),
                                llm.generate(wenlan_core::llm_provider::LlmRequest {
                                    system_prompt: Some(classify_prompt.clone()),
                                    user_prompt: truncated,
                                    max_tokens: 128,
                                    temperature: 0.1,
                                    label: None,
                                    timeout_secs: None,
                                }),
                            )
                            .await
                            {
                                Ok(Ok(output)) => {
                                    if let Some(c) =
                                        wenlan_core::llm_provider::parse_classify_response(&output)
                                    {
                                        let classified_space = c.space;
                                        let proposed_space = classified_space
                                            .as_deref()
                                            .map(str::trim)
                                            .filter(|s| !s.is_empty())
                                            .map(str::to_string);
                                        memory_type = c.memory_type;
                                        quality = c.quality;
                                        importance = c.importance;
                                        match db_inner
                                            .registered_space_or_none(classified_space.as_deref())
                                            .await
                                        {
                                            Ok(Some(space)) => domain = Some(space),
                                            Ok(None) => {
                                                if let Some(space) = proposed_space.as_deref() {
                                                    tracing::warn!(
                                                        "[chat-import-enrich] ignoring unregistered classifier space {:?}; memory remains unscoped",
                                                        space
                                                    );
                                                }
                                            }
                                            Err(e) => {
                                                tracing::warn!(
                                                    "[chat-import-enrich] classifier space lookup failed for {source_id}: {e}"
                                                );
                                            }
                                        }
                                    }
                                }
                                Ok(Err(e)) => {
                                    tracing::warn!(
                                        "[chat-import-enrich] classify failed for {source_id}: {e}"
                                    );
                                }
                                Err(_) => {
                                    tracing::warn!(
                                        "[chat-import-enrich] classify timed out for {source_id}"
                                    );
                                }
                            }
                        }

                        let supersede_mode = if memory_type == "decision" {
                            "archive"
                        } else {
                            "hide"
                        };

                        if let Err(e) = db_inner
                            .apply_enrichment(
                                &source_id,
                                &memory_type,
                                domain.as_deref(),
                                quality.as_deref(),
                                supersede_mode,
                                None,
                                None,
                                None,
                                None,
                                importance,
                            )
                            .await
                        {
                            tracing::warn!(
                                "[chat-import-enrich] apply_enrichment failed for {source_id}: {e}"
                            );
                            // Still attempt Phase 2 — partial enrichment is better than none.
                        }

                        // --- Phase 2: post-ingest enrichment ---
                        if let Err(e) = wenlan_core::post_ingest::run_post_ingest_enrichment(
                            &db_inner,
                            &source_id,
                            &content,
                            None,
                            Some(memory_type.as_str()),
                            domain.as_deref(),
                            None,
                            llm_inner.as_ref(),
                            &prompts,
                            &tuning.refinery,
                            &tuning.distillation,
                            knowledge_path.as_deref(),
                            None, // cancel — bulk import is not debounced
                            None, // precomputed_kg
                        )
                        .await
                        {
                            tracing::warn!(
                                "[chat-import-enrich] post_ingest failed for {source_id}: {e}"
                            );
                        }
                    }
                })
                .await;
        }
    });

    let vendor_str = batch.vendor.as_str().to_string();
    Ok(Json(ImportChatExportResponse {
        import_id,
        vendor: vendor_str,
        conversations_total: batch.conversations.len(),
        conversations_new: result.conversations_ingested,
        conversations_skipped_existing: result.conversations_skipped_existing,
        memories_stored: result.memories_stored,
    }))
}

// ---------------------------------------------------------------------------
// GET /api/import/state
// ---------------------------------------------------------------------------

/// GET /api/import/state — list non-terminal imports (pending/in-progress).
pub async fn handle_list_pending_imports(
    State(state): State<Arc<RwLock<ServerState>>>,
) -> Result<Json<Vec<wenlan_types::import::PendingImport>>, ServerError> {
    let db = {
        let guard = state.read().await;
        guard
            .db
            .as_ref()
            .ok_or_else(|| ServerError::ChatImport("database not initialized".into()))?
            .clone()
    };
    let rows = db
        .list_pending_imports()
        .await
        .map_err(|e| ServerError::ChatImport(format!("list_pending_imports: {e}")))?;
    Ok(Json(
        rows.into_iter()
            .map(|s| wenlan_types::import::PendingImport {
                id: s.id,
                vendor: s.vendor.as_str().to_string(),
                stage: s.stage.as_str().to_string(),
                source_path: s.source_path,
                processed_conversations: s.processed_conversations,
                total_conversations: s.total_conversations,
            })
            .collect(),
    ))
}

#[cfg(test)]
mod chat_export_route_tests {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    use crate::state::ServerState;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    #[tokio::test]
    async fn post_chat_export_with_missing_file_returns_400() {
        let state = Arc::new(RwLock::new(ServerState::default()));
        let app = crate::router::build_router(state);
        let body = serde_json::json!({ "path": "/tmp/definitely-not-a-real-zip-xyz.zip" });
        let req = Request::builder()
            .method("POST")
            .uri("/api/import/chat-export")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), 400);
    }
}
