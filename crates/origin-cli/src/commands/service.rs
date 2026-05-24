// SPDX-License-Identifier: Apache-2.0
//! Cross-platform service registration for the Origin daemon.
//!
//! Wraps the `service-manager` crate to register `origin-server` with the
//! host's native service manager (launchd, systemd-user, Windows SCM via winsw).

use anyhow::{Context, Result};
use service_manager::{
    ServiceInstallCtx, ServiceLabel, ServiceLevel, ServiceManager, ServiceStartCtx, ServiceStopCtx,
    ServiceUninstallCtx,
};
use std::path::{Path, PathBuf};

use crate::client::origin_host_from_env;

pub const SERVICE_LABEL: &str = "com.origin.server";

fn label() -> Result<ServiceLabel> {
    SERVICE_LABEL.parse().context("invalid service label")
}

fn manager() -> Result<Box<dyn ServiceManager>> {
    // Windows note: `<dyn ServiceManager>::native()` returns `ScServiceManager`
    // (sc.exe) on Windows. sc.exe requires Administrator privileges. We do NOT
    // try to use winsw: service-manager 0.11's WinSwServiceManager invokes
    // `winsw install <name>.xml` which winsw v2 does not understand (v2 expects
    // rename-pattern config next to its executable). Users on Windows need to
    // run `origin install` from an elevated terminal.
    let mut m = <dyn ServiceManager>::native().context("detect native service manager")?;
    // launchd and systemd-user both support user-level. Windows sc.exe does not
    // and silently keeps system-level after this call. macOS + Linux benefit.
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
        // origin-server is a plain console app; it does not speak the Windows
        // Service Control Protocol (no SetServiceStatus / control handler).
        // sc.exe install succeeds but `sc start` times out at 30s (error 1053:
        // "service did not respond"). Until origin-server is wrapped with the
        // `windows-service` crate or via Task Scheduler, the `install`
        // subcommand is not supported on Windows.
        anyhow::bail!(
            "`origin install` is not yet supported on Windows.\n\
             Run the daemon manually instead:\n\
             \n  Set-Location <install-dir>\n  $env:ORIGIN_BIND_ADDR = \"127.0.0.1:7878\"\n  Start-Process .\\origin-server.exe -WindowStyle Hidden\n\
             \n\
             To auto-start at logon, register a per-user Task Scheduler task:\n\
             \n  schtasks /create /tn OriginServer /sc onlogon /tr \"<install-dir>\\origin-server.exe\"\n\
             \n\
             Tracked: cross-platform Windows service support."
        );
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
        // `origin install` is gated off on Windows; `uninstall` is a no-op for
        // the matching reason. If a previous build left a stale sc.exe entry,
        // remove it manually: `sc.exe delete com.origin.server`.
        anyhow::bail!(
            "`origin uninstall` is not yet supported on Windows. \
             If a stale service exists: sc.exe delete com.origin.server"
        );
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
        // `sc.exe query <label>` exits 0 when the service is registered with
        // the Windows Service Control Manager, 1060 when it is not. We don't
        // need admin rights for a read-only query.
        std::process::Command::new("sc.exe")
            .args(["query", SERVICE_LABEL])
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
            println!("Service: {} (registered with sc.exe)", SERVICE_LABEL);
        } else {
            println!("Service: {} (not installed)", SERVICE_LABEL);
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
