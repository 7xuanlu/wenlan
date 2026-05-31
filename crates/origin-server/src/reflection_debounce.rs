// SPDX-License-Identifier: Apache-2.0
//! Debounced background reflection (T22).
//!
//! When an agent stores many memories in a burst, the per-store detached
//! enrichment spawn (`memory_routes::handle_store_memory`) would otherwise fire
//! N overlapping `run_post_ingest_enrichment` tasks — N classify+extract LLM
//! calls, N page-growth passes — most of which are wasted because a newer write
//! from the same agent lands before the older reflection finishes.
//!
//! [`ReflectionDebouncer`] coalesces those spawns **per agent key**. Each
//! `schedule(key, delay, work)` call cancels the previously-scheduled work for
//! that key, distinguishing two cases so cancellation is always clean:
//!   - **Pre-delay** (work has not started — still inside its debounce `sleep`):
//!     the task is [`tokio::task::JoinHandle::abort`]ed. Nothing has run, so
//!     aborting drops no state.
//!   - **In-flight** (work has started running): the task is NOT aborted.
//!     Instead a shared [`AtomicBool`] is flipped; the running `work` checks it
//!     at clean step boundaries (cooperative cancellation; the actual
//!     checkpoints live in
//!     `origin_core::post_ingest::run_post_ingest_enrichment`) and returns early
//!     without half-applying a step. Aborting a started enrichment is exactly
//!     the half-write risk this design avoids.
//!
//! Only the **latest** write in a burst actually enriches. No write is dropped:
//! cancelling the older reflection only defers it — the newer one supersedes it
//! and enriches the same agent's freshly-written rows.
//!
//! Anti-starvation: a never-settling writer that reschedules the same key
//! forever would never reflect. After [`MAX_CANCELLATIONS`] consecutive
//! cancellations the next `schedule` lets the in-flight reflection run to
//! completion (force-run) so reflection makes progress under sustained load.
//!
//! This struct lives in `origin-server` (the detached-spawn owner). The
//! cancellation *signal* it produces (`Arc<AtomicBool>`) is passed into
//! `origin-core`'s `run_post_ingest_enrichment` as `Option<&AtomicBool>`; core
//! stays framework-agnostic and only sees a plain atomic.
//!
//! No new dependency: built on `tokio::spawn` + `JoinHandle::abort` +
//! `std::sync::atomic::AtomicBool` only (no `tokio-util` / `CancellationToken`).

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::task::JoinHandle;

/// After this many consecutive cancellations of the same key, the next
/// `schedule` lets the in-flight reflection finish instead of cancelling it.
/// Bounds worst-case reflection latency under a writer that never settles.
pub const MAX_CANCELLATIONS: u32 = 5;

/// One in-flight (or pending) reflection for a single key.
struct Slot {
    /// Handle to the delay+work task. Aborted ONLY while the task is still in
    /// its pre-delay `sleep` (see `started`); once work has begun we cancel
    /// cooperatively via `cancel` instead of aborting.
    handle: JoinHandle<()>,
    /// Cooperative-cancel flag the running `work` checks at step boundaries.
    /// Flipping it stops an already-started reflection at a clean boundary.
    cancel: Arc<AtomicBool>,
    /// Set to `true` by the task the instant its debounce delay elapses and it
    /// is about to call `work`. Gates abort: `false` ⇒ safe to abort (nothing
    /// ran), `true` ⇒ must cooperate via `cancel` (never abort mid-work).
    started: Arc<AtomicBool>,
    /// How many times this key's reflection has been cancelled in a row without
    /// completing. Reset to 0 whenever a force-run is allowed through.
    cancellations: u32,
}

/// Per-key debouncer for background reflection spawns.
///
/// Cheap to clone (an `Arc` around the slot map); share one instance across all
/// store handlers via `ServerState`.
#[derive(Clone, Default)]
pub struct ReflectionDebouncer {
    slots: Arc<Mutex<HashMap<String, Slot>>>,
}

impl ReflectionDebouncer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Schedule `work` to run for `key` after `delay`, cancelling/coalescing any
    /// previously-scheduled work for the same key.
    ///
    /// `work` receives an `Arc<AtomicBool>` cancel flag; it MUST check it at
    /// clean boundaries and return early when set (so cancellation never
    /// half-applies state). The latest scheduled work for a key always runs
    /// (subject only to a still-later `schedule` superseding it), so a burst of
    /// K rapid writes for one agent collapses to a single reflection of the
    /// last write rather than K overlapping ones.
    pub fn schedule<F, Fut>(&self, key: &str, delay: Duration, work: F)
    where
        F: FnOnce(Arc<AtomicBool>) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let cancel = Arc::new(AtomicBool::new(false));
        let started = Arc::new(AtomicBool::new(false));

        // Cancel any prior work for this key and carry forward its cancellation
        // streak so the anti-starvation ceiling counts across reschedules.
        let prior_count = {
            let mut slots = self.slots.lock().unwrap();
            if let Some(prev) = slots.remove(key) {
                if prev.cancellations >= MAX_CANCELLATIONS {
                    // Anti-starvation force-run: do NOT signal cancel and do NOT
                    // abort — let the in-flight reflection finish on its own.
                    // (Its JoinHandle is dropped/detached; the task completes and
                    // self-cleans, but since we just removed its map entry it
                    // simply finds no entry to remove.) Reset the streak so the
                    // freshly-scheduled work below is not itself starved next.
                    drop(prev);
                    0
                } else {
                    // Always signal cooperative cancel (covers the in-flight
                    // case). Abort ONLY if work has not started yet — i.e. the
                    // task is still inside its debounce `sleep`. Aborting a
                    // started task could kill it mid-step; the flag handles that
                    // case at a clean boundary instead.
                    prev.cancel.store(true, Ordering::Relaxed);
                    if !prev.started.load(Ordering::Relaxed) {
                        prev.handle.abort();
                    }
                    prev.cancellations + 1
                }
            } else {
                0
            }
        };

        let cancel_for_task = cancel.clone();
        let started_for_task = started.clone();
        let slots_for_cleanup = self.slots.clone();
        let key_owned = key.to_string();

        let handle = tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            // A newer schedule that arrived during the sleep either aborted us
            // (we never reach here) or set the cancel flag — bail before work.
            if cancel_for_task.load(Ordering::Relaxed) {
                return;
            }
            // Mark started BEFORE running work: from here a concurrent reschedule
            // must cooperate (flag) rather than abort. There is a tiny window
            // between this store and the next `schedule`'s `started.load`; if a
            // reschedule reads `started == false` and aborts right after we set
            // it true, abort still only fires at the next `.await` inside work,
            // where the very first checkpoint reads the (now-set) cancel flag and
            // returns early anyway. Either way no step is half-applied.
            started_for_task.store(true, Ordering::Relaxed);
            work(cancel_for_task.clone()).await;

            // Self-cleanup: drop our own slot if it is still ours (a newer
            // schedule for the same key may have already replaced it).
            let mut slots = slots_for_cleanup.lock().unwrap();
            if let Some(slot) = slots.get(&key_owned) {
                if Arc::ptr_eq(&slot.cancel, &cancel_for_task) {
                    slots.remove(&key_owned);
                }
            }
        });

        let mut slots = self.slots.lock().unwrap();
        slots.insert(
            key.to_string(),
            Slot {
                handle,
                cancel,
                started,
                cancellations: prior_count,
            },
        );
    }

    /// Number of live (pending or running, not yet cleaned-up) keys. Test hook
    /// for the no-unbounded-growth guarantee.
    #[cfg(test)]
    pub fn live_keys(&self) -> usize {
        self.slots.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::time::Duration;

    /// Settle helper: advance wall-clock so spawned delay+work tasks complete.
    async fn settle(ms: u64) {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }

    #[tokio::test]
    async fn test_single_schedule_runs_work_once() {
        let deb = ReflectionDebouncer::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let c = counter.clone();
        deb.schedule("agentA", Duration::from_millis(30), move |_cancel| {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::Relaxed);
            }
        });
        settle(120).await;
        assert_eq!(
            counter.load(Ordering::Relaxed),
            1,
            "work should run exactly once"
        );
    }

    #[tokio::test]
    async fn test_reschedule_cancels_prior_before_delay() {
        let deb = ReflectionDebouncer::new();
        let ran1 = Arc::new(AtomicUsize::new(0));
        let ran2 = Arc::new(AtomicUsize::new(0));

        let r1 = ran1.clone();
        deb.schedule("agentA", Duration::from_millis(200), move |_c| {
            let r1 = r1.clone();
            async move {
                r1.fetch_add(1, Ordering::Relaxed);
            }
        });
        // Reschedule immediately with a much shorter delay — prior work1 is
        // still inside its 200ms sleep, so abort() must prevent it running.
        let r2 = ran2.clone();
        deb.schedule("agentA", Duration::from_millis(30), move |_c| {
            let r2 = r2.clone();
            async move {
                r2.fetch_add(1, Ordering::Relaxed);
            }
        });

        settle(300).await;
        assert_eq!(
            ran1.load(Ordering::Relaxed),
            0,
            "work1 must be cancelled pre-delay"
        );
        assert_eq!(
            ran2.load(Ordering::Relaxed),
            1,
            "work2 must run exactly once"
        );
    }

    #[tokio::test]
    async fn test_reschedule_signals_cancel_to_inflight_work() {
        let deb = ReflectionDebouncer::new();
        let observed_cancel = Arc::new(AtomicUsize::new(0));
        let work2_done = Arc::new(AtomicUsize::new(0));

        // work1 starts quickly, then polls the cancel flag in a loop.
        let oc = observed_cancel.clone();
        deb.schedule("agentA", Duration::from_millis(10), move |cancel| {
            let oc = oc.clone();
            async move {
                for _ in 0..200 {
                    if cancel.load(Ordering::Relaxed) {
                        oc.fetch_add(1, Ordering::Relaxed);
                        return; // bail early — cooperative cancellation
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            }
        });

        // Let work1 start running (past its 10ms delay) before rescheduling, so
        // we exercise the in-flight AtomicBool path, not the pre-delay abort.
        settle(40).await;

        let wd = work2_done.clone();
        deb.schedule("agentA", Duration::from_millis(10), move |_c| {
            let wd = wd.clone();
            async move {
                wd.fetch_add(1, Ordering::Relaxed);
            }
        });

        settle(200).await;
        assert_eq!(
            observed_cancel.load(Ordering::Relaxed),
            1,
            "in-flight work1 must observe cancel and bail early"
        );
        assert_eq!(work2_done.load(Ordering::Relaxed), 1, "work2 must complete");
    }

    #[tokio::test]
    async fn test_concurrent_keys_are_independent() {
        let deb = ReflectionDebouncer::new();
        let a_final = Arc::new(AtomicUsize::new(0));
        let b_ran = Arc::new(AtomicUsize::new(0));

        // Schedule agentB once — it must NOT be disturbed by agentA reschedules.
        let b = b_ran.clone();
        deb.schedule("agentB", Duration::from_millis(40), move |_c| {
            let b = b.clone();
            async move {
                b.fetch_add(1, Ordering::Relaxed);
            }
        });

        // Schedule + reschedule agentA.
        deb.schedule("agentA", Duration::from_millis(200), |_c| async {});
        let af = a_final.clone();
        deb.schedule("agentA", Duration::from_millis(30), move |_c| {
            let af = af.clone();
            async move {
                af.fetch_add(1, Ordering::Relaxed);
            }
        });

        settle(300).await;
        assert_eq!(
            b_ran.load(Ordering::Relaxed),
            1,
            "agentB work must run untouched"
        );
        assert_eq!(
            a_final.load(Ordering::Relaxed),
            1,
            "final agentA work must run"
        );
    }

    #[tokio::test]
    async fn test_max_deferral_ceiling_forces_run() {
        let deb = ReflectionDebouncer::new();
        let runs = Arc::new(AtomicUsize::new(0));

        // Each scheduled work starts after a short delay and then runs long
        // enough to still be in-flight while we hammer reschedules. Each
        // reschedule normally signals cancel to the in-flight one — but after
        // MAX_CANCELLATIONS the ceiling must let an in-flight reflection run to
        // completion (force-run path), so `runs` reaches >= 1.
        for _ in 0..(MAX_CANCELLATIONS + 2) {
            let r = runs.clone();
            deb.schedule("agentA", Duration::from_millis(5), move |cancel| {
                let r = r.clone();
                async move {
                    // Long-running work that respects cancellation.
                    for _ in 0..100 {
                        if cancel.load(Ordering::Relaxed) {
                            return;
                        }
                        tokio::time::sleep(Duration::from_millis(3)).await;
                    }
                    r.fetch_add(1, Ordering::Relaxed);
                }
            });
            // Let each work item start (past its 5ms delay) before the next
            // reschedule, so we drive the in-flight cancel/ceiling path.
            settle(15).await;
        }

        settle(500).await;
        assert!(
            runs.load(Ordering::Relaxed) >= 1,
            "after MAX_CANCELLATIONS a reflection must be force-run to completion, got {}",
            runs.load(Ordering::Relaxed)
        );
    }

    #[tokio::test]
    async fn test_completed_slot_is_cleaned_up() {
        let deb = ReflectionDebouncer::new();
        deb.schedule("agentA", Duration::from_millis(20), |_c| async {});
        deb.schedule("agentB", Duration::from_millis(20), |_c| async {});
        settle(200).await;
        assert_eq!(
            deb.live_keys(),
            0,
            "completed slots must be removed so the map cannot grow unbounded"
        );
    }
}
