// SPDX-License-Identifier: Apache-2.0
//! Spawn `origin-server` in a tmp data dir on an ephemeral port for L2 eval.
//!
//! `origin-core` lives in a different crate from `origin-server`, so the
//! `CARGO_BIN_EXE_origin-server` env var that Cargo sets for same-crate
//! integration tests is **not** available here. Binary path resolution is
//! therefore runtime, not compile-time:
//!
//!   1. `ORIGIN_SERVER_BIN` env var (explicit operator override).
//!   2. `<workspace>/target/<profile>/origin-server` derived from
//!      `std::env::current_exe()` (the test binary lives in
//!      `target/<profile>/deps/<x>`, so two `parent()` calls land us at
//!      `target/<profile>/`).
//!
//! `cargo build -p origin-server` must run before harness use; the
//! `scripts/refresh-l2-baselines.sh` script handles this.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

/// Spawned `origin-server` instance owned by an eval harness.
///
/// Field declaration order is LOAD-BEARING: `child` drops first so its
/// `kill_on_drop` runs before `_data_dir` (TempDir) tries to remove the
/// directory the daemon still has open.
pub struct DaemonHandle {
    child: Child,
    pidfile: PathBuf,
    /// Bound port. Populated from stdout `ORIGIN_LISTENING_ON=` line or
    /// `ORIGIN_PORT_FILE`, whichever wins the race.
    pub port: u16,
    /// `TempDir` RAII guard — kept alive for the duration of the handle.
    _data_dir: tempfile::TempDir,
    /// Same path as `_data_dir.path()`, materialised so callers don't have
    /// to reach through the private guard.
    pub data_dir_path: PathBuf,
}

impl DaemonHandle {
    /// Spawn `origin-server` on an ephemeral 127.0.0.1 port with a fresh
    /// `tempfile::TempDir` mounted as `ORIGIN_DATA_DIR`.
    ///
    /// Health-probes `/api/health` for up to 10s before returning. Reaps
    /// stale orphan pidfiles from prior runs first.
    pub async fn spawn() -> Result<Self> {
        // Best-effort reap of leftover orphans from prior runs.
        let _ = crate::eval::orphan_reaper::cleanup_eval_orphans();

        let data_dir = tempfile::tempdir().context("create tmp data_dir")?;
        let data_dir_path = data_dir.path().to_path_buf();
        let port_file = data_dir.path().join("port");

        let binary = resolve_server_binary()?;
        let mut cmd = Command::new(&binary);
        cmd.env("ORIGIN_BIND_ADDR", "127.0.0.1:0")
            .env("ORIGIN_DATA_DIR", &data_dir_path)
            .env("ORIGIN_PORT_FILE", &port_file)
            // Eval scenarios pre-seed the DB themselves; the daemon's own
            // background LLM autoload would compete for resources and bloat
            // L2 latency vs L1.
            .env("ORIGIN_DISABLE_LLM_AUTOLOAD", "1")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            // tokio sends SIGKILL on Drop. Belt + suspenders with our
            // explicit Drop impl below.
            .kill_on_drop(true);

        let mut child = cmd
            .spawn()
            .with_context(|| format!("spawn origin-server at {}", binary.display()))?;
        let pid = child.id().context("spawned daemon has no PID")?;

        // Pidfile for orphan reaper. Format: `<pid>\n<data_dir>`.
        let pid_dir = dirs::home_dir()
            .map(|h| h.join(".cache/origin-eval/daemons"))
            .ok_or_else(|| anyhow::anyhow!("HOME not set"))?;
        std::fs::create_dir_all(&pid_dir)?;
        let pidfile = pid_dir.join(format!("{}.pid", pid));
        std::fs::write(&pidfile, format!("{}\n{}", pid, data_dir_path.display()))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("spawn returned no stdout pipe"))?;

        let port = match Self::wait_for_port(&port_file, stdout).await {
            Ok(p) => p,
            Err(e) => {
                // Spawn failed before health probe — clean up before bubbling.
                let _ = child.start_kill();
                let _ = std::fs::remove_file(&pidfile);
                return Err(e);
            }
        };

        // Health probe — 10s budget for cold-boot indexing.
        let url = format!("http://127.0.0.1:{}/api/health", port);
        let client = reqwest::Client::new();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match client.get(&url).send().await {
                Ok(r) if r.status().is_success() => break,
                _ if Instant::now() > deadline => {
                    let _ = child.start_kill();
                    let _ = std::fs::remove_file(&pidfile);
                    anyhow::bail!("daemon failed /api/health probe within 10s");
                }
                _ => tokio::time::sleep(Duration::from_millis(100)).await,
            }
        }

        Ok(DaemonHandle {
            child,
            pidfile,
            port,
            _data_dir: data_dir,
            data_dir_path,
        })
    }

    /// Block until the daemon publishes its bound port. Two channels:
    /// (1) stdout `ORIGIN_LISTENING_ON=<addr>` line, (2) the file at
    /// `port_file`. First channel to deliver wins; both have a 30s deadline.
    async fn wait_for_port(
        port_file: &Path,
        stdout: tokio::process::ChildStdout,
    ) -> Result<u16> {
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        loop {
            if Instant::now() > deadline {
                anyhow::bail!("timed out waiting for daemon port");
            }
            // Cheap file probe first — likely already written if daemon is fast.
            if let Ok(contents) = std::fs::read_to_string(port_file) {
                if let Ok(port) = contents.trim().parse::<u16>() {
                    return Ok(port);
                }
            }
            // Race a line-read with a 100ms poll so we don't block forever
            // if the daemon picks the port-file channel exclusively.
            line.clear();
            tokio::select! {
                read = reader.read_line(&mut line) => {
                    match read {
                        Ok(0) => anyhow::bail!("daemon stdout closed before port announce"),
                        Ok(_) => {
                            if let Some(addr) = line.trim_end().strip_prefix("ORIGIN_LISTENING_ON=") {
                                let port_str = addr.rsplit(':').next().unwrap_or("");
                                return port_str.parse().context("parse port from stdout");
                            }
                            // Unrelated log line — keep going.
                        }
                        Err(e) => anyhow::bail!("stdout read error: {}", e),
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(100)) => {}
            }
        }
    }

    /// Build a `http://127.0.0.1:<port><path>` URL for an HTTP call.
    pub fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{}", self.port, path)
    }

    /// Path to the on-disk pidfile written by `spawn`. Exposed for tests
    /// that want to assert the orphan reaper handshake.
    pub fn pidfile(&self) -> &Path {
        &self.pidfile
    }
}

impl Drop for DaemonHandle {
    fn drop(&mut self) {
        // 1) Send SIGTERM → 1s grace → SIGKILL. The kill_on_drop on Command
        //    would SIGKILL immediately; this is explicit so the daemon can
        //    flush WAL on exit. Tokio's kill_on_drop still acts as the final
        //    safety net if start_kill / signal delivery fail.
        if let Some(pid) = self.child.id() {
            #[cfg(unix)]
            unsafe {
                if libc::kill(pid as i32, libc::SIGTERM) == 0 {
                    std::thread::sleep(Duration::from_secs(1));
                    libc::kill(pid as i32, libc::SIGKILL);
                }
            }
            #[cfg(not(unix))]
            {
                let _ = pid;
                let _ = self.child.start_kill();
            }
        }

        // 2) Remove pidfile. If the file is already gone (e.g. reaper ran
        //    concurrently) silently ignore the error — pidfile cleanup is
        //    best-effort.
        if self.pidfile.exists() {
            if let Err(e) = std::fs::remove_file(&self.pidfile) {
                log::warn!(
                    "DaemonHandle: failed to remove pidfile {}: {}",
                    self.pidfile.display(),
                    e
                );
            }
        }
        // 3) TempDir Drop reaps the data_dir on its own. If it fails
        //    (Windows file-in-use, etc.), the orphan reaper's next sweep
        //    picks it up via the pidfile data_dir line — but since we just
        //    removed the pidfile above, that's a one-shot leak. For now
        //    acceptable; revisit if Windows tests show leaks.
    }
}

/// Resolve the path to `origin-server`. Honors `ORIGIN_SERVER_BIN` env var
/// for explicit operator override; otherwise derives from `current_exe()`
/// (test binary lives at `<workspace>/target/<profile>/deps/<x>`, so two
/// `parent()` calls put us at `<workspace>/target/<profile>/`).
///
/// Returns an error if the resolved path doesn't exist, with a hint to run
/// `cargo build -p origin-server` first.
fn resolve_server_binary() -> Result<PathBuf> {
    if let Ok(explicit) = std::env::var("ORIGIN_SERVER_BIN") {
        let p = PathBuf::from(&explicit);
        if !p.exists() {
            anyhow::bail!(
                "ORIGIN_SERVER_BIN={} does not exist; build origin-server first",
                explicit
            );
        }
        return Ok(p);
    }
    let current = std::env::current_exe().context("current_exe()")?;
    let target_dir = current
        .parent()
        .and_then(|p| p.parent())
        .ok_or_else(|| anyhow::anyhow!("current_exe path has no grandparent: {:?}", current))?;
    let bin_name = if cfg!(windows) {
        "origin-server.exe"
    } else {
        "origin-server"
    };
    let candidate = target_dir.join(bin_name);
    if !candidate.exists() {
        anyhow::bail!(
            "origin-server binary not found at {} — run `cargo build -p origin-server` first \
             (or set ORIGIN_SERVER_BIN)",
            candidate.display()
        );
    }
    Ok(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_server_binary_honors_explicit_env_override() {
        let tmp = tempfile::tempdir().unwrap();
        let fake_bin = tmp.path().join("origin-server");
        std::fs::write(&fake_bin, "").unwrap();
        let prev = std::env::var("ORIGIN_SERVER_BIN").ok();
        std::env::set_var("ORIGIN_SERVER_BIN", &fake_bin);
        let resolved = resolve_server_binary().unwrap();
        assert_eq!(resolved, fake_bin);
        match prev {
            Some(v) => std::env::set_var("ORIGIN_SERVER_BIN", v),
            None => std::env::remove_var("ORIGIN_SERVER_BIN"),
        }
    }

    #[test]
    fn resolve_server_binary_rejects_missing_explicit_path() {
        let prev = std::env::var("ORIGIN_SERVER_BIN").ok();
        std::env::set_var("ORIGIN_SERVER_BIN", "/definitely/does/not/exist/origin-server");
        let err = resolve_server_binary().unwrap_err();
        assert!(err.to_string().contains("does not exist"), "got: {}", err);
        match prev {
            Some(v) => std::env::set_var("ORIGIN_SERVER_BIN", v),
            None => std::env::remove_var("ORIGIN_SERVER_BIN"),
        }
    }
}
