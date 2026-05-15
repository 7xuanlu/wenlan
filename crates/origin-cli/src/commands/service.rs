// SPDX-License-Identifier: Apache-2.0
//! LaunchAgent management for the local Origin daemon.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::client::origin_host_from_env;

pub(crate) const PLIST_LABEL: &str = "com.origin.server";
const PLIST_TEMPLATE: &str =
    include_str!("../../../origin-server/resources/com.origin.server.plist");

pub fn plist_path() -> PathBuf {
    dirs::home_dir()
        .expect("HOME not set")
        .join("Library/LaunchAgents")
        .join(format!("{}.plist", PLIST_LABEL))
}

fn log_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("origin")
        .join("logs")
}

pub(crate) fn sibling_server_path_for_origin(origin_exe: &Path) -> PathBuf {
    origin_exe
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("origin-server")
}

fn current_server_path() -> Result<PathBuf> {
    let origin_exe = std::env::current_exe().context("cannot determine origin CLI path")?;
    let server = sibling_server_path_for_origin(&origin_exe);
    if !server.exists() {
        anyhow::bail!(
            "origin-server not found next to origin at {}. Re-run the Origin installer.",
            server.display()
        );
    }
    Ok(server)
}

pub(crate) fn plist_content(server_path: &Path, log_path: &Path) -> String {
    PLIST_TEMPLATE
        .replace("__ORIGIN_SERVER_PATH__", &server_path.to_string_lossy())
        .replace("__LOG_PATH__", &log_path.to_string_lossy())
}

pub fn install() -> Result<()> {
    let plist = plist_path();
    let log_path = log_dir();
    let server_path = current_server_path()?;

    std::fs::create_dir_all(&log_path)?;

    if let Some(parent) = plist.parent() {
        std::fs::create_dir_all(parent)?;
    }

    if plist.exists() {
        let _ = std::process::Command::new("launchctl")
            .arg("unload")
            .arg(&plist)
            .output();
    }

    std::fs::write(&plist, plist_content(&server_path, &log_path))?;
    println!("Wrote {}", plist.display());

    let output = std::process::Command::new("launchctl")
        .arg("load")
        .arg(&plist)
        .output()?;

    if output.status.success() {
        println!(
            "Loaded {} - daemon will start automatically on login",
            PLIST_LABEL
        );
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("launchctl load failed: {}", stderr);
    }

    Ok(())
}

pub fn uninstall() -> Result<()> {
    let plist = plist_path();

    if !plist.exists() {
        println!("{} is not installed", PLIST_LABEL);
        return Ok(());
    }

    let output = std::process::Command::new("launchctl")
        .arg("unload")
        .arg(&plist)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("launchctl unload warning: {}", stderr);
    }

    std::fs::remove_file(&plist)?;
    println!(
        "Removed {} - daemon will no longer auto-start",
        plist.display()
    );

    Ok(())
}

pub async fn print_status() -> Result<()> {
    let plist = plist_path();

    if plist.exists() {
        println!("Plist: {} (installed)", plist.display());
    } else {
        println!("Plist: not installed");
    }

    let output = std::process::Command::new("launchctl")
        .arg("list")
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let registered = stdout.lines().any(|line| line.contains(PLIST_LABEL));
    println!(
        "Launchd: {}",
        if registered {
            "registered"
        } else {
            "not registered"
        }
    );

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_origin_server_next_to_origin_cli() {
        let origin = Path::new("/tmp/origin/bin/origin");
        assert_eq!(
            sibling_server_path_for_origin(origin),
            PathBuf::from("/tmp/origin/bin/origin-server")
        );
    }

    #[test]
    fn plist_points_launchd_at_origin_server() {
        let server = Path::new("/tmp/origin/bin/origin-server");
        let log_path = Path::new("/tmp/origin/logs");
        let content = plist_content(server, log_path);

        assert!(content.contains("/tmp/origin/bin/origin-server"));
        assert!(!content.contains("/tmp/origin/bin/origin</string>"));
        assert!(content.contains("/tmp/origin/logs"));
    }
}
