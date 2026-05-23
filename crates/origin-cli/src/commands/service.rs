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
    let mut m = <dyn ServiceManager>::native().context("detect native service manager")?;
    // launchd and systemd-user both support user-level; Windows SCM does not.
    // We try user-level first and silently fall back to system-level on platforms
    // that reject it. The caller may need admin/elevation on Windows in that case.
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

pub fn install() -> Result<()> {
    let label_value = label()?;
    let program = current_server_path()?;
    let m = manager()?;

    m.install(ServiceInstallCtx {
        label: label_value.clone(),
        program,
        args: vec![],
        contents: None,
        username: None,
        working_directory: None,
        environment: None,
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
