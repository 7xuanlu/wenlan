// SPDX-License-Identifier: Apache-2.0
//! Bounded, event-driven priority lane for freshly imported memories.
//!
//! An import should become useful while the user is still looking at it.  This
//! module only owns the small in-memory state that gives the existing ambient
//! selectors a short head start.  It deliberately does not create another
//! worker, queue, or database table.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::sync::{watch, Notify};

use super::{
    AmbientJob, HostActivitySnapshot, ResourceBlockReason, ResourcePolicy, ResourceSnapshot,
    ResourceStatus, WriteSignal, ON_DEVICE_INFERENCE_HEADROOM_BYTES,
};

pub(super) const IMPORT_PRIORITY_METADATA_KEY: &str = "import_priority_until_v1";
pub(super) const IMPORT_PRIORITY_WINDOW: Duration = Duration::from_secs(10 * 60);
pub(super) const IMPORT_PRIORITY_MAX_AMBIENT_SLICES: u8 = 64;
pub(super) const IMPORT_PRIORITY_TICK: Duration = Duration::from_secs(2);

const IMPORT_PRIORITY_JOBS: [AmbientJob; 5] = [
    AmbientJob::Classification,
    AmbientJob::StructuredExtract,
    AmbientJob::Entity,
    AmbientJob::Title,
    AmbientJob::PageGrowth,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ImportPriorityRequest {
    pub(super) deadline_epoch: i64,
    pub(super) generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ImportSynthesisPhase {
    Detect,
    Emergence,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ImportPrioritySnapshot {
    pub(super) deadline: Instant,
    pub(super) deadline_epoch: i64,
    pub(super) generation: u64,
    pub(super) ambient_slices: u8,
    pub(super) next_job: AmbientJob,
    pub(super) synthesis_phase: Option<ImportSynthesisPhase>,
}

/// State is bounded by both a wall-clock deadline and an ambient-slice count.
/// The scheduler may be restarted at any point; only the wall-clock deadline
/// is persisted, so a restart never revives an old import forever.
#[derive(Debug, Default)]
pub(super) struct ImportPriority {
    deadline: Option<Instant>,
    deadline_epoch: i64,
    generation: u64,
    ambient_slices: u8,
    next_job: usize,
    round_attempts: u8,
    round_had_work: bool,
    synthesis_phase: Option<ImportSynthesisPhase>,
    started_logged: bool,
    synthesis_finished: bool,
}

impl ImportPriority {
    fn deadline_epoch_now() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or(0)
            .saturating_add(IMPORT_PRIORITY_WINDOW.as_secs() as i64)
    }

    fn activate(&mut self, deadline_epoch: i64, now: Instant, now_epoch: i64) {
        let remaining = deadline_epoch.saturating_sub(now_epoch);
        if remaining <= 0 {
            self.deadline = None;
            self.deadline_epoch = 0;
            return;
        }
        // Be defensive about hand-edited/corrupt metadata. A persisted import
        // can never extend a fresh scheduler window beyond ten minutes.
        let bounded_remaining = remaining.min(IMPORT_PRIORITY_WINDOW.as_secs() as i64);
        self.deadline = Some(now + Duration::from_secs(bounded_remaining as u64));
        self.deadline_epoch = now_epoch.saturating_add(bounded_remaining);
        self.ambient_slices = 0;
        self.next_job = 0;
        self.round_attempts = 0;
        self.round_had_work = false;
        self.synthesis_phase = None;
        self.started_logged = false;
        self.synthesis_finished = false;
    }

    pub(super) fn prepare_request(&mut self, now: Instant) -> ImportPriorityRequest {
        self.generation = self.generation.wrapping_add(1);
        let now_epoch = chrono::Utc::now().timestamp();
        let deadline_epoch = Self::deadline_epoch_now();
        self.activate(deadline_epoch, now, now_epoch);
        ImportPriorityRequest {
            deadline_epoch,
            generation: self.generation,
        }
    }

    pub(super) fn restore(&mut self, deadline_epoch: i64, now: Instant, now_epoch: i64) -> bool {
        if self.deadline.is_some() {
            return true;
        }
        if deadline_epoch <= now_epoch {
            return false;
        }
        self.generation = self.generation.wrapping_add(1);
        self.activate(deadline_epoch, now, now_epoch);
        self.deadline.is_some()
    }

    pub(super) fn snapshot(&self) -> Option<ImportPrioritySnapshot> {
        let deadline = self.deadline?;
        Some(ImportPrioritySnapshot {
            deadline,
            deadline_epoch: self.deadline_epoch,
            generation: self.generation,
            ambient_slices: self.ambient_slices,
            next_job: IMPORT_PRIORITY_JOBS[self.next_job],
            synthesis_phase: self.synthesis_phase,
        })
    }

    pub(super) fn mark_started(&mut self) -> bool {
        if self.started_logged {
            false
        } else {
            self.started_logged = true;
            true
        }
    }

    pub(super) fn note_ambient(&mut self, generation: u64, selected: bool) -> bool {
        if self.generation != generation {
            return false;
        }
        self.ambient_slices = self
            .ambient_slices
            .saturating_add(1)
            .min(IMPORT_PRIORITY_MAX_AMBIENT_SLICES);
        self.round_attempts = self.round_attempts.saturating_add(1);
        self.round_had_work |= selected;
        self.next_job = (self.next_job + 1) % IMPORT_PRIORITY_JOBS.len();
        if self.round_attempts == IMPORT_PRIORITY_JOBS.len() as u8 {
            if !self.round_had_work {
                self.synthesis_phase = Some(ImportSynthesisPhase::Detect);
            }
            self.round_attempts = 0;
            self.round_had_work = false;
        }
        if self.ambient_slices >= IMPORT_PRIORITY_MAX_AMBIENT_SLICES {
            self.synthesis_phase = Some(ImportSynthesisPhase::Detect);
        }
        true
    }

    pub(super) fn note_synthesis_phase(
        &mut self,
        generation: u64,
        phase: ImportSynthesisPhase,
    ) -> bool {
        if self.generation != generation {
            return false;
        }
        self.synthesis_phase = match phase {
            ImportSynthesisPhase::Detect => Some(ImportSynthesisPhase::Emergence),
            ImportSynthesisPhase::Emergence => None,
        };
        self.synthesis_finished = phase == ImportSynthesisPhase::Emergence;
        true
    }

    pub(super) fn should_finish(&self, now: Instant) -> bool {
        self.deadline.is_none_or(|deadline| now >= deadline) || self.synthesis_finished
    }

    pub(super) fn generation(&self) -> u64 {
        self.generation
    }

    pub(super) fn clear_if_generation(&mut self, generation: u64) -> bool {
        if self.generation != generation {
            return false;
        }
        self.deadline = None;
        self.deadline_epoch = 0;
        self.synthesis_phase = None;
        self.started_logged = false;
        self.synthesis_finished = false;
        true
    }

    pub(super) fn active(&self) -> bool {
        self.deadline
            .is_some_and(|deadline| Instant::now() < deadline)
    }
}

impl WriteSignal {
    /// Return a request token and activate the bounded state without waking
    /// the scheduler. Routes persist the token's deadline first, then call
    /// `signal_import_request`, which closes the persistence race with a
    /// concurrent completion clear.
    pub(super) fn prepare_import_request(&self) -> ImportPriorityRequest {
        self.import_priority
            .lock()
            .expect("import priority mutex poisoned")
            .prepare_request(Instant::now())
    }

    /// Activate and wake an already-persisted import request. A stale token
    /// cannot wake a newer generation, but a notification itself is harmless
    /// because `Notify` coalesces wakeups.
    pub(super) fn signal_import_request(&self, request: ImportPriorityRequest) {
        if self
            .import_priority
            .lock()
            .expect("import priority mutex poisoned")
            .generation()
            == request.generation
        {
            // Both startup admission and the scheduler may be waiting. Wake
            // all current waiters and retain one permit for a registration race.
            self.import_wake.notify_waiters();
            self.import_wake.notify_one();
        }
    }

    /// Convenience for non-route callers that do not need to persist a
    /// deadline. Import routes use the prepare/persist/signal sequence above.
    #[cfg(test)]
    pub(super) fn request_import(&self) -> ImportPriorityRequest {
        let request = self.prepare_import_request();
        self.signal_import_request(request);
        request
    }

    /// Persist and publish one import request under the same async mutex used
    /// by completion. This makes the deadline write and the wake one atomic
    /// handoff with respect to a scheduler completion: a newer request can
    /// never have its durable deadline erased by an older clear.
    pub(crate) async fn persist_and_request_import(
        &self,
        db: &wenlan_core::db::MemoryDB,
        batch_id: Option<&str>,
    ) -> Result<(), wenlan_core::WenlanError> {
        let _persist_guard = self.import_persist_lock.lock().await;
        let request = self.prepare_import_request();
        db.set_app_metadata(
            IMPORT_PRIORITY_METADATA_KEY,
            &request.deadline_epoch.to_string(),
        )
        .await?;
        if let Some(batch_id) = batch_id {
            db.set_app_metadata(
                &format!("import_batch_priority_v1:{batch_id}"),
                &request.deadline_epoch.to_string(),
            )
            .await?;
        }
        self.signal_import_request(request);
        Ok(())
    }

    pub(super) fn restore_import_priority(&self, deadline_epoch: i64) -> bool {
        self.import_priority
            .lock()
            .expect("import priority mutex poisoned")
            .restore(
                deadline_epoch,
                Instant::now(),
                chrono::Utc::now().timestamp(),
            )
    }

    pub(super) fn import_priority_snapshot(&self) -> Option<ImportPrioritySnapshot> {
        self.import_priority
            .lock()
            .expect("import priority mutex poisoned")
            .snapshot()
    }

    pub(super) fn import_priority_mark_started(&self) -> bool {
        self.import_priority
            .lock()
            .expect("import priority mutex poisoned")
            .mark_started()
    }

    pub(super) fn import_priority_note_ambient(&self, generation: u64, selected: bool) -> bool {
        self.import_priority
            .lock()
            .expect("import priority mutex poisoned")
            .note_ambient(generation, selected)
    }

    pub(super) fn import_priority_note_phase(
        &self,
        generation: u64,
        phase: ImportSynthesisPhase,
    ) -> bool {
        self.import_priority
            .lock()
            .expect("import priority mutex poisoned")
            .note_synthesis_phase(generation, phase)
    }

    pub(super) fn import_priority_should_finish(&self, now: Instant) -> bool {
        self.import_priority
            .lock()
            .expect("import priority mutex poisoned")
            .should_finish(now)
    }

    pub(super) fn clear_import_priority_if_generation(&self, generation: u64) -> bool {
        self.import_priority
            .lock()
            .expect("import priority mutex poisoned")
            .clear_if_generation(generation)
    }

    pub(super) async fn finish_import_priority(
        &self,
        db: &wenlan_core::db::MemoryDB,
        generation: u64,
    ) -> Result<bool, wenlan_core::WenlanError> {
        let _persist_guard = self.import_persist_lock.lock().await;
        if self
            .import_priority
            .lock()
            .expect("import priority mutex poisoned")
            .generation()
            != generation
        {
            return Ok(false);
        }
        db.set_app_metadata(IMPORT_PRIORITY_METADATA_KEY, "0")
            .await?;
        Ok(self.clear_import_priority_if_generation(generation))
    }

    pub(super) fn import_priority_active(&self) -> bool {
        self.import_priority
            .lock()
            .expect("import priority mutex poisoned")
            .active()
    }

    pub(super) fn import_wake(&self) -> std::sync::Arc<Notify> {
        self.import_wake.clone()
    }
}

/// The import wake is a coalesced event, so a fresh import does not wait for
/// either the startup delay or the ordinary 30-second poll interval.
pub(super) async fn sleep_or_import_or_shutdown(
    shutdown: &mut watch::Receiver<bool>,
    import_wake: &Notify,
    duration: Duration,
) -> bool {
    if crate::lifecycle::shutdown_requested(shutdown) {
        return true;
    }
    let notified = import_wake.notified();
    tokio::select! {
        _ = tokio::time::sleep(duration) => false,
        _ = notified => false,
        result = shutdown.changed() => result.is_err() || crate::lifecycle::shutdown_requested(shutdown),
    }
}

/// Import priority ignores foreground activity and the CPU threshold, but
/// continues to honour memory reserve, model headroom, thermal pressure,
/// unavailable host signals, and an in-flight startup model reservation.
pub(super) fn import_priority_block_reason(
    status: ResourceStatus,
    host_activity: HostActivitySnapshot,
    startup_model_load_reserved: bool,
    route_uses_on_device: bool,
) -> Option<ResourceBlockReason> {
    import_priority_block_reason_with_headroom(
        status,
        host_activity,
        startup_model_load_reserved,
        if route_uses_on_device {
            ON_DEVICE_INFERENCE_HEADROOM_BYTES
        } else {
            0
        },
    )
}

/// Variant used while admitting the startup model itself. Its working set is
/// the extra memory reserve, rather than the regular ambient two-gibibyte
/// headroom, and the startup load is the operation being admitted so it must
/// not veto itself as an already-reserved route.
pub(super) fn import_priority_block_reason_with_headroom(
    status: ResourceStatus,
    host_activity: HostActivitySnapshot,
    startup_model_load_reserved: bool,
    additional_memory_headroom_bytes: u64,
) -> Option<ResourceBlockReason> {
    let snapshot: ResourceSnapshot = if let Some(snapshot) = status.snapshot {
        snapshot
    } else {
        return Some(
            status
                .block_reason
                .filter(|reason| {
                    matches!(
                        reason,
                        ResourceBlockReason::Warming | ResourceBlockReason::Unavailable
                    )
                })
                .unwrap_or(ResourceBlockReason::Unavailable),
        );
    };
    let memory_policy = ResourcePolicy::conservative()
        .with_additional_memory_headroom(additional_memory_headroom_bytes);
    if let Some(reason) = memory_policy.block_reason(snapshot) {
        if !matches!(reason, ResourceBlockReason::CpuBusy) {
            return Some(reason);
        }
    }
    if startup_model_load_reserved && additional_memory_headroom_bytes > 0 {
        return Some(ResourceBlockReason::Warming);
    }
    match host_activity {
        HostActivitySnapshot::Unavailable => Some(ResourceBlockReason::HostActivityUnavailable),
        HostActivitySnapshot::Observed { thermal_state, .. } if thermal_state != 0 => {
            Some(ResourceBlockReason::ThermalPressure)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn admitted(cpu_usage_percent: f32, available_memory_bytes: u64) -> ResourceStatus {
        ResourceStatus {
            admitted: true,
            snapshot: Some(ResourceSnapshot {
                cpu_usage_percent,
                available_memory_bytes,
                total_memory_bytes: 16 * super::super::GIB,
            }),
            block_reason: None,
        }
    }

    #[test]
    fn priority_round_rotates_only_import_jobs_and_caps_slices() {
        let now = Instant::now();
        let mut priority = ImportPriority::default();
        let generation = priority.prepare_request(now).generation;
        let mut jobs = Vec::new();
        for _ in 0..IMPORT_PRIORITY_JOBS.len() {
            jobs.push(priority.snapshot().expect("active priority").next_job);
            priority.note_ambient(generation, false);
        }
        assert_eq!(
            jobs,
            vec![
                AmbientJob::Classification,
                AmbientJob::StructuredExtract,
                AmbientJob::Entity,
                AmbientJob::Title,
                AmbientJob::PageGrowth,
            ]
        );
        assert_eq!(
            priority.snapshot().unwrap().synthesis_phase,
            Some(ImportSynthesisPhase::Detect)
        );
        for _ in 0..IMPORT_PRIORITY_MAX_AMBIENT_SLICES {
            priority.note_ambient(generation, false);
        }
        assert_eq!(
            priority.snapshot().unwrap().ambient_slices,
            IMPORT_PRIORITY_MAX_AMBIENT_SLICES
        );
    }

    #[test]
    fn completed_phases_finish_and_new_request_is_not_advanced_by_old_work() {
        let now = Instant::now();
        let mut priority = ImportPriority::default();
        let old = priority.prepare_request(now).generation;
        for _ in 0..IMPORT_PRIORITY_MAX_AMBIENT_SLICES {
            priority.note_ambient(old, true);
        }
        assert_eq!(
            priority.snapshot().unwrap().synthesis_phase,
            Some(ImportSynthesisPhase::Detect)
        );
        assert!(!priority.should_finish(now));
        priority.note_synthesis_phase(old, ImportSynthesisPhase::Detect);
        priority.note_synthesis_phase(old, ImportSynthesisPhase::Emergence);
        assert!(priority.should_finish(now));
        let fresh = priority.prepare_request(now).generation;
        assert_ne!(old, fresh);
        assert!(!priority.note_synthesis_phase(old, ImportSynthesisPhase::Emergence));
        assert!(!priority.note_ambient(old, true));
        assert!(!priority.clear_if_generation(old));
        assert!(!priority.should_finish(now));
        assert_eq!(priority.snapshot().unwrap().ambient_slices, 0);
    }

    #[test]
    fn expired_restore_is_rejected_and_deadline_is_bounded() {
        let mut priority = ImportPriority::default();
        let now = Instant::now();
        let epoch = chrono::Utc::now().timestamp();
        assert!(!priority.restore(epoch - 1, now, epoch));
        assert!(priority.restore(epoch + 3600, now, epoch));
        let snapshot = priority.snapshot().unwrap();
        assert!(snapshot.deadline <= now + IMPORT_PRIORITY_WINDOW);
    }

    #[tokio::test]
    async fn import_wake_interrupts_wait_and_shutdown_stays_authoritative() {
        let signal = WriteSignal::default();
        let (sender, mut shutdown) = watch::channel(false);
        signal.request_import();
        assert!(!tokio::time::timeout(
            Duration::from_millis(100),
            sleep_or_import_or_shutdown(
                &mut shutdown,
                &signal.import_wake(),
                Duration::from_secs(60)
            )
        )
        .await
        .expect("import wake must not wait for idle"));
        sender.send(true).unwrap();
        assert!(
            sleep_or_import_or_shutdown(
                &mut shutdown,
                &signal.import_wake(),
                Duration::from_secs(60)
            )
            .await
        );
    }

    #[test]
    fn active_cpu_is_allowed_but_memory_and_thermal_are_not() {
        let active_host = HostActivitySnapshot::Observed {
            idle_for: Duration::ZERO,
            thermal_state: 0,
        };
        assert_eq!(
            import_priority_block_reason(
                admitted(95.0, 8 * super::super::GIB),
                active_host,
                false,
                false
            ),
            None
        );
        assert_eq!(
            import_priority_block_reason(
                admitted(1.0, 1 * super::super::GIB),
                active_host,
                false,
                false
            ),
            Some(ResourceBlockReason::MemoryPressure)
        );
        assert_eq!(
            import_priority_block_reason(
                admitted(1.0, 8 * super::super::GIB),
                HostActivitySnapshot::Observed {
                    idle_for: Duration::from_secs(90),
                    thermal_state: 1
                },
                false,
                false,
            ),
            Some(ResourceBlockReason::ThermalPressure)
        );
    }
}
