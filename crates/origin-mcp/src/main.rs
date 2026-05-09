use std::net::IpAddr;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use origin_mcp::client::{discover_origin_url, OriginClient};
use origin_mcp::tools::{OriginMcpServer, TransportMode};
use origin_mcp::{auth, serve, token};
use rmcp::{transport::stdio, ServiceExt};

#[derive(Parser)]
#[command(
    name = "origin-mcp",
    about = "MCP server for Origin — personal agent memory layer",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Origin server URL (e.g. http://127.0.0.1:7878). Auto-discovers if not set.
    #[arg(long, global = true)]
    origin_url: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start Streamable HTTP MCP server for remote clients (claude.ai, ChatGPT)
    Serve(ServeArgs),
    /// Manage bearer tokens for authentication
    Token(TokenArgs),
}

#[derive(Parser)]
struct ServeArgs {
    /// Port to listen on
    #[arg(long, default_value = "8080")]
    port: u16,

    /// Host to bind to
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Bearer token for authentication
    #[arg(long)]
    token: Option<String>,

    /// Path to file containing the bearer token
    #[arg(long)]
    token_file: Option<PathBuf>,

    /// Disable authentication (only allowed on loopback)
    #[arg(long)]
    no_auth: bool,

    /// Agent name for source_agent on writes
    #[arg(long, default_value = "remote-mcp")]
    agent_name: String,

    /// User ID for multi-user/cross-device support (optional)
    #[arg(long)]
    user_id: Option<String>,

    /// Comma-separated list of allowed Origin headers
    #[arg(long, default_value = "https://claude.ai,https://chatgpt.com")]
    allowed_origins: String,
}

#[derive(Parser)]
struct TokenArgs {
    #[command(subcommand)]
    action: TokenAction,
}

#[derive(Subcommand)]
enum TokenAction {
    /// Generate a new bearer token
    Generate {
        /// Output file path
        #[arg(long, default_value = "~/.config/origin-mcp/token")]
        output: String,
    },
}

fn default_token_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("origin-mcp")
        .join("token")
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("origin_mcp=info".parse()?),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let cli = Cli::parse();

    match cli.command {
        None => run_stdio(cli.origin_url).await,
        Some(Commands::Serve(args)) => run_serve(args, cli.origin_url).await,
        Some(Commands::Token(args)) => run_token(args),
    }
}

async fn run_stdio(origin_url: Option<String>) -> anyhow::Result<()> {
    let base_url = discover_origin_url(origin_url);
    tracing::info!("Connecting to Origin at {}", base_url);

    let client = OriginClient::new(base_url);

    if let Some(msg) = client.version_handshake().await {
        tracing::warn!("{msg}");
        eprintln!("Warning: {msg}");
    }

    tokio::spawn(async {
        if let Some(msg) = origin_mcp::self_update_check::check().await {
            tracing::info!("{msg}");
            eprintln!("Note: {msg}");
        }
    });

    let server = OriginMcpServer::new(client, TransportMode::Stdio, "claude-code".into(), None);
    let service = server
        .serve(stdio())
        .await
        .inspect_err(|e| tracing::error!("Failed to start MCP server: {}", e))?;

    tracing::info!("origin-mcp server running on stdio");
    service.waiting().await?;
    Ok(())
}

async fn run_serve(args: ServeArgs, origin_url: Option<String>) -> anyhow::Result<()> {
    let resolved_token = resolve_token(&args)?;

    if resolved_token.is_none() && !args.no_auth {
        anyhow::bail!(
            "Authentication required. Use --token, --token-file, or --no-auth.\n\
             Generate a token with: origin-mcp token generate"
        );
    }
    if args.no_auth {
        let host_addr: IpAddr = args
            .host
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid host address: {}", args.host))?;
        if !auth::is_loopback(&host_addr) {
            anyhow::bail!(
                "--no-auth is only allowed on loopback addresses (127.0.0.1 or ::1), not {}",
                args.host
            );
        }
    }

    let base_url = discover_origin_url(origin_url);
    tracing::info!("Connecting to Origin at {}", base_url);

    if let Some(msg) = OriginClient::new(base_url.clone())
        .version_handshake()
        .await
    {
        tracing::warn!("{msg}");
        eprintln!("Warning: {msg}");
    }

    tokio::spawn(async {
        if let Some(msg) = origin_mcp::self_update_check::check().await {
            tracing::info!("{msg}");
            eprintln!("Note: {msg}");
        }
    });

    let allowed_origins: Vec<String> = args
        .allowed_origins
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    let config = serve::ServeConfig {
        port: args.port,
        host: args.host,
        origin_url: base_url,
        token: resolved_token,
        agent_name: args.agent_name,
        user_id: args.user_id,
        allowed_origins,
    };

    serve::run_serve(config).await
}

fn resolve_token(args: &ServeArgs) -> anyhow::Result<Option<String>> {
    if let Some(ref t) = args.token {
        return Ok(Some(t.clone()));
    }
    if let Some(ref path) = args.token_file {
        let t = token::read_token(path)?;
        return Ok(Some(t));
    }
    if args.no_auth {
        return Ok(None);
    }
    let default_path = default_token_path();
    if default_path.exists() {
        let t = token::read_token(&default_path)?;
        tracing::info!("Using token from {}", default_path.display());
        return Ok(Some(t));
    }
    Ok(None)
}

fn run_token(args: TokenArgs) -> anyhow::Result<()> {
    match args.action {
        TokenAction::Generate { output } => {
            let path = if let Some(rest) = output.strip_prefix("~/") {
                let home = dirs::home_dir()
                    .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
                home.join(rest)
            } else {
                PathBuf::from(&output)
            };

            let new_token = token::generate_token();
            token::write_token(&path, &new_token)?;

            eprintln!("Token saved to {}", path.display());
            eprintln!("Token: {}", new_token);
            eprintln!();
            eprintln!("Use with: origin-mcp serve --token-file {}", path.display());
            Ok(())
        }
    }
}
