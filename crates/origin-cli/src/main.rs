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
    /// Recall the working memory bundle for a query.
    Recall {
        /// Query to recall context for.
        query: String,
    },
    /// Store a memory. Provide text positionally, or use --file, or pipe via stdin.
    Store {
        /// Content text. If omitted and --file unset, read from stdin.
        text: Option<String>,
        /// Read content from a file.
        #[arg(short, long)]
        file: Option<std::path::PathBuf>,
        /// Memory type (e.g. fact, task, decision).
        #[arg(short = 't', long = "type")]
        memory_type: Option<String>,
    },
    /// List recent memories.
    List {
        /// Max results.
        #[arg(short, long, default_value_t = 20)]
        limit: usize,
        /// Filter by memory type.
        #[arg(short = 't', long = "type")]
        memory_type: Option<String>,
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
        Commands::Recall { query } => {
            commands::recall::run(&client, format, cli.quiet, query).await?
        }
        Commands::Store {
            text,
            file,
            memory_type,
        } => commands::store::run(&client, format, cli.quiet, text, file, memory_type).await?,
        Commands::List { limit, memory_type } => {
            commands::list::run(&client, format, cli.quiet, limit, memory_type).await?
        }
    }
    Ok(())
}
