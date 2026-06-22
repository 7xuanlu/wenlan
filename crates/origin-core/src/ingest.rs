//! Canonical post-store ingest enrichment.
//!
//! This module holds the ONE shared enrichment path that runs after a memory's
//! primary chunk is upserted: LLM classify + extract + `apply_enrichment` + tags
//! (Phase 1), entity/title/page enrichment (Phase 2), and dual-pool
//! dedup/contradiction resolution (Phase 3).
//!
//! It was extracted verbatim from the server `handle_store_memory`
//! deferred-reflection task so that the daemon ingest path, the eval seed
//! pipeline, and the importer all enrich through the SAME code. Sharing this
//! eliminates a training-serving skew: a write-time feature added here reaches
//! every consumer (including eval) automatically, instead of silently lagging in
//! a divergent eval shortcut. (Google "Rules of ML", Rule #32: "Re-use code
//! between your training pipeline and your serving pipeline whenever possible.")

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::db::MemoryDB;
use crate::events::{EventEmitter, NoopEmitter};
use crate::llm_provider::LlmProvider;
use crate::prompts::PromptRegistry;
use crate::tuning::{DistillationConfig, RefineryConfig};

/// Caller-supplied overrides plus the initial sync-path classification that the
/// canonical enrichment refines. These mirror the locals the server
/// `handle_store_memory` sync path computes before dispatching the deferred
/// reflection task.
#[derive(Debug, Clone, Default)]
pub struct EnrichmentOpts {
    /// Placeholder memory_type stored on the sync path (e.g. `"fact"`, or the
    /// caller-supplied subtype). The classifier replaces it unless the caller
    /// supplied a concrete (non-alias) type.
    pub initial_memory_type: String,
    /// Domain/space the sync path already resolved, if any.
    pub initial_domain: Option<String>,
    /// Supersede mode the sync path computed for `initial_memory_type`.
    pub initial_supersede_mode: String,
    /// Structured fields the caller supplied (skips the extract LLM call).
    pub initial_structured_fields: Option<String>,
    /// True when the caller passed a non-empty `memory_type`.
    pub agent_supplied_memory_type: bool,
    /// True when the caller-supplied `memory_type` was a profile alias (so the
    /// classifier's concrete subtype is taken even though a type was supplied).
    pub agent_supplied_profile_alias: bool,
    /// True when the caller supplied `structured_fields` (skips extract).
    pub agent_supplied_structured_fields: bool,
}

/// The classification the enrichment resolved. The server discards this (the
/// HTTP 200 already returned before the task runs); the eval seed pipeline and
/// future callers consume it for logging / assertions. Every field reflects the
/// value actually written via `apply_enrichment`.
#[derive(Debug, Clone, Default)]
pub struct EnrichmentOutcome {
    pub final_memory_type: String,
    pub final_domain: Option<String>,
    pub final_quality: Option<String>,
    pub final_importance: Option<u8>,
    pub final_structured_fields: Option<String>,
    pub final_retrieval_cue: Option<String>,
    pub final_event_date: Option<i64>,
    pub final_event_end: Option<i64>,
}

/// Phase 1 of the canonical enrichment, in isolation: LLM classify + extract +
/// `apply_enrichment` + auto-create-space + tags. Returns the resolved
/// classification (also written to the row via `apply_enrichment`).
///
/// This is the reusable unit. `run_canonical_enrichment` calls it as its Phase 1
/// (when an LLM is present); the eval seed pipeline calls it directly to backfill
/// importance/event_date/quality/structured_fields/retrieval_cue onto
/// already-upserted memories WITHOUT re-running the bulk entity/title/page passes
/// that the eval pipeline owns via its scale-optimized batched variants. Sharing
/// this exact pass is what keeps the eval seed and production in lockstep (Google
/// "Rules of ML", Rule #32) instead of the eval shortcut silently lacking the
/// classify/extract signals.
///
/// `llm` is non-optional here by construction — Phase 1 only ran inside the
/// `if let Some(llm)` guard in the original server task. Callers with no LLM skip
/// this entirely. Log-and-degrade at every step; never propagates an error.
pub async fn run_classification_enrichment(
    db: &MemoryDB,
    source_id: &str,
    content: &str,
    llm: &Arc<dyn LlmProvider>,
    prompts: &PromptRegistry,
    opts: &EnrichmentOpts,
) -> EnrichmentOutcome {
    let mut final_memory_type = opts.initial_memory_type.clone();
    let mut final_domain = opts.initial_domain.clone();
    let mut final_supersede_mode = opts.initial_supersede_mode.clone();
    let mut final_quality: Option<String> = None;
    let mut final_importance: Option<u8> = None;
    let mut final_structured_fields: Option<String> = opts.initial_structured_fields.clone();
    let mut final_retrieval_cue: Option<String> = None;
    let mut final_event_date: Option<i64> = None;
    let mut final_event_end: Option<i64> = None;
    let mut classified_tags: Vec<String> = Vec::new();

    // Classify. Agent-supplied memory_type stays authoritative unless they passed
    // a profile alias (in which case we take the classifier's concrete subtype).
    // Other fields — domain/quality/tags — come from classify regardless.
    let truncated: String = content.chars().take(1000).collect();
    // 30s matches `OnDeviceProvider::generate`'s own timeout. 5s was too
    // aggressive under a burst: when 10 concurrent stores each fire classify +
    // extract, the 10th call's turn at the single LLM worker lands at ~30s.
    match tokio::time::timeout(
        std::time::Duration::from_secs(30),
        llm.generate(crate::llm_provider::LlmRequest {
            system_prompt: Some(prompts.classify_memory_quality.clone()),
            user_prompt: truncated,
            max_tokens: 128,
            temperature: 0.1,
            label: Some("classify".into()),
            timeout_secs: None,
        }),
    )
    .await
    {
        Ok(Ok(output)) => {
            if let Some(c) = crate::llm_provider::parse_classify_response(&output) {
                if !opts.agent_supplied_memory_type || opts.agent_supplied_profile_alias {
                    final_memory_type = c.memory_type.clone();
                    // Recompute supersede_mode for the refined type.
                    final_supersede_mode = if final_memory_type == "decision" {
                        "archive".to_string()
                    } else {
                        "hide".to_string()
                    };
                }
                if final_domain.is_none() {
                    final_domain = c.space;
                }
                final_quality = c.quality;
                final_importance = c.importance;
                classified_tags = c.tags;
            }
        }
        Ok(Err(e)) => log::warn!("[ingest] classify failed: {e}"),
        Err(_) => log::warn!("[ingest] classify timed out after 30s"),
    }

    // Extract structured fields — only if the agent didn't supply them.
    if !opts.agent_supplied_structured_fields {
        let prompt = crate::memory_schema::extraction_prompt_with_template(
            &final_memory_type,
            &prompts.extract_structured_fields,
        );
        let truncated: String = content.chars().take(1500).collect();
        // Same reasoning as the classify timeout above — 30s is the provider's
        // own cap; defense-in-depth so a stalled extract doesn't hang this task.
        match tokio::time::timeout(
            std::time::Duration::from_secs(30),
            llm.generate(crate::llm_provider::LlmRequest {
                system_prompt: Some(prompt),
                user_prompt: truncated,
                max_tokens: 256,
                temperature: 0.1,
                label: None,
                timeout_secs: None,
            }),
        )
        .await
        {
            Ok(Ok(output)) => {
                let extracted = crate::llm_provider::parse_extraction_response(&output);
                final_event_date = extracted.event_date;
                final_event_end = extracted.event_end;
                if let Some(f) = extracted.structured_fields {
                    final_structured_fields = Some(f);
                }
                if let Some(c) = extracted.retrieval_cue {
                    final_retrieval_cue = Some(c);
                }
            }
            Ok(Err(e)) => log::warn!("[ingest] extract failed: {e}"),
            Err(_) => log::warn!("[ingest] extract timed out after 30s"),
        }
    }

    // Apply the refined classification to the stored memory.
    if let Err(e) = db
        .apply_enrichment(
            source_id,
            &final_memory_type,
            final_domain.as_deref(),
            final_quality.as_deref(),
            &final_supersede_mode,
            final_structured_fields.as_deref(),
            final_retrieval_cue.as_deref(),
            final_event_date,
            final_event_end,
            final_importance,
        )
        .await
    {
        log::warn!("[ingest] apply_enrichment failed: {e}");
    }

    // Auto-create a space for the classified domain if one came back from
    // classify and the sync path didn't already see it.
    if let Some(ref domain) = final_domain {
        if opts.initial_domain.as_deref() != Some(domain.as_str()) {
            if let Err(e) = db.auto_create_space_if_needed(domain).await {
                log::warn!("[ingest] auto-create space failed: {e}");
            }
        }
    }

    // Write tags to MemoryDB.
    if !classified_tags.is_empty() {
        if let Err(e) = db
            .set_document_tags("memory", source_id, classified_tags)
            .await
        {
            log::warn!("[ingest] set_document_tags failed: {e}");
        }
    }

    EnrichmentOutcome {
        final_memory_type,
        final_domain,
        final_quality,
        final_importance,
        final_structured_fields,
        final_retrieval_cue,
        final_event_date,
        final_event_end,
    }
}

/// Run the canonical post-store enrichment for a single already-upserted memory.
///
/// Behaviour is log-and-degrade at every step (an LLM/DB error warns and the
/// step is skipped, never propagated) — identical to the server task it
/// replaces, which returned `()`. The returned [`EnrichmentOutcome`] reflects
/// the values written.
///
/// Locking: this fn holds NO `RwLock` guard. The caller snapshots `db`, `llm`,
/// `prompts`, `refinery`, `distillation`, and `knowledge_path` out of its state
/// guard and drops that guard BEFORE calling (per the never-hold-a-guard-across-
/// `.await` rule). `cancel` threads the reflection-debounce cooperative-cancel
/// flag; `None` runs every step to completion.
#[allow(clippy::too_many_arguments)]
pub async fn run_canonical_enrichment(
    db: &MemoryDB,
    source_id: &str,
    content: &str,
    entity_id: Option<&str>,
    llm: Option<&Arc<dyn LlmProvider>>,
    prompts: &PromptRegistry,
    refinery: &RefineryConfig,
    distillation: &DistillationConfig,
    knowledge_path: Option<&std::path::Path>,
    opts: &EnrichmentOpts,
    cancel: Option<&AtomicBool>,
) -> EnrichmentOutcome {
    // Phase 1: classify + extract + apply_enrichment + tags. Runs only when an
    // LLM is available; the no-LLM path leaves the classification at the sync
    // placeholder (initial_*). Extracted into `run_classification_enrichment`
    // so the eval seed can reuse exactly this pass — additively, over
    // already-upserted rows — without re-running the bulk entity/title/page
    // passes its scale-optimized pipeline already owns.
    let outcome = match llm {
        Some(llm) => {
            run_classification_enrichment(db, source_id, content, llm, prompts, opts).await
        }
        None => EnrichmentOutcome {
            final_memory_type: opts.initial_memory_type.clone(),
            final_domain: opts.initial_domain.clone(),
            final_quality: None,
            final_importance: None,
            final_structured_fields: opts.initial_structured_fields.clone(),
            final_retrieval_cue: None,
            final_event_date: None,
            final_event_end: None,
        },
    };

    // Phase 2: existing post-ingest enrichment (entity linking, title
    // enrichment, page growth). Runs with the enriched fields Phase 1 produced.
    if let Err(e) = crate::post_ingest::run_post_ingest_enrichment(
        db,
        source_id,
        content,
        entity_id,
        Some(outcome.final_memory_type.as_str()),
        outcome.final_domain.as_deref(),
        outcome.final_structured_fields.as_deref(),
        llm,
        prompts,
        refinery,
        distillation,
        knowledge_path,
        cancel,
        None, // precomputed_kg
    )
    .await
    {
        log::warn!("[ingest] post-ingest enrichment failed: {e}");
    }

    // Phase 3 (T14): dual-pool dedup + contradiction resolution.
    //
    // Behind `ORIGIN_ENABLE_DUAL_POOL_RESOLVE` (default OFF -> no-op and the
    // path is byte-identical). Runs LAST so the `event_date` Phase 1 wrote is
    // visible for bidirectional temporal expiry. Best-effort: log-and-degrade.
    if crate::db::dual_pool_resolve_enabled() {
        let resolve_emitter: Arc<dyn EventEmitter> = Arc::new(NoopEmitter);
        match crate::synthesis::refinement_queue::resolve_dual_pool(
            db,
            source_id,
            llm,
            prompts,
            &resolve_emitter,
        )
        .await
        {
            Ok(outcome) => {
                if !outcome.invalidated.is_empty()
                    || outcome.expired_incoming
                    || !outcome.flagged_for_review.is_empty()
                    || !outcome.dedup_proposals.is_empty()
                {
                    log::info!(
                        "[ingest] dual-pool resolve: invalidated={:?} expired_incoming={} \
                         flagged={:?} dedup_proposals={}",
                        outcome.invalidated,
                        outcome.expired_incoming,
                        outcome.flagged_for_review,
                        outcome.dedup_proposals.len(),
                    );
                }
            }
            Err(e) => log::warn!("[ingest] dual-pool resolve failed: {e}"),
        }
    }

    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm_provider::SequencedMockProvider;
    use crate::sources::RawDocument;

    async fn test_db() -> (MemoryDB, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let db = MemoryDB::new(db_path.as_path(), Arc::new(NoopEmitter))
            .await
            .unwrap();
        (db, dir)
    }

    fn seed_doc(source_id: &str, content: &str) -> RawDocument {
        RawDocument {
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
            stability: Some("new".to_string()),
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
        }
    }

    fn opts_no_agent_overrides() -> EnrichmentOpts {
        EnrichmentOpts {
            initial_memory_type: "fact".to_string(),
            initial_domain: None,
            initial_supersede_mode: "hide".to_string(),
            initial_structured_fields: None,
            agent_supplied_memory_type: false,
            agent_supplied_profile_alias: false,
            agent_supplied_structured_fields: false,
        }
    }

    /// The canonical enrichment must classify + extract via the LLM and persist
    /// the refined classification. This is the property the old eval shortcut
    /// LACKED: it wrote entity + title + page only, never `importance` (T8
    /// salience) or `event_date` (T11/T20 temporal). The outcome carries the
    /// parsed importance + event_date; the DB row carries the refined
    /// memory_type + space (proving `apply_enrichment` ran end-to-end).
    #[tokio::test]
    async fn canonical_enrichment_classifies_and_persists() {
        let (db, _dir) = test_db().await;
        let source_id = "mem_canon_test_1";
        let content =
            "Switched the team standup to 9am on 2026-01-15 because mornings work better.";
        db.upsert_documents(vec![seed_doc(source_id, content)])
            .await
            .unwrap();

        // Call 0 = classify, call 1 = extract (SequencedMockProvider advances).
        let llm: Arc<dyn LlmProvider> = Arc::new(SequencedMockProvider::new(vec![
            r#"{"memory_type":"preference","domain":"work","quality":"high","importance":4,"tags":["standup","schedule"]}"#,
            r#"{"event_date":"2026-01-15","retrieval_cue":"standup time change","topic":"standup"}"#,
        ]));

        let prompts = PromptRegistry::default();
        let refinery = RefineryConfig::default();
        let distillation = DistillationConfig::default();
        let opts = opts_no_agent_overrides();

        let outcome = run_canonical_enrichment(
            &db,
            source_id,
            content,
            None,
            Some(&llm),
            &prompts,
            &refinery,
            &distillation,
            None,
            &opts,
            None,
        )
        .await;

        // Outcome reflects the parsed classify + extract (the write-time signals
        // the eval shortcut never produced).
        assert_eq!(outcome.final_memory_type, "preference");
        assert_eq!(outcome.final_domain.as_deref(), Some("work"));
        assert_eq!(outcome.final_quality.as_deref(), Some("high"));
        assert_eq!(outcome.final_importance, Some(4));
        let expected_event_date = chrono::NaiveDate::from_ymd_opt(2026, 1, 15)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp();
        assert_eq!(outcome.final_event_date, Some(expected_event_date));

        // apply_enrichment persisted the refined type + space to the stored row.
        let (mt, space) = db.get_memory_classification(source_id).await.unwrap();
        assert_eq!(mt.as_deref(), Some("preference"));
        assert_eq!(space.as_deref(), Some("work"));
    }

    /// With no LLM available, Phase 1 is skipped entirely (no classify/extract),
    /// the classification stays at the initial placeholder, and the fn still
    /// returns a well-formed outcome — matching the server's behaviour when
    /// `state.llm` is `None`.
    #[tokio::test]
    async fn canonical_enrichment_no_llm_keeps_placeholder() {
        let (db, _dir) = test_db().await;
        let source_id = "mem_canon_test_2";
        let content = "A plain fact with no enrichment provider available at all.";
        db.upsert_documents(vec![seed_doc(source_id, content)])
            .await
            .unwrap();

        let prompts = PromptRegistry::default();
        let refinery = RefineryConfig::default();
        let distillation = DistillationConfig::default();
        let opts = opts_no_agent_overrides();

        let outcome = run_canonical_enrichment(
            &db,
            source_id,
            content,
            None,
            None,
            &prompts,
            &refinery,
            &distillation,
            None,
            &opts,
            None,
        )
        .await;

        assert_eq!(outcome.final_memory_type, "fact");
        assert_eq!(outcome.final_importance, None);
        assert_eq!(outcome.final_event_date, None);
    }
}
