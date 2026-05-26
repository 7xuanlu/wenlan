// SPDX-License-Identifier: Apache-2.0
//! Spawn `origin-server` in a tmp data dir on an ephemeral port for L2 eval.
//!
//! `origin-core` lives in a different crate from `origin-server`, so the
//! `CARGO_BIN_EXE_origin-server` env var that Cargo sets for same-crate
//! integration tests is **not** available here. Binary path resolution is
//! therefore runtime, not compile-time:
//!
//!   1. `ORIGIN_SERVER_BIN` env var (explicit operator override).
//!   2. Standard cargo layout: `<current_exe>/../../origin-server`
//!      (deps dir → target/<profile>/origin-server).
//!   3. cargo-nextest layout: `target/nextest/<profile>/<pkg>/<x>` and
//!      `target/<profile>/origin-server` siblings, probed in order.
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

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("spawn returned no stdout pipe"))?;

        // Wait for port BEFORE writing pidfile — if the daemon never binds
        // (missing libs, port conflict, panic during startup), no orphan
        // pidfile lingers referencing a never-started PID.
        let port = match Self::wait_for_port(&port_file, stdout).await {
            Ok(p) => p,
            Err(e) => {
                let _ = child.start_kill();
                return Err(e);
            }
        };

        // Now that the daemon is bound + responsive, register it for the
        // orphan reaper. Format: `<pid>\n<data_dir>`.
        let pid_dir = dirs::home_dir()
            .map(|h| h.join(".cache/origin-eval/daemons"))
            .ok_or_else(|| anyhow::anyhow!("HOME not set"))?;
        std::fs::create_dir_all(&pid_dir)?;
        let pidfile = pid_dir.join(format!("{}.pid", pid));
        std::fs::write(&pidfile, format!("{}\n{}", pid, data_dir_path.display()))?;

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
    async fn wait_for_port(port_file: &Path, stdout: tokio::process::ChildStdout) -> Result<u16> {
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
                    if port != 0 {
                        return Ok(port);
                    }
                    // Port 0 means the file was caught mid-write or the
                    // daemon hasn't replaced its placeholder yet. Keep
                    // looping rather than handing port 0 (which would mean
                    // "let the OS pick", but for a client URL is a bug).
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
                                let port: u16 = port_str.parse().context("parse port from stdout")?;
                                if port == 0 {
                                    anyhow::bail!("daemon announced port 0 on stdout");
                                }
                                return Ok(port);
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
        // 1) Immediate SIGKILL via `start_kill` (same signal `kill_on_drop`
        //    would send). No SIGTERM-grace + sleep — Drop runs on whatever
        //    thread holds us, often a tokio worker (test runtime is
        //    `current_thread`), and a 1s `std::thread::sleep` there would
        //    stall the executor. The daemon writes only to an ephemeral
        //    `TempDir` that the next field-drop reaps, so ungraceful
        //    teardown loses no useful state.
        let _ = self.child.start_kill();

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
/// for explicit operator override; otherwise probes a small set of
/// well-known target layouts:
///
///   1. `<current_exe>/../../origin-server` — standard `cargo test` layout
///      where the integration-test binary lives at
///      `target/<profile>/deps/<x>`.
///   2. `<current_exe>/../../../../<profile>/origin-server` — cargo-nextest
///      layout, where binaries live at `target/nextest/<profile>/<pkg>/<x>`.
///
/// Returns an error listing every probed path if none exist, with a hint to
/// run `cargo build -p origin-server` first.
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
    let bin_name = if cfg!(windows) {
        "origin-server.exe"
    } else {
        "origin-server"
    };

    let mut tried: Vec<PathBuf> = Vec::new();

    // Candidate 1: cargo test default — `target/<profile>/deps/<x>` → ../..
    if let Some(p) = current
        .parent()
        .and_then(|p| p.parent())
        .map(|d| d.join(bin_name))
    {
        if p.exists() {
            return Ok(p);
        }
        tried.push(p);
    }

    // Candidate 2: cargo-nextest — `target/nextest/<profile>/<pkg>/<x>` →
    // walk up to `<profile>/`, then to the canonical bin at `target/<profile>/`.
    if let Some(profile_dir) = current
        .parent() // .../<pkg>/
        .and_then(|p| p.parent())
    // .../<profile>/
    {
        let nextest_sibling = profile_dir.join(bin_name);
        if nextest_sibling.exists() {
            return Ok(nextest_sibling);
        }
        tried.push(nextest_sibling);
        if let Some(profile_name) = profile_dir.file_name() {
            // .../target/nextest/<profile>/<pkg>/<x>
            //   → grandparent of profile_dir is `target/nextest/`,
            //     so go up one more for `target/`.
            if let Some(target_root) = profile_dir.parent().and_then(|p| p.parent()) {
                let canonical = target_root.join(profile_name).join(bin_name);
                if canonical.exists() {
                    return Ok(canonical);
                }
                tried.push(canonical);
            }
        }
    }

    anyhow::bail!(
        "origin-server binary not found. Tried: {:?}. \
         Run `cargo build -p origin-server` first, or set ORIGIN_SERVER_BIN to the binary path.",
        tried
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// `resolve_server_binary` reads + the tests below set `ORIGIN_SERVER_BIN`
    /// — a process-global. Cargo runs these tests in parallel by default,
    /// so we lock here to keep the save/set/restore atomic.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn resolve_server_binary_honors_explicit_env_override() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
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
        let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let prev = std::env::var("ORIGIN_SERVER_BIN").ok();
        std::env::set_var(
            "ORIGIN_SERVER_BIN",
            "/definitely/does/not/exist/origin-server",
        );
        let err = resolve_server_binary().unwrap_err();
        assert!(err.to_string().contains("does not exist"), "got: {}", err);
        match prev {
            Some(v) => std::env::set_var("ORIGIN_SERVER_BIN", v),
            None => std::env::remove_var("ORIGIN_SERVER_BIN"),
        }
    }
}
