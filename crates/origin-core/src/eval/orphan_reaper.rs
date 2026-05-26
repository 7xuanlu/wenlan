// SPDX-License-Identifier: Apache-2.0
//! Sweep `~/.cache/origin-eval/daemons/*.pid` for orphan daemons.
//!
//! Drop-based cleanup in `DaemonHandle` doesn't run on panic-during-unwind /
//! SIGKILL / `cargo test --abort`. Pidfiles let us reap on the next harness run.

use std::path::Path;

/// Scan `daemons_dir` for `<pid>.pid` files. For each:
///   - Read the PID (first line)
///   - Probe with `kill(pid, 0)` (Unix) to check if process still exists
///   - If gone: remove the pidfile AND any sibling tmp data_dir referenced on
///     line 2 of the pidfile (only if it lives under `std::env::temp_dir()`).
///
/// Returns the count of pidfiles reaped. Errors during individual cleanups
/// are silently skipped so a single bad entry never aborts the sweep.
pub fn cleanup_eval_orphans_in(daemons_dir: &Path) -> anyhow::Result<usize> {
    if !daemons_dir.exists() {
        return Ok(0);
    }
    let mut reaped = 0;
    for entry in std::fs::read_dir(daemons_dir)? {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("pid") {
            continue;
        }
        let Ok(contents) = std::fs::read_to_string(&path) else {
            continue;
        };
        let mut lines = contents.lines();
        let pid_str = lines.next().unwrap_or("").trim();
        let Ok(pid) = pid_str.parse::<i32>() else {
            continue;
        };
        if process_alive(pid) {
            continue;
        }
        let _ = std::fs::remove_file(&path);
        if let Some(data_dir) = lines.next() {
            let dd = Path::new(data_dir.trim());
            if dd.exists() && is_under_temp_dir(dd) {
                let _ = std::fs::remove_dir_all(dd);
            }
        }
        reaped += 1;
    }
    Ok(reaped)
}

/// Convenience: reap under the canonical `~/.cache/origin-eval/daemons/`.
pub fn cleanup_eval_orphans() -> anyhow::Result<usize> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("HOME not set"))?;
    cleanup_eval_orphans_in(&home.join(".cache/origin-eval/daemons"))
}

/// Decide if `dd` lives under the system temp dir, comparing canonical
/// paths so macOS's `/var/folders/...` vs `/private/var/folders/...`
/// symlink mismatch doesn't bypass the guard. Falls back to refusing the
/// reap if canonicalization fails (rare; happens when the path itself is
/// a dangling symlink — conservative direction, since we can always reap
/// on the next sweep but cannot un-rm a wrongly-removed dir).
///
/// **Windows case-folding is not handled** because `process_alive` on
/// Windows always returns `true` (no dead-PID detection without
/// OpenProcess wiring), so this branch never runs on Windows.
fn is_under_temp_dir(dd: &Path) -> bool {
    let temp = std::env::temp_dir();
    match (dd.canonicalize(), temp.canonicalize()) {
        (Ok(dd_canon), Ok(temp_canon)) => dd_canon.starts_with(&temp_canon),
        _ => false,
    }
}

#[cfg(unix)]
fn process_alive(pid: i32) -> bool {
    // SAFETY: `kill(pid, 0)` is a pure existence/permission probe — sends no signal.
    unsafe { libc::kill(pid, 0) == 0 }
}

#[cfg(windows)]
fn process_alive(_pid: i32) -> bool {
    // Conservative: don't reap on Windows until OpenProcess/GetExitCodeProcess
    // wiring lands (Windows eval support is out of scope for P2).
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reaper_removes_stale_pidfile_for_dead_pid() {
        let tmp = tempfile::tempdir().unwrap();
        let pidfile = tmp.path().join("99999999.pid");
        // 99999999 is above the kernel's PID ceiling on every supported system.
        std::fs::write(&pidfile, "99999999").unwrap();
        let reaped = cleanup_eval_orphans_in(tmp.path()).unwrap();
        assert_eq!(reaped, 1);
        assert!(!pidfile.exists());
    }

    #[test]
    fn reaper_leaves_live_pidfile_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let live_pid = std::process::id();
        let pidfile = tmp.path().join(format!("{}.pid", live_pid));
        std::fs::write(&pidfile, live_pid.to_string()).unwrap();
        let reaped = cleanup_eval_orphans_in(tmp.path()).unwrap();
        assert_eq!(reaped, 0);
        assert!(pidfile.exists());
    }

    #[test]
    fn reaper_ignores_non_pid_files() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("readme.txt"), "hello").unwrap();
        let reaped = cleanup_eval_orphans_in(tmp.path()).unwrap();
        assert_eq!(reaped, 0);
        assert!(tmp.path().join("readme.txt").exists());
    }

    #[test]
    fn reaper_reaps_tmp_data_dir_referenced_on_line_2() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = std::env::temp_dir().join(format!(
            "origin-eval-reaper-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&data_dir).unwrap();
        let pidfile = tmp.path().join("99999998.pid");
        std::fs::write(&pidfile, format!("99999998\n{}", data_dir.display())).unwrap();
        cleanup_eval_orphans_in(tmp.path()).unwrap();
        assert!(!pidfile.exists());
        assert!(!data_dir.exists(), "data_dir should have been reaped");
    }

    #[test]
    fn reaper_refuses_to_reap_data_dir_outside_tmp() {
        // Safety guard: only data_dir entries under env::temp_dir() are
        // candidates for removal. Anything else stays put.
        let tmp = tempfile::tempdir().unwrap();
        let outside_dir = tempfile::tempdir().unwrap(); // separate tempdir RAII guard
                                                        // Build a path that does NOT live under env::temp_dir() by walking up
                                                        // far enough — point at the staging tempdir itself (separate guard, NOT under TMPDIR base path).
                                                        // env::temp_dir() returns a canonical macOS /var/folders/... path on macOS;
                                                        // we'll construct an obviously-non-tmp path under the worktree.
        let non_tmp = Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf();
        if !non_tmp.exists() {
            return; // pathological CI env — skip
        }
        let pidfile = tmp.path().join("99999997.pid");
        std::fs::write(&pidfile, format!("99999997\n{}", non_tmp.display())).unwrap();
        cleanup_eval_orphans_in(tmp.path()).unwrap();
        assert!(!pidfile.exists(), "pidfile reaped");
        assert!(non_tmp.exists(), "non-tmp dir must NOT be reaped");
        drop(outside_dir);
    }

    #[test]
    fn reaper_returns_zero_when_dir_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("does-not-exist");
        let reaped = cleanup_eval_orphans_in(&missing).unwrap();
        assert_eq!(reaped, 0);
    }
}
