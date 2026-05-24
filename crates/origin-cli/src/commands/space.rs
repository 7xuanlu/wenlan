// SPDX-License-Identifier: Apache-2.0
//! `origin space list/add/default/move/show` — manage memory spaces.

use anyhow::Result;
use clap::Subcommand;

use crate::client::OriginClient;
use crate::output::OutputFormat;

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

async fn list(_c: &OriginClient, _f: OutputFormat, _q: bool) -> Result<()> {
    todo!("Task 2")
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
