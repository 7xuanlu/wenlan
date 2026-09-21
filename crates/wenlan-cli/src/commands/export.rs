// SPDX-License-Identifier: Apache-2.0
//! `wenlan export okf <DIR>` — write pages as a pure OKF v0.2 bundle.

use anyhow::Result;
use clap::Subcommand;
use std::path::{Path, PathBuf};

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

/// Expand a leading `~` component against `home` (`~` or `~/x`; a `~`
/// anywhere else is left alone). Pure for testability: the caller supplies
/// `dirs::home_dir()`.
fn expand_tilde(dir: &Path, home: Option<PathBuf>) -> Result<PathBuf> {
    let mut components = dir.components();
    let leading_tilde = matches!(
        components.next(),
        Some(std::path::Component::Normal(first)) if first == "~"
    );
    if !leading_tilde {
        return Ok(dir.to_path_buf());
    }
    let home = home.ok_or_else(|| {
        anyhow::anyhow!("cannot expand ~ in export dir: home directory is unknown")
    })?;
    Ok(home.join(components.as_path()))
}

async fn run_okf(
    client: &WenlanClient,
    format: ResolvedFormat,
    quiet: bool,
    dir: &Path,
) -> Result<()> {
    // A relative DIR always means "from here": expand `~` first, then
    // absolutize against the CLI's own cwd (the daemon only sees the result).
    let expanded = expand_tilde(dir, dirs::home_dir())?;
    let absolute = std::path::absolute(&expanded)?;
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
        anyhow::bail!(
            "okf export reported {} path(s) it could not write or sweep; see the daemon log",
            stats.failed
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::expand_tilde;
    use std::path::{Path, PathBuf};

    #[test]
    fn tilde_expands_only_as_the_leading_component() {
        let home = Some(PathBuf::from("/home/u"));
        assert_eq!(
            expand_tilde(Path::new("~"), home.clone()).unwrap(),
            PathBuf::from("/home/u")
        );
        assert_eq!(
            expand_tilde(Path::new("~/x"), home.clone()).unwrap(),
            PathBuf::from("/home/u/x")
        );
        assert_eq!(
            expand_tilde(Path::new("x/~"), home.clone()).unwrap(),
            PathBuf::from("x/~")
        );
        assert_eq!(
            expand_tilde(Path::new("relative/dir"), home.clone()).unwrap(),
            PathBuf::from("relative/dir")
        );
        assert!(expand_tilde(Path::new("~/x"), None).is_err());
        assert!(expand_tilde(Path::new("~"), None).is_err());
    }
}
