// SPDX-License-Identifier: Apache-2.0
use clap::{Parser, Subcommand};
use output::OutputFormat;
use wenlan_cli::{client, commands, output};

#[derive(Parser)]
#[command(
    name = "wenlan",
    version,
    about = "Origin CLI. Set up and use the local Origin runtime."
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
    /// Show daemon, service, model, and API key state.
    Status,
    /// Guided setup for local memory, a local model, or an Anthropic key.
    Setup {
        /// Set up without a model or API key.
        #[arg(long)]
        basic: bool,
        /// Download and select a local model, for example qwen3-4b.
        #[arg(long, value_name = "MODEL_ID")]
        model: Option<String>,
        /// Read an Anthropic key from this environment variable.
        #[arg(long = "anthropic-api-key-env", value_name = "ENV_VAR")]
        anthropic_api_key_env: Option<String>,
        /// Skip confirmation prompts where possible.
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// Install the Origin daemon as a macOS LaunchAgent.
    Install,
    /// Uninstall the Origin LaunchAgent.
    Uninstall,
    /// Restart the Origin daemon (stop then start). Required after an upgrade.
    Restart,
    /// Diagnose daemon, model, and API key setup.
    Doctor,
    /// Manage local models.
    Model {
        #[command(subcommand)]
        command: commands::setup::ModelCommand,
    },
    /// Manage provider API keys.
    Key {
        #[command(subcommand)]
        command: commands::setup::KeyCommand,
    },
    /// Configure Origin MCP for supported clients.
    Mcp {
        #[command(subcommand)]
        command: commands::mcp::McpCommand,
    },
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
    /// Manage registered agents (list / show / edit).
    Agents {
        #[command(subcommand)]
        cmd: commands::agents::AgentsCmd,
    },
    /// Manage memory spaces (list, add, default, move, show).
    Space {
        #[command(subcommand)]
        cmd: commands::space::SpaceCmd,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let client = client::WenlanClient::from_env();
    // Resolve Auto once based on stdout TTY state. Subcommands receive Json or Table only.
    let format = cli.format.resolve();
    match cli.command {
        Commands::Status => commands::status::run(&client, format, cli.quiet).await?,
        Commands::Setup {
            basic,
            model,
            anthropic_api_key_env,
            yes,
        } => {
            commands::setup::run_setup(commands::setup::SetupArgs {
                basic,
                model,
                anthropic_api_key_env,
                yes,
            })
            .await?
        }
        Commands::Install => commands::service::install()?,
        Commands::Uninstall => commands::service::uninstall()?,
        Commands::Restart => commands::service::restart()?,
        Commands::Doctor => commands::setup::run_doctor().await?,
        Commands::Model { command } => commands::setup::run_model(command).await?,
        Commands::Key { command } => commands::setup::run_key(command).await?,
        Commands::Mcp { command } => commands::mcp::run(command, cli.quiet)?,
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
        Commands::Agents { cmd } => commands::agents::run(&client, format, cli.quiet, cmd).await?,
        Commands::Space { cmd } => commands::space::run(&client, format, cli.quiet, cmd).await?,
    }
    Ok(())
}
