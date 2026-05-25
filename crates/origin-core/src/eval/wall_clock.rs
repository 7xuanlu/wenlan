// SPDX-License-Identifier: Apache-2.0
//! Wall-clock watchdog for eval runs. Aborts when EVAL_MAX_WALL_SECS exceeded.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub struct WallClockWatchdog {
    exceeded: Arc<AtomicBool>,
}

impl WallClockWatchdog {
    /// Start the watchdog. After `cap` elapses, sets the exceeded flag.
    /// Logs elapsed seconds every 60s while running.
    pub fn start(cap: Duration) -> Self {
        Self::start_with_check_interval(cap, Duration::from_secs(60))
    }

    /// Same as `start` but lets you control the check interval — primarily
    /// useful for tests that need sub-second cap detection.
    pub fn start_with_check_interval(cap: Duration, check: Duration) -> Self {
        let exceeded = Arc::new(AtomicBool::new(false));
        let flag = exceeded.clone();
        let start = std::time::Instant::now();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(check).await;
                let elapsed = start.elapsed();
                // Log heartbeat at the natural check cadence (e.g. every 60s
                // in production).
                log::info!(
                    "eval wall-clock: {}s elapsed (cap {}s)",
                    elapsed.as_secs(),
                    cap.as_secs()
                );
                if elapsed >= cap {
                    flag.store(true, Ordering::SeqCst);
                    log::error!(
                        "EVAL_MAX_WALL_SECS cap exceeded: {}s > {}s",
                        elapsed.as_secs(),
                        cap.as_secs()
                    );
                    return;
                }
            }
        });
        WallClockWatchdog { exceeded }
    }

    /// A watchdog that never fires — useful when EVAL_MAX_WALL_SECS is unset
    /// or the harness wants explicit opt-out.
    pub fn disabled() -> Self {
        WallClockWatchdog {
            exceeded: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn is_exceeded(&self) -> bool {
        self.exceeded.load(Ordering::SeqCst)
    }

    /// Parse `EVAL_MAX_WALL_SECS` env (default 14400 = 4h).
    pub fn from_env() -> Self {
        let secs: u64 = std::env::var("EVAL_MAX_WALL_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(14400);
        Self::start(Duration::from_secs(secs))
    }
}
