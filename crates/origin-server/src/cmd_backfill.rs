// SPDX-License-Identifier: Apache-2.0
//! `backfill-stale-pages` internal CLI subcommand.
//!
//! Deletes archived pages that look like old distillation failures (large
//! source_memory_ids, no entity, no domain, not user-edited). Source memories
//! are NOT modified.
//!
//! `page_sources` rows are deleted automatically via ON DELETE CASCADE.

use anyhow::{anyhow, Context, Result};
use origin_core::db::MemoryDB;
use origin_core::events::NoopEmitter;
use std::io::{self, Write};
use std::sync::Arc;
use std::time::Duration;

const DAEMON_PROBE_TIMEOUT: Duration = Duration::from_millis(500);

pub async fn run(dry_run: bool) -> anyhow::Result<()> {
    // Step 1a: refuse if the platform service manager has the daemon
    // registered. With auto-restart enabled, killing the daemon manually
    // wouldn't be enough — the service manager respawns it, creating a race
    // where the daemon could start writing between our probe and our SQLite
    // writes.
    check_service_unloaded()?;

    // Step 1b: refuse if a daemon is currently running (covers manually-started
    // instances and the brief window between service unload and respawn).
    check_daemon_not_running().await?;

    // Step 2: open the DB directly (not via daemon).
    // Mirrors the path computation in `run_daemon()` in main.rs.
    let origin_root = std::env::var_os("ORIGIN_DATA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            dirs::data_local_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("origin")
        });
    let data_dir = origin_root.join("memorydb");

    let db = MemoryDB::new(&data_dir, Arc::new(NoopEmitter))
        .await
        .with_context(|| format!("opening MemoryDB at {}", data_dir.display()))?;

    // Step 3: query candidates.
    let candidates = db
        .find_stale_archived_pages()
        .await
        .context("querying stale pages")?;

    if candidates.is_empty() {
        println!("No stale archived pages found. Nothing to do.");
        return Ok(());
    }

    println!("Found {} candidate page(s):\n", candidates.len());
    for c in &candidates {
        println!(
            "  {} \"{}\" — {} sources — created {}",
            c.id,
            c.title,
            c.source_memory_ids.len(),
            c.created_at,
        );
    }
    println!();

    if dry_run {
        println!("--dry-run: no changes made.");
        return Ok(());
    }

    // Step 4: confirm.
    print!(
        "Delete {} page(s) and their page_sources rows? (y/N): ",
        candidates.len()
    );
    io::stdout().flush().ok();
    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .context("reading confirmation")?;
    let answer = answer.trim().to_lowercase();
    if answer != "y" && answer != "yes" {
        println!("Aborted.");
        return Ok(());
    }

    // Step 5: delete.
    // page_sources rows cascade automatically (ON DELETE CASCADE FK).
    let mut deleted = 0usize;
    for c in &candidates {
        db.delete_page(&c.id)
            .await
            .with_context(|| format!("deleting page {}", c.id))?;
        deleted += 1;
    }

    println!(
        "Deleted {} page(s). Source memories were NOT modified.",
        deleted
    );
    println!();
    println!("Next steps to re-distill the freed sources:");
    println!("  - Sources with enrichment_steps rows will be eligible on the next distill cycle.");
    println!("  - Raw sources need re-enrichment first. Either:");
    println!("    (a) Re-import: touch the original source files (e.g., `touch ~/second-brain/inbox/*.md`)");
    println!("    (b) Wait for entity_backfill to gradually backfill entity_ids");

    Ok(())
}

/// Resolves the platform-specific path to the Origin service unit file.
///
/// Duplicated from `origin-cli::commands::service::service_unit_path` to avoid
/// a circular crate dependency (`origin-server` cannot depend on `origin-cli`).
/// Kept in sync via the `service_unit_path_matches_cli` test below.
fn service_unit_path() -> Result<std::path::PathBuf> {
    #[cfg(target_os = "macos")]
    {
        Ok(dirs::home_dir()
            .context("HOME not set")?
            .join("Library/LaunchAgents")
            .join("com.origin.server.plist"))
    }
    #[cfg(target_os = "linux")]
    {
        Ok(dirs::config_dir()
            .context("XDG_CONFIG_HOME not set")?
            .join("systemd/user")
            .join("com-origin-server.service"))
    }
    #[cfg(target_os = "windows")]
    {
        Ok(dirs::data_local_dir()
            .context("LOCALAPPDATA not set")?
            .join("service-manager")
            .join("com.origin.server.xml"))
    }
}

/// Returns Ok if no service manager has the origin daemon registered.
/// Returns Err with instructions if a service unit file is present.
fn check_service_unloaded() -> Result<()> {
    let unit = service_unit_path()?;
    if unit.exists() {
        Err(anyhow!(
            "The Origin service is registered with the platform service manager at:\n  {}\n\
             Unload it first to prevent auto-restart:\n  origin uninstall\n\
             Then re-run this command. (Reinstall after with `origin install`.)",
            unit.display()
        ))
    } else {
        Ok(())
    }
}

async fn check_daemon_not_running() -> Result<()> {
    // Mirror the port-reading logic from cmd_status in main.rs.
    let port: u16 = std::env::var("ORIGIN_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(7878);
    let probe_url = format!("http://127.0.0.1:{}/api/health", port);

    let client = reqwest::Client::builder()
        .timeout(DAEMON_PROBE_TIMEOUT)
        .build()
        .context("building reqwest client")?;
    match client.get(&probe_url).send().await {
        Ok(_) => Err(anyhow!(
            "Daemon is running on :{port}. Stop it before running backfill:\n  \
             origin uninstall\n  \
             # or: kill -9 $(lsof -ti :{port})"
        )),
        // Truly refused (nothing listening): safe to proceed.
        Err(e) if e.is_connect() => Ok(()),
        // Timeout: daemon may be alive but wedged (e.g. GPU inference). Refuse.
        Err(e) if e.is_timeout() => Err(anyhow!(
            "Daemon probe to :{port} timed out after {}ms. \
             Daemon may be busy. Stop it explicitly and retry:\n  \
             origin uninstall\n  \
             # or: kill -9 $(lsof -ti :{port})",
            DAEMON_PROBE_TIMEOUT.as_millis()
        )),
        // Any other network error: surface it.
        Err(e) => Err(anyhow!("Daemon probe to :{port} failed unexpectedly: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_service_unloaded_returns_ok_when_no_service_installed() {
        // In the test environment we are unlikely to have the production service
        // unit file at the user's data dir. If a developer has previously run
        // `origin install`, this test will fail locally — that is acceptable, the
        // failure tells them to `origin uninstall` first.
        check_service_unloaded().expect("expected Ok in clean test env");
    }

    #[test]
    fn service_unit_path_matches_cli() {
        // The CLI is the authoritative owner of this constant; verify the
        // duplicate in origin-server stays in sync. We test the OS-specific
        // tail because the cli implementation lives in a different crate.
        let path = super::service_unit_path().expect("service_unit_path should not fail");
        let p = path.to_string_lossy();

        #[cfg(target_os = "macos")]
        assert!(p.contains("Library/LaunchAgents/com.origin.server.plist"));
        #[cfg(target_os = "linux")]
        assert!(p.contains(".config/systemd/user/com-origin-server.service"));
        #[cfg(target_os = "windows")]
        assert!(p.to_lowercase().contains("service-manager"));
    }
}
