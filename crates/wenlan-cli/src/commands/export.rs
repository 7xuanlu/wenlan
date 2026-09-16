// SPDX-License-Identifier: Apache-2.0
//! `wenlan export okf <DIR>` — write pages as a pure OKF v0.2 bundle.

use anyhow::Result;
use clap::Subcommand;
use std::path::PathBuf;

use crate::client::WenlanClient;
use crate::output::{print_json, ResolvedFormat};

#[derive(Subcommand)]
pub enum ExportCommand {
    /// Export pages as a pure OKF v0.2 bundle (index.md, pages/, sources/).
    /// Pass --space to limit the bundle to one Space.
    Okf {
        /// Target directory. Created when missing; otherwise it must be
        /// empty or hold a previous Wenlan OKF export.
        dir: PathBuf,
    },
}

pub async fn run(
    client: &WenlanClient,
    format: ResolvedFormat,
    quiet: bool,
    command: ExportCommand,
) -> Result<()> {
    match command {
        ExportCommand::Okf { dir } => run_okf(client, format, quiet, &dir).await,
    }
}

async fn run_okf(
    client: &WenlanClient,
    format: ResolvedFormat,
    quiet: bool,
    dir: &PathBuf,
) -> Result<()> {
    // The daemon resolves its own `~/`, so the CLI absolutizes against its
    // own cwd first: a relative DIR always means "from here".
    let absolute = std::path::absolute(dir)?;
    let stats = client
        .export_pages_okf(absolute.to_string_lossy().into_owned())
        .await?;
    if !quiet {
        match format {
            ResolvedFormat::Json => print_json(&stats)?,
            ResolvedFormat::Table => println!(
                "Exported {} pages to {} (skipped {}, failed {})",
                stats.exported,
                absolute.display(),
                stats.skipped,
                stats.failed
            ),
        }
    }
    if stats.failed > 0 {
        anyhow::bail!("okf export reported {} failed page(s)", stats.failed);
    }
    Ok(())
}
