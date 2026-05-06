// SPDX-License-Identifier: Apache-2.0
mod client;
mod commands;
mod output;

use clap::{Parser, Subcommand};
use output::OutputFormat;

#[derive(Parser)]
#[command(
    name = "origin",
    version,
    about = "Origin CLI — talk to the local Origin daemon"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Output format. Auto-detects JSON when piped, table on TTY.
    #[arg(long, value_enum, default_value_t = OutputFormat::Auto, global = true)]
    format: OutputFormat,

    /// Suppress all non-error output. Useful for scripts.
    #[arg(long, short, global = true)]
    quiet: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Show daemon health + version.
    Status,
    /// Search memories by query (vector + FTS hybrid).
    Search {
        /// Search query.
        query: String,
        /// Max results (default 10).
        #[arg(short, long, default_value_t = 10)]
        limit: usize,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let client = client::OriginClient::from_env();
    // Resolve Auto once based on stdout TTY state. Subcommands receive Json or Table only.
    let format = cli.format.resolve();
    match cli.command {
        Commands::Status => commands::status::run(&client, format, cli.quiet).await?,
        Commands::Search { query, limit } => {
            commands::search::run(&client, format, cli.quiet, query, limit).await?
        }
    }
    Ok(())
}
