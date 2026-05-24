// SPDX-License-Identifier: Apache-2.0
//! Configure Origin MCP in supported clients.

use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Subcommand, ValueEnum};
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const SERVER_NAME: &str = "origin";
const FALLBACK_SERVER_COMMAND: &str = "npx";
const FALLBACK_SERVER_ARGS: [&str; 2] = ["-y", "origin-mcp"];

#[derive(Clone, Debug, PartialEq, Eq)]
struct ServerCommand {
    command: String,
    args: Vec<String>,
}

#[derive(Subcommand)]
pub enum McpCommand {
    /// Add Origin MCP to a supported client.
    Add(AddArgs),
}

#[derive(Args)]
pub struct AddArgs {
    /// Client to configure.
    #[arg(value_enum)]
    client: McpClient,
    /// Print the command or file edit without changing anything.
    #[arg(long)]
    dry_run: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum McpClient {
    /// Claude Code without the Origin plugin.
    ClaudeCode,
    /// OpenAI Codex CLI.
    Codex,
    /// Gemini CLI.
    Gemini,
    /// Cursor editor.
    Cursor,
    /// Claude Desktop.
    ClaudeDesktop,
    /// VS Code workspace MCP config.
    #[value(name = "vscode")]
    Vscode,
}

pub fn run(command: McpCommand, quiet: bool) -> Result<()> {
    match command {
        McpCommand::Add(args) => add(args, quiet),
    }
}

fn add(args: AddArgs, quiet: bool) -> Result<()> {
    let server = server_command();
    match args.client {
        McpClient::ClaudeCode => add_native(
            "claude-code",
            "claude",
            native_args("mcp", &["add", "-s", "user", SERVER_NAME, "--"], &server),
            args.dry_run,
            quiet,
            Some(claude_code_tools_only_note()),
        ),
        McpClient::Codex => add_native(
            "codex",
            "codex",
            native_args("mcp", &["add", SERVER_NAME, "--"], &server),
            args.dry_run,
            quiet,
            None,
        ),
        McpClient::Gemini => add_native(
            "gemini",
            "gemini",
            native_args("mcp", &["add", "-s", "user", SERVER_NAME], &server),
            args.dry_run,
            quiet,
            None,
        ),
        McpClient::Cursor => add_json_config(
            "cursor",
            home_path(&[".cursor", "mcp.json"])?,
            "mcpServers",
            &server,
            args.dry_run,
            quiet,
        ),
        McpClient::ClaudeDesktop => add_json_config(
            "claude-desktop",
            home_path(&[
                "Library",
                "Application Support",
                "Claude",
                "claude_desktop_config.json",
            ])?,
            "mcpServers",
            &server,
            args.dry_run,
            quiet,
        ),
        McpClient::Vscode => {
            let path = std::env::current_dir()
                .context("determine current directory")?
                .join(".vscode")
                .join("mcp.json");
            add_json_config("vscode", path, "servers", &server, args.dry_run, quiet)
        }
    }
}

fn add_native(
    client: &str,
    binary: &str,
    add_args: Vec<String>,
    dry_run: bool,
    quiet: bool,
    note: Option<&str>,
) -> Result<()> {
    if dry_run {
        println!("Would run:");
        println!("  {} {}", binary, add_args.join(" "));
        if let Some(note) = note {
            println!();
            println!("{note}");
        }
        return Ok(());
    }

    run_external(binary, &add_args)?;

    if !quiet {
        println!("Configured Origin MCP for {client}.");
        if let Some(note) = note {
            println!("{note}");
        }
    }
    Ok(())
}

fn run_external(binary: &str, args: &[String]) -> Result<()> {
    // Resolve via PATHEXT so .cmd / .bat shims on Windows match. CreateProcess
    // only auto-appends `.exe`, which would miss a fake `claude.cmd` in tests
    // and also any real Windows shell-script wrappers the user installed.
    let resolved = which::which(binary)
        .with_context(|| format!("could not find `{binary}`. Is it installed and on PATH?"))?;
    let status = Command::new(&resolved)
        .args(args)
        .status()
        .with_context(|| format!("could not run `{binary}`. Is it installed and on PATH?"))?;

    if !status.success() {
        bail!(
            "`{} {}` failed with status {}",
            binary,
            args.join(" "),
            status
        );
    }

    Ok(())
}

fn add_json_config(
    client: &str,
    path: PathBuf,
    section_name: &str,
    server: &ServerCommand,
    dry_run: bool,
    quiet: bool,
) -> Result<()> {
    let server = server_json(server);
    let mut config = read_json_config(&path)?;
    let changed = upsert_server(&mut config, section_name, server)?;

    if !changed {
        if !quiet {
            println!(
                "Origin MCP already configured for {client} at {}.",
                path.display()
            );
        }
        return Ok(());
    }

    if dry_run {
        println!(
            "Would set `{section_name}.{SERVER_NAME}` in {}:",
            path.display()
        );
        println!(
            "{}",
            serde_json::to_string_pretty(&config[section_name][SERVER_NAME])?
        );
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create config directory {}", parent.display()))?;
    }

    let backup = if path.exists() {
        Some(backup_file(&path)?)
    } else {
        None
    };

    write_json_atomic(&path, &config)?;

    if !quiet {
        println!("Updated {} for Origin MCP.", path.display());
        if let Some(backup) = backup {
            println!("Backup: {}", backup.display());
        }
    }

    Ok(())
}

fn read_json_config(path: &Path) -> Result<Value> {
    if !path.exists() {
        return Ok(json!({}));
    }

    let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("invalid JSON in {}", path.display()))
}

fn upsert_server(config: &mut Value, section_name: &str, server: Value) -> Result<bool> {
    let root = config
        .as_object_mut()
        .ok_or_else(|| anyhow!("MCP config root must be a JSON object"))?;

    let section = root
        .entry(section_name.to_string())
        .or_insert_with(|| json!({}));

    let servers = section
        .as_object_mut()
        .ok_or_else(|| anyhow!("`{section_name}` must be a JSON object"))?;

    if servers.get(SERVER_NAME) == Some(&server) {
        return Ok(false);
    }

    servers.insert(SERVER_NAME.to_string(), server);
    Ok(true)
}

fn server_json(server: &ServerCommand) -> Value {
    let mut value = json!({
        "command": server.command,
    });
    if !server.args.is_empty() {
        value["args"] = json!(server.args);
    }
    value
}

fn backup_file(path: &Path) -> Result<PathBuf> {
    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow!("config path has no file name: {}", path.display()))?
        .to_string_lossy();
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock before UNIX epoch")?
        .as_millis();
    let backup = path.with_file_name(format!("{file_name}.bak.{stamp}.{}", std::process::id()));
    fs::copy(path, &backup)
        .with_context(|| format!("write backup {} from {}", backup.display(), path.display()))?;
    Ok(backup)
}

fn write_json_atomic(path: &Path, value: &Value) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("config path has no parent: {}", path.display()))?;
    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow!("config path has no file name: {}", path.display()))?
        .to_string_lossy();
    let tmp = parent.join(format!(".{file_name}.tmp.{}", std::process::id()));
    let body = format!("{}\n", serde_json::to_string_pretty(value)?);
    fs::write(&tmp, body).with_context(|| format!("write temp config {}", tmp.display()))?;
    fs::rename(&tmp, path)
        .with_context(|| format!("replace {} with {}", path.display(), tmp.display()))?;
    Ok(())
}

fn home_path(parts: &[&str]) -> Result<PathBuf> {
    // Prefer the HOME/USERPROFILE env vars before falling back to the OS
    // resolver. `dirs::home_dir()` on Windows calls SHGetKnownFolderPath
    // directly and ignores USERPROFILE, which breaks integration tests
    // that build an isolated home dir. Production users have USERPROFILE
    // set to the same value the API returns, so the priority swap is a
    // no-op for them.
    let mut path = std::env::var_os("HOME")
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var_os("USERPROFILE").filter(|s| !s.is_empty()))
        .map(PathBuf::from)
        .or_else(dirs::home_dir)
        .ok_or_else(|| anyhow!("could not determine home directory"))?;
    for part in parts {
        path.push(part);
    }
    Ok(path)
}

fn claude_code_tools_only_note() -> &'static str {
    "Claude Code MCP tools only: remember, recall, context, doctor, and related Origin tools. \
This does not install Origin plugin skills like /brief, /handoff, /distill, or /init."
}

fn server_command() -> ServerCommand {
    if let Some(path) = sibling_origin_mcp() {
        return ServerCommand {
            command: path.display().to_string(),
            args: Vec::new(),
        };
    }

    ServerCommand {
        command: FALLBACK_SERVER_COMMAND.to_string(),
        args: FALLBACK_SERVER_ARGS
            .iter()
            .map(|arg| (*arg).to_string())
            .collect(),
    }
}

fn sibling_origin_mcp() -> Option<PathBuf> {
    let exe = env::current_exe().ok()?;
    let dir = exe.parent()?;
    let candidate = dir.join("origin-mcp");
    candidate.is_file().then_some(candidate)
}

fn native_args(prefix: &str, args: &[&str], server: &ServerCommand) -> Vec<String> {
    let mut out = Vec::with_capacity(1 + args.len() + 1 + server.args.len());
    out.push(prefix.to_string());
    out.extend(args.iter().map(|arg| (*arg).to_string()));
    out.push(server.command.clone());
    out.extend(server.args.iter().cloned());
    out
}
