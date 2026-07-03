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
/// Bounded per-poll drain of the document-enrichment queue. Serial (one doc at a
/// time); caps how many queued documents a single poll processes so a large
/// backlog can't monopolize the poll loop (steeps, page-watcher). Per-chunk
/// checkpointing means the remainder is simply picked up on the next poll.
const MAX_DOC_ENRICH_PER_POLL: usize = 4;

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
                    &prompts,
                    &refinery_cfg,
                    &confidence_cfg,
                    &distillation_cfg,
                    wenlan_core::refinery::TriggerKind::Idle,
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
                    &prompts,
                    &refinery_cfg,
                    &confidence_cfg,
                    &distillation_cfg,
                    wenlan_core::refinery::TriggerKind::Backstop,
                    "Backstop",
                )
                .await;
                last_backstop = now;
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
        }
    });
}

/// One Directory-source sync + document-enrichment-queue-drive pass (§4).
/// Factored out of the 30s poll loop so it is unit-testable without the timer.
///
/// Each call:
/// 1. syncs every registered Directory source via the SHARED
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

    // 1. Sync every Directory source (log-and-continue on error).
    for source in config
        .sources
        .iter()
        .filter(|s| s.source_type == wenlan_types::sources::SourceType::Directory)
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
    prompts: &wenlan_core::prompts::PromptRegistry,
    refinery_cfg: &wenlan_core::tuning::RefineryConfig,
    confidence_cfg: &wenlan_core::tuning::ConfidenceConfig,
    distillation_cfg: &wenlan_core::tuning::DistillationConfig,
    trigger: wenlan_core::refinery::TriggerKind,
    label: &str,
) {
    let started = std::time::Instant::now();
    match wenlan_core::refinery::run_periodic_steep_with_api(
        db,
        llm,
        api_llm,
        synthesis_llm,
        prompts,
        refinery_cfg,
        confidence_cfg,
        distillation_cfg,
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
}
