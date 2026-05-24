// SPDX-License-Identifier: Apache-2.0
//! `origin space list/add/default/move/show` — manage memory spaces.

use anyhow::Result;
use clap::Subcommand;

use crate::client::OriginClient;
use crate::output::OutputFormat;

fn read_default_from_toml() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let path = std::path::PathBuf::from(home).join(".origin/spaces.toml");
    let body = std::fs::read_to_string(&path).ok()?;
    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("default") {
            let rest = rest.trim_start();
            if let Some(rest) = rest.strip_prefix('=') {
                let val = rest.trim().trim_matches('"').to_string();
                if !val.is_empty() {
                    return Some(val);
                }
            }
        }
    }
    None
}

#[derive(Subcommand)]
pub enum SpaceCmd {
    /// List all registered spaces.
    List,
    /// Register a new space.
    Add {
        /// Space name (e.g. "career", "health", "ideas").
        name: String,
        /// Also set this space as the default.
        #[arg(long)]
        default: bool,
    },
    /// Get or set the default space.
    Default {
        /// Space name to set as default. Omit to print the current default.
        name: Option<String>,
    },
    /// Bulk-reassign all memories from one space to another.
    Move {
        /// Source space.
        from: String,
        /// Destination space.
        to: String,
    },
    /// Show detail for a space — memory count, page count, last activity.
    Show {
        /// Space name.
        name: String,
    },
}

pub async fn run(
    client: &OriginClient,
    format: OutputFormat,
    quiet: bool,
    cmd: SpaceCmd,
) -> Result<()> {
    match cmd {
        SpaceCmd::List => list(client, format, quiet).await,
        SpaceCmd::Add { name, default } => add(client, format, quiet, &name, default).await,
        SpaceCmd::Default { name } => default_cmd(client, format, quiet, name.as_deref()).await,
        SpaceCmd::Move { from, to } => move_cmd(client, format, quiet, &from, &to).await,
        SpaceCmd::Show { name } => show(client, format, quiet, &name).await,
    }
}

async fn list(client: &OriginClient, format: OutputFormat, quiet: bool) -> Result<()> {
    let spaces = client.list_spaces().await?;
    let default = read_default_from_toml();
    if quiet {
        return Ok(());
    }
    match format {
        OutputFormat::Json => crate::output::print_json(&spaces)?,
        OutputFormat::Table => {
            if spaces.is_empty() {
                println!("(no spaces registered)");
                return Ok(());
            }
            println!(
                "{:<20} {:<10} {:<10} {:<8}",
                "NAME", "MEMORIES", "ENTITIES", "DEFAULT?"
            );
            for s in &spaces {
                let is_default = default.as_deref() == Some(s.name.as_str());
                println!(
                    "{:<20} {:<10} {:<10} {:<8}",
                    s.name,
                    s.memory_count,
                    s.entity_count,
                    if is_default { "yes" } else { "" }
                );
            }
        }
        OutputFormat::Auto => unreachable!("Auto resolved by main before dispatch"),
    }
    Ok(())
}
async fn add(_c: &OriginClient, _f: OutputFormat, _q: bool, _n: &str, _d: bool) -> Result<()> {
    todo!("Task 3")
}
async fn default_cmd(
    _c: &OriginClient,
    _f: OutputFormat,
    _q: bool,
    _n: Option<&str>,
) -> Result<()> {
    todo!("Task 4")
}
async fn move_cmd(
    _c: &OriginClient,
    _f: OutputFormat,
    _q: bool,
    _f2: &str,
    _t: &str,
) -> Result<()> {
    todo!("Task 5")
}
async fn show(_c: &OriginClient, _f: OutputFormat, _q: bool, _n: &str) -> Result<()> {
    todo!("Task 6")
}
