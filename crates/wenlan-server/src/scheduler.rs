// SPDX-License-Identifier: Apache-2.0
//! Event-driven steep scheduler.
//!
//! Owns all steep scheduling: BurstEnd (per-agent adaptive gap), Idle (global
//! 10-min quiet), Daily (first-wake-after-24h), and Backstop (6-hour safety net).
//! Replaces the former steep loop in main.rs.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::state::SharedState;

/// 30-minute ceiling for adaptive gap — matches ACTIVITY_GAP_SECS in wenlan-core.
const BURST_GAP_CEILING: Duration = Duration::from_secs(1800);
/// 5-minute floor — prevents premature firing on fast writers.
const BURST_GAP_FLOOR: Duration = Duration::from_secs(300);
/// Minimum writes to qualify as a recap-worthy burst.
const MIN_BURST_WRITES: usize = 3;
/// Global idle threshold — all agents quiet for this long triggers Idle steep.
const IDLE_THRESHOLD: Duration = Duration::from_secs(600);
/// Backstop interval — safety net fires all phases.
const BACKSTOP_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
/// Poll interval — how often the scheduler checks trigger conditions.
const POLL_INTERVAL: Duration = Duration::from_secs(30);
/// Initial delay — lets on-device model warm up before first backstop.
const INITIAL_DELAY: Duration = Duration::from_secs(60);
const DERIVED_RECEIPT_SWEEP_INTERVAL: Duration = Duration::from_secs(30 * 60);
/// Bounded per-poll drain of the document-enrichment queue. Serial (one doc at a
/// time); caps how many queued documents a single poll processes so a large
/// backlog can't monopolize the poll loop (steeps, page-watcher). Per-chunk
/// checkpointing means the remainder is simply picked up on the next poll.
const MAX_DOC_ENRICH_PER_POLL: usize = 4;

fn derived_receipt_sweep_due(last: Option<Instant>, now: Instant) -> bool {
    last.is_none_or(|last| now.duration_since(last) >= DERIVED_RECEIPT_SWEEP_INTERVAL)
}

async fn run_derived_receipt_sweep_if_due<F, Fut, E>(
    last: &mut Option<Instant>,
    now: Instant,
    sweep: F,
) -> Result<bool, E>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), E>>,
{
    if !derived_receipt_sweep_due(*last, now) {
        return Ok(false);
    }
    let result = sweep().await;
    *last = Some(now);
    result.map(|()| true)
}

/// Lightweight write-event tracker shared between store handlers and the scheduler.
///
/// `handle_store_memory` calls `record()` after each successful store.
/// The scheduler reads snapshots and drains completed bursts via `drain_up_to()`.
#[derive(Clone, Default)]
pub struct WriteSignal {
    inner: Arc<Mutex<HashMap<String, Vec<Instant>>>>,
}

impl WriteSignal {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a write event for an agent. Called by handle_store_memory BEFORE
    /// spawning post-ingest enrichment (captures timestamp immediately).
    pub fn record(&self, agent: &str) {
        let mut map = self.inner.lock().unwrap();
        map.entry(agent.to_string())
            .or_default()
            .push(Instant::now());
    }

    /// Record a write with an explicit timestamp (for testing).
    #[cfg(test)]
    pub fn record_at(&self, agent: &str, at: Instant) {
        let mut map = self.inner.lock().unwrap();
        map.entry(agent.to_string()).or_default().push(at);
    }

    /// Snapshot all agents and their timestamps. Does NOT drain.
    pub fn snapshot(&self) -> HashMap<String, Vec<Instant>> {
        self.inner.lock().unwrap().clone()
    }

    /// Atomically drain timestamps <= cutoff for one agent.
    /// Returns the drained timestamps. Timestamps after cutoff remain
    /// for the next burst — prevents TOCTOU race.
    pub fn drain_up_to(&self, agent: &str, cutoff: Instant) -> Vec<Instant> {
        let mut map = self.inner.lock().unwrap();
        if let Some(timestamps) = map.get_mut(agent) {
            let (drained, remaining): (Vec<_>, Vec<_>) =
                timestamps.drain(..).partition(|t| *t <= cutoff);
            if remaining.is_empty() {
                map.remove(agent);
            } else {
                *timestamps = remaining;
            }
            drained
        } else {
            Vec::new()
        }
    }

    /// True if any agent has written since `since`.
    pub fn has_activity_since(&self, since: Instant) -> bool {
        let map = self.inner.lock().unwrap();
        map.values().any(|ts| ts.iter().any(|t| *t > since))
    }
}

/// Compute the adaptive gap for a burst given its write timestamps.
///
/// Formula: `clamp(2 * median_interval, BURST_GAP_FLOOR, BURST_GAP_CEILING)`.
/// With < 2 timestamps (0 intervals), returns `BURST_GAP_CEILING` — a single
/// write naturally times out at the ceiling.
pub fn adaptive_gap(timestamps: &[Instant]) -> Duration {
    if timestamps.len() < 2 {
        return BURST_GAP_CEILING;
    }

    let mut sorted: Vec<Instant> = timestamps.to_vec();
    sorted.sort();

    let mut intervals: Vec<Duration> = Vec::with_capacity(sorted.len() - 1);
    for pair in sorted.windows(2) {
        intervals.push(pair[1].duration_since(pair[0]));
    }

    // Median of intervals
    intervals.sort();
    let median = if intervals.len().is_multiple_of(2) {
        let mid = intervals.len() / 2;
        (intervals[mid - 1] + intervals[mid]) / 2
    } else {
        intervals[intervals.len() / 2]
    };

    let gap = median * 2;
    gap.clamp(BURST_GAP_FLOOR, BURST_GAP_CEILING)
}

/// Spawn the event-driven steep scheduler.
///
/// Runs a single tokio task with a 30-second poll loop that checks four
/// trigger conditions: BurstEnd, Idle, Daily, and Backstop. All `fire()` calls
/// are awaited inline — the poll interval is a minimum, not a hard clock.
pub fn spawn_scheduler(shared: SharedState, write_signal: WriteSignal) {
    tokio::spawn(async move {
        tokio::time::sleep(INITIAL_DELAY).await;

        let mut last_backstop = Instant::now();
        let mut idle_fired = false;
        let mut last_poll_activity = Instant::now();
        // Fire enrichment sweep every 30 min when an LLM provider is available.
        const ENRICHMENT_SWEEP_INTERVAL: Duration = Duration::from_secs(30 * 60);
        let mut last_enrichment_sweep = Instant::now()
            .checked_sub(ENRICHMENT_SWEEP_INTERVAL)
            .unwrap_or_else(Instant::now);
        // Fire the doc-reconcile sweep every 30 min when an LLM provider is available.
        const RECONCILE_SWEEP_INTERVAL: Duration = Duration::from_secs(30 * 60);
        let mut last_reconcile_sweep = Instant::now()
            .checked_sub(RECONCILE_SWEEP_INTERVAL)
            .unwrap_or_else(Instant::now);
        // Fire the citation-backfill sweep every 30 min when an LLM provider is available.
        const CITATION_SWEEP_INTERVAL: Duration = Duration::from_secs(30 * 60);
        let mut last_citation_sweep = Instant::now()
            .checked_sub(CITATION_SWEEP_INTERVAL)
            .unwrap_or_else(Instant::now);
        let mut last_derived_receipt_sweep = None;

        // Load persisted daily timestamp from DB (survives restarts)
        let last_daily_epoch = load_last_daily(&shared).await;
        let mut last_daily = if last_daily_epoch > 0 {
            // Convert epoch secs to an Instant offset from now.
            let now_epoch = chrono::Utc::now().timestamp();
            let secs_ago = (now_epoch - last_daily_epoch).max(0) as u64;
            Instant::now()
                .checked_sub(Duration::from_secs(secs_ago))
                .unwrap_or_else(Instant::now) // can't go back that far → fire on next eligible poll
        } else {
            // No record → fire Daily on first eligible poll.
            // Offset must exceed 24h so `duration_since(last_daily) > 24h` is true.
            Instant::now()
                .checked_sub(Duration::from_secs(25 * 60 * 60))
                .unwrap_or_else(Instant::now)
        };

        tracing::info!(
            "[scheduler] started — poll every {}s",
            POLL_INTERVAL.as_secs()
        );

        loop {
            tokio::time::sleep(POLL_INTERVAL).await;

            // Reset idle flag if any new activity arrived since last poll
            if write_signal.has_activity_since(last_poll_activity) {
                idle_fired = false;
            }
            last_poll_activity = Instant::now();

            // Snapshot shared state — drop the read guard immediately
            let snapshot = {
                let s = shared.read().await;
                s.db.clone().map(|db| {
                    (
                        db,
                        s.llm.clone(),
                        s.api_llm.clone(),
                        s.synthesis_llm.clone(),
                        s.external_llm.clone(),
                        s.prompts.clone(),
                        s.tuning.refinery.clone(),
                        s.tuning.confidence.clone(),
                        s.tuning.distillation.clone(),
                    )
                })
            };

            let Some((
                db,
                llm,
                api_llm,
                synthesis_llm,
                external_llm,
                prompts,
                refinery_cfg,
                confidence_cfg,
                distillation_cfg,
            )) = snapshot
            else {
                tracing::debug!("[scheduler] db not initialized, skipping poll");
                continue;
            };

            let now = Instant::now();

            // --- 0. Filesystem page watcher: md → DB ---
            //
            // md is canonical. When the user edits a page in Obsidian / VS
            // Code / etc., reflect the change back into the DB so refinery
            // and search stay aligned with what the user actually wrote.
            // Cheap: a dir scan + frontmatter parse + content compare per
            // file. No LLM, no embedding, no network. Skips files whose
            // origin_version frontmatter trails the DB (daemon wrote
            // last). Runs every poll so freshness ≈ POLL_INTERVAL.
            let knowledge_path = wenlan_core::config::load_config().knowledge_path_or_default();
            match wenlan_core::sources::page_watcher::sync_filesystem_edits(&db, &knowledge_path)
                .await
            {
                Ok(stats) if stats.applied > 0 => {
                    tracing::info!(
                        "[scheduler] page-watcher applied {} fs_edit(s)",
                        stats.applied
                    );
                }
                Ok(_) => {}
                Err(e) => tracing::warn!("[scheduler] page-watcher error: {e}"),
            }

            // --- 0b. Directory sources: mtime/hash sync + document-enrichment
            //         queue drive (§4). ---
            //
            // Mirrors the page-watcher Step-0 as a cheap per-poll pass: run the
            // SAME sync routine the HTTP handler runs over each registered
            // Directory source (mtime+hash diff, deletion propagation — no LLM),
            // then advance the enrichment queue by claiming and processing the
            // next pending document(s) serially through run_document_enrichment.
            // A paused queue backing off is skipped by claim_next_pending until
            // its next_retry_at elapses (backoff auto-resume). `db`/`llm`/`prompts`
            // are already snapshotted out of the read guard above.
            let doc_llm = api_llm.as_ref().or(llm.as_ref());
            let processed =
                run_directory_sync_tick(&db, doc_llm, &prompts, MAX_DOC_ENRICH_PER_POLL).await;
            if processed > 0 {
                tracing::info!("[scheduler] document enrichment processed {processed} doc(s)");
            }

            // --- 1. BurstEnd: per-agent adaptive gap detection ---
            let snap = write_signal.snapshot();
            for (agent, timestamps) in &snap {
                if timestamps.is_empty() {
                    continue;
                }
                let gap = adaptive_gap(timestamps);
                let last_write = *timestamps.iter().max().unwrap();
                if now.duration_since(last_write) > gap {
                    if timestamps.len() >= MIN_BURST_WRITES {
                        tracing::info!(
                            "[scheduler] BurstEnd for agent '{}' — {} writes, gap {:?}",
                            agent,
                            timestamps.len(),
                            gap
                        );
                        fire_steep_safe(
                            &db,
                            llm.as_ref(),
                            api_llm.as_ref(),
                            synthesis_llm.as_ref(),
                            external_llm.as_ref(),
                            &prompts,
                            &refinery_cfg,
                            &confidence_cfg,
                            &distillation_cfg,
                            wenlan_core::refinery::TriggerKind::BurstEnd,
                            "BurstEnd",
                        )
                        .await;
                    }
                    write_signal.drain_up_to(agent, last_write);
                }
            }

            // --- 2. Idle: global quiet for IDLE_THRESHOLD ---
            // No need to check write_signal.snapshot().is_empty() — undrained
            // sub-threshold bursts (e.g., a single write waiting for its 30-min
            // ceiling) should not block Idle. Synthesis phases (community_detection,
            // emergence, re-distill, decision_logs) don't overlap with BurstEnd
            // phases (recaps, refinement_queue), so running them concurrently with
            // pending burst timestamps is safe.
            let idle_horizon = now.checked_sub(IDLE_THRESHOLD).unwrap_or_else(Instant::now);
            if !idle_fired && !write_signal.has_activity_since(idle_horizon) {
                tracing::info!(
                    "[scheduler] Idle — all agents quiet for {}s",
                    IDLE_THRESHOLD.as_secs()
                );
                fire_steep_safe(
                    &db,
                    llm.as_ref(),
                    api_llm.as_ref(),
                    synthesis_llm.as_ref(),
                    external_llm.as_ref(),
                    &prompts,
                    &refinery_cfg,
                    &confidence_cfg,
                    &distillation_cfg,
                    wenlan_core::refinery::TriggerKind::Idle,
                    "Idle",
                )
                .await;
                let maintenance_llm = synthesis_llm
                    .as_ref()
                    .or(api_llm.as_ref())
                    .or(external_llm.as_ref())
                    .or(llm.as_ref());
                fire_maintenance_safe(
                    db.as_ref(),
                    maintenance_llm,
                    &prompts,
                    &distillation_cfg,
                    Some(knowledge_path.as_path()),
                    "Idle",
                )
                .await;
                idle_fired = true;
            }

            // --- 3. Daily: first-wake-after-24h ---
            if now.duration_since(last_daily) > Duration::from_secs(24 * 60 * 60) {
                tracing::info!("[scheduler] Daily — first fire in >24h");
                fire_steep_safe(
                    &db,
                    llm.as_ref(),
                    api_llm.as_ref(),
                    synthesis_llm.as_ref(),
                    external_llm.as_ref(),
                    &prompts,
                    &refinery_cfg,
                    &confidence_cfg,
                    &distillation_cfg,
                    wenlan_core::refinery::TriggerKind::Daily,
                    "Daily",
                )
                .await;
                last_daily = now;
                let epoch = chrono::Utc::now().timestamp().to_string();
                if let Err(e) = db.set_app_metadata("last_daily_steep_ts", &epoch).await {
                    tracing::warn!("[scheduler] failed to persist last_daily_steep_ts: {}", e);
                }
            }

            // --- 4. Backstop: safety net ---
            if now.duration_since(last_backstop) > BACKSTOP_INTERVAL {
                tracing::info!("[scheduler] Backstop — safety net fire");
                fire_steep_safe(
                    &db,
                    llm.as_ref(),
                    api_llm.as_ref(),
                    synthesis_llm.as_ref(),
                    external_llm.as_ref(),
                    &prompts,
                    &refinery_cfg,
                    &confidence_cfg,
                    &distillation_cfg,
                    wenlan_core::refinery::TriggerKind::Backstop,
                    "Backstop",
                )
                .await;
                let maintenance_llm = synthesis_llm
                    .as_ref()
                    .or(api_llm.as_ref())
                    .or(external_llm.as_ref())
                    .or(llm.as_ref());
                fire_maintenance_safe(
                    db.as_ref(),
                    maintenance_llm,
                    &prompts,
                    &distillation_cfg,
                    Some(knowledge_path.as_path()),
                    "Backstop",
                )
                .await;
                last_backstop = now;
            }

            if let Err(error) =
                run_derived_receipt_sweep_if_due(&mut last_derived_receipt_sweep, now, || {
                    db.record_derived_artifact_sweep()
                })
                .await
            {
                tracing::warn!("[scheduler] derived receipt sweep error: {error}");
            }

            // --- 5. Enrichment sweep: back-fill entity linkage for memories
            //        ingested while the LLM was unavailable. ---
            if wenlan_core::db::entity_sweep_enabled()
                && now.duration_since(last_enrichment_sweep) >= ENRICHMENT_SWEEP_INTERVAL
            {
                // Pick the best available LLM: prefer api_llm (cloud, reliable),
                // fall back to on-device llm.
                let sweep_llm = api_llm.as_ref().or(llm.as_ref()).cloned();
                if let Some(provider) = sweep_llm {
                    let db_ref = db.clone();
                    let prompts_ref = prompts.clone();
                    tokio::spawn(async move {
                        let extract_fn = |content: String| {
                            let p = provider.clone();
                            let pr = prompts_ref.clone();
                            let db2 = db_ref.clone();
                            async move {
                                wenlan_core::kg::entity_extraction::extract_entities_for_content(
                                    &db2, &p, &pr, &content,
                                )
                                .await
                            }
                        };
                        match db_ref.run_enrichment_sweep(extract_fn, 50).await {
                            Ok(0) => {}
                            Ok(n) => tracing::info!(
                                "[scheduler] enrichment sweep processed {n} memories"
                            ),
                            Err(e) => tracing::warn!("[scheduler] enrichment sweep error: {e}"),
                        }
                    });
                    last_enrichment_sweep = now;
                }
            }

            // --- 6. Doc-reconcile sweep: propose doc-grounded revisions for captures
            //        that contradict ingested documents (L3). Human-gated; never silent. ---
            if wenlan_core::db::doc_reconcile_enabled()
                && now.duration_since(last_reconcile_sweep) >= RECONCILE_SWEEP_INTERVAL
            {
                let sweep_llm = api_llm.as_ref().or(llm.as_ref()).cloned();
                if let Some(provider) = sweep_llm {
                    let db_ref = db.clone();
                    let prompts_ref = prompts.clone();
                    let refinery_ref = refinery_cfg.clone();
                    let distillation_ref = distillation_cfg.clone();
                    tokio::spawn(async move {
                        match wenlan_core::reconcile::run_reconcile_tick(
                            &db_ref,
                            &provider,
                            &prompts_ref,
                            &refinery_ref,
                            &distillation_ref,
                        )
                        .await
                        {
                            Ok(r) if r.skipped_backpressure => tracing::info!(
                                "[scheduler] reconcile sweep held: pending queue at cap"
                            ),
                            Ok(r) if r.judged > 0 => tracing::info!(
                                "[scheduler] reconcile sweep judged {} item(s), proposed {} revision(s)",
                                r.judged,
                                r.proposed
                            ),
                            Ok(_) => {}
                            Err(e) => tracing::warn!("[scheduler] reconcile sweep error: {e}"),
                        }
                    });
                    last_reconcile_sweep = now;
                }
            }

            // --- 7. Citation-backfill sweep: annotate legacy pages (citations
            //        IS NULL) with per-claim [N] markers. Annotate-only —
            //        legacy prose is never rewritten (see citations.rs). ---
            if wenlan_core::db::citation_backfill_enabled()
                && now.duration_since(last_citation_sweep) >= CITATION_SWEEP_INTERVAL
            {
                let sweep_llm = api_llm.as_ref().or(llm.as_ref()).cloned();
                if let Some(provider) = sweep_llm {
                    let db_ref = db.clone();
                    let prompts_ref = prompts.clone();
                    tokio::spawn(async move {
                        if let Err(e) = wenlan_core::citations::run_citation_backfill_tick(
                            &db_ref,
                            &provider,
                            &prompts_ref,
                        )
                        .await
                        {
                            tracing::warn!("[scheduler] citation backfill sweep error: {e}");
                        }
                    });
                    last_citation_sweep = now;
                }
            }
        }
    });
}

/// Background polling respects an explicit pause but keeps probing unavailable
/// roots so transient filesystem failures can recover automatically.
fn should_poll_directory_source(source: &wenlan_types::sources::Source) -> bool {
    source.source_type == wenlan_types::sources::SourceType::Directory
        && !matches!(source.status, wenlan_types::sources::SyncStatus::Paused)
}

/// One Directory-source sync + document-enrichment-queue-drive pass (§4).
/// Factored out of the 30s poll loop so it is unit-testable without the timer.
///
/// Each call:
/// 1. syncs every recoverable Directory source via the SHARED
///    [`crate::source_routes::sync_directory_source`] (cheap mtime/hash diff +
///    deletion propagation — no LLM, no network), enqueueing changed files, then
/// 2. drains up to `max_docs` claimable documents through
///    [`wenlan_core::document_enrichment::run_document_enrichment`] serially (one
///    at a time — the daemon is the single writer).
///
/// Backoff is honored for free: [`wenlan_core::db::MemoryDB::claim_next_pending`]
/// returns only `pending` rows or `paused` rows whose `next_retry_at` has
/// elapsed, so a paused queue waiting on its backoff is skipped. A pause during
/// the drain stops this cycle (don't hammer a down provider); the next poll
/// resumes once the backoff clears. Returns the number of documents processed.
async fn run_directory_sync_tick(
    db: &Arc<wenlan_core::db::MemoryDB>,
    llm: Option<&Arc<dyn wenlan_core::llm_provider::LlmProvider>>,
    prompts: &wenlan_core::prompts::PromptRegistry,
    max_docs: usize,
) -> usize {
    let config = wenlan_core::config::load_config();
    let knowledge_path = config.knowledge_path_or_default();

    // 1. Sync recoverable Directory sources (log-and-continue on error).
    for source in config
        .sources
        .iter()
        .filter(|source| should_poll_directory_source(source))
    {
        if let Err(e) =
            crate::source_routes::sync_directory_source(db.clone(), source, &config).await
        {
            tracing::warn!("[scheduler] directory sync '{}' failed: {e}", source.id);
        }
    }

    // 2. Drive the enrichment queue serially (bounded drain).
    let mut processed = 0usize;
    for _ in 0..max_docs {
        match db.claim_next_pending().await {
            Ok(Some(entry)) => {
                let outcome = wenlan_core::document_enrichment::run_document_enrichment(
                    db,
                    &entry,
                    Some(&knowledge_path),
                    llm,
                    prompts,
                )
                .await;
                processed += 1;
                if outcome.paused {
                    // Provider/DB is failing; stop draining this cycle. The row is
                    // paused with a backoff and auto-resumes on a later poll.
                    break;
                }
            }
            Ok(None) => break,
            Err(e) => {
                tracing::warn!("[scheduler] claim_next_pending failed: {e}");
                break;
            }
        }
    }
    processed
}

async fn fire_maintenance_safe(
    db: &wenlan_core::db::MemoryDB,
    llm: Option<&Arc<dyn wenlan_core::llm_provider::LlmProvider>>,
    prompts: &wenlan_core::prompts::PromptRegistry,
    distillation_cfg: &wenlan_core::tuning::DistillationConfig,
    knowledge_path: Option<&std::path::Path>,
    label: &str,
) {
    let config = wenlan_core::maintenance::MaintenanceTickConfig {
        page_match_threshold: distillation_cfg.page_match_threshold,
        formation_threshold: distillation_cfg.formation_threshold,
        page_min_cluster_size: distillation_cfg.page_min_cluster_size,
        token_limit: distillation_cfg.ondevice_token_limit,
        max_unlinked_cluster_size: distillation_cfg.max_unlinked_cluster_size,
        max_grouped_cluster_size: distillation_cfg.max_grouped_cluster_size,
        max_per_tick: 5,
    };
    match wenlan_core::maintenance::run_maintenance_tick(db, llm, prompts, &config, knowledge_path)
        .await
    {
        Ok(result) => {
            tracing::info!(
                "[scheduler] {label} maintenance: {} merge card(s), {} discovery card(s), {} retro card(s) from {} expected (paused={}), {} machine refresh(es), {} human card(s), {} orphan label(s), {} overview refresh(es)",
                result.merge_cards_emitted,
                result.discovery_cards_emitted,
                result.retro_cards_emitted,
                result.retro_expected_card_volume,
                result.retro_paused,
                result.stale_machine_refreshed,
                result.stale_human_cards,
                result.orphan_labels_checked,
                result.overview_refreshed
            );
        }
        Err(e) => tracing::warn!("[scheduler] {label} maintenance error: {e}"),
    }
}

/// Load the persisted last_daily_steep_ts from DB. Returns epoch seconds or 0.
async fn load_last_daily(shared: &SharedState) -> i64 {
    let s = shared.read().await;
    if let Some(db) = s.db.as_ref() {
        match db.get_app_metadata("last_daily_steep_ts").await {
            Ok(Some(val)) => val.parse::<i64>().unwrap_or(0),
            _ => 0,
        }
    } else {
        0
    }
}

/// Fire a steep with panic isolation. If the steep panics (e.g., OOM during
/// distillation of a massive cluster), the scheduler loop continues.
#[allow(clippy::too_many_arguments)]
async fn fire_steep_safe(
    db: &wenlan_core::db::MemoryDB,
    llm: Option<&std::sync::Arc<dyn wenlan_core::llm_provider::LlmProvider>>,
    api_llm: Option<&std::sync::Arc<dyn wenlan_core::llm_provider::LlmProvider>>,
    synthesis_llm: Option<&std::sync::Arc<dyn wenlan_core::llm_provider::LlmProvider>>,
    external_llm: Option<&std::sync::Arc<dyn wenlan_core::llm_provider::LlmProvider>>,
    prompts: &wenlan_core::prompts::PromptRegistry,
    refinery_cfg: &wenlan_core::tuning::RefineryConfig,
    confidence_cfg: &wenlan_core::tuning::ConfidenceConfig,
    distillation_cfg: &wenlan_core::tuning::DistillationConfig,
    trigger: wenlan_core::refinery::TriggerKind,
    label: &str,
) {
    let result = std::panic::AssertUnwindSafe(fire_steep(
        db,
        llm,
        api_llm,
        synthesis_llm,
        external_llm,
        prompts,
        refinery_cfg,
        confidence_cfg,
        distillation_cfg,
        trigger,
        label,
    ));
    if let Err(e) = futures::FutureExt::catch_unwind(result).await {
        let msg = if let Some(s) = e.downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = e.downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown panic".to_string()
        };
        tracing::error!(
            "[scheduler] {} PANICKED — scheduler continues: {}",
            label,
            msg
        );
    }
}

/// Fire a steep with the given trigger, log result summary and activity entries.
#[allow(clippy::too_many_arguments)]
async fn fire_steep(
    db: &wenlan_core::db::MemoryDB,
    llm: Option<&std::sync::Arc<dyn wenlan_core::llm_provider::LlmProvider>>,
    api_llm: Option<&std::sync::Arc<dyn wenlan_core::llm_provider::LlmProvider>>,
    synthesis_llm: Option<&std::sync::Arc<dyn wenlan_core::llm_provider::LlmProvider>>,
    external_llm: Option<&std::sync::Arc<dyn wenlan_core::llm_provider::LlmProvider>>,
    prompts: &wenlan_core::prompts::PromptRegistry,
    refinery_cfg: &wenlan_core::tuning::RefineryConfig,
    confidence_cfg: &wenlan_core::tuning::ConfidenceConfig,
    distillation_cfg: &wenlan_core::tuning::DistillationConfig,
    trigger: wenlan_core::refinery::TriggerKind,
    label: &str,
) {
    let started = std::time::Instant::now();
    let knowledge_path = wenlan_core::config::load_config().knowledge_path_or_default();
    match wenlan_core::refinery::run_periodic_steep_with_api(
        db,
        llm,
        api_llm,
        synthesis_llm,
        external_llm,
        prompts,
        refinery_cfg,
        confidence_cfg,
        distillation_cfg,
        Some(&knowledge_path),
        trigger,
    )
    .await
    {
        Ok(result) => {
            let errors = result.phases.iter().filter(|p| p.error.is_some()).count();
            tracing::info!(
                "[scheduler] {} complete in {}ms — {} phases ({} errors), {} decayed, {} recaps, {} distilled, {} pending",
                label,
                started.elapsed().as_millis(),
                result.phases.len(),
                errors,
                result.memories_decayed,
                result.recaps_generated,
                result.distilled,
                result.pending_remaining,
            );

            // Log non-Silent phases as activity entries (same as old steep loop)
            for phase in &result.phases {
                if phase.nudge != wenlan_core::refinery::Nudge::Silent {
                    if let Some(ref headline) = phase.headline {
                        if let Err(e) = db
                            .log_agent_activity("origin", "steep", &[], None, headline)
                            .await
                        {
                            tracing::warn!(
                                "[scheduler] log activity for phase {} failed: {}",
                                phase.name,
                                e
                            );
                        }
                    }
                }
            }
        }
        Err(e) => tracing::warn!("[scheduler] {} error: {}", label, e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adaptive_gap_empty_returns_ceiling() {
        assert_eq!(adaptive_gap(&[]), BURST_GAP_CEILING);
    }

    #[tokio::test]
    async fn derived_receipt_sweep_dispatches_initially_then_every_thirty_minutes() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let now = Instant::now();
        let mut last = None;
        let calls = AtomicUsize::new(0);
        assert!(run_derived_receipt_sweep_if_due(&mut last, now, || async {
            calls.fetch_add(1, Ordering::Relaxed);
            Ok::<(), ()>(())
        })
        .await
        .unwrap());
        assert_eq!(calls.load(Ordering::Relaxed), 1);

        assert!(!run_derived_receipt_sweep_if_due(
            &mut last,
            now + DERIVED_RECEIPT_SWEEP_INTERVAL - Duration::from_secs(1),
            || async {
                calls.fetch_add(1, Ordering::Relaxed);
                Ok::<(), ()>(())
            },
        )
        .await
        .unwrap());
        assert_eq!(calls.load(Ordering::Relaxed), 1);

        assert!(run_derived_receipt_sweep_if_due(
            &mut last,
            now + DERIVED_RECEIPT_SWEEP_INTERVAL,
            || async {
                calls.fetch_add(1, Ordering::Relaxed);
                Ok::<(), ()>(())
            },
        )
        .await
        .unwrap());
        assert_eq!(calls.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn test_adaptive_gap_single_write_returns_ceiling() {
        assert_eq!(adaptive_gap(&[Instant::now()]), BURST_GAP_CEILING);
    }

    #[test]
    fn test_adaptive_gap_fast_writer() {
        // Writes every 30s → median 30s → 2*30s = 60s → clamped to floor (5 min)
        let base = Instant::now();
        let timestamps: Vec<Instant> = (0..10)
            .map(|i| base + Duration::from_secs(30 * i))
            .collect();
        assert_eq!(adaptive_gap(&timestamps), BURST_GAP_FLOOR);
    }

    #[test]
    fn test_adaptive_gap_slow_writer() {
        // Writes every 10 min → median 600s → 2*600s = 1200s (20 min)
        let base = Instant::now();
        let timestamps: Vec<Instant> = (0..5)
            .map(|i| base + Duration::from_secs(600 * i))
            .collect();
        assert_eq!(adaptive_gap(&timestamps), Duration::from_secs(1200));
    }

    #[test]
    fn test_adaptive_gap_very_slow_writer_capped() {
        // Writes every 20 min → median 1200s → 2*1200s = 2400s → capped at 1800s
        let base = Instant::now();
        let timestamps: Vec<Instant> = (0..3)
            .map(|i| base + Duration::from_secs(1200 * i))
            .collect();
        assert_eq!(adaptive_gap(&timestamps), BURST_GAP_CEILING);
    }

    #[test]
    fn test_adaptive_gap_two_writes() {
        // 2 writes 3 min apart → median 180s → 2*180s = 360s (between floor and ceiling)
        let base = Instant::now();
        let timestamps = vec![base, base + Duration::from_secs(180)];
        assert_eq!(adaptive_gap(&timestamps), Duration::from_secs(360));
    }

    #[test]
    fn test_write_signal_record_and_snapshot() {
        let ws = WriteSignal::new();
        let now = Instant::now();
        ws.record_at("claude", now);
        ws.record_at("claude", now + Duration::from_secs(10));
        ws.record_at("obsidian", now);

        let snap = ws.snapshot();
        assert_eq!(snap.get("claude").unwrap().len(), 2);
        assert_eq!(snap.get("obsidian").unwrap().len(), 1);
    }

    #[test]
    fn test_drain_up_to_preserves_later_writes() {
        let ws = WriteSignal::new();
        let t1 = Instant::now();
        let t2 = t1 + Duration::from_secs(10);
        let t3 = t2 + Duration::from_secs(10);

        ws.record_at("claude", t1);
        ws.record_at("claude", t2);
        ws.record_at("claude", t3);

        // Drain up to t2 — t3 should survive
        let drained = ws.drain_up_to("claude", t2);
        assert_eq!(drained.len(), 2);

        let snap = ws.snapshot();
        let remaining = snap.get("claude").unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0], t3);
    }

    #[test]
    fn test_drain_up_to_removes_key_when_empty() {
        let ws = WriteSignal::new();
        let t1 = Instant::now();
        ws.record_at("claude", t1);

        ws.drain_up_to("claude", t1);
        let snap = ws.snapshot();
        assert!(!snap.contains_key("claude"));
    }

    #[test]
    fn test_has_activity_since() {
        let ws = WriteSignal::new();
        let t1 = Instant::now();
        ws.record_at("claude", t1 + Duration::from_secs(5));

        assert!(ws.has_activity_since(t1));
        assert!(!ws.has_activity_since(t1 + Duration::from_secs(10)));
    }

    #[test]
    fn test_adaptive_gap_irregular_pattern() {
        // Mix of fast and slow writes — median should reflect the middle
        let base = Instant::now();
        // 5 writes: gaps of 10s, 10s, 300s, 10s → sorted intervals: 10, 10, 10, 300
        // median of even count = (10 + 10) / 2 = 10s → 2*10 = 20s → clamped to floor (5 min)
        let timestamps = vec![
            base,
            base + Duration::from_secs(10),
            base + Duration::from_secs(20),
            base + Duration::from_secs(320),
            base + Duration::from_secs(330),
        ];
        assert_eq!(adaptive_gap(&timestamps), BURST_GAP_FLOOR);
    }

    #[test]
    fn test_burst_detection_scenario() {
        // Simulate: 5 writes over 2.5 min, then 6 min silence → should be detected as burst end
        let ws = WriteSignal::new();
        let base = Instant::now();
        for i in 0..5 {
            ws.record_at("claude", base + Duration::from_secs(30 * i));
        }
        // Last write at base + 120s. Adaptive gap = floor (5 min) = 300s.
        // At base + 420s (120 + 300), the burst should be detected as ended.
        let gap = adaptive_gap(&ws.snapshot()["claude"]);
        assert_eq!(gap, BURST_GAP_FLOOR); // 5 min

        let last_write = base + Duration::from_secs(120);
        let check_time = last_write + BURST_GAP_FLOOR + Duration::from_secs(1);
        assert!(check_time.duration_since(last_write) > gap);

        // Verify drain preserves nothing (all writes before cutoff)
        let drained = ws.drain_up_to("claude", last_write);
        assert_eq!(drained.len(), 5);
        assert!(ws.snapshot().is_empty());
    }

    #[test]
    fn test_concurrent_agents_independent() {
        // Two agents writing — draining one doesn't affect the other
        let ws = WriteSignal::new();
        let base = Instant::now();

        for i in 0..5 {
            ws.record_at("claude", base + Duration::from_secs(30 * i));
        }
        for i in 0..3 {
            ws.record_at("obsidian", base + Duration::from_secs(60 * i));
        }

        // Drain claude only
        let cutoff = base + Duration::from_secs(120);
        ws.drain_up_to("claude", cutoff);

        let snap = ws.snapshot();
        assert!(!snap.contains_key("claude"));
        assert_eq!(snap["obsidian"].len(), 3);
    }

    // ── §4 Directory-source sync + enrichment-queue-drive tick ───────────────

    /// Isolate `WENLAN_DATA_DIR` (config lives there) to a tempdir for the
    /// duration of a test; restore the prior value on drop.
    struct DataDirGuard {
        previous: Option<std::ffi::OsString>,
        _tmp: tempfile::TempDir,
    }

    impl DataDirGuard {
        fn new() -> Self {
            let tmp = tempfile::tempdir().unwrap();
            let previous = std::env::var_os("WENLAN_DATA_DIR");
            std::env::set_var("WENLAN_DATA_DIR", tmp.path());
            Self {
                previous,
                _tmp: tmp,
            }
        }
    }

    impl Drop for DataDirGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => std::env::set_var("WENLAN_DATA_DIR", value),
                None => std::env::remove_var("WENLAN_DATA_DIR"),
            }
        }
    }

    fn register_directory_source(id: &str, path: &std::path::Path) {
        wenlan_core::config::save_config(&wenlan_core::config::Config {
            sources: vec![wenlan_types::sources::Source {
                id: id.to_string(),
                source_type: wenlan_types::sources::SourceType::Directory,
                path: path.to_path_buf(),
                status: wenlan_types::sources::SyncStatus::Active,
                last_sync: None,
                file_count: 0,
                memory_count: 0,
                last_sync_errors: 0,
                last_sync_error_detail: None,
            }],
            ..wenlan_core::config::Config::default()
        })
        .unwrap();
    }

    #[test]
    fn directory_sync_tick_polls_recoverable_sources_but_not_paused() {
        let mut source = wenlan_types::sources::Source {
            id: "directory-notes".to_string(),
            source_type: wenlan_types::sources::SourceType::Directory,
            path: std::path::PathBuf::from("/tmp/notes"),
            status: wenlan_types::sources::SyncStatus::Active,
            last_sync: None,
            file_count: 0,
            memory_count: 0,
            last_sync_errors: 0,
            last_sync_error_detail: None,
        };

        assert!(should_poll_directory_source(&source));

        source.status =
            wenlan_types::sources::SyncStatus::Unavailable("filesystem stalled".to_string());
        assert!(should_poll_directory_source(&source));

        source.status =
            wenlan_types::sources::SyncStatus::Error("transient file error".to_string());
        assert!(should_poll_directory_source(&source));

        source.status = wenlan_types::sources::SyncStatus::Paused;
        assert!(!should_poll_directory_source(&source));

        source.status = wenlan_types::sources::SyncStatus::Active;
        source.source_type = wenlan_types::sources::SourceType::Obsidian;
        assert!(!should_poll_directory_source(&source));
    }

    async fn new_test_db() -> (Arc<wenlan_core::db::MemoryDB>, tempfile::TempDir) {
        let db_dir = tempfile::tempdir().unwrap();
        let db = Arc::new(
            wenlan_core::db::MemoryDB::new(
                db_dir.path(),
                Arc::new(wenlan_core::events::NoopEmitter),
            )
            .await
            .unwrap(),
        );
        (db, db_dir)
    }

    /// One poll tick over a Directory source with a fresh file must enqueue AND
    /// process it into searchable chunks plus a SOURCE page. With no LLM, the
    /// enrichment route still embeds every chunk (searchable) and writes the
    /// deterministic stub SOURCE page — exactly what the page-watcher Step-0
    /// precedent does for its own cheap per-poll pass.
    #[tokio::test]
    async fn directory_sync_tick_processes_new_file_into_chunks_and_source_page() {
        let _lock = crate::TEST_DATA_DIR_LOCK
            .get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await;
        let _env = DataDirGuard::new();

        let source_root = tempfile::tempdir().unwrap();
        let file_path = source_root.path().join("note.txt");
        let mut body =
            String::from("Wenlanborg is the code name for the folder ingestion subsystem.\n\n");
        for i in 0..40 {
            body.push_str(&format!(
                "Paragraph {i} describes the document ingestion pipeline in concrete detail so the \
                 chunker splits it into multiple sections rather than a single chunk.\n\n"
            ));
        }
        std::fs::write(&file_path, &body).unwrap();

        let source_id = "directory-notes".to_string();
        register_directory_source(&source_id, source_root.path());

        let (db, _db_dir) = new_test_db().await;
        let prompts = wenlan_core::prompts::PromptRegistry::default();

        // One tick: sync (enqueue the file) + drive the queue (process it).
        let processed = run_directory_sync_tick(&db, None, &prompts, 10).await;
        assert_eq!(processed, 1, "the one new file is claimed and processed");

        // The file's chunks are stored + searchable.
        let knowledge_path = wenlan_core::config::load_config().knowledge_path_or_default();
        let doc_source_id = wenlan_core::sources::directory::document_source_id(
            &source_id,
            &file_path,
            Some(&knowledge_path),
        );
        let chunks = db
            .get_memories_by_source_id("memory", &doc_source_id)
            .await
            .unwrap();
        assert!(
            !chunks.is_empty(),
            "the new file must produce stored chunks"
        );

        let results = db
            .search_memory("Wenlanborg", 30, None, None, None, None, None, None)
            .await
            .unwrap();
        assert!(
            results.iter().any(|r| r.source_id == doc_source_id),
            "the new file's chunks must be searchable"
        );

        // A SOURCE page was written for the document.
        let pages = db.list_pages("active", 100, 0).await.unwrap();
        assert!(
            pages.iter().any(|p| p.creation_kind == "source"),
            "a source page must be written for the document"
        );

        // Queue row marked done.
        let q = db
            .get_queue_entry(&source_id, &file_path.to_string_lossy())
            .await
            .unwrap()
            .expect("queue entry exists after processing");
        assert_eq!(q.status, "done");
    }

    /// A paused queue row whose backoff has not elapsed must be SKIPPED by the
    /// tick (backoff auto-resume): `claim_next_pending` never returns it, so it
    /// is not processed and no chunks materialize.
    #[tokio::test]
    async fn directory_sync_tick_skips_paused_queue_with_future_retry() {
        let _lock = crate::TEST_DATA_DIR_LOCK
            .get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await;
        let _env = DataDirGuard::new();

        // Registered but empty source: sync finds nothing to enqueue.
        let source_root = tempfile::tempdir().unwrap();
        let source_id = "directory-notes".to_string();
        register_directory_source(&source_id, source_root.path());

        let (db, _db_dir) = new_test_db().await;
        let prompts = wenlan_core::prompts::PromptRegistry::default();

        // A paused document whose retry is an hour out.
        let paused_path = source_root
            .path()
            .join("paused.txt")
            .to_string_lossy()
            .to_string();
        db.enqueue_document(&source_id, &paused_path, Some("hash-paused"))
            .await
            .unwrap();
        let future_retry = chrono::Utc::now().timestamp() + 3600;
        db.mark_paused(
            &source_id,
            &paused_path,
            "analysis LLM failed",
            Some(future_retry),
        )
        .await
        .unwrap();

        let processed = run_directory_sync_tick(&db, None, &prompts, 10).await;
        assert_eq!(processed, 0, "a paused row with a future retry is skipped");

        let q = db
            .get_queue_entry(&source_id, &paused_path)
            .await
            .unwrap()
            .expect("paused entry remains");
        assert_eq!(q.status, "paused", "the row is still paused, not processed");

        let knowledge_path = wenlan_core::config::load_config().knowledge_path_or_default();
        let doc_source_id = wenlan_core::sources::directory::document_source_id(
            &source_id,
            std::path::Path::new(&paused_path),
            Some(&knowledge_path),
        );
        let chunks = db
            .get_memories_by_source_id("memory", &doc_source_id)
            .await
            .unwrap();
        assert!(
            chunks.is_empty(),
            "a skipped paused document must not be processed into chunks"
        );
    }

    struct MaintenanceTestProvider {
        body: String,
    }

    #[async_trait::async_trait]
    impl wenlan_core::llm_provider::LlmProvider for MaintenanceTestProvider {
        async fn generate(
            &self,
            _request: wenlan_core::llm_provider::LlmRequest,
        ) -> Result<String, wenlan_core::llm_provider::LlmError> {
            Ok(self.body.clone())
        }

        fn is_available(&self) -> bool {
            true
        }

        fn name(&self) -> &str {
            "maintenance-test"
        }

        fn backend(&self) -> wenlan_core::llm_provider::LlmBackend {
            wenlan_core::llm_provider::LlmBackend::Api
        }

        fn kind(&self) -> &'static str {
            "mock"
        }
    }

    async fn store_test_memory(db: &wenlan_core::db::MemoryDB, id: &str, content: &str) {
        db.upsert_documents(vec![wenlan_types::RawDocument {
            source: "memory".to_string(),
            source_id: id.to_string(),
            title: id.to_string(),
            content: content.to_string(),
            last_modified: chrono::Utc::now().timestamp(),
            memory_type: Some("fact".to_string()),
            source_agent: Some("test".to_string()),
            confirmed: Some(true),
            ..Default::default()
        }])
        .await
        .unwrap();
    }

    async fn insert_test_page(
        db: &wenlan_core::db::MemoryDB,
        title: &str,
        content: &str,
        source_ids: &[&str],
        creation_kind: &str,
    ) -> String {
        let source_memory_ids: Vec<String> = if creation_kind == "distilled" {
            source_ids.iter().map(|id| (*id).to_string()).collect()
        } else {
            Vec::new()
        };
        let result = wenlan_core::post_write::create_page_with_tuning(
            db,
            wenlan_types::requests::CreateConceptRequest {
                title: title.to_string(),
                content: content.to_string(),
                summary: None,
                entity_id: None,
                source_memory_ids,
                creation_kind: Some(creation_kind.to_string()),
                space: Some("work".to_string()),
                workspace: Some("work".to_string()),
            },
            "test",
            None,
            source_ids.len().max(1),
            1.1,
        )
        .await
        .unwrap();
        if creation_kind != "distilled" && !source_ids.is_empty() {
            let source_memory_ids: Vec<String> =
                source_ids.iter().map(|id| (*id).to_string()).collect();
            wenlan_core::post_write::page_write(
                db,
                wenlan_core::post_write::PageWrite::Attach {
                    page_id: &result.id,
                    source_memory_ids: &source_memory_ids,
                    link_reason: "test_fixture_attach",
                    agent: "test",
                },
            )
            .await
            .unwrap();
        }
        db.set_page_review_status(&result.id, "confirmed")
            .await
            .unwrap();
        result.id
    }

    #[tokio::test]
    async fn maintenance_tick_detects_page_merge_cards_and_routes_stale_pages() {
        let (db, _db_dir) = new_test_db().await;
        let source = "Rust ownership prevents data races at compile time.";
        for id in [
            "mem_dup_a",
            "mem_dup_b",
            "mem_dup_c",
            "mem_machine",
            "mem_human",
        ] {
            store_test_memory(&db, id, source).await;
        }

        let page_dup_a = insert_test_page(
            &db,
            "Rust ownership",
            "Rust ownership prevents data races at compile time.",
            &["mem_dup_a", "mem_dup_b", "mem_dup_c"],
            "distilled",
        )
        .await;
        let page_dup_b = insert_test_page(
            &db,
            "Rust borrowing",
            "Rust ownership prevents data races at compile time.",
            &["mem_dup_a", "mem_dup_b", "mem_dup_c"],
            "distilled",
        )
        .await;
        let page_machine_stale = insert_test_page(
            &db,
            "Machine stale page",
            "Old machine-owned prose.",
            &["mem_machine"],
            "research",
        )
        .await;
        let page_human_stale = insert_test_page(
            &db,
            "Human stale page",
            "Human-written prose must remain untouched.",
            &["mem_human"],
            "authored",
        )
        .await;
        let _page_orphan_source = insert_test_page(
            &db,
            "Orphan source",
            "This page links to [[Missing Topic]].",
            &["mem_machine"],
            "research",
        )
        .await;
        db.set_page_stale(&page_machine_stale, "source_updated")
            .await
            .unwrap();
        db.set_page_stale(&page_human_stale, "source_updated")
            .await
            .unwrap();

        let llm: std::sync::Arc<dyn wenlan_core::llm_provider::LlmProvider> =
            std::sync::Arc::new(MaintenanceTestProvider {
                body: format!("{source} [1]"),
            });
        let prompts = wenlan_core::prompts::PromptRegistry::default();

        let result = wenlan_core::maintenance::run_maintenance_tick(
            &db,
            Some(&llm),
            &prompts,
            &wenlan_core::maintenance::MaintenanceTickConfig {
                page_match_threshold: 0.85,
                formation_threshold: 0.60,
                page_min_cluster_size: 3,
                token_limit: 3500,
                max_unlinked_cluster_size: 20,
                max_grouped_cluster_size: 20,
                max_per_tick: 5,
            },
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.merge_cards_emitted, 1);
        assert_eq!(result.stale_machine_refreshed, 1);
        assert_eq!(result.stale_human_cards, 1);
        assert!(
            result.orphan_labels_checked >= 1,
            "the maintenance tick must run the orphan wikilink check"
        );
        assert_eq!(result.overview_refreshed, 1);

        let proposals = db.get_pending_refinements().await.unwrap();
        let merge_card = proposals
            .iter()
            .find(|p| p.action == "page_merge")
            .expect("near-duplicate pages must emit a page_merge card");
        assert_eq!(merge_card.source_ids.len(), 2);
        assert!(merge_card.source_ids.contains(&page_dup_a));
        assert!(merge_card.source_ids.contains(&page_dup_b));

        let machine = db
            .get_page(&page_machine_stale)
            .await
            .unwrap()
            .expect("machine page remains");
        assert_eq!(machine.stale_reason, None);
        assert!(
            machine
                .content
                .contains("Rust ownership prevents data races"),
            "machine-owned stale page should be refreshed in place"
        );

        let human = db
            .get_page(&page_human_stale)
            .await
            .unwrap()
            .expect("human page remains");
        assert_eq!(human.stale_reason, None);
        assert_eq!(human.content, "Human-written prose must remain untouched.");

        let revisions = db.list_pending_revisions(10).await.unwrap();
        assert!(
            revisions
                .iter()
                .any(|r| r.target_source_id == page_human_stale),
            "human-owned stale page should stage a revision card"
        );

        assert!(
            db.find_active_page_id_by_title("Overview")
                .await
                .unwrap()
                .is_some(),
            "overview refresh must create or update the reserved Overview page"
        );
    }
}
