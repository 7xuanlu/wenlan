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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let client = client::OriginClient::from_env();
    match cli.command {
        Commands::Status => commands::status::run(&client, cli.format, cli.quiet).await?,
    }
    Ok(())
}
