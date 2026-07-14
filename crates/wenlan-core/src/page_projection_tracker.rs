use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Debug, Default)]
pub struct PageProjectionTracker {
    active_writes: AtomicU64,
    generation: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageProjectionSample {
    active_writes: u64,
    generation: u64,
}

impl PageProjectionSample {
    pub const fn has_active_writes(self) -> bool {
        self.active_writes != 0
    }

    pub const fn generation(self) -> u64 {
        self.generation
    }
}

impl PageProjectionTracker {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn begin_write(self: &Arc<Self>) -> PageProjectionWriteGuard {
        self.active_writes.fetch_add(1, Ordering::AcqRel);
        PageProjectionWriteGuard {
            tracker: Arc::clone(self),
        }
    }

    pub fn sample(&self) -> PageProjectionSample {
        PageProjectionSample {
            active_writes: self.active_writes.load(Ordering::Acquire),
            generation: self.generation.load(Ordering::Acquire),
        }
    }
}

#[derive(Debug)]
pub struct PageProjectionWriteGuard {
    tracker: Arc<PageProjectionTracker>,
}

impl PageProjectionWriteGuard {
    pub(crate) fn belongs_to(&self, tracker: &Arc<PageProjectionTracker>) -> bool {
        Arc::ptr_eq(&self.tracker, tracker)
    }
}

impl Drop for PageProjectionWriteGuard {
    fn drop(&mut self) {
        self.tracker.generation.fetch_add(1, Ordering::AcqRel);
        self.tracker.active_writes.fetch_sub(1, Ordering::AcqRel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_tracks_active_interval_and_completion_generation() {
        let tracker = PageProjectionTracker::new();
        let before = tracker.sample();
        let guard = tracker.begin_write();
        assert!(tracker.sample().has_active_writes());
        assert_eq!(tracker.sample().generation(), before.generation());
        drop(guard);
        assert!(!tracker.sample().has_active_writes());
        assert_eq!(tracker.sample().generation(), before.generation() + 1);
    }

    #[test]
    fn every_success_error_and_unwind_exit_clears_active_state() {
        let tracker = PageProjectionTracker::new();

        fn early_error(tracker: &Arc<PageProjectionTracker>) -> Result<(), ()> {
            let _guard = tracker.begin_write();
            Err(())
        }
        assert_eq!(early_error(&tracker), Err(()));
        assert!(!tracker.sample().has_active_writes());

        let unwind_tracker = Arc::clone(&tracker);
        let unwind = std::panic::catch_unwind(move || {
            let _guard = unwind_tracker.begin_write();
            panic!("fixture unwind");
        });
        assert!(unwind.is_err());
        assert!(!tracker.sample().has_active_writes());
        assert_eq!(tracker.sample().generation(), 2);
    }

    #[test]
    fn overlapping_guards_are_counted_and_tokens_are_owner_bound() {
        let tracker = PageProjectionTracker::new();
        let other = PageProjectionTracker::new();
        let first = tracker.begin_write();
        let second = tracker.begin_write();
        assert_eq!(tracker.sample().active_writes, 2);
        assert!(first.belongs_to(&tracker));
        assert!(!first.belongs_to(&other));
        drop(first);
        assert!(tracker.sample().has_active_writes());
        drop(second);
        assert!(!tracker.sample().has_active_writes());
        assert_eq!(tracker.sample().generation(), 2);
    }
}
