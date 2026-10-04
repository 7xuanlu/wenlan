// SPDX-License-Identifier: Apache-2.0
//! `wenlan sources add <path> [--type directory|okf]` — register a source and
//! sync it now.
//!
//! Thin HTTP client: POST /api/sources to register the path as a Directory or
//! OKF bundle source (idempotent — an already-registered path is treated as
//! success), then POST /api/sources/{id}/sync, then render the returned stats.
//! An OKF bundle lands in the Space sent by the client's Space header, else
//! the default Space. No DB access, no new endpoints (per AGENTS.md crate
//! boundaries).

use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::client::{SyncStats, WenlanClient};
use crate::output::{print_json, ResolvedFormat};

#[derive(clap::Subcommand)]
pub enum SourcesCommand {
    /// Add a folder or file source and sync it now.
    Add {
        /// Path to a directory or file to add as a source.
        path: PathBuf,
        /// Kind of source. `okf` imports an OKF bundle (for example an
        /// OpenWiki folder) into one Space; pass --space to choose it.
        #[arg(long = "type", value_enum, default_value_t = SourceKind::Directory)]
        source_type: SourceKind,
    },
}

/// The source kinds `sources add` registers.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    /// A folder or single file of documents.
    Directory,
    /// An OKF knowledge bundle.
    Okf,
}

impl SourceKind {
    fn as_str(self) -> &'static str {
        match self {
            SourceKind::Directory => "directory",
            SourceKind::Okf => "okf",
        }
    }
}

pub async fn run_sources(
    client: &WenlanClient,
    format: ResolvedFormat,
    quiet: bool,
    command: SourcesCommand,
) -> Result<()> {
    match command {
        SourcesCommand::Add { path, source_type } => {
            run(client, format, quiet, path, source_type).await
        }
    }
}

pub async fn run(
    client: &WenlanClient,
    format: ResolvedFormat,
    quiet: bool,
    path: PathBuf,
    source_type: SourceKind,
) -> Result<()> {
    // Resolve to an absolute, canonical path so the daemon stores a stable key
    // and an idempotent re-run matches the existing registration.
    let abs = std::fs::canonicalize(&path)
        .with_context(|| format!("resolving path: {}", path.display()))?;
    let abs_str = abs.to_string_lossy().to_string();

    let id = register_source(client, source_type, &abs_str).await?;
    let stats = client.sync_source(&id).await?;

    if quiet {
        return Ok(());
    }
    match format {
        ResolvedFormat::Json => print_json(&stats)?,
        ResolvedFormat::Table => print!("{}", format_stats(&id, &stats)),
    }
    Ok(())
}

/// Register `abs_path` as a source of `source_type`, idempotently, returning
/// its id.
///
/// The happy path is a single POST /api/sources. If the path is already
/// registered the POST fails (the daemon rejects a duplicate), so we recover
/// the existing id from the source list instead of erroring. A genuine failure
/// (path missing, reserved root, daemon down) surfaces the original POST error
/// because the list will not contain the path.
async fn register_source(
    client: &WenlanClient,
    source_type: SourceKind,
    abs_path: &str,
) -> Result<String> {
    match client.add_source(source_type.as_str(), abs_path).await {
        Ok(source) => Ok(source.id),
        Err(add_err) => {
            let existing = client.list_sources().await.ok().and_then(|sources| {
                sources
                    .into_iter()
                    .find(|s| s.path.to_string_lossy() == abs_path)
            });
            match existing {
                Some(s) => Ok(s.id),
                None => Err(add_err),
            }
        }
    }
}

fn format_stats(id: &str, stats: &SyncStats) -> String {
    let mut out = format!(
        "Synced {}: {} file(s) found, {} ingested, {} skipped, {} error(s)\n",
        id, stats.files_found, stats.ingested, stats.skipped, stats.errors,
    );
    if let Some(detail) = &stats.error_detail {
        out.push_str(&format!("  error detail: {}\n", detail));
    }
    if let Some(paused) = &stats.paused {
        out.push_str(&format!("  enrichment paused: {}\n", paused));
    }
    if let (Some(queued), Some(waiting)) = (stats.queued_files, stats.waiting_files) {
        out.push_str(&format!(
            "  {} file(s) queued for preparation, {} waiting for the next batch\n",
            queued, waiting
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(found: usize, ingested: usize, skipped: usize, errors: usize) -> SyncStats {
        SyncStats {
            files_found: found,
            ingested,
            skipped,
            errors,
            error_detail: None,
            paused: None,
            queued_files: None,
            waiting_files: None,
        }
    }

    #[test]
    fn format_stats_renders_counts() {
        let out = format_stats("directory-notes", &stats(3, 2, 1, 0));
        assert!(out.contains("directory-notes"));
        assert!(out.contains("3 file(s) found"));
        assert!(out.contains("2 ingested"));
        assert!(out.contains("1 skipped"));
        assert!(out.contains("0 error(s)"));
    }

    #[test]
    fn format_stats_surfaces_error_detail_and_pause() {
        let s = SyncStats {
            error_detail: Some("file_read_errors".to_string()),
            paused: Some("llm backoff".to_string()),
            ..stats(4, 1, 0, 3)
        };
        let out = format_stats("directory-docs", &s);
        assert!(out.contains("file_read_errors"));
        assert!(out.contains("llm backoff"));
        assert!(!out.contains("queued"), "no batch line for a folder: {out}");
    }

    #[test]
    fn format_stats_renders_okf_batch_position() {
        let s = SyncStats {
            queued_files: Some(1000),
            waiting_files: Some(200),
            ..stats(1200, 1000, 0, 0)
        };
        let out = format_stats("okf-wiki", &s);
        assert!(out.contains("1000 file(s) queued for preparation, 200 waiting"));
    }
}
