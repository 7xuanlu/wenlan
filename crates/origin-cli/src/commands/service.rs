// SPDX-License-Identifier: Apache-2.0
//! Cross-platform service registration for the Origin daemon.
//!
//! - macOS: launchd LaunchAgent via the `service-manager` crate.
//! - Linux: systemd --user unit via the `service-manager` crate.
//! - Windows: per-user ONLOGON Task Scheduler entry via `schtasks.exe`.
//!   We bypass `service-manager`'s `ScServiceManager` because origin-server
//!   is a plain console app and does not implement the Windows Service
//!   Control Protocol (`sc start` would time out at 30s with error 1053).

use anyhow::{Context, Result};
use service_manager::{
    ServiceInstallCtx, ServiceLabel, ServiceLevel, ServiceManager, ServiceStartCtx, ServiceStopCtx,
    ServiceUninstallCtx,
};
use std::path::{Path, PathBuf};

use crate::client::origin_host_from_env;

pub const SERVICE_LABEL: &str = "com.origin.server";

/// Windows Task Scheduler does not love dots in task names. The macOS launchd
/// and systemd-user paths still use the canonical reverse-DNS `SERVICE_LABEL`.
#[cfg(target_os = "windows")]
pub const WINDOWS_TASK_NAME: &str = "OriginServer";

fn label() -> Result<ServiceLabel> {
    SERVICE_LABEL.parse().context("invalid service label")
}

#[cfg(target_os = "windows")]
fn run_schtasks(args: &[&str], action: &str) -> Result<std::process::Output> {
    let output = std::process::Command::new("schtasks.exe")
        .args(args)
        .output()
        .with_context(|| format!("spawn schtasks.exe ({action})"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        anyhow::bail!(
            "schtasks.exe {} failed (exit {}): {}{}",
            action,
            output.status.code().unwrap_or(-1),
            stderr.trim(),
            if stdout.trim().is_empty() {
                String::new()
            } else {
                format!("\nstdout: {}", stdout.trim())
            }
        );
    }
    Ok(output)
}

fn manager() -> Result<Box<dyn ServiceManager>> {
    // macOS + Linux only. Windows install/uninstall short-circuit before
    // calling this and drive schtasks.exe directly (see install/uninstall).
    let mut m = <dyn ServiceManager>::native().context("detect native service manager")?;
    let _ = m.set_level(ServiceLevel::User);
    Ok(m)
}

/// Resolves the platform-specific path to the Origin service unit file.
///
/// Mirrors the on-disk path that `service-manager` 0.11 actually writes:
/// - macOS (launchd): `~/Library/LaunchAgents/<qualified_name>.plist`
///   (`to_qualified_name()` keeps the qualifier, e.g. `com.origin.server.plist`).
/// - Linux (systemd-user): `<config_dir>/systemd/user/<script_name>.service`
///   (`ServiceLabel::to_script_name()` joins org+app with `-` and DROPS the
///   qualifier, so `com.origin.server` becomes `origin-server.service`).
/// - Windows (sc.exe): no on-disk unit file. Service state lives in the
///   Windows registry — see `is_installed()` for the probe.
#[cfg(not(target_os = "windows"))]
pub fn service_unit_path() -> Result<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        Ok(dirs::home_dir()
            .context("HOME not set")?
            .join("Library/LaunchAgents")
            .join(format!("{}.plist", SERVICE_LABEL)))
    }
    #[cfg(target_os = "linux")]
    {
        let label = label()?;
        Ok(dirs::config_dir()
            .context("XDG_CONFIG_HOME not set")?
            .join("systemd/user")
            .join(format!("{}.service", label.to_script_name())))
    }
}

fn current_server_path() -> Result<PathBuf> {
    let origin_exe = std::env::current_exe().context("cannot determine origin CLI path")?;
    let mut server = origin_exe
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("origin-server");
    if cfg!(target_os = "windows") {
        server.set_extension("exe");
    }
    if !server.exists() {
        anyhow::bail!(
            "origin-server not found next to origin at {}. Re-run the Origin installer.",
            server.display()
        );
    }
    Ok(server)
}

/// Resolves the data root the daemon will use at runtime. Mirrors
/// `crates/origin-server/src/main.rs` so launchd log paths line up with the
/// on-disk layout the daemon owns. macOS-only because launchd is the only
/// service backend that consumes pre-rendered log paths today.
#[cfg(target_os = "macos")]
fn origin_data_root() -> PathBuf {
    std::env::var_os("ORIGIN_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::data_local_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("origin")
        })
}

/// Builds a launchd plist that mirrors `service-manager`'s default output for
/// `OnFailure` restart + user-level + autostart, with the extra keys the old
/// embedded `com.origin.server.plist` template carried: `StandardOutPath`,
/// `StandardErrorPath`, and `EnvironmentVariables.RUST_LOG`.
///
/// `LaunchdInstallConfig` in service-manager 0.11 only exposes `keep_alive`;
/// stdout/stderr paths must come through `ServiceInstallCtx.contents` as a
/// pre-rendered plist string. This function is the minimal stand-in for the
/// crate's internal `make_plist`.
#[cfg(target_os = "macos")]
fn build_launchd_plist(
    program: &Path,
    stdout_path: &Path,
    stderr_path: &Path,
    rust_log: &str,
) -> String {
    let mut buf = String::new();
    buf.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    buf.push_str(
        "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n",
    );
    buf.push_str("<plist version=\"1.0\">\n<dict>\n");
    buf.push_str("\t<key>Label</key>\n");
    buf.push_str(&format!("\t<string>{}</string>\n", SERVICE_LABEL));
    buf.push_str("\t<key>ProgramArguments</key>\n");
    buf.push_str("\t<array>\n");
    buf.push_str(&format!(
        "\t\t<string>{}</string>\n",
        program.to_string_lossy()
    ));
    buf.push_str("\t</array>\n");
    // Mirrors service-manager's RestartPolicy::OnFailure rendering: KeepAlive
    // dict with SuccessfulExit=false. The matching `Disabled` key keeps the
    // service from auto-loading until start() removes it (cross-platform parity).
    buf.push_str("\t<key>KeepAlive</key>\n");
    buf.push_str("\t<dict>\n");
    buf.push_str("\t\t<key>SuccessfulExit</key>\n");
    buf.push_str("\t\t<false/>\n");
    buf.push_str("\t</dict>\n");
    buf.push_str("\t<key>RunAtLoad</key>\n\t<true/>\n");
    buf.push_str("\t<key>Disabled</key>\n\t<true/>\n");
    buf.push_str("\t<key>StandardOutPath</key>\n");
    buf.push_str(&format!(
        "\t<string>{}</string>\n",
        stdout_path.to_string_lossy()
    ));
    buf.push_str("\t<key>StandardErrorPath</key>\n");
    buf.push_str(&format!(
        "\t<string>{}</string>\n",
        stderr_path.to_string_lossy()
    ));
    buf.push_str("\t<key>EnvironmentVariables</key>\n");
    buf.push_str("\t<dict>\n");
    buf.push_str("\t\t<key>RUST_LOG</key>\n");
    buf.push_str(&format!("\t\t<string>{}</string>\n", rust_log));
    buf.push_str("\t</dict>\n");
    buf.push_str("</dict>\n</plist>\n");
    buf
}

pub fn install() -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        // origin-server is a plain console app and does not speak the Windows
        // Service Control Protocol, so sc.exe install + start would time out
        // at 30s with error 1053. Use Task Scheduler instead: register a
        // per-user ONLOGON task and trigger it immediately. Matches the
        // user-scope semantics of launchd LaunchAgent (macOS) and
        // systemd --user (Linux), without needing a service dispatcher in
        // origin-server.
        let program = current_server_path()?;
        let program_str = program.to_string_lossy();
        run_schtasks(
            &[
                "/create",
                "/tn",
                WINDOWS_TASK_NAME,
                "/sc",
                "ONLOGON",
                "/tr",
                &program_str,
                "/f",
            ],
            "create scheduled task",
        )?;
        run_schtasks(&["/run", "/tn", WINDOWS_TASK_NAME], "run scheduled task")?;
        println!(
            "Installed and started Windows scheduled task '{}' (origin-server).",
            WINDOWS_TASK_NAME
        );
        return Ok(());
    }

    #[cfg_attr(target_os = "windows", allow(unreachable_code))]
    let label_value = label()?;
    let program = current_server_path()?;
    let m = manager()?;

    // Apply RUST_LOG=info to every platform. launchd consumes
    // `EnvironmentVariables`, systemd-user consumes `Environment=`. winsw +
    // sc.exe ignore the field (Windows daemons still rely on `RUST_LOG`
    // exported in the user environment).
    let environment = Some(vec![("RUST_LOG".to_string(), "info".to_string())]);

    // macOS: hand-roll the plist so we can keep `StandardOutPath` and
    // `StandardErrorPath` parity with the legacy template. service-manager 0.11
    // has no struct field for those keys, so the only honest knob is
    // `ServiceInstallCtx.contents`.
    let contents = {
        #[cfg(target_os = "macos")]
        {
            let log_dir = origin_data_root().join("logs");
            // Best-effort: launchd creates parent dirs for log files in many
            // builds, but creating ahead of time guarantees the daemon never
            // racing the dir into existence on first start.
            let _ = std::fs::create_dir_all(&log_dir);
            let stdout_path = log_dir.join("origin-server.stdout.log");
            let stderr_path = log_dir.join("origin-server.stderr.log");
            Some(build_launchd_plist(
                &program,
                &stdout_path,
                &stderr_path,
                "info",
            ))
        }
        #[cfg(not(target_os = "macos"))]
        {
            None
        }
    };

    m.install(ServiceInstallCtx {
        label: label_value.clone(),
        program,
        args: vec![],
        contents,
        username: None,
        working_directory: None,
        environment,
        autostart: true,
        restart_policy: service_manager::RestartPolicy::OnFailure {
            delay_secs: None,
            max_retries: None,
            reset_after_secs: None,
        },
    })
    .context("install service")?;

    m.start(ServiceStartCtx { label: label_value })
        .context("start service")?;
    println!("Installed and started {}.", SERVICE_LABEL);
    Ok(())
}

pub fn uninstall() -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        // /end returns nonzero if the task is not currently running; that
        // is not an error worth surfacing, so swallow the exit code.
        let _ = std::process::Command::new("schtasks.exe")
            .args(["/end", "/tn", WINDOWS_TASK_NAME])
            .output();
        run_schtasks(
            &["/delete", "/tn", WINDOWS_TASK_NAME, "/f"],
            "delete scheduled task",
        )?;
        println!(
            "Uninstalled Windows scheduled task '{}'.",
            WINDOWS_TASK_NAME
        );
        return Ok(());
    }

    #[cfg_attr(target_os = "windows", allow(unreachable_code))]
    let label_value = label()?;
    let m = manager()?;
    let _ = m.stop(ServiceStopCtx {
        label: label_value.clone(),
    });
    m.uninstall(ServiceUninstallCtx { label: label_value })
        .context("uninstall service")?;
    println!("Uninstalled {}.", SERVICE_LABEL);
    Ok(())
}

pub fn is_installed() -> bool {
    #[cfg(target_os = "windows")]
    {
        // `schtasks /query /tn <name>` exits 0 when the task exists, 1 when
        // it does not. No admin rights needed for the read-only query.
        std::process::Command::new("schtasks.exe")
            .args(["/query", "/tn", WINDOWS_TASK_NAME])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    #[cfg(not(target_os = "windows"))]
    {
        service_unit_path().map(|p| p.exists()).unwrap_or(false)
    }
}

pub async fn print_status() -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        if is_installed() {
            println!(
                "Service: scheduled task '{}' (registered)",
                WINDOWS_TASK_NAME
            );
        } else {
            println!(
                "Service: scheduled task '{}' (not installed)",
                WINDOWS_TASK_NAME
            );
        }
    }
    #[cfg(not(target_os = "windows"))]
    match service_unit_path() {
        Ok(path) if path.exists() => println!("Service unit: {} (installed)", path.display()),
        Ok(path) => println!("Service unit: {} (not installed)", path.display()),
        Err(e) => println!("Service unit: unable to resolve ({})", e),
    }

    let url = format!("{}/api/health", origin_host_from_env());
    match reqwest::get(&url).await {
        Ok(resp) if resp.status().is_success() => {
            let body = resp.text().await.unwrap_or_default();
            println!("Health: ok ({})", url);
            println!("{}", body);
        }
        Ok(resp) => {
            println!("Health: unhealthy (status {})", resp.status());
        }
        Err(e) => {
            println!("Health: not reachable ({})", e);
        }
    }

    Ok(())
}
