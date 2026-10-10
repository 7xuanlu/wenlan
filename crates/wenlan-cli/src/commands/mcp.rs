// SPDX-License-Identifier: Apache-2.0
//! Connect Wenlan MCP to supported clients.

use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, ValueEnum};
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const SERVER_NAME: &str = "wenlan";
const FALLBACK_SERVER_COMMAND: &str = "npx";
const FALLBACK_SERVER_ARGS: [&str; 2] = ["-y", "wenlan-mcp"];

#[derive(Clone, Debug, PartialEq, Eq)]
struct ServerCommand {
    command: String,
    args: Vec<String>,
}

#[derive(Args)]
pub struct ConnectArgs {
    /// Client to configure.
    #[arg(value_enum)]
    pub client: McpClient,
    /// Print the command or file edit without changing anything.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum McpClient {
    /// Claude Code without the Wenlan plugin.
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

pub fn run_connect(args: ConnectArgs, quiet: bool) -> Result<()> {
    add(args, quiet)
}

fn add(args: ConnectArgs, quiet: bool) -> Result<()> {
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
            claude_desktop_config_file()?,
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
        println!("Configured Wenlan MCP for {client}.");
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
                "Wenlan MCP already configured for {client} at {}.",
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
        println!("Updated {} for Wenlan MCP.", path.display());
        if let Some(backup) = backup {
            println!("Backup: {}", backup.display());
        }
        if let Some(hint) = restart_hint(client) {
            println!("{hint}");
        }
    }

    Ok(())
}

/// What to do next once the config file is written. These clients read their
/// MCP servers at startup, so a running copy will not see the new entry.
fn restart_hint(client: &str) -> Option<&'static str> {
    match client {
        "claude-desktop" => Some("Fully quit and reopen Claude Desktop to load Wenlan."),
        "cursor" => Some("Restart Cursor to load Wenlan."),
        "vscode" => Some("Reload the VS Code window to load Wenlan."),
        _ => None,
    }
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

fn home_dir_path() -> Result<PathBuf> {
    // Prefer the HOME/USERPROFILE env vars before falling back to the OS
    // resolver. `dirs::home_dir()` on Windows calls SHGetKnownFolderPath
    // directly and ignores USERPROFILE, which breaks integration tests
    // that build an isolated home dir. Production users have USERPROFILE
    // set to the same value the API returns, so the priority swap is a
    // no-op for them.
    std::env::var_os("HOME")
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var_os("USERPROFILE").filter(|s| !s.is_empty()))
        .map(PathBuf::from)
        .or_else(dirs::home_dir)
        .ok_or_else(|| anyhow!("could not determine home directory"))
}

fn home_path(parts: &[&str]) -> Result<PathBuf> {
    let mut path = home_dir_path()?;
    for part in parts {
        path.push(part);
    }
    Ok(path)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HostOs {
    MacOs,
    Windows,
    Linux,
}

impl HostOs {
    fn current() -> Self {
        match env::consts::OS {
            "macos" => Self::MacOs,
            "windows" => Self::Windows,
            _ => Self::Linux,
        }
    }
}

/// An environment variable as an absolute path. Empty and relative values are
/// treated as unset: both `%APPDATA%` and `$XDG_CONFIG_HOME` are defined as
/// absolute, and a relative one would resolve against the current directory.
fn absolute_env_path(key: &str) -> Option<PathBuf> {
    env::var_os(key)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

/// Where Claude Desktop keeps `claude_desktop_config.json` on `os`:
/// macOS `~/Library/Application Support/Claude`, Windows `%APPDATA%\Claude`
/// (default `~\AppData\Roaming`), Linux `$XDG_CONFIG_HOME/Claude` (default
/// `~/.config`). Pure so every OS's answer is unit-testable from any host.
fn claude_desktop_config_path(
    os: HostOs,
    home: &Path,
    appdata: Option<&Path>,
    xdg_config_home: Option<&Path>,
) -> PathBuf {
    let config_dir = match os {
        HostOs::MacOs => home.join("Library").join("Application Support"),
        HostOs::Windows => appdata
            .map(Path::to_path_buf)
            .unwrap_or_else(|| home.join("AppData").join("Roaming")),
        HostOs::Linux => xdg_config_home
            .map(Path::to_path_buf)
            .unwrap_or_else(|| home.join(".config")),
    };
    config_dir.join("Claude").join("claude_desktop_config.json")
}

fn claude_desktop_config_file() -> Result<PathBuf> {
    Ok(claude_desktop_config_path(
        HostOs::current(),
        &home_dir_path()?,
        absolute_env_path("APPDATA").as_deref(),
        absolute_env_path("XDG_CONFIG_HOME").as_deref(),
    ))
}

fn claude_code_tools_only_note() -> &'static str {
    "Claude Code MCP tools only: capture, recall, context, doctor, and related Wenlan tools. \
This does not install Wenlan plugin skills like /brief, /handoff, /distill, or /setup."
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

/// File names `wenlan-mcp` can have next to the CLI: with the platform's
/// executable suffix first (`wenlan-mcp.exe` on Windows, where the release
/// archive ships it that way), then the bare name.
fn sibling_mcp_names(exe_suffix: &str) -> Vec<String> {
    let mut names = vec![format!("wenlan-mcp{exe_suffix}")];
    if !exe_suffix.is_empty() {
        names.push("wenlan-mcp".to_string());
    }
    names
}

fn sibling_origin_mcp() -> Option<PathBuf> {
    let exe = env::current_exe().ok()?;
    let dir = exe.parent()?;
    sibling_mcp_names(env::consts::EXE_SUFFIX)
        .into_iter()
        .map(|name| dir.join(name))
        .find(|candidate| candidate.is_file())
}

fn native_args(prefix: &str, args: &[&str], server: &ServerCommand) -> Vec<String> {
    let mut out = Vec::with_capacity(1 + args.len() + 1 + server.args.len());
    out.push(prefix.to_string());
    out.extend(args.iter().map(|arg| (*arg).to_string()));
    out.push(server.command.clone());
    out.extend(server.args.iter().cloned());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(path: &str) -> PathBuf {
        PathBuf::from(path)
    }

    #[test]
    fn claude_desktop_path_on_macos_is_under_library_application_support() {
        // APPDATA / XDG_CONFIG_HOME are meaningless on macOS and must not
        // redirect the file.
        let path = claude_desktop_config_path(
            HostOs::MacOs,
            Path::new("/Users/u"),
            Some(Path::new("/ignored/appdata")),
            Some(Path::new("/ignored/xdg")),
        );
        assert_eq!(
            path,
            p("/Users/u/Library/Application Support/Claude/claude_desktop_config.json")
        );
    }

    #[test]
    fn claude_desktop_path_on_windows_uses_appdata() {
        let path = claude_desktop_config_path(
            HostOs::Windows,
            Path::new("/users/u"),
            Some(Path::new("/roaming")),
            None,
        );
        assert_eq!(path, p("/roaming/Claude/claude_desktop_config.json"));
    }

    #[test]
    fn claude_desktop_path_on_windows_defaults_to_roaming_under_home() {
        let path = claude_desktop_config_path(HostOs::Windows, Path::new("/users/u"), None, None);
        assert_eq!(
            path,
            p("/users/u/AppData/Roaming/Claude/claude_desktop_config.json")
        );
    }

    #[test]
    fn claude_desktop_path_on_linux_honors_xdg_config_home() {
        let path = claude_desktop_config_path(
            HostOs::Linux,
            Path::new("/home/u"),
            Some(Path::new("/ignored/appdata")),
            Some(Path::new("/xdg")),
        );
        assert_eq!(path, p("/xdg/Claude/claude_desktop_config.json"));
    }

    #[test]
    fn claude_desktop_path_on_linux_defaults_to_dot_config() {
        let path = claude_desktop_config_path(HostOs::Linux, Path::new("/home/u"), None, None);
        assert_eq!(path, p("/home/u/.config/Claude/claude_desktop_config.json"));
    }

    #[test]
    fn host_os_current_matches_the_compile_target() {
        let expected = match std::env::consts::OS {
            "macos" => HostOs::MacOs,
            "windows" => HostOs::Windows,
            _ => HostOs::Linux,
        };
        assert_eq!(HostOs::current(), expected);
    }

    #[test]
    fn sibling_mcp_prefers_the_exe_suffix_then_the_bare_name() {
        assert_eq!(
            sibling_mcp_names(".exe"),
            vec!["wenlan-mcp.exe".to_string(), "wenlan-mcp".to_string()]
        );
        assert_eq!(sibling_mcp_names(""), vec!["wenlan-mcp".to_string()]);
    }

    #[test]
    fn restart_hints_name_the_client_and_skip_native_clients() {
        assert!(restart_hint("claude-desktop")
            .expect("claude desktop hint")
            .contains("Claude Desktop"));
        assert!(restart_hint("cursor")
            .expect("cursor hint")
            .contains("Cursor"));
        assert!(restart_hint("vscode")
            .expect("vscode hint")
            .contains("VS Code"));
        assert_eq!(restart_hint("claude-code"), None);
        assert_eq!(restart_hint("codex"), None);
        assert_eq!(restart_hint("gemini"), None);
    }
}
