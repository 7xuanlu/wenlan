// SPDX-License-Identifier: Apache-2.0
//! `DaemonHandle` integration test — spawn, query, drop without orphans.
//!
//! Skipped (treated as `Ok(())`) if `origin-server` isn't built — CI matrix
//! that doesn't pre-build the daemon won't fail this. Run locally with:
//!
//! ```bash
//! cargo build -p origin-server
//! cargo test -p origin-core --test daemon_handle --features eval-harness -- --test-threads=1
//! ```

#![cfg(feature = "eval-harness")]

use origin_core::eval::http_harness::DaemonHandle;

fn skip_if_no_daemon() -> bool {
    let path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().and_then(|q| q.parent()).map(|d| d.to_path_buf()));
    if let Some(target_dir) = path {
        let bin = target_dir.join(if cfg!(windows) {
            "origin-server.exe"
        } else {
            "origin-server"
        });
        if !bin.exists() {
            eprintln!(
                "[daemon_handle] skipping: {} not built — run `cargo build -p origin-server`",
                bin.display()
            );
            return true;
        }
    }
    false
}

fn pgrep_origin_server() -> usize {
    let output = std::process::Command::new("pgrep")
        .args(["-f", "origin-server"])
        .output();
    let Ok(output) = output else { return 0 };
    if !output.status.success() {
        return 0;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count()
}

#[tokio::test]
async fn spawn_and_drop_leaves_no_orphans() {
    if skip_if_no_daemon() {
        return;
    }
    let _ = origin_core::eval::orphan_reaper::cleanup_eval_orphans();
    let initial = pgrep_origin_server();
    {
        let daemon = DaemonHandle::spawn().await.expect("spawn");
        let resp = reqwest::get(daemon.url("/api/health"))
            .await
            .expect("/api/health");
        assert_eq!(resp.status(), 200);
    }
    // Drop's libc::kill SIGTERM → 1s sleep → SIGKILL. Give kernel another
    // 2s to fully unmap the process.
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    let final_count = pgrep_origin_server();
    assert_eq!(
        final_count, initial,
        "orphan daemon leaked (initial={initial} final={final_count})"
    );
}

#[tokio::test]
async fn spawn_uses_ephemeral_port_not_default_7878() {
    if skip_if_no_daemon() {
        return;
    }
    let daemon = DaemonHandle::spawn().await.expect("spawn");
    let url = daemon.url("/api/health");
    assert!(url.starts_with("http://127.0.0.1:"), "got {url}");
    let port_str = url
        .strip_prefix("http://127.0.0.1:")
        .unwrap()
        .strip_suffix("/api/health")
        .unwrap();
    let port: u16 = port_str.parse().unwrap();
    assert_ne!(port, 7878, "should be ephemeral, not default 7878");
    assert_ne!(port, 0, "port 0 means OS never returned a real bind");
}

#[tokio::test]
async fn smoke_preflight_runs_warmup_against_live_daemon() {
    if skip_if_no_daemon() {
        return;
    }
    let daemon = DaemonHandle::spawn().await.expect("spawn");
    // 3 warmup calls — enough to hit the path twice past cold-init,
    // small enough to stay under the integration-test budget.
    origin_core::eval::l2_runner::smoke_preflight(&daemon, 3)
        .await
        .expect("smoke_preflight");
}

#[tokio::test]
async fn pidfile_exists_during_lifetime_and_removed_on_drop() {
    if skip_if_no_daemon() {
        return;
    }
    let pidfile;
    {
        let daemon = DaemonHandle::spawn().await.expect("spawn");
        pidfile = daemon.pidfile().to_path_buf();
        assert!(pidfile.exists(), "pidfile must exist while daemon runs");
    }
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    assert!(
        !pidfile.exists(),
        "pidfile should have been removed by Drop"
    );
}
