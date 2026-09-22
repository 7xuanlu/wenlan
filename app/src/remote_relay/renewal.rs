// SPDX-License-Identifier: AGPL-3.0-only
//! In-memory timing policy for renewing an already enrolled relay route.

use std::time::{Duration, Instant, SystemTime};

const SUCCESS_RENEWAL_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(60 * 60);
const RETRY_DELAYS: [Duration; 5] = [
    Duration::from_secs(5 * 60),
    Duration::from_secs(10 * 60),
    Duration::from_secs(20 * 60),
    Duration::from_secs(40 * 60),
    MAX_RETRY_DELAY,
];

const CLOCK_DRIFT_TOLERANCE: Duration = Duration::from_secs(1);

pub(crate) struct RenewalSchedule {
    armed_at: Instant,
    wait: Duration,
    last_wall: SystemTime,
    last_monotonic: Instant,
    failures: u8,
    renewal_required: bool,
}

impl RenewalSchedule {
    pub(crate) fn new(now: SystemTime, monotonic: Instant) -> Self {
        Self {
            armed_at: monotonic,
            wait: SUCCESS_RENEWAL_INTERVAL,
            last_wall: now,
            last_monotonic: monotonic,
            failures: 0,
            renewal_required: false,
        }
    }

    pub(crate) fn due(&mut self, now: SystemTime, monotonic: Instant) -> bool {
        self.observe(now, monotonic);

        let elapsed = monotonic.checked_duration_since(self.armed_at);
        let retry_ready = elapsed.is_some_and(|elapsed| elapsed >= self.wait);

        // A wall-clock discontinuity requires a renewal immediately while the
        // schedule is healthy. Once a renewal has failed, its retry backoff
        // remains authoritative.
        (self.renewal_required && self.failures == 0) || retry_ready
    }

    pub(crate) fn succeeded(&mut self, now: SystemTime, monotonic: Instant) {
        self.observe(now, monotonic);
        self.armed_at = monotonic;
        self.wait = SUCCESS_RENEWAL_INTERVAL;
        self.failures = 0;
        self.renewal_required = false;
    }

    pub(crate) fn failed(
        &mut self,
        now: SystemTime,
        monotonic: Instant,
        retry_after: Option<Duration>,
    ) {
        self.observe(now, monotonic);

        let retry_index = usize::from(self.failures.min((RETRY_DELAYS.len() - 1) as u8));
        let backoff = RETRY_DELAYS[retry_index];
        let server_delay = retry_after.unwrap_or(Duration::ZERO).min(MAX_RETRY_DELAY);

        self.armed_at = monotonic;
        self.wait = backoff.max(server_delay);
        self.failures = self.failures.saturating_add(1);
    }

    fn observe(&mut self, now: SystemTime, monotonic: Instant) {
        let wall_elapsed = now.duration_since(self.last_wall).ok();
        let monotonic_elapsed = monotonic.checked_duration_since(self.last_monotonic);
        let wall_went_backwards = wall_elapsed.is_none();
        let clocks_diverged = match (wall_elapsed, monotonic_elapsed) {
            (Some(wall), Some(monotonic)) => wall > monotonic.saturating_add(CLOCK_DRIFT_TOLERANCE),
            _ => true,
        };

        self.last_wall = now;
        self.last_monotonic = monotonic;
        if wall_went_backwards || clocks_diverged {
            self.renewal_required = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: Duration = Duration::from_secs(60 * 60);
    const MINUTE: Duration = Duration::from_secs(60);

    fn clocks() -> (SystemTime, Instant) {
        (SystemTime::UNIX_EPOCH, Instant::now())
    }

    fn advance(
        (wall, monotonic): (SystemTime, Instant),
        elapsed: Duration,
    ) -> (SystemTime, Instant) {
        (
            wall.checked_add(elapsed).unwrap(),
            monotonic.checked_add(elapsed).unwrap(),
        )
    }

    #[test]
    fn successful_renewal_is_due_after_six_hours() {
        let clocks = clocks();
        let mut schedule = RenewalSchedule::new(clocks.0, clocks.1);

        assert!(!schedule.due(clocks.0, clocks.1));
        let before = advance(clocks, SUCCESS_RENEWAL_INTERVAL - Duration::from_nanos(1));
        assert!(!schedule.due(before.0, before.1));
        let due = advance(clocks, SUCCESS_RENEWAL_INTERVAL);
        assert!(schedule.due(due.0, due.1));
    }

    #[test]
    fn failures_use_bounded_exponential_backoff() {
        let clocks = clocks();
        let mut schedule = RenewalSchedule::new(clocks.0, clocks.1);
        let mut current = clocks;

        for delay in RETRY_DELAYS {
            schedule.failed(current.0, current.1, None);
            let before = advance(current, delay - Duration::from_nanos(1));
            assert!(!schedule.due(before.0, before.1));
            current = advance(current, delay);
            assert!(schedule.due(current.0, current.1));
        }

        schedule.failed(current.0, current.1, None);
        let before_cap = advance(current, MAX_RETRY_DELAY - Duration::from_nanos(1));
        assert!(!schedule.due(before_cap.0, before_cap.1));
        let at_cap = advance(current, MAX_RETRY_DELAY);
        assert!(schedule.due(at_cap.0, at_cap.1));
    }

    #[test]
    fn retry_after_is_bounded_and_never_shortens_backoff() {
        let clocks = clocks();
        let mut schedule = RenewalSchedule::new(clocks.0, clocks.1);

        schedule.failed(clocks.0, clocks.1, Some(Duration::from_secs(20 * 60)));
        let before_header = advance(
            clocks,
            Duration::from_secs(20 * 60) - Duration::from_nanos(1),
        );
        assert!(!schedule.due(before_header.0, before_header.1));
        let at_header = advance(clocks, Duration::from_secs(20 * 60));
        assert!(schedule.due(at_header.0, at_header.1));

        schedule.failed(at_header.0, at_header.1, Some(Duration::MAX));
        let before_cap = advance(at_header, MAX_RETRY_DELAY - Duration::from_nanos(1));
        assert!(!schedule.due(before_cap.0, before_cap.1));
        let at_cap = advance(at_header, MAX_RETRY_DELAY);
        assert!(schedule.due(at_cap.0, at_cap.1));
    }

    #[test]
    fn success_resets_failures_and_wall_discontinuities() {
        let clocks = clocks();
        let mut schedule = RenewalSchedule::new(clocks.0, clocks.1);
        schedule.failed(clocks.0, clocks.1, None);

        let recovered = advance(clocks, MINUTE);
        schedule.succeeded(recovered.0, recovered.1);
        let one_hour = advance(recovered, HOUR);
        assert!(!schedule.due(one_hour.0, one_hour.1));
        let six_hours = advance(recovered, SUCCESS_RENEWAL_INTERVAL);
        assert!(schedule.due(six_hours.0, six_hours.1));

        schedule.succeeded(six_hours.0, six_hours.1);
        let backward = (
            six_hours.0.checked_sub(Duration::from_secs(1)).unwrap(),
            six_hours.1.checked_add(Duration::from_secs(1)).unwrap(),
        );
        assert!(schedule.due(backward.0, backward.1));
    }

    #[test]
    fn forward_wall_movement_detects_sleep_and_clock_jumps() {
        let initial_clocks = clocks();
        let mut schedule = RenewalSchedule::new(initial_clocks.0, initial_clocks.1);
        let after_sleep = (
            initial_clocks
                .0
                .checked_add(Duration::from_secs(60 * 60))
                .unwrap(),
            initial_clocks.1,
        );
        assert!(schedule.due(after_sleep.0, after_sleep.1));

        let jumped_clocks = clocks();
        let mut schedule = RenewalSchedule::new(jumped_clocks.0, jumped_clocks.1);
        let wall_jump = (
            jumped_clocks
                .0
                .checked_add(Duration::from_secs(60 * 60))
                .unwrap(),
            jumped_clocks.1.checked_add(Duration::from_secs(1)).unwrap(),
        );
        assert!(schedule.due(wall_jump.0, wall_jump.1));
    }

    #[test]
    fn extreme_retry_after_does_not_overflow_or_extend_past_one_hour() {
        let clocks = clocks();
        let mut schedule = RenewalSchedule::new(clocks.0, clocks.1);
        schedule.failed(clocks.0, clocks.1, Some(Duration::MAX));

        let before_cap = advance(clocks, MAX_RETRY_DELAY - Duration::from_nanos(1));
        assert!(!schedule.due(before_cap.0, before_cap.1));
        let at_cap = advance(clocks, MAX_RETRY_DELAY);
        assert!(schedule.due(at_cap.0, at_cap.1));
    }

    #[test]
    fn backwards_clock_failure_obeys_retry_backoff() {
        let clocks = clocks();
        let mut schedule = RenewalSchedule::new(clocks.0, clocks.1);
        let backwards = (
            clocks.0.checked_sub(Duration::from_secs(1)).unwrap(),
            clocks.1.checked_add(Duration::from_secs(1)).unwrap(),
        );
        assert!(schedule.due(backwards.0, backwards.1));
        schedule.failed(backwards.0, backwards.1, None);

        for seconds in [30, 60, 90, 120] {
            let poll = (
                backwards
                    .0
                    .checked_add(Duration::from_secs(seconds))
                    .unwrap(),
                backwards
                    .1
                    .checked_add(Duration::from_secs(seconds))
                    .unwrap(),
            );
            assert!(!schedule.due(poll.0, poll.1));
        }
        let retry = advance(backwards, RETRY_DELAYS[0]);
        assert!(schedule.due(retry.0, retry.1));
    }
}
