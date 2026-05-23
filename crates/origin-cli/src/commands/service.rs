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
        Ok(dirs::config_dir()
            .context("XDG_CONFIG_HOME not set")?
            .join("systemd/user")
            .join(format!("{}.service", SERVICE_LABEL.replace('.', "-"))))
    }
    #[cfg(target_os = "windows")]
    {
        Ok(dirs::data_local_dir()
            .context("LOCALAPPDATA not set")?
            .join("service-manager")
            .join(format!("{}.xml", SERVICE_LABEL)))
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
    service_unit_path().map(|p| p.exists()).unwrap_or(false)
}

pub async fn print_status() -> Result<()> {
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
