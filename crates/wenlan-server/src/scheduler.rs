// SPDX-License-Identifier: Apache-2.0
//! Event-driven steep scheduler.
//!
//! Owns all steep scheduling: BurstEnd (per-agent adaptive gap), Idle (global
//! 10-min quiet), Daily (first-wake-after-24h), and Backstop (6-hour safety net).
//! Replaces the former steep loop in main.rs.

use std::collections::{HashMap, VecDeque};
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
/// Daily interval — first quiet turn after 24 hours.
const DAILY_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
/// Poll interval — how often the scheduler checks trigger conditions.
const POLL_INTERVAL: Duration = Duration::from_secs(30);
/// Initial delay — lets on-device model warm up before first backstop.
const INITIAL_DELAY: Duration = Duration::from_secs(60);
const DERIVED_RECEIPT_SWEEP_INTERVAL: Duration = Duration::from_secs(30 * 60);
const ENRICHMENT_SWEEP_INTERVAL: Duration = Duration::from_secs(30 * 60);
const RECONCILE_SWEEP_INTERVAL: Duration = Duration::from_secs(30 * 60);
const CITATION_SWEEP_INTERVAL: Duration = Duration::from_secs(30 * 60);
/// Conservative hotfix envelope: reuse the existing ten-minute quiet horizon
/// between ambient LLM requests until supported-Mac profiling freezes a
/// measured duty-cycle policy.
const AMBIENT_COOLDOWN: Duration = IDLE_THRESHOLD;
const AUTOMATIC_STEEP_PHASE_CURSOR_PREFIX: &str = "automatic_steep_phase_cursor_v1";
const AUTOMATIC_MAINTENANCE_STAGE_CURSOR_KEY: &str = "automatic_maintenance_stage_cursor_v1";
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AmbientJob {
    Document,
    Classification,
    StructuredExtract,
    Entity,
    Title,
    PageGrowth,
    Reconcile,
    Citation,
}

impl AmbientJob {
    const ALL: [Self; 8] = [
        Self::Document,
        Self::Classification,
        Self::StructuredExtract,
        Self::Entity,
        Self::Title,
        Self::PageGrowth,
        Self::Reconcile,
        Self::Citation,
    ];
}

#[derive(Debug, Clone, Copy)]
struct AmbientAvailability {
    document: bool,
    classification: bool,
    structured_extract: bool,
    entity: bool,
    title: bool,
    page_growth: bool,
    reconcile: bool,
    citation: bool,
}

impl AmbientAvailability {
    /// Automatic lanes are a consent boundary as well as a capability check.
    /// Even deterministic document preparation stays queued until the pinned
    /// provider is both authorized and healthy, so a later pin can run the
    /// canonical document pipeline instead of inheriting a terminal stub.
    fn for_provider(provider_available: bool) -> Self {
        Self {
            document: provider_available,
            classification: provider_available,
            structured_extract: provider_available,
            entity: provider_available && wenlan_core::db::entity_sweep_enabled(),
            title: provider_available,
            page_growth: provider_available,
            reconcile: provider_available && wenlan_core::db::doc_reconcile_enabled(),
            citation: provider_available && wenlan_core::db::citation_backfill_enabled(),
        }
    }

    const fn supports(self, job: AmbientJob) -> bool {
        match job {
            AmbientJob::Document => self.document,
            AmbientJob::Classification => self.classification,
            AmbientJob::StructuredExtract => self.structured_extract,
            AmbientJob::Entity => self.entity,
            AmbientJob::Title => self.title,
            AmbientJob::PageGrowth => self.page_growth,
            AmbientJob::Reconcile => self.reconcile,
            AmbientJob::Citation => self.citation,
        }
    }
}

struct AmbientSchedule {
    cursor: usize,
    next_allowed_at: Instant,
    last_classification: Option<Instant>,
    last_structured_extract: Option<Instant>,
    last_entity: Option<Instant>,
    last_title: Option<Instant>,
    last_page_growth: Option<Instant>,
    last_reconcile: Option<Instant>,
    last_citation: Option<Instant>,
}

impl AmbientSchedule {
    fn new(now: Instant) -> Self {
        Self {
            cursor: 0,
            next_allowed_at: now,
            last_classification: None,
            last_structured_extract: None,
            last_entity: None,
            last_title: None,
            last_page_growth: None,
            last_reconcile: None,
            last_citation: None,
        }
    }

    fn select_due(
        &mut self,
        now: Instant,
        availability: AmbientAvailability,
    ) -> Option<AmbientJob> {
        for _ in 0..AmbientJob::ALL.len() {
            let job = AmbientJob::ALL[self.cursor];
            self.cursor = (self.cursor + 1) % AmbientJob::ALL.len();
            if !availability.supports(job) {
                continue;
            }
            let due = match job {
                AmbientJob::Document => true,
                AmbientJob::Classification => self
                    .last_classification
                    .is_none_or(|last| now.duration_since(last) >= ENRICHMENT_SWEEP_INTERVAL),
                AmbientJob::StructuredExtract => self
                    .last_structured_extract
                    .is_none_or(|last| now.duration_since(last) >= ENRICHMENT_SWEEP_INTERVAL),
                AmbientJob::Entity => self
                    .last_entity
                    .is_none_or(|last| now.duration_since(last) >= ENRICHMENT_SWEEP_INTERVAL),
                AmbientJob::Title => self
                    .last_title
                    .is_none_or(|last| now.duration_since(last) >= ENRICHMENT_SWEEP_INTERVAL),
                AmbientJob::PageGrowth => self
                    .last_page_growth
                    .is_none_or(|last| now.duration_since(last) >= ENRICHMENT_SWEEP_INTERVAL),
                AmbientJob::Reconcile => self
                    .last_reconcile
                    .is_none_or(|last| now.duration_since(last) >= RECONCILE_SWEEP_INTERVAL),
                AmbientJob::Citation => self
                    .last_citation
                    .is_none_or(|last| now.duration_since(last) >= CITATION_SWEEP_INTERVAL),
            };
            if !due {
                continue;
            }
            return Some(job);
        }
        None
    }

    /// Back off an empty periodic lane, but leave known backlog due. The global
    /// thermal cooldown still limits actual work; this only prevents a second
    /// 30-minute delay from turning catch-up into a multi-week drain.
    fn note_job_result(&mut self, job: AmbientJob, now: Instant, selected: bool) {
        if selected {
            return;
        }
        match job {
            AmbientJob::Document => {}
            AmbientJob::Classification => self.last_classification = Some(now),
            AmbientJob::StructuredExtract => self.last_structured_extract = Some(now),
            AmbientJob::Entity => self.last_entity = Some(now),
            AmbientJob::Title => self.last_title = Some(now),
            AmbientJob::PageGrowth => self.last_page_growth = Some(now),
            AmbientJob::Reconcile => self.last_reconcile = Some(now),
            AmbientJob::Citation => self.last_citation = Some(now),
        }
    }

    fn note_thermal_work_completion(&mut self, now: Instant) {
        self.next_allowed_at = now + AMBIENT_COOLDOWN;
    }
}

fn ambient_turn_allowed(now: Instant, last_activity: Instant, next_allowed_at: Instant) -> bool {
    now.saturating_duration_since(last_activity) >= IDLE_THRESHOLD && now >= next_allowed_at
}

fn automatic_heavy_turn_allowed(
    ambient_turn_owed: bool,
    now: Instant,
    last_activity: Instant,
    next_allowed_at: Instant,
) -> bool {
    !ambient_turn_owed && ambient_turn_allowed(now, last_activity, next_allowed_at)
}

fn refresh_last_user_activity(write_signal: &WriteSignal, last_user_activity: &mut Instant) {
    if let Some(latest) = write_signal
        .snapshot()
        .values()
        .flat_map(|timestamps| timestamps.iter().copied())
        .max()
    {
        *last_user_activity = (*last_user_activity).max(latest);
    }
}

fn should_backoff_ambient_lane(selected: bool, llm_calls: usize) -> bool {
    !selected && llm_calls == 0
}

fn ambient_work_consumes_thermal_turn(job: AmbientJob, selected: bool, llm_calls: usize) -> bool {
    llm_calls > 0
        || (matches!(
            job,
            AmbientJob::Document | AmbientJob::PageGrowth | AmbientJob::Reconcile
        ) && selected)
}

/// Ambient-only provider facade that fails closed after forwarding one LLM
/// request. The scheduler is serialized today; this guard keeps the thermal
/// invariant true if a slice later gains a hidden nested call.
struct AmbientBudgetProvider {
    inner: Arc<dyn wenlan_core::llm_provider::LlmProvider>,
    calls: Arc<std::sync::atomic::AtomicUsize>,
}

impl AmbientBudgetProvider {
    fn new(inner: Arc<dyn wenlan_core::llm_provider::LlmProvider>) -> Self {
        Self::with_shared_calls(inner, Arc::new(std::sync::atomic::AtomicUsize::new(0)))
    }

    fn with_shared_calls(
        inner: Arc<dyn wenlan_core::llm_provider::LlmProvider>,
        calls: Arc<std::sync::atomic::AtomicUsize>,
    ) -> Self {
        Self { inner, calls }
    }

    fn call_count(&self) -> usize {
        self.calls.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl wenlan_core::llm_provider::LlmProvider for AmbientBudgetProvider {
    fn generate<'life0, 'async_trait>(
        &'life0 self,
        request: wenlan_core::llm_provider::LlmRequest,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<String, wenlan_core::llm_provider::LlmError>>
                + Send
                + 'async_trait,
        >,
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            if self
                .calls
                .compare_exchange(
                    0,
                    1,
                    std::sync::atomic::Ordering::SeqCst,
                    std::sync::atomic::Ordering::SeqCst,
                )
                .is_err()
            {
                return Err(wenlan_core::llm_provider::LlmError::NotAvailable);
            }
            self.inner.generate(request).await
        })
    }

    fn is_available(&self) -> bool {
        self.inner.is_available()
    }

    fn name(&self) -> &str {
        self.inner.name()
    }

    fn backend(&self) -> wenlan_core::llm_provider::LlmBackend {
        self.inner.backend()
    }

    fn synthesis_token_limit(&self) -> usize {
        self.inner.synthesis_token_limit()
    }

    fn recommended_max_output(&self) -> u32 {
        self.inner.recommended_max_output()
    }

    fn context_size(&self) -> u32 {
        self.inner.context_size()
    }

    fn kind(&self) -> &'static str {
        self.inner.kind()
    }

    fn model_id(&self) -> String {
        self.inner.model_id()
    }
}

fn with_shared_automatic_budget(
    provider: Option<&Arc<dyn wenlan_core::llm_provider::LlmProvider>>,
    calls: Arc<std::sync::atomic::AtomicUsize>,
) -> Option<Arc<dyn wenlan_core::llm_provider::LlmProvider>> {
    provider.map(|provider| {
        Arc::new(AmbientBudgetProvider::with_shared_calls(
            provider.clone(),
            calls,
        )) as Arc<dyn wenlan_core::llm_provider::LlmProvider>
    })
}

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

    /// Record a write event for an agent. The store route calls this after the
    /// durable write so ambient work observes the full quiet horizon.
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum AutomaticTrigger {
    Maintenance,
    BurstEnd {
        agent: String,
        last_write: Instant,
        writes: usize,
        gap: Duration,
    },
    Idle,
    Daily,
    Backstop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MaintenanceAdmission {
    None,
    Ready,
    YieldToDueSteep,
}

fn maintenance_admission(
    maintenance_pending: bool,
    maintenance_stage_ran_since_steep: bool,
) -> MaintenanceAdmission {
    match (maintenance_pending, maintenance_stage_ran_since_steep) {
        (false, _) => MaintenanceAdmission::None,
        (true, false) => MaintenanceAdmission::Ready,
        (true, true) => MaintenanceAdmission::YieldToDueSteep,
    }
}

impl AutomaticTrigger {
    fn steep_kind(&self) -> Option<wenlan_core::refinery::TriggerKind> {
        match self {
            Self::Maintenance => None,
            Self::BurstEnd { .. } => Some(wenlan_core::refinery::TriggerKind::BurstEnd),
            Self::Idle => Some(wenlan_core::refinery::TriggerKind::Idle),
            Self::Daily => Some(wenlan_core::refinery::TriggerKind::Daily),
            Self::Backstop => Some(wenlan_core::refinery::TriggerKind::Backstop),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct AutomaticPhaseOutcome {
    progressed: bool,
    more: bool,
    retryable: bool,
    paused: bool,
}

impl From<&wenlan_core::refinery::SteepPhaseSliceReport> for AutomaticPhaseOutcome {
    fn from(report: &wenlan_core::refinery::SteepPhaseSliceReport) -> Self {
        Self {
            progressed: report.progressed,
            more: report.more,
            retryable: report.retryable,
            paused: report.paused,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct AutomaticMaintenanceOutcome {
    progressed: bool,
    more: bool,
    retryable: bool,
    paused: bool,
}

impl From<&wenlan_core::maintenance::MaintenanceSliceReport> for AutomaticMaintenanceOutcome {
    fn from(report: &wenlan_core::maintenance::MaintenanceSliceReport) -> Self {
        Self {
            progressed: report.progressed,
            more: report.more,
            retryable: report.retryable,
            paused: report.paused,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AutomaticBatchDisposition {
    Pending,
    Complete,
}

/// One finite automatic steep round. The trigger's completion marker is not
/// advanced until every eligible phase has yielded once. A phase with proven
/// additional progress moves to the tail, so it cannot monopolize the round.
struct AutomaticSteepBatch {
    trigger: AutomaticTrigger,
    phases: VecDeque<wenlan_core::refinery::Phase>,
}

/// Automatic work is deliberately narrower than the foreground steep API.
/// A phase earns this allowlist only after its scheduler entry point has a
/// durable cursor and a source-proven per-turn work bound.
fn automatic_phase_allowed(phase: wenlan_core::refinery::Phase) -> bool {
    matches!(phase, wenlan_core::refinery::Phase::ReDistill)
}

fn automatic_kind_has_work(kind: wenlan_core::refinery::TriggerKind) -> bool {
    wenlan_core::refinery::Phase::ALL
        .iter()
        .copied()
        .any(|phase| kind.runs_phase(phase) && automatic_phase_allowed(phase))
}

impl AutomaticSteepBatch {
    fn new(trigger: AutomaticTrigger, cursor: Option<wenlan_core::refinery::Phase>) -> Self {
        let kind = trigger
            .steep_kind()
            .expect("maintenance is scheduled through its own stage round");
        let mut phases = wenlan_core::refinery::Phase::ALL
            .iter()
            .copied()
            .filter(|phase| kind.runs_phase(*phase) && automatic_phase_allowed(*phase))
            .collect::<VecDeque<_>>();
        if let Some(cursor) = cursor {
            if let Some(position) = phases.iter().position(|phase| *phase == cursor) {
                phases.rotate_left(position);
            }
        }
        Self { trigger, phases }
    }

    fn next_phase(&self) -> Option<wenlan_core::refinery::Phase> {
        self.phases.front().copied()
    }

    #[cfg(test)]
    fn remaining_phases(&self) -> Vec<wenlan_core::refinery::Phase> {
        self.phases.iter().copied().collect()
    }

    fn complete_phase(
        &mut self,
        attempted: wenlan_core::refinery::Phase,
        outcome: AutomaticPhaseOutcome,
    ) -> AutomaticBatchDisposition {
        let selected = self
            .phases
            .pop_front()
            .expect("automatic batch cannot complete a phase while empty");
        debug_assert_eq!(selected, attempted);
        if outcome.progressed && outcome.more && !outcome.retryable && !outcome.paused {
            self.phases.push_back(attempted);
        }
        if self.phases.is_empty() {
            AutomaticBatchDisposition::Complete
        } else {
            AutomaticBatchDisposition::Pending
        }
    }

    fn cursor_after_attempt(
        &self,
        attempted: wenlan_core::refinery::Phase,
    ) -> wenlan_core::refinery::Phase {
        self.next_phase().unwrap_or_else(|| {
            let kind = self
                .trigger
                .steep_kind()
                .expect("automatic steep batch always has a steep trigger");
            let attempted_index = wenlan_core::refinery::Phase::ALL
                .iter()
                .position(|phase| *phase == attempted)
                .unwrap_or(0);
            (1..=wenlan_core::refinery::Phase::ALL.len())
                .map(|offset| {
                    wenlan_core::refinery::Phase::ALL
                        [(attempted_index + offset) % wenlan_core::refinery::Phase::ALL.len()]
                })
                .find(|phase| kind.runs_phase(*phase) && automatic_phase_allowed(*phase))
                .expect("a constructed automatic steep batch has an allowlisted phase")
        })
    }
}

struct AutomaticMaintenanceRound {
    stages: VecDeque<wenlan_core::maintenance::MaintenanceStage>,
}

impl AutomaticMaintenanceRound {
    fn new(cursor: Option<wenlan_core::maintenance::MaintenanceStage>) -> Self {
        let mut stages = wenlan_core::maintenance::MaintenanceStage::ALL
            .iter()
            .copied()
            .collect::<VecDeque<_>>();
        if let Some(cursor) = cursor {
            if let Some(position) = stages.iter().position(|stage| *stage == cursor) {
                stages.rotate_left(position);
            }
        }
        Self { stages }
    }

    fn next_stage(&self) -> Option<wenlan_core::maintenance::MaintenanceStage> {
        self.stages.front().copied()
    }

    #[cfg(test)]
    fn remaining_stages(&self) -> Vec<wenlan_core::maintenance::MaintenanceStage> {
        self.stages.iter().copied().collect()
    }

    fn complete_stage(
        &mut self,
        attempted: wenlan_core::maintenance::MaintenanceStage,
        outcome: AutomaticMaintenanceOutcome,
    ) -> AutomaticBatchDisposition {
        let selected = self
            .stages
            .pop_front()
            .expect("maintenance round cannot complete a stage while empty");
        debug_assert_eq!(selected, attempted);
        if outcome.progressed && outcome.more && !outcome.retryable && !outcome.paused {
            self.stages.push_back(attempted);
        }
        if self.stages.is_empty() {
            AutomaticBatchDisposition::Complete
        } else {
            AutomaticBatchDisposition::Pending
        }
    }

    fn cursor_after_attempt(
        &self,
        attempted: wenlan_core::maintenance::MaintenanceStage,
    ) -> wenlan_core::maintenance::MaintenanceStage {
        self.next_stage().unwrap_or_else(|| {
            let attempted_index = wenlan_core::maintenance::MaintenanceStage::ALL
                .iter()
                .position(|stage| *stage == attempted)
                .unwrap_or(0);
            wenlan_core::maintenance::MaintenanceStage::ALL
                [(attempted_index + 1) % wenlan_core::maintenance::MaintenanceStage::ALL.len()]
        })
    }
}

async fn load_automatic_maintenance_cursor(
    db: &wenlan_core::db::MemoryDB,
) -> Option<wenlan_core::maintenance::MaintenanceStage> {
    let value = db
        .get_app_metadata(AUTOMATIC_MAINTENANCE_STAGE_CURSOR_KEY)
        .await
        .ok()
        .flatten()?;
    wenlan_core::maintenance::MaintenanceStage::ALL
        .iter()
        .copied()
        .find(|stage| stage.as_str() == value)
}

async fn persist_automatic_maintenance_cursor(
    db: &wenlan_core::db::MemoryDB,
    stage: wenlan_core::maintenance::MaintenanceStage,
) {
    if let Err(error) = db
        .set_app_metadata(AUTOMATIC_MAINTENANCE_STAGE_CURSOR_KEY, stage.as_str())
        .await
    {
        tracing::warn!("[scheduler] failed to persist maintenance stage cursor '{stage}': {error}");
    }
}

fn automatic_phase_cursor_key(trigger: wenlan_core::refinery::TriggerKind) -> String {
    let suffix = match trigger {
        wenlan_core::refinery::TriggerKind::BurstEnd => "burst_end",
        wenlan_core::refinery::TriggerKind::Idle => "idle",
        wenlan_core::refinery::TriggerKind::Daily => "daily",
        wenlan_core::refinery::TriggerKind::Backstop => "backstop",
    };
    format!("{AUTOMATIC_STEEP_PHASE_CURSOR_PREFIX}_{suffix}")
}

async fn load_automatic_phase_cursor(
    db: &wenlan_core::db::MemoryDB,
    trigger: wenlan_core::refinery::TriggerKind,
) -> Option<wenlan_core::refinery::Phase> {
    let value = db
        .get_app_metadata(&automatic_phase_cursor_key(trigger))
        .await
        .ok()
        .flatten()?;
    wenlan_core::refinery::Phase::ALL
        .iter()
        .copied()
        .find(|phase| phase.as_str() == value)
}

async fn persist_automatic_phase_cursor(
    db: &wenlan_core::db::MemoryDB,
    trigger: wenlan_core::refinery::TriggerKind,
    phase: wenlan_core::refinery::Phase,
) {
    if let Err(error) = db
        .set_app_metadata(&automatic_phase_cursor_key(trigger), phase.as_str())
        .await
    {
        tracing::warn!(
            "[scheduler] failed to persist {:?} phase cursor '{}': {error}",
            trigger,
            phase
        );
    }
}

fn queues_maintenance_followup(trigger: &AutomaticTrigger) -> bool {
    matches!(trigger, AutomaticTrigger::Idle | AutomaticTrigger::Backstop)
}

/// Drain completed write bursts that cannot produce any bounded automatic
/// phase. This is bookkeeping only: it must not consume a thermal turn or
/// leave an unsupported BurstEnd trigger resident forever.
fn drain_expired_unactionable_bursts(write_signal: &WriteSignal, now: Instant) -> usize {
    let snapshot = write_signal.snapshot();
    let burst_end_supported = automatic_kind_has_work(wenlan_core::refinery::TriggerKind::BurstEnd);
    let mut drained = 0usize;
    for (agent, timestamps) in snapshot {
        if timestamps.is_empty() {
            continue;
        }
        if timestamps.len() >= MIN_BURST_WRITES && burst_end_supported {
            continue;
        }
        let Some(last_write) = timestamps.iter().copied().max() else {
            continue;
        };
        if now.saturating_duration_since(last_write) > adaptive_gap(&timestamps) {
            drained += write_signal.drain_up_to(&agent, last_write).len();
        }
    }
    drained
}

/// Choose at most one automatic heavy trigger for a scheduler poll. Burst
/// candidates are deterministic so a map iteration cannot accidentally turn
/// one poll into N inference-heavy runs.
fn select_due_automatic_trigger(
    now: Instant,
    snapshot: &HashMap<String, Vec<Instant>>,
    maintenance: MaintenanceAdmission,
    last_user_activity: Instant,
    idle_fired: bool,
    last_daily: Instant,
    last_backstop: Instant,
) -> Option<AutomaticTrigger> {
    if maintenance == MaintenanceAdmission::Ready {
        return Some(AutomaticTrigger::Maintenance);
    }
    let mut bursts = snapshot
        .iter()
        .filter_map(|(agent, timestamps)| {
            if timestamps.len() < MIN_BURST_WRITES {
                return None;
            }
            let last_write = timestamps.iter().copied().max()?;
            let gap = adaptive_gap(timestamps);
            (now.saturating_duration_since(last_write) > gap).then(|| AutomaticTrigger::BurstEnd {
                agent: agent.clone(),
                last_write,
                writes: timestamps.len(),
                gap,
            })
        })
        .collect::<Vec<_>>();
    bursts.sort_by(|left, right| match (left, right) {
        (
            AutomaticTrigger::BurstEnd {
                agent: left_agent, ..
            },
            AutomaticTrigger::BurstEnd {
                agent: right_agent, ..
            },
        ) => left_agent.cmp(right_agent),
        _ => std::cmp::Ordering::Equal,
    });
    if automatic_kind_has_work(wenlan_core::refinery::TriggerKind::BurstEnd) {
        if let Some(burst) = bursts.into_iter().next() {
            return Some(burst);
        }
    }
    if !idle_fired
        && now.saturating_duration_since(last_user_activity) >= IDLE_THRESHOLD
        && automatic_kind_has_work(wenlan_core::refinery::TriggerKind::Idle)
    {
        return Some(AutomaticTrigger::Idle);
    }
    if now.saturating_duration_since(last_daily) > DAILY_INTERVAL
        && automatic_kind_has_work(wenlan_core::refinery::TriggerKind::Daily)
    {
        return Some(AutomaticTrigger::Daily);
    }
    if now.saturating_duration_since(last_backstop) > BACKSTOP_INTERVAL
        && automatic_kind_has_work(wenlan_core::refinery::TriggerKind::Backstop)
    {
        return Some(AutomaticTrigger::Backstop);
    }
    (maintenance != MaintenanceAdmission::None).then_some(AutomaticTrigger::Maintenance)
}

/// Spawn the event-driven steep scheduler.
///
/// Runs a single tokio task with a 30-second poll loop. All work is awaited
/// inline, and the sticky lifecycle signal is checked at every owned boundary
/// so shutdown finishes the current item without starting another.
pub fn spawn_scheduler(
    shared: SharedState,
    write_signal: WriteSignal,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if crate::lifecycle::sleep_or_shutdown(&mut shutdown, INITIAL_DELAY).await {
            tracing::info!("[scheduler] shutdown before initial delay completed");
            return;
        }

        let mut last_backstop = Instant::now();
        let mut idle_fired = false;
        let mut last_poll_activity = Instant::now();
        let ambient_started_at = Instant::now();
        let mut last_user_activity = write_signal
            .snapshot()
            .values()
            .flat_map(|timestamps| timestamps.iter().copied())
            .max()
            .unwrap_or(ambient_started_at);
        let mut ambient_schedule = AmbientSchedule::new(ambient_started_at);
        let mut last_derived_receipt_sweep = None;
        let mut maintenance_pending = false;
        let mut maintenance_stage_ran_since_steep = false;
        let mut steep_batch: Option<AutomaticSteepBatch> = None;
        let mut maintenance_round: Option<AutomaticMaintenanceRound> = None;
        let mut ambient_turn_owed = false;

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
            if crate::lifecycle::sleep_or_shutdown(&mut shutdown, POLL_INTERVAL).await {
                break;
            }

            // Reset idle flag if any new activity arrived since last poll
            if write_signal.has_activity_since(last_poll_activity) {
                idle_fired = false;
                if let Some(latest) = write_signal
                    .snapshot()
                    .values()
                    .flat_map(|timestamps| timestamps.iter().copied())
                    .max()
                {
                    last_user_activity = last_user_activity.max(latest);
                }
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

            // Read routing consent and the knowledge root once per poll. Missing
            // pins authorize no background inference; deterministic work stays
            // available and pinned-but-missing providers never cross sources.
            let runtime_config = wenlan_core::config::load_config();
            let everyday_pin = wenlan_core::refinery::EverydaySource::parse(
                runtime_config.everyday_source.as_deref(),
            );
            let synthesis_pin = wenlan_core::refinery::SynthesisSource::parse(
                runtime_config.synthesis_source.as_deref(),
            );
            if crate::lifecycle::shutdown_requested(&shutdown) {
                break;
            }
            // --- 0. Filesystem page watcher: md → DB ---
            //
            // md is canonical. When the user edits a page in Obsidian / VS
            // Code / etc., reflect the change back into the DB so refinery
            // and search stay aligned with what the user actually wrote.
            // Cheap: a dir scan + frontmatter parse + content compare per
            // file. No LLM, no embedding, no network. Skips files whose
            // origin_version frontmatter trails the DB (daemon wrote
            // last). Runs every poll so freshness ≈ POLL_INTERVAL.
            let knowledge_path = runtime_config.knowledge_path_or_default();
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
            if crate::lifecycle::shutdown_requested(&shutdown) {
                break;
            }

            // --- 0b. Directory sources: cheap mtime/hash sync only (§4). ---
            //
            // Mirrors the page-watcher Step-0 as a cheap per-poll pass: run the
            // SAME sync routine the HTTP handler runs over each registered
            // Directory source (mtime+hash diff, deletion propagation — no LLM),
            // Changed files are queued here; the ambient controller claims at
            // most one bounded document slice after the quiet/cooldown gates.
            sync_directory_sources(&db).await;
            if crate::lifecycle::shutdown_requested(&shutdown) {
                break;
            }

            // Filesystem sync can take long enough for fresh writes to arrive;
            // all time comparisons below must use a post-sync clock sample.
            let now = Instant::now();

            // All automatic heavy work shares the same admission gate as the
            // ambient lanes. A due trigger stays due while the user is active
            // or while the ten-minute thermal cooldown is in force.
            refresh_last_user_activity(&write_signal, &mut last_user_activity);
            drain_expired_unactionable_bursts(&write_signal, now);
            let snap = write_signal.snapshot();

            let selected_automatic = automatic_heavy_turn_allowed(
                ambient_turn_owed,
                now,
                last_user_activity,
                ambient_schedule.next_allowed_at,
            )
            .then(|| {
                steep_batch
                    .as_ref()
                    .map(|batch| batch.trigger.clone())
                    .or_else(|| {
                        select_due_automatic_trigger(
                            now,
                            &snap,
                            maintenance_admission(
                                maintenance_pending,
                                maintenance_stage_ran_since_steep,
                            ),
                            last_user_activity,
                            idle_fired,
                            last_daily,
                            last_backstop,
                        )
                    })
            })
            .flatten();
            let mut automatic_work_ran = false;

            if let Some(trigger) = selected_automatic {
                let shared_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
                let budgeted_llm = with_shared_automatic_budget(llm.as_ref(), shared_calls.clone());
                let budgeted_api_llm =
                    with_shared_automatic_budget(api_llm.as_ref(), shared_calls.clone());
                let budgeted_synthesis_llm =
                    with_shared_automatic_budget(synthesis_llm.as_ref(), shared_calls.clone());
                let budgeted_external_llm =
                    with_shared_automatic_budget(external_llm.as_ref(), shared_calls.clone());
                let maintenance_llm = resolve_maintenance_provider(
                    synthesis_pin,
                    budgeted_synthesis_llm.as_ref(),
                    budgeted_api_llm.as_ref(),
                    budgeted_external_llm.as_ref(),
                    budgeted_llm.as_ref(),
                );
                let label = match &trigger {
                    AutomaticTrigger::Maintenance => "Maintenance",
                    AutomaticTrigger::BurstEnd { .. } => "BurstEnd",
                    AutomaticTrigger::Idle => "Idle",
                    AutomaticTrigger::Daily => "Daily",
                    AutomaticTrigger::Backstop => "Backstop",
                };
                // Setup and provider resolution may race with a stop request.
                // Re-check the sticky signal at the final launch boundary so
                // shutdown never starts a new steep/maintenance item.
                if crate::lifecycle::shutdown_requested(&shutdown) {
                    break;
                }

                match trigger {
                    AutomaticTrigger::Maintenance => {
                        if maintenance_round.is_none() {
                            let cursor = load_automatic_maintenance_cursor(&db).await;
                            maintenance_round = Some(AutomaticMaintenanceRound::new(cursor));
                        }
                        let stage = maintenance_round
                            .as_ref()
                            .and_then(AutomaticMaintenanceRound::next_stage)
                            .expect("pending maintenance round has a stage");
                        tracing::info!(
                            "[scheduler] Maintenance stage={stage} — deferred automatic turn"
                        );
                        let outcome = fire_maintenance_stage_safe(
                            db.as_ref(),
                            maintenance_llm.as_ref(),
                            &prompts,
                            &distillation_cfg,
                            Some(knowledge_path.as_path()),
                            stage,
                            label,
                        )
                        .await;
                        let (disposition, cursor) = {
                            let round = maintenance_round
                                .as_mut()
                                .expect("maintenance round survives its stage");
                            let disposition = round.complete_stage(stage, outcome);
                            (disposition, round.cursor_after_attempt(stage))
                        };
                        persist_automatic_maintenance_cursor(&db, cursor).await;
                        if disposition == AutomaticBatchDisposition::Complete {
                            maintenance_round = None;
                            maintenance_pending = false;
                            maintenance_stage_ran_since_steep = false;
                        } else {
                            maintenance_stage_ran_since_steep = true;
                        }
                    }
                    trigger => {
                        maintenance_stage_ran_since_steep = false;
                        if steep_batch.is_none() {
                            let kind = trigger
                                .steep_kind()
                                .expect("maintenance handled in the previous match arm");
                            let cursor = load_automatic_phase_cursor(&db, kind).await;
                            steep_batch = Some(AutomaticSteepBatch::new(trigger.clone(), cursor));
                        }
                        let (kind, phase) = {
                            let batch = steep_batch
                                .as_ref()
                                .expect("automatic steep batch initialized above");
                            (
                                batch
                                    .trigger
                                    .steep_kind()
                                    .expect("automatic steep batch has a steep trigger"),
                                batch
                                    .next_phase()
                                    .expect("automatic steep batch has an eligible phase"),
                            )
                        };
                        if let AutomaticTrigger::BurstEnd {
                            agent, writes, gap, ..
                        } = &trigger
                        {
                            tracing::info!(
                                "[scheduler] BurstEnd phase={} for agent '{}' — {} writes, gap {:?}",
                                phase,
                                agent,
                                writes,
                                gap
                            );
                        } else {
                            tracing::info!("[scheduler] {label} phase={phase}");
                        }
                        let outcome = fire_steep_phase_safe(
                            &db,
                            budgeted_llm.as_ref(),
                            budgeted_api_llm.as_ref(),
                            budgeted_synthesis_llm.as_ref(),
                            budgeted_external_llm.as_ref(),
                            &prompts,
                            &refinery_cfg,
                            &confidence_cfg,
                            &distillation_cfg,
                            kind,
                            phase,
                            label,
                        )
                        .await;
                        let (disposition, cursor) = {
                            let batch = steep_batch
                                .as_mut()
                                .expect("automatic steep batch survives its phase");
                            let disposition = batch.complete_phase(phase, outcome);
                            (disposition, batch.cursor_after_attempt(phase))
                        };
                        // Persist after every attempt, including captured errors
                        // and panics, so one poison phase cannot pin restarts.
                        persist_automatic_phase_cursor(&db, kind, cursor).await;

                        if disposition == AutomaticBatchDisposition::Complete {
                            let completed = steep_batch
                                .take()
                                .expect("completed automatic steep batch exists")
                                .trigger;
                            if queues_maintenance_followup(&completed) {
                                maintenance_pending = true;
                            }
                            match completed {
                                AutomaticTrigger::BurstEnd {
                                    agent, last_write, ..
                                } => {
                                    write_signal.drain_up_to(&agent, last_write);
                                }
                                AutomaticTrigger::Idle => idle_fired = true,
                                AutomaticTrigger::Daily => {
                                    last_daily = Instant::now();
                                    let epoch = chrono::Utc::now().timestamp().to_string();
                                    if let Err(error) =
                                        db.set_app_metadata("last_daily_steep_ts", &epoch).await
                                    {
                                        tracing::warn!(
                                            "[scheduler] failed to persist last_daily_steep_ts: {error}"
                                        );
                                    }
                                }
                                AutomaticTrigger::Backstop => last_backstop = Instant::now(),
                                AutomaticTrigger::Maintenance => unreachable!(
                                    "maintenance never enters an automatic steep batch"
                                ),
                            }
                        }
                    }
                }
                automatic_work_ran = true;
                // A multi-phase steep or maintenance round must yield one
                // admitted slot to the ambient round-robin before continuing.
                ambient_turn_owed = true;
                let completion = Instant::now();
                ambient_schedule.note_thermal_work_completion(completion);
                tracing::info!(
                    "[scheduler] automatic trigger={} llm_calls={} elapsed_ms={} next_eligible_ms={}",
                    label,
                    shared_calls.load(std::sync::atomic::Ordering::SeqCst),
                    completion.saturating_duration_since(now).as_millis(),
                    ambient_schedule
                        .next_allowed_at
                        .saturating_duration_since(completion)
                        .as_millis(),
                );
            }
            if crate::lifecycle::shutdown_requested(&shutdown) {
                break;
            }

            if let Err(error) =
                run_derived_receipt_sweep_if_due(&mut last_derived_receipt_sweep, now, || {
                    db.record_derived_artifact_sweep()
                })
                .await
            {
                tracing::warn!("[scheduler] derived receipt sweep error: {error}");
            }
            if crate::lifecycle::shutdown_requested(&shutdown) {
                break;
            }

            // --- 5. Ambient enrichment: one due job, one durable slice, one
            //        LLM request maximum. Never detached. ---
            refresh_last_user_activity(&write_signal, &mut last_user_activity);
            let ambient_now = Instant::now();
            if !automatic_work_ran
                && ambient_turn_allowed(
                    ambient_now,
                    last_user_activity,
                    ambient_schedule.next_allowed_at,
                )
            {
                let ambient_provider_available = resolve_ambient_provider(
                    everyday_pin,
                    api_llm.as_ref(),
                    external_llm.as_ref(),
                    llm.as_ref(),
                )
                .is_some();
                let availability = AmbientAvailability::for_provider(ambient_provider_available);
                if let Some(job) = ambient_schedule.select_due(ambient_now, availability) {
                    // Availability/selection is intentionally cheap, but may
                    // still race with shutdown. Do not start another ambient
                    // item after the stop signal became sticky.
                    if crate::lifecycle::shutdown_requested(&shutdown) {
                        break;
                    }
                    let report = run_ambient_job_safe(
                        job,
                        &db,
                        llm.as_ref(),
                        api_llm.as_ref(),
                        external_llm.as_ref(),
                        everyday_pin,
                        &prompts,
                        &refinery_cfg,
                        &distillation_cfg,
                        Some(knowledge_path.as_path()),
                    )
                    .await;
                    let completion = Instant::now();
                    ambient_schedule.note_job_result(
                        report.job,
                        completion,
                        !should_backoff_ambient_lane(report.selected, report.llm_calls),
                    );
                    // Fresh-document preparation can be CPU-heavy even before
                    // an LLM call, so a selected document also consumes the
                    // conservative thermal turn budget.
                    if report.panicked
                        || ambient_work_consumes_thermal_turn(
                            report.job,
                            report.selected,
                            report.llm_calls,
                        )
                    {
                        ambient_schedule.note_thermal_work_completion(completion);
                    }
                    tracing::info!(
                        "[scheduler] ambient job={:?} selected={} llm_calls={} panicked={} elapsed_ms={} next_eligible_ms={}",
                        report.job,
                        report.selected,
                        report.llm_calls,
                        report.panicked,
                        report.elapsed.as_millis(),
                        ambient_schedule
                            .next_allowed_at
                            .saturating_duration_since(completion)
                            .as_millis(),
                    );
                }
                // The ambient lane received its admission opportunity. Empty
                // work is enough to release the debt; known selected work owns
                // the shared cooldown through `note_thermal_work_completion`.
                ambient_turn_owed = false;
            }
            if crate::lifecycle::shutdown_requested(&shutdown) {
                break;
            }
        }
        tracing::info!("[scheduler] stopped after shutdown request");
    })
}

/// Background polling respects an explicit pause but keeps probing unavailable
/// roots so transient filesystem failures can recover automatically.
fn should_poll_directory_source(source: &wenlan_types::sources::Source) -> bool {
    source.source_type == wenlan_types::sources::SourceType::Directory
        && !matches!(source.status, wenlan_types::sources::SyncStatus::Paused)
}

/// One Directory-source sync + document-enrichment-queue-drive pass (§4).
/// Factored out of the 30s poll loop so it is unit-testable without the timer.
/// Sync every recoverable Directory source via the shared source route. This
/// only discovers changes and updates the durable queue; LLM work is owned by
/// the ambient scheduler below.
async fn sync_directory_sources(db: &Arc<wenlan_core::db::MemoryDB>) {
    let config = wenlan_core::config::load_config();
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
}

#[derive(Debug)]
struct AmbientTurnReport {
    job: AmbientJob,
    selected: bool,
    llm_calls: usize,
    panicked: bool,
    elapsed: Duration,
}

fn resolve_ambient_provider(
    everyday_pin: Option<wenlan_core::refinery::EverydaySource>,
    api_llm: Option<&Arc<dyn wenlan_core::llm_provider::LlmProvider>>,
    external_llm: Option<&Arc<dyn wenlan_core::llm_provider::LlmProvider>>,
    llm: Option<&Arc<dyn wenlan_core::llm_provider::LlmProvider>>,
) -> Option<Arc<dyn wenlan_core::llm_provider::LlmProvider>> {
    wenlan_core::refinery::resolve_everyday(everyday_pin, api_llm, external_llm, llm)
        .llm
        .cloned()
}

fn resolve_maintenance_provider(
    synthesis_pin: Option<wenlan_core::refinery::SynthesisSource>,
    synthesis_llm: Option<&Arc<dyn wenlan_core::llm_provider::LlmProvider>>,
    api_llm: Option<&Arc<dyn wenlan_core::llm_provider::LlmProvider>>,
    external_llm: Option<&Arc<dyn wenlan_core::llm_provider::LlmProvider>>,
    llm: Option<&Arc<dyn wenlan_core::llm_provider::LlmProvider>>,
) -> Option<Arc<dyn wenlan_core::llm_provider::LlmProvider>> {
    wenlan_core::refinery::resolve_synthesis(
        synthesis_pin,
        synthesis_llm,
        api_llm,
        external_llm,
        llm,
    )
    .llm
    .cloned()
}

#[allow(clippy::too_many_arguments)]
async fn run_ambient_job_safe(
    job: AmbientJob,
    db: &Arc<wenlan_core::db::MemoryDB>,
    llm: Option<&Arc<dyn wenlan_core::llm_provider::LlmProvider>>,
    api_llm: Option<&Arc<dyn wenlan_core::llm_provider::LlmProvider>>,
    external_llm: Option<&Arc<dyn wenlan_core::llm_provider::LlmProvider>>,
    everyday_pin: Option<wenlan_core::refinery::EverydaySource>,
    prompts: &wenlan_core::prompts::PromptRegistry,
    refinery: &wenlan_core::tuning::RefineryConfig,
    distillation: &wenlan_core::tuning::DistillationConfig,
    knowledge_path: Option<&std::path::Path>,
) -> AmbientTurnReport {
    let started = Instant::now();
    let future = std::panic::AssertUnwindSafe(run_ambient_job(
        job,
        db,
        llm,
        api_llm,
        external_llm,
        everyday_pin,
        prompts,
        refinery,
        distillation,
        knowledge_path,
    ));
    match futures::FutureExt::catch_unwind(future).await {
        Ok(report) => report,
        Err(error) => {
            let message = if let Some(message) = error.downcast_ref::<&str>() {
                (*message).to_string()
            } else if let Some(message) = error.downcast_ref::<String>() {
                message.clone()
            } else {
                "unknown panic".to_string()
            };
            tracing::error!(
                "[scheduler] ambient job={job:?} PANICKED — scheduler continues: {message}"
            );
            AmbientTurnReport {
                job,
                selected: true,
                llm_calls: 0,
                panicked: true,
                elapsed: started.elapsed(),
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_ambient_job(
    job: AmbientJob,
    db: &Arc<wenlan_core::db::MemoryDB>,
    llm: Option<&Arc<dyn wenlan_core::llm_provider::LlmProvider>>,
    api_llm: Option<&Arc<dyn wenlan_core::llm_provider::LlmProvider>>,
    external_llm: Option<&Arc<dyn wenlan_core::llm_provider::LlmProvider>>,
    everyday_pin: Option<wenlan_core::refinery::EverydaySource>,
    prompts: &wenlan_core::prompts::PromptRegistry,
    refinery: &wenlan_core::tuning::RefineryConfig,
    distillation: &wenlan_core::tuning::DistillationConfig,
    knowledge_path: Option<&std::path::Path>,
) -> AmbientTurnReport {
    let started = Instant::now();
    let observed = resolve_ambient_provider(everyday_pin, api_llm, external_llm, llm)
        .map(|provider| Arc::new(AmbientBudgetProvider::new(provider)));
    let provider: Option<Arc<dyn wenlan_core::llm_provider::LlmProvider>> = observed
        .as_ref()
        .map(|provider| provider.clone() as Arc<dyn wenlan_core::llm_provider::LlmProvider>);

    let selected = match job {
        AmbientJob::Document => {
            run_document_enrichment_slice_tick(db, provider.as_ref(), prompts).await > 0
        }
        AmbientJob::Classification => {
            let Some(provider) = provider.as_ref() else {
                return AmbientTurnReport {
                    job,
                    selected: false,
                    llm_calls: 0,
                    panicked: false,
                    elapsed: started.elapsed(),
                };
            };
            match wenlan_core::ingest::run_classification_enrichment_slice(db, provider, prompts)
                .await
            {
                Ok(report) => report.selected,
                Err(error) => {
                    tracing::warn!("[scheduler] classification slice error: {error}");
                    false
                }
            }
        }
        AmbientJob::StructuredExtract => {
            let Some(provider) = provider.as_ref() else {
                return AmbientTurnReport {
                    job,
                    selected: false,
                    llm_calls: 0,
                    panicked: false,
                    elapsed: started.elapsed(),
                };
            };
            match wenlan_core::ingest::run_structured_extract_slice(db, provider, prompts).await {
                Ok(report) => report.selected,
                Err(error) => {
                    tracing::warn!("[scheduler] structured extraction slice error: {error}");
                    false
                }
            }
        }
        AmbientJob::Entity => {
            let Some(provider) = provider.clone() else {
                return AmbientTurnReport {
                    job,
                    selected: false,
                    llm_calls: 0,
                    panicked: false,
                    elapsed: started.elapsed(),
                };
            };
            let prompts = prompts.clone();
            match db
                .run_entity_enrichment_slice_with_auto_link(
                    refinery.entity_link_distance as f32,
                    move |content: String| {
                        let provider = provider.clone();
                        let prompts = prompts.clone();
                        async move {
                            wenlan_core::kg::entity_extraction::extract_kg(
                                &provider, &prompts, &content,
                            )
                            .await
                        }
                    },
                )
                .await
            {
                Ok(selected) => selected > 0,
                Err(error) => {
                    tracing::warn!("[scheduler] entity enrichment slice error: {error}");
                    false
                }
            }
        }
        AmbientJob::Title => {
            let Some(provider) = provider.as_ref() else {
                return AmbientTurnReport {
                    job,
                    selected: false,
                    llm_calls: 0,
                    panicked: false,
                    elapsed: started.elapsed(),
                };
            };
            match wenlan_core::post_ingest::run_title_enrichment_slice(db, provider).await {
                Ok(report) => report.selected,
                Err(error) => {
                    tracing::warn!("[scheduler] title enrichment slice error: {error}");
                    false
                }
            }
        }
        AmbientJob::PageGrowth => {
            let Some(provider) = provider.as_ref() else {
                return AmbientTurnReport {
                    job,
                    selected: false,
                    llm_calls: 0,
                    panicked: false,
                    elapsed: started.elapsed(),
                };
            };
            match wenlan_core::post_ingest::run_page_growth_slice(
                db,
                provider,
                prompts,
                distillation.page_growth_threshold,
                knowledge_path,
            )
            .await
            {
                Ok(report) => report.selected,
                Err(error) => {
                    tracing::warn!("[scheduler] page growth slice error: {error}");
                    false
                }
            }
        }
        AmbientJob::Reconcile => {
            let Some(provider) = provider.as_ref() else {
                return AmbientTurnReport {
                    job,
                    selected: false,
                    llm_calls: 0,
                    panicked: false,
                    elapsed: started.elapsed(),
                };
            };
            match wenlan_core::reconcile::run_reconcile_slice(
                db,
                provider,
                prompts,
                refinery,
                distillation,
            )
            .await
            {
                Ok(report) => report.progressed,
                Err(error) => {
                    tracing::warn!("[scheduler] reconcile slice error: {error}");
                    false
                }
            }
        }
        AmbientJob::Citation => {
            let Some(provider) = provider.as_ref() else {
                return AmbientTurnReport {
                    job,
                    selected: false,
                    llm_calls: 0,
                    panicked: false,
                    elapsed: started.elapsed(),
                };
            };
            match wenlan_core::citations::run_citation_backfill_slice(db, provider, prompts).await {
                Ok(selected) => selected > 0,
                Err(error) => {
                    tracing::warn!("[scheduler] citation backfill slice error: {error}");
                    false
                }
            }
        }
    };

    AmbientTurnReport {
        job,
        selected,
        llm_calls: observed
            .as_ref()
            .map_or(0, |provider| provider.call_count()),
        panicked: false,
        elapsed: started.elapsed(),
    }
}

/// Claim at most one document and advance it by at most one LLM request.
/// Paused rows retain their existing backoff through `claim_next_pending`.
async fn run_document_enrichment_slice_tick(
    db: &Arc<wenlan_core::db::MemoryDB>,
    llm: Option<&Arc<dyn wenlan_core::llm_provider::LlmProvider>>,
    prompts: &wenlan_core::prompts::PromptRegistry,
) -> usize {
    let knowledge_path = wenlan_core::config::load_config().knowledge_path_or_default();
    match db.claim_next_pending().await {
        Ok(Some(entry)) => {
            let slice = std::panic::AssertUnwindSafe(
                wenlan_core::document_enrichment::run_document_enrichment_slice(
                    db,
                    &entry,
                    Some(&knowledge_path),
                    llm,
                    prompts,
                ),
            );
            match futures::FutureExt::catch_unwind(slice).await {
                Ok(_) => 1,
                Err(panic) => {
                    wenlan_core::document_enrichment::pause_document_enrichment_after_panic(
                        db, &entry,
                    )
                    .await;
                    std::panic::resume_unwind(panic);
                }
            }
        }
        Ok(None) => 0,
        Err(error) => {
            tracing::warn!("[scheduler] claim_next_pending failed: {error}");
            0
        }
    }
}

async fn fire_maintenance_stage_safe(
    db: &wenlan_core::db::MemoryDB,
    llm: Option<&Arc<dyn wenlan_core::llm_provider::LlmProvider>>,
    prompts: &wenlan_core::prompts::PromptRegistry,
    distillation_cfg: &wenlan_core::tuning::DistillationConfig,
    knowledge_path: Option<&std::path::Path>,
    stage: wenlan_core::maintenance::MaintenanceStage,
    label: &str,
) -> AutomaticMaintenanceOutcome {
    let config = wenlan_core::maintenance::MaintenanceTickConfig {
        page_match_threshold: distillation_cfg.page_match_threshold,
        formation_threshold: distillation_cfg.formation_threshold,
        page_min_cluster_size: distillation_cfg.page_min_cluster_size,
        token_limit: distillation_cfg.ondevice_token_limit,
        max_unlinked_cluster_size: distillation_cfg.max_unlinked_cluster_size,
        max_grouped_cluster_size: distillation_cfg.max_grouped_cluster_size,
        max_per_tick: 5,
    };
    let result =
        std::panic::AssertUnwindSafe(wenlan_core::maintenance::run_maintenance_stage_slice(
            db,
            llm,
            prompts,
            &config,
            knowledge_path,
            stage,
        ));
    match futures::FutureExt::catch_unwind(result).await {
        Ok(Ok(report)) => {
            let result = &report.result;
            tracing::info!(
                "[scheduler] {label} maintenance stage={stage}: selected={}, progressed={}, more={}, retryable={}, paused={}; work pages={}, pairs={}, source_rows={}, raw_seeds={}, eligible_seed_probes={}, ANN_rows={}, fully_filtered_seeds={}, truncated={}; {} merge card(s), {} discovery card(s), {} retro card(s) from {} observed, {} machine refresh(es), {} human card(s), {} orphan label(s), {} overview refresh(es)",
                report.selected,
                report.progressed,
                report.more,
                report.retryable,
                report.paused,
                report.work.pages_examined,
                report.work.pairs_examined,
                report.work.source_rows_examined,
                report.work.seeds_examined,
                report.work.eligible_seeds_probed,
                report.work.neighbor_rows_examined,
                report.work.fully_filtered_seeds,
                report.work.truncated,
                result.merge_cards_emitted,
                result.discovery_cards_emitted,
                result.retro_cards_emitted,
                result.retro_expected_card_volume,
                result.stale_machine_refreshed,
                result.stale_human_cards,
                result.orphan_labels_checked,
                result.overview_refreshed
            );
            AutomaticMaintenanceOutcome::from(&report)
        }
        Ok(Err(error)) => {
            tracing::warn!("[scheduler] {label} maintenance stage={stage} error: {error}");
            AutomaticMaintenanceOutcome {
                retryable: true,
                ..AutomaticMaintenanceOutcome::default()
            }
        }
        Err(error) => {
            let message = if let Some(message) = error.downcast_ref::<&str>() {
                message.to_string()
            } else if let Some(message) = error.downcast_ref::<String>() {
                message.clone()
            } else {
                "unknown panic".to_string()
            };
            tracing::error!(
                "[scheduler] {label} maintenance stage={stage} PANICKED — scheduler continues: {message}"
            );
            AutomaticMaintenanceOutcome {
                retryable: true,
                ..AutomaticMaintenanceOutcome::default()
            }
        }
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

/// Fire one steep phase with panic isolation. Every outcome returns to the
/// finite batch so its durable cursor advances even after an error or panic.
#[allow(clippy::too_many_arguments)]
async fn fire_steep_phase_safe(
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
    phase: wenlan_core::refinery::Phase,
    label: &str,
) -> AutomaticPhaseOutcome {
    let result = std::panic::AssertUnwindSafe(fire_steep_phase(
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
        phase,
        label,
    ));
    match futures::FutureExt::catch_unwind(result).await {
        Ok(outcome) => outcome,
        Err(error) => {
            let message = if let Some(message) = error.downcast_ref::<&str>() {
                message.to_string()
            } else if let Some(message) = error.downcast_ref::<String>() {
                message.clone()
            } else {
                "unknown panic".to_string()
            };
            tracing::error!(
                "[scheduler] {label} phase={phase} PANICKED — scheduler continues: {message}"
            );
            AutomaticPhaseOutcome {
                retryable: true,
                ..AutomaticPhaseOutcome::default()
            }
        }
    }
}

/// Fire one phase with the given trigger, log its result, and return scheduler
/// control metadata. Phase errors are captured inside `SteepPhaseSliceReport`.
#[allow(clippy::too_many_arguments)]
async fn fire_steep_phase(
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
    phase: wenlan_core::refinery::Phase,
    label: &str,
) -> AutomaticPhaseOutcome {
    let started = std::time::Instant::now();
    let knowledge_path = wenlan_core::config::load_config().knowledge_path_or_default();
    match wenlan_core::refinery::run_periodic_steep_phase_with_api(
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
        phase,
    )
    .await
    {
        Ok(report) => {
            let errors = report
                .result
                .phases
                .iter()
                .filter(|phase| phase.error.is_some())
                .count();
            tracing::info!(
                "[scheduler] {label} phase={phase} complete in {}ms — {} error(s), selected={}, progressed={}, more={}, retryable={}, paused={}",
                started.elapsed().as_millis(),
                errors,
                report.selected,
                report.progressed,
                report.more,
                report.retryable,
                report.paused,
            );

            for phase_result in &report.result.phases {
                if phase_result.nudge != wenlan_core::refinery::Nudge::Silent {
                    if let Some(ref headline) = phase_result.headline {
                        if let Err(e) = db
                            .log_agent_activity("origin", "steep", &[], None, headline)
                            .await
                        {
                            tracing::warn!(
                                "[scheduler] log activity for phase {} failed: {}",
                                phase_result.name,
                                e
                            );
                        }
                    }
                }
            }
            AutomaticPhaseOutcome::from(&report)
        }
        Err(error) => {
            tracing::warn!("[scheduler] {label} phase={phase} error: {error}");
            AutomaticPhaseOutcome {
                retryable: true,
                ..AutomaticPhaseOutcome::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ambient_schedule_includes_fixed_memory_stages() {
        let now = Instant::now();
        let mut schedule = AmbientSchedule::new(now);
        let available = AmbientAvailability {
            document: true,
            classification: true,
            structured_extract: true,
            entity: true,
            title: true,
            page_growth: true,
            reconcile: true,
            citation: true,
        };
        assert_eq!(
            (0..8)
                .filter_map(|_| schedule.select_due(now, available))
                .collect::<Vec<_>>(),
            vec![
                AmbientJob::Document,
                AmbientJob::Classification,
                AmbientJob::StructuredExtract,
                AmbientJob::Entity,
                AmbientJob::Title,
                AmbientJob::PageGrowth,
                AmbientJob::Reconcile,
                AmbientJob::Citation,
            ]
        );
    }

    #[test]
    fn unconfigured_or_unavailable_pin_disables_every_automatic_llm_lane() {
        let availability = AmbientAvailability::for_provider(false);
        for job in AmbientJob::ALL {
            assert!(
                !availability.supports(job),
                "{job:?} must leave durable work pending until an authorized provider is available"
            );
        }
    }

    #[test]
    fn automatic_batch_runs_one_eligible_phase_per_turn_and_completes_only_after_last_phase() {
        let mut batch = AutomaticSteepBatch::new(AutomaticTrigger::Idle, None);
        let expected = wenlan_core::refinery::Phase::ALL
            .iter()
            .copied()
            .filter(|phase| {
                wenlan_core::refinery::TriggerKind::Idle.runs_phase(*phase)
                    && automatic_phase_allowed(*phase)
            })
            .collect::<Vec<_>>();
        assert_eq!(batch.remaining_phases(), expected.as_slice());

        for (index, expected_phase) in expected.iter().copied().enumerate() {
            assert_eq!(batch.next_phase(), Some(expected_phase));
            let disposition = batch.complete_phase(
                expected_phase,
                AutomaticPhaseOutcome {
                    progressed: true,
                    ..AutomaticPhaseOutcome::default()
                },
            );
            if index + 1 == expected.len() {
                assert_eq!(disposition, AutomaticBatchDisposition::Complete);
            } else {
                assert_eq!(disposition, AutomaticBatchDisposition::Pending);
            }
        }
    }

    #[test]
    fn automatic_batch_contains_only_bounded_redistill() {
        for trigger in [AutomaticTrigger::Idle, AutomaticTrigger::Backstop] {
            let batch = AutomaticSteepBatch::new(trigger, None);
            assert_eq!(
                batch.remaining_phases(),
                vec![wenlan_core::refinery::Phase::ReDistill]
            );
        }

        assert!(AutomaticSteepBatch::new(AutomaticTrigger::Daily, None)
            .remaining_phases()
            .is_empty());
    }

    #[test]
    fn automatic_cursor_never_leaves_safe_allowlist() {
        let mut batch = AutomaticSteepBatch::new(
            AutomaticTrigger::Idle,
            Some(wenlan_core::refinery::Phase::ReDistill),
        );
        assert_eq!(
            batch.complete_phase(
                wenlan_core::refinery::Phase::ReDistill,
                AutomaticPhaseOutcome::default(),
            ),
            AutomaticBatchDisposition::Complete
        );
        assert_eq!(
            batch.cursor_after_attempt(wenlan_core::refinery::Phase::ReDistill),
            wenlan_core::refinery::Phase::ReDistill
        );
    }

    #[test]
    fn successful_more_phase_rotates_to_tail() {
        let mut batch = AutomaticSteepBatch::new(
            AutomaticTrigger::Idle,
            Some(wenlan_core::refinery::Phase::ReDistill),
        );
        assert_eq!(
            batch.next_phase(),
            Some(wenlan_core::refinery::Phase::ReDistill)
        );

        assert_eq!(
            batch.complete_phase(
                wenlan_core::refinery::Phase::ReDistill,
                AutomaticPhaseOutcome {
                    progressed: true,
                    more: true,
                    ..AutomaticPhaseOutcome::default()
                },
            ),
            AutomaticBatchDisposition::Pending
        );
        assert_eq!(
            batch.next_phase(),
            Some(wenlan_core::refinery::Phase::ReDistill),
            "the sole bounded phase waits for the next admitted thermal turn"
        );
        assert_eq!(
            batch.remaining_phases().last(),
            Some(&wenlan_core::refinery::Phase::ReDistill)
        );
    }

    #[test]
    fn retryable_or_paused_phase_is_not_requeued_in_current_trigger() {
        for outcome in [
            AutomaticPhaseOutcome {
                progressed: true,
                more: true,
                retryable: true,
                paused: false,
            },
            AutomaticPhaseOutcome {
                progressed: true,
                more: true,
                retryable: false,
                paused: true,
            },
        ] {
            let mut batch = AutomaticSteepBatch::new(
                AutomaticTrigger::Idle,
                Some(wenlan_core::refinery::Phase::ReDistill),
            );
            batch.complete_phase(wenlan_core::refinery::Phase::ReDistill, outcome);
            assert!(!batch
                .remaining_phases()
                .contains(&wenlan_core::refinery::Phase::ReDistill));
        }
    }

    #[test]
    fn maintenance_round_stays_pending_until_every_stage_attempted() {
        let mut round = AutomaticMaintenanceRound::new(None);
        assert_eq!(
            round.remaining_stages(),
            wenlan_core::maintenance::MaintenanceStage::ALL
        );

        for (index, stage) in wenlan_core::maintenance::MaintenanceStage::ALL
            .iter()
            .copied()
            .enumerate()
        {
            assert_eq!(round.next_stage(), Some(stage));
            let disposition = round.complete_stage(stage, AutomaticMaintenanceOutcome::default());
            if index + 1 == wenlan_core::maintenance::MaintenanceStage::ALL.len() {
                assert_eq!(disposition, AutomaticBatchDisposition::Complete);
            } else {
                assert_eq!(disposition, AutomaticBatchDisposition::Pending);
            }
        }
    }

    #[test]
    fn maintenance_round_cursor_rotates_a_paused_stage_behind_the_rest() {
        let mut round = AutomaticMaintenanceRound::new(Some(
            wenlan_core::maintenance::MaintenanceStage::RetroReview,
        ));
        assert_eq!(
            round.next_stage(),
            Some(wenlan_core::maintenance::MaintenanceStage::RetroReview)
        );
        round.complete_stage(
            wenlan_core::maintenance::MaintenanceStage::RetroReview,
            AutomaticMaintenanceOutcome {
                progressed: true,
                more: true,
                paused: true,
                retryable: false,
            },
        );
        assert_eq!(
            round.next_stage(),
            Some(wenlan_core::maintenance::MaintenanceStage::NearDuplicate),
            "paused/retryable work waits for a later maintenance round"
        );
    }

    #[test]
    fn maintenance_successful_more_stage_rotates_to_tail() {
        let stage = wenlan_core::maintenance::MaintenanceStage::NearDuplicate;
        let mut round = AutomaticMaintenanceRound::new(Some(stage));

        let disposition = round.complete_stage(
            stage,
            AutomaticMaintenanceOutcome {
                progressed: true,
                more: true,
                retryable: false,
                paused: false,
            },
        );

        assert_eq!(disposition, AutomaticBatchDisposition::Pending);
        assert_eq!(round.remaining_stages().last().copied(), Some(stage));
        assert_eq!(
            round.remaining_stages().len(),
            wenlan_core::maintenance::MaintenanceStage::ALL.len(),
            "bounded cursor work must stay in the same finite round until EOF"
        );
    }

    #[tokio::test]
    async fn automatic_phase_cursor_persists_after_retryable_attempt() {
        let (db, _db_dir) = new_test_db().await;
        let mut batch = AutomaticSteepBatch::new(
            AutomaticTrigger::Idle,
            Some(wenlan_core::refinery::Phase::ReDistill),
        );
        batch.complete_phase(
            wenlan_core::refinery::Phase::ReDistill,
            AutomaticPhaseOutcome {
                retryable: true,
                ..AutomaticPhaseOutcome::default()
            },
        );
        let cursor = batch.cursor_after_attempt(wenlan_core::refinery::Phase::ReDistill);
        persist_automatic_phase_cursor(&db, wenlan_core::refinery::TriggerKind::Idle, cursor).await;

        assert_eq!(
            load_automatic_phase_cursor(&db, wenlan_core::refinery::TriggerKind::Idle).await,
            Some(wenlan_core::refinery::Phase::ReDistill)
        );
    }

    #[tokio::test]
    async fn maintenance_stage_cursor_persists_after_attempt() {
        let (db, _db_dir) = new_test_db().await;
        let mut round = AutomaticMaintenanceRound::new(Some(
            wenlan_core::maintenance::MaintenanceStage::StalePage,
        ));
        round.complete_stage(
            wenlan_core::maintenance::MaintenanceStage::StalePage,
            AutomaticMaintenanceOutcome::default(),
        );
        let cursor =
            round.cursor_after_attempt(wenlan_core::maintenance::MaintenanceStage::StalePage);
        persist_automatic_maintenance_cursor(&db, cursor).await;

        assert_eq!(
            load_automatic_maintenance_cursor(&db).await,
            Some(wenlan_core::maintenance::MaintenanceStage::Overview)
        );
    }

    #[test]
    fn ambient_schedule_round_robins_all_due_jobs() {
        let now = Instant::now();
        let mut schedule = AmbientSchedule::new(now);
        assert!(
            schedule.last_entity.is_none(),
            "entity is due on first turn"
        );
        assert!(
            schedule.last_reconcile.is_none(),
            "reconcile is due on first turn"
        );
        assert!(
            schedule.last_citation.is_none(),
            "citation is due on first turn"
        );
        let available = AmbientAvailability {
            document: true,
            classification: true,
            structured_extract: true,
            entity: true,
            title: true,
            page_growth: true,
            reconcile: true,
            citation: true,
        };

        assert_eq!(
            schedule.select_due(now, available),
            Some(AmbientJob::Document)
        );
        assert_eq!(
            schedule.select_due(now, available),
            Some(AmbientJob::Classification)
        );
        assert_eq!(
            schedule.select_due(now, available),
            Some(AmbientJob::StructuredExtract)
        );
        assert_eq!(
            schedule.select_due(now, available),
            Some(AmbientJob::Entity)
        );
        assert_eq!(schedule.select_due(now, available), Some(AmbientJob::Title));
        assert_eq!(
            schedule.select_due(now, available),
            Some(AmbientJob::PageGrowth)
        );
        assert_eq!(
            schedule.select_due(now, available),
            Some(AmbientJob::Reconcile)
        );
        assert_eq!(
            schedule.select_due(now, available),
            Some(AmbientJob::Citation)
        );
    }

    #[test]
    fn selected_backlog_lane_stays_due_after_global_cooldown() {
        let now = Instant::now();
        let mut schedule = AmbientSchedule::new(now);
        let available = AmbientAvailability {
            document: true,
            classification: true,
            structured_extract: true,
            entity: true,
            title: true,
            page_growth: true,
            reconcile: true,
            citation: true,
        };

        assert_eq!(
            schedule.select_due(now, available),
            Some(AmbientJob::Document)
        );
        assert_eq!(
            schedule.select_due(now, available),
            Some(AmbientJob::Classification)
        );
        schedule.note_job_result(AmbientJob::Classification, now, true);
        assert_eq!(
            schedule.select_due(now, available),
            Some(AmbientJob::StructuredExtract)
        );
        assert_eq!(
            schedule.select_due(now, available),
            Some(AmbientJob::Entity)
        );
        assert_eq!(schedule.select_due(now, available), Some(AmbientJob::Title));
        assert_eq!(
            schedule.select_due(now, available),
            Some(AmbientJob::PageGrowth)
        );
        assert_eq!(
            schedule.select_due(now, available),
            Some(AmbientJob::Reconcile)
        );
        assert_eq!(
            schedule.select_due(now, available),
            Some(AmbientJob::Citation)
        );
        assert_eq!(
            schedule.select_due(now, available),
            Some(AmbientJob::Document)
        );
        assert_eq!(
            schedule.select_due(now, available),
            Some(AmbientJob::Classification),
            "known backlog should be paced by the global cooldown, not another 30-minute delay"
        );
    }

    #[test]
    fn attempted_inference_is_not_treated_as_an_empty_lane() {
        assert!(!should_backoff_ambient_lane(false, 1));
        assert!(should_backoff_ambient_lane(false, 0));
        assert!(!should_backoff_ambient_lane(true, 0));
    }

    #[test]
    fn page_growth_cpu_match_attempt_consumes_thermal_turn() {
        assert!(ambient_work_consumes_thermal_turn(
            AmbientJob::PageGrowth,
            true,
            0,
        ));
        assert!(!ambient_work_consumes_thermal_turn(
            AmbientJob::PageGrowth,
            false,
            0,
        ));
    }

    #[test]
    fn refresh_activity_observes_writes_that_arrive_during_a_poll() {
        let writes = WriteSignal::new();
        let base = Instant::now();
        let fresh = base + Duration::from_secs(5);
        let mut last_user_activity = base;
        writes.record_at("claude", fresh);

        refresh_last_user_activity(&writes, &mut last_user_activity);

        assert_eq!(last_user_activity, fresh);
    }

    #[test]
    fn ambient_turn_requires_quiet_window_and_cooldown() {
        let now = Instant::now();
        assert!(!ambient_turn_allowed(
            now,
            now - IDLE_THRESHOLD + Duration::from_secs(1),
            now - Duration::from_secs(1),
        ));
        assert!(!ambient_turn_allowed(
            now,
            now - IDLE_THRESHOLD,
            now + Duration::from_secs(1),
        ));
        assert!(ambient_turn_allowed(now, now - IDLE_THRESHOLD, now,));
    }

    #[test]
    fn pending_automatic_round_yields_one_admission_to_ambient_lane() {
        let now = Instant::now();
        let quiet_since = now - IDLE_THRESHOLD;
        assert!(automatic_heavy_turn_allowed(false, now, quiet_since, now));
        assert!(
            !automatic_heavy_turn_allowed(true, now, quiet_since, now),
            "an unfinished steep/maintenance round must not monopolize every admitted turn"
        );
    }

    #[test]
    fn unsupported_burst_end_does_not_preempt_bounded_idle_work() {
        let now = Instant::now();
        let mut snapshot = HashMap::new();
        snapshot.insert(
            "zeta".to_string(),
            vec![
                now - Duration::from_secs(1_000),
                now - Duration::from_secs(900),
                now - Duration::from_secs(800),
            ],
        );
        snapshot.insert(
            "alpha".to_string(),
            vec![
                now - Duration::from_secs(1_100),
                now - Duration::from_secs(1_000),
                now - Duration::from_secs(900),
            ],
        );

        let selected = select_due_automatic_trigger(
            now,
            &snapshot,
            MaintenanceAdmission::None,
            now - IDLE_THRESHOLD,
            false,
            now - DAILY_INTERVAL - Duration::from_secs(1),
            now - BACKSTOP_INTERVAL - Duration::from_secs(1),
        );

        assert_eq!(selected, Some(AutomaticTrigger::Idle));
    }

    #[test]
    fn unsupported_mature_burst_is_drained_without_a_thermal_turn() {
        let now = Instant::now();
        let writes = WriteSignal::new();
        for offset in [1_100, 1_000, 900] {
            writes.record_at("alpha", now - Duration::from_secs(offset));
        }

        assert_eq!(drain_expired_unactionable_bursts(&writes, now), 3);
        assert!(writes.snapshot().is_empty());
    }

    #[test]
    fn automatic_trigger_priority_leaves_later_due_work_for_future_turns() {
        let now = Instant::now();
        let snapshot = HashMap::new();

        assert_eq!(
            select_due_automatic_trigger(
                now,
                &snapshot,
                MaintenanceAdmission::None,
                now - IDLE_THRESHOLD,
                false,
                now - DAILY_INTERVAL - Duration::from_secs(1),
                now - BACKSTOP_INTERVAL - Duration::from_secs(1),
            ),
            Some(AutomaticTrigger::Idle)
        );
        assert_eq!(
            select_due_automatic_trigger(
                now,
                &snapshot,
                MaintenanceAdmission::None,
                now - IDLE_THRESHOLD,
                true,
                now - DAILY_INTERVAL - Duration::from_secs(1),
                now - BACKSTOP_INTERVAL - Duration::from_secs(1),
            ),
            Some(AutomaticTrigger::Backstop)
        );
    }

    #[test]
    fn pending_maintenance_yields_to_due_steep_after_one_stage() {
        let now = Instant::now();
        let mut snapshot = HashMap::new();
        snapshot.insert(
            "busy-agent".to_string(),
            vec![
                now - Duration::from_secs(1_100),
                now - Duration::from_secs(1_000),
                now - Duration::from_secs(900),
            ],
        );

        assert_eq!(
            select_due_automatic_trigger(
                now,
                &snapshot,
                MaintenanceAdmission::Ready,
                now - IDLE_THRESHOLD,
                false,
                now - DAILY_INTERVAL - Duration::from_secs(1),
                now - BACKSTOP_INTERVAL - Duration::from_secs(1),
            ),
            Some(AutomaticTrigger::Maintenance)
        );

        assert_eq!(
            select_due_automatic_trigger(
                now,
                &snapshot,
                MaintenanceAdmission::YieldToDueSteep,
                now - IDLE_THRESHOLD,
                false,
                now - DAILY_INTERVAL - Duration::from_secs(1),
                now - BACKSTOP_INTERVAL - Duration::from_secs(1),
            ),
            Some(AutomaticTrigger::Idle)
        );

        let no_bursts = HashMap::new();
        assert_eq!(
            select_due_automatic_trigger(
                now,
                &no_bursts,
                MaintenanceAdmission::YieldToDueSteep,
                now - IDLE_THRESHOLD,
                true,
                now - DAILY_INTERVAL - Duration::from_secs(1),
                now - BACKSTOP_INTERVAL - Duration::from_secs(1),
            ),
            Some(AutomaticTrigger::Backstop)
        );
        assert_eq!(
            select_due_automatic_trigger(
                now,
                &no_bursts,
                MaintenanceAdmission::YieldToDueSteep,
                now - IDLE_THRESHOLD,
                true,
                now,
                now - BACKSTOP_INTERVAL - Duration::from_secs(1),
            ),
            Some(AutomaticTrigger::Backstop)
        );
    }

    #[test]
    fn idle_and_backstop_enqueue_a_separate_maintenance_turn() {
        assert!(queues_maintenance_followup(&AutomaticTrigger::Idle));
        assert!(queues_maintenance_followup(&AutomaticTrigger::Backstop));
        assert!(!queues_maintenance_followup(&AutomaticTrigger::Daily));
        assert!(!queues_maintenance_followup(&AutomaticTrigger::Maintenance));
    }

    #[tokio::test]
    async fn scheduler_shutdown_interrupts_initial_delay() {
        let shared = Arc::new(tokio::sync::RwLock::new(
            crate::state::ServerState::default(),
        ));
        let writes = WriteSignal::new();
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let task = spawn_scheduler(shared, writes, shutdown_rx);

        shutdown_tx.send_replace(true);
        tokio::time::timeout(Duration::from_millis(250), task)
            .await
            .expect("shutdown must interrupt the scheduler's 60-second initial delay")
            .expect("scheduler task must exit cleanly");
    }

    #[test]
    fn ambient_thermal_work_completion_starts_conservative_cooldown() {
        let now = Instant::now();
        let mut schedule = AmbientSchedule::new(now);
        schedule.note_thermal_work_completion(now);
        assert_eq!(schedule.next_allowed_at, now + AMBIENT_COOLDOWN);
    }

    #[tokio::test]
    async fn ambient_provider_hard_caps_forwarded_inference_calls() {
        use wenlan_core::llm_provider::{LlmProvider, LlmRequest};

        let inner = Arc::new(MaintenanceTestProvider {
            body: "response".to_string(),
        });
        let provider = AmbientBudgetProvider::new(inner.clone());
        let request = || LlmRequest {
            system_prompt: None,
            user_prompt: "test".to_string(),
            max_tokens: 8,
            temperature: 0.0,
            label: Some("ambient_budget_test".to_string()),
            timeout_secs: None,
        };

        assert!(provider.generate(request()).await.is_ok());
        assert!(
            provider.generate(request()).await.is_err(),
            "a second inference in one ambient slice must fail closed"
        );
        assert_eq!(provider.call_count(), 1, "telemetry counts forwarded calls");
    }

    #[tokio::test]
    async fn automatic_provider_roles_share_one_poll_inference_budget() {
        use std::sync::atomic::AtomicUsize;
        use wenlan_core::llm_provider::{LlmProvider, LlmRequest};

        let calls = Arc::new(AtomicUsize::new(0));
        let inner = Arc::new(MaintenanceTestProvider {
            body: "response".to_string(),
        });
        let local = AmbientBudgetProvider::with_shared_calls(inner.clone(), calls.clone());
        let synthesis = AmbientBudgetProvider::with_shared_calls(inner.clone(), calls);
        let request = || LlmRequest {
            system_prompt: None,
            user_prompt: "test".to_string(),
            max_tokens: 8,
            temperature: 0.0,
            label: Some("automatic_budget_test".to_string()),
            timeout_secs: None,
        };

        assert!(local.generate(request()).await.is_ok());
        assert!(
            synthesis.generate(request()).await.is_err(),
            "provider roles in one automatic turn must share one inference cap"
        );
        assert_eq!(local.call_count(), 1);
        assert_eq!(synthesis.call_count(), 1);
    }

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
    async fn directory_sync_and_document_slice_are_separate_steps() {
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

        sync_directory_sources(&db).await;
        let queued = db
            .get_queue_entry(&source_id, &file_path.to_string_lossy())
            .await
            .unwrap()
            .expect("sync enqueues the file");
        assert_eq!(queued.status, "pending");

        let processed = run_document_enrichment_slice_tick(&db, None, &prompts).await;
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
            .search_memory(
                "Wenlanborg",
                30,
                None,
                &wenlan_core::read_scope::ReadScope::Global,
                None,
                None,
                None,
                None,
            )
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

        sync_directory_sources(&db).await;
        let processed = run_document_enrichment_slice_tick(&db, None, &prompts).await;
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

    #[tokio::test]
    async fn document_provider_panic_pauses_claimed_generation() {
        let _lock = crate::TEST_DATA_DIR_LOCK
            .get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await;
        let _env = DataDirGuard::new();

        let source_root = tempfile::tempdir().unwrap();
        let file_path = source_root.path().join("panic-note.txt");
        std::fs::write(
            &file_path,
            "Wenlan should preserve and retry a claimed document after a provider panic.",
        )
        .unwrap();
        let source_id = "directory-panic";
        register_directory_source(source_id, source_root.path());

        let (db, _db_dir) = new_test_db().await;
        sync_directory_sources(&db).await;
        let before = db
            .get_queue_entry(source_id, &file_path.to_string_lossy())
            .await
            .unwrap()
            .expect("directory sync enqueues the document");
        assert_eq!(before.status, "pending");

        let panicking: Arc<dyn wenlan_core::llm_provider::LlmProvider> =
            Arc::new(PanicTestProvider);
        let report = run_ambient_job_safe(
            AmbientJob::Document,
            &db,
            Some(&panicking),
            None,
            None,
            Some(wenlan_core::refinery::EverydaySource::OnDevice),
            &wenlan_core::prompts::PromptRegistry::default(),
            &wenlan_core::tuning::RefineryConfig::default(),
            &wenlan_core::tuning::DistillationConfig::default(),
            None,
        )
        .await;

        assert!(
            report.panicked,
            "panic remains visible to scheduler accounting"
        );
        let after = db
            .get_queue_entry(source_id, &file_path.to_string_lossy())
            .await
            .unwrap()
            .expect("claimed generation remains queued");
        assert_eq!(after.status, "paused");
        assert_eq!(after.attempt_count, 1);
        assert!(after.next_retry_at.is_some());
        assert!(after
            .error_detail
            .as_deref()
            .is_some_and(|reason| reason.contains("panicked")));
        assert_eq!(after.last_completed_chunk, before.last_completed_chunk);
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

    struct AvailabilityTestProvider {
        name: &'static str,
        available: bool,
    }

    #[async_trait::async_trait]
    impl wenlan_core::llm_provider::LlmProvider for AvailabilityTestProvider {
        async fn generate(
            &self,
            _request: wenlan_core::llm_provider::LlmRequest,
        ) -> Result<String, wenlan_core::llm_provider::LlmError> {
            Ok(self.name.to_string())
        }

        fn is_available(&self) -> bool {
            self.available
        }

        fn name(&self) -> &str {
            self.name
        }

        fn backend(&self) -> wenlan_core::llm_provider::LlmBackend {
            wenlan_core::llm_provider::LlmBackend::Api
        }

        fn kind(&self) -> &'static str {
            "mock"
        }
    }

    struct PanicTestProvider;

    #[async_trait::async_trait]
    impl wenlan_core::llm_provider::LlmProvider for PanicTestProvider {
        async fn generate(
            &self,
            _request: wenlan_core::llm_provider::LlmRequest,
        ) -> Result<String, wenlan_core::llm_provider::LlmError> {
            panic!("ambient provider panic")
        }

        fn is_available(&self) -> bool {
            true
        }

        fn name(&self) -> &str {
            "panic-test"
        }

        fn backend(&self) -> wenlan_core::llm_provider::LlmBackend {
            wenlan_core::llm_provider::LlmBackend::Api
        }

        fn kind(&self) -> &'static str {
            "mock"
        }
    }

    #[test]
    fn ambient_provider_selection_honors_explicit_on_device_pin() {
        let api: Arc<dyn wenlan_core::llm_provider::LlmProvider> =
            Arc::new(AvailabilityTestProvider {
                name: "available-api",
                available: true,
            });
        let local: Arc<dyn wenlan_core::llm_provider::LlmProvider> =
            Arc::new(AvailabilityTestProvider {
                name: "available-local",
                available: true,
            });

        let selected = resolve_ambient_provider(
            Some(wenlan_core::refinery::EverydaySource::OnDevice),
            Some(&api),
            None,
            Some(&local),
        )
        .expect("the on-device pin should select the exact approved source");

        assert_eq!(selected.name(), "available-local");
    }

    #[test]
    fn maintenance_provider_selection_requires_explicit_pin() {
        let api: Arc<dyn wenlan_core::llm_provider::LlmProvider> =
            Arc::new(AvailabilityTestProvider {
                name: "available-api",
                available: true,
            });
        let external: Arc<dyn wenlan_core::llm_provider::LlmProvider> =
            Arc::new(AvailabilityTestProvider {
                name: "available-external",
                available: true,
            });

        assert!(
            resolve_maintenance_provider(None, None, Some(&api), Some(&external), None).is_none()
        );
    }

    #[test]
    fn maintenance_provider_selection_does_not_fallback_from_missing_pin() {
        let api: Arc<dyn wenlan_core::llm_provider::LlmProvider> =
            Arc::new(AvailabilityTestProvider {
                name: "available-api",
                available: true,
            });

        assert!(resolve_maintenance_provider(
            Some(wenlan_core::refinery::SynthesisSource::External),
            None,
            Some(&api),
            None,
            None,
        )
        .is_none());
    }

    #[test]
    fn maintenance_provider_selection_honors_exact_external_pin() {
        let api: Arc<dyn wenlan_core::llm_provider::LlmProvider> =
            Arc::new(AvailabilityTestProvider {
                name: "available-api",
                available: true,
            });
        let external: Arc<dyn wenlan_core::llm_provider::LlmProvider> =
            Arc::new(AvailabilityTestProvider {
                name: "available-external",
                available: true,
            });

        let selected = resolve_maintenance_provider(
            Some(wenlan_core::refinery::SynthesisSource::External),
            None,
            Some(&api),
            Some(&external),
            None,
        )
        .expect("the explicit external pin should select the external slot");

        assert_eq!(selected.name(), "available-external");
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

    #[tokio::test]
    async fn ambient_provider_panic_isolated_and_next_turn_still_runs() {
        let (db, _db_dir) = new_test_db().await;
        store_test_memory(
            &db,
            "ambient-panic-recovery",
            "The launch decision belongs to the work project.",
        )
        .await;
        db.upsert_enrichment_origin(
            "ambient-panic-recovery",
            wenlan_core::db::EnrichmentOrigin {
                memory_type_explicit: false,
                structured_fields_explicit: false,
                space_rejected: false,
            },
        )
        .await
        .unwrap();

        let panicking: Arc<dyn wenlan_core::llm_provider::LlmProvider> =
            Arc::new(PanicTestProvider);
        let first = run_ambient_job_safe(
            AmbientJob::Classification,
            &db,
            Some(&panicking),
            None,
            None,
            Some(wenlan_core::refinery::EverydaySource::OnDevice),
            &wenlan_core::prompts::PromptRegistry::default(),
            &wenlan_core::tuning::RefineryConfig::default(),
            &wenlan_core::tuning::DistillationConfig::default(),
            None,
        )
        .await;
        assert!(
            first.panicked,
            "the panic is surfaced to scheduler accounting"
        );
        assert!(
            first.selected,
            "a panicked lane stays eligible after the thermal cooldown"
        );

        let healthy: Arc<dyn wenlan_core::llm_provider::LlmProvider> =
            Arc::new(MaintenanceTestProvider {
                body: r#"{"memory_type":"decision","domain":null,"quality":"high","importance":8,"tags":["launch"]}"#
                    .to_string(),
            });
        let second = run_ambient_job_safe(
            AmbientJob::Classification,
            &db,
            Some(&healthy),
            None,
            None,
            Some(wenlan_core::refinery::EverydaySource::OnDevice),
            &wenlan_core::prompts::PromptRegistry::default(),
            &wenlan_core::tuning::RefineryConfig::default(),
            &wenlan_core::tuning::DistillationConfig::default(),
            None,
        )
        .await;

        assert!(!second.panicked);
        assert!(second.selected);
        assert_eq!(second.llm_calls, 1);
        assert!(db
            .get_enrichment_steps("ambient-panic-recovery")
            .await
            .unwrap()
            .iter()
            .any(|step| step.step == "classify" && step.status == "ok"));
    }

    #[tokio::test]
    async fn ambient_classification_turn_forwards_once_and_commits_receipt() {
        let (db, _db_dir) = new_test_db().await;
        store_test_memory(
            &db,
            "ambient-classification",
            "The launch decision belongs to the work project.",
        )
        .await;
        db.upsert_enrichment_origin(
            "ambient-classification",
            wenlan_core::db::EnrichmentOrigin {
                memory_type_explicit: false,
                structured_fields_explicit: false,
                space_rejected: false,
            },
        )
        .await
        .unwrap();
        let provider: Arc<dyn wenlan_core::llm_provider::LlmProvider> =
            Arc::new(MaintenanceTestProvider {
                body: r#"{"memory_type":"decision","domain":null,"quality":"high","importance":8,"tags":["launch"]}"#
                    .to_string(),
            });

        let report = run_ambient_job(
            AmbientJob::Classification,
            &db,
            Some(&provider),
            None,
            None,
            Some(wenlan_core::refinery::EverydaySource::OnDevice),
            &wenlan_core::prompts::PromptRegistry::default(),
            &wenlan_core::tuning::RefineryConfig::default(),
            &wenlan_core::tuning::DistillationConfig::default(),
            None,
        )
        .await;

        assert!(report.selected);
        assert_eq!(report.llm_calls, 1);
        let steps = db
            .get_enrichment_steps("ambient-classification")
            .await
            .unwrap();
        let classify = steps
            .iter()
            .find(|step| step.step == "classify")
            .expect("classification receipt");
        assert_eq!(classify.status, "ok");
        assert_eq!(classify.input_version, Some(1));
    }

    #[tokio::test]
    async fn ambient_pending_memory_forwards_zero_classification_calls() {
        let (db, _db_dir) = new_test_db().await;
        let mut pending = wenlan_types::RawDocument {
            source: "memory".to_string(),
            source_id: "ambient-pending-classification".to_string(),
            title: "Pending revision".to_string(),
            content: "This revision must not be enriched before approval.".to_string(),
            last_modified: chrono::Utc::now().timestamp(),
            memory_type: Some("fact".to_string()),
            source_agent: Some("test".to_string()),
            confirmed: Some(true),
            ..Default::default()
        };
        pending.pending_revision = true;
        db.upsert_documents(vec![pending]).await.unwrap();
        let provider: Arc<dyn wenlan_core::llm_provider::LlmProvider> =
            Arc::new(MaintenanceTestProvider {
                body: "must not be called".to_string(),
            });

        let report = run_ambient_job(
            AmbientJob::Classification,
            &db,
            Some(&provider),
            None,
            None,
            Some(wenlan_core::refinery::EverydaySource::OnDevice),
            &wenlan_core::prompts::PromptRegistry::default(),
            &wenlan_core::tuning::RefineryConfig::default(),
            &wenlan_core::tuning::DistillationConfig::default(),
            None,
        )
        .await;

        assert!(!report.selected);
        assert_eq!(report.llm_calls, 0);
        assert!(db
            .get_enrichment_steps("ambient-pending-classification")
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn ambient_page_growth_no_match_forwards_zero_calls_and_commits_receipt() {
        let (db, _db_dir) = new_test_db().await;
        store_test_memory(
            &db,
            "ambient-growth-no-match",
            "A standalone memory with no matching Page.",
        )
        .await;
        assert!(db
            .record_enrichment_step_at_version(
                "ambient-growth-no-match",
                "entity_extract",
                "ok",
                None,
                1,
            )
            .await
            .unwrap());
        let provider: Arc<dyn wenlan_core::llm_provider::LlmProvider> =
            Arc::new(MaintenanceTestProvider {
                body: "must not be called".to_string(),
            });

        let report = run_ambient_job(
            AmbientJob::PageGrowth,
            &db,
            Some(&provider),
            None,
            None,
            Some(wenlan_core::refinery::EverydaySource::OnDevice),
            &wenlan_core::prompts::PromptRegistry::default(),
            &wenlan_core::tuning::RefineryConfig::default(),
            &wenlan_core::tuning::DistillationConfig::default(),
            None,
        )
        .await;

        assert!(report.selected);
        assert_eq!(report.llm_calls, 0);
        let growth = db
            .get_enrichment_steps("ambient-growth-no-match")
            .await
            .unwrap()
            .into_iter()
            .find(|step| step.step == "page_growth")
            .expect("terminal no-match receipt");
        assert_eq!(growth.status, "ok");
        assert_eq!(growth.input_version, Some(1));
    }

    #[tokio::test]
    async fn ambient_reconcile_backpressure_uses_lane_rescan_backoff() {
        let (db, _db_dir) = new_test_db().await;
        let mut pending = Vec::new();
        for i in 0..=wenlan_core::reconcile::RECONCILE_PENDING_CAP {
            pending.push(wenlan_types::RawDocument {
                source: "memory".to_string(),
                source_id: format!("ambient-reconcile-pending-{i}"),
                title: "Pending reconcile revision".to_string(),
                content: "A pending revision awaiting human review.".to_string(),
                last_modified: chrono::Utc::now().timestamp(),
                source_agent: Some("reconcile".to_string()),
                confirmed: None,
                pending_revision: true,
                supersedes: Some(format!("ambient-reconcile-target-{i}")),
                ..Default::default()
            });
        }
        db.upsert_documents(pending).await.unwrap();
        let provider: Arc<dyn wenlan_core::llm_provider::LlmProvider> =
            Arc::new(MaintenanceTestProvider {
                body: "must not be called".to_string(),
            });

        let report = run_ambient_job(
            AmbientJob::Reconcile,
            &db,
            Some(&provider),
            None,
            None,
            Some(wenlan_core::refinery::EverydaySource::OnDevice),
            &wenlan_core::prompts::PromptRegistry::default(),
            &wenlan_core::tuning::RefineryConfig::default(),
            &wenlan_core::tuning::DistillationConfig::default(),
            None,
        )
        .await;

        assert_eq!(report.llm_calls, 0);
        assert!(
            !report.selected,
            "administrative backpressure is no work and must receive lane backoff"
        );

        let now = Instant::now();
        let mut schedule = AmbientSchedule::new(now);
        schedule.note_job_result(
            report.job,
            now,
            !should_backoff_ambient_lane(report.selected, report.llm_calls),
        );
        assert_eq!(schedule.last_reconcile, Some(now));
        let reconcile_only = AmbientAvailability {
            document: false,
            classification: false,
            structured_extract: false,
            entity: false,
            title: false,
            page_growth: false,
            reconcile: true,
            citation: false,
        };
        assert_eq!(
            schedule.select_due(
                now + RECONCILE_SWEEP_INTERVAL - Duration::from_secs(1),
                reconcile_only
            ),
            None
        );
        assert_eq!(
            schedule.select_due(now + RECONCILE_SWEEP_INTERVAL, reconcile_only),
            Some(AmbientJob::Reconcile)
        );
    }

    #[tokio::test]
    async fn ambient_reconcile_zero_candidate_progress_stays_due_but_is_thermally_paced() {
        let (db, _db_dir) = new_test_db().await;
        db.upsert_documents(vec![wenlan_types::RawDocument {
            source: "memory".to_string(),
            source_id: "ambient-reconcile-doc-only".to_string(),
            title: "Document-only frontier item".to_string(),
            content: "A folder document with no capture candidate still advances the frontier."
                .to_string(),
            last_modified: chrono::Utc::now().timestamp(),
            source_agent: Some("folder".to_string()),
            confirmed: Some(true),
            content_hash: Some("ambient-reconcile-doc-hash".to_string()),
            ..Default::default()
        }])
        .await
        .unwrap();
        let provider: Arc<dyn wenlan_core::llm_provider::LlmProvider> =
            Arc::new(MaintenanceTestProvider {
                body: "must not be called".to_string(),
            });

        let report = run_ambient_job(
            AmbientJob::Reconcile,
            &db,
            Some(&provider),
            None,
            None,
            Some(wenlan_core::refinery::EverydaySource::OnDevice),
            &wenlan_core::prompts::PromptRegistry::default(),
            &wenlan_core::tuning::RefineryConfig::default(),
            &wenlan_core::tuning::DistillationConfig::default(),
            None,
        )
        .await;

        assert!(
            db.get_app_metadata("reconcile_frontier_docs")
                .await
                .unwrap()
                .is_some(),
            "the zero-candidate item advances the durable frontier"
        );
        assert_eq!(report.llm_calls, 0);
        assert!(
            report.selected,
            "durable frontier progress is real work even without a judge call"
        );

        let now = Instant::now();
        let mut schedule = AmbientSchedule::new(now);
        schedule.note_job_result(
            report.job,
            now,
            !should_backoff_ambient_lane(report.selected, report.llm_calls),
        );
        assert_eq!(
            schedule.last_reconcile, None,
            "known backlog must not receive the 30-minute empty-lane backoff"
        );
        assert!(ambient_work_consumes_thermal_turn(
            report.job,
            report.selected,
            report.llm_calls,
        ));
        schedule.note_thermal_work_completion(now);
        assert_eq!(schedule.next_allowed_at, now + AMBIENT_COOLDOWN);
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
    async fn maintenance_provider_panic_isolated_and_scheduler_state_survives() {
        struct AvailabilityPanicProvider;

        #[async_trait::async_trait]
        impl wenlan_core::llm_provider::LlmProvider for AvailabilityPanicProvider {
            async fn generate(
                &self,
                _request: wenlan_core::llm_provider::LlmRequest,
            ) -> Result<String, wenlan_core::llm_provider::LlmError> {
                unreachable!("availability check must run first")
            }

            fn is_available(&self) -> bool {
                panic!("maintenance availability panic")
            }

            fn name(&self) -> &str {
                "availability-panic-test"
            }

            fn backend(&self) -> wenlan_core::llm_provider::LlmBackend {
                wenlan_core::llm_provider::LlmBackend::Api
            }
        }

        let (db, _db_dir) = new_test_db().await;
        store_test_memory(
            &db,
            "maintenance-panic-source",
            "A source update that requires a machine-page refresh.",
        )
        .await;
        let page_id = insert_test_page(
            &db,
            "Maintenance panic page",
            "Old machine-owned prose.",
            &["maintenance-panic-source"],
            "research",
        )
        .await;
        db.set_page_stale(&page_id, "source_updated").await.unwrap();
        let provider: Arc<dyn wenlan_core::llm_provider::LlmProvider> =
            Arc::new(AvailabilityPanicProvider);
        let selected = resolve_maintenance_provider(
            Some(wenlan_core::refinery::SynthesisSource::External),
            None,
            None,
            Some(&provider),
            None,
        );

        fire_maintenance_stage_safe(
            db.as_ref(),
            selected.as_ref(),
            &wenlan_core::prompts::PromptRegistry::default(),
            &wenlan_core::tuning::DistillationConfig::default(),
            None,
            wenlan_core::maintenance::MaintenanceStage::StalePage,
            "panic-test",
        )
        .await;

        assert!(db.get_page(&page_id).await.unwrap().is_some());
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
