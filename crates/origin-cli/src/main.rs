// SPDX-License-Identifier: Apache-2.0
use clap::{Parser, Subcommand};

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

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum OutputFormat {
    Auto,
    Json,
    Table,
}

#[derive(Subcommand)]
enum Commands {
    /// Show daemon health + version (stub — wired in later task).
    Status,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Status => {
            println!(
                "origin-cli v0.3.0 — status subcommand stub (Task 3 wires the real implementation)"
            );
            Ok(())
        }
    }
}
