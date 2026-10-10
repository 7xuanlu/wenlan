// SPDX-License-Identifier: Apache-2.0
//! Connect Wenlan MCP to supported clients.

use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, ValueEnum};
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::io::{self, ErrorKind};
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
    add_json_config_with(client, path, section_name, server, dry_run, quiet, || {})
}

/// `add_json_config`, with a hook that runs after the file is read and parsed
/// and before anything is written. Production passes a no-op; a test uses it
/// to change the file in exactly the window the re-read guards.
fn add_json_config_with(
    client: &str,
    path: PathBuf,
    section_name: &str,
    server: &ServerCommand,
    dry_run: bool,
    quiet: bool,
    after_read: impl FnOnce(),
) -> Result<()> {
    let server = server_json(server);
    let ConfigFile { value, raw } = read_json_config(&path)?;
    let mut config = value;
    after_read();
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

    // Abandon the update, before a backup exists, if the file is no longer the
    // bytes this edit was built from.
    ensure_unchanged(&path, raw.as_deref())?;

    // Only a file that was READ has something to back up. Whether the file is
    // there comes from that read (`NotFound` and nothing else means absent),
    // never from `Path::exists`, which is also `false` when stat is denied.
    let backup = if raw.is_some() {
        Some(backup_file(&path)?)
    } else {
        None
    };

    write_json_atomic(&path, &config, raw.as_deref())?;

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

/// A client's config file as it was found. `raw` is the exact text that was
/// parsed, or `None` when the file is measurably absent.
struct ConfigFile {
    value: Value,
    raw: Option<String>,
}

fn read_json_config(path: &Path) -> Result<ConfigFile> {
    classify_config_read(path, fs::read_to_string(path))
}

/// `ErrorKind::NotFound` is the ONLY answer that means "no file yet". Every
/// other failure (permission, a symlink loop, a name that is not a directory)
/// means the file may well be there, so the config is NOT treated as empty:
/// writing a fresh `{}` skeleton over it would destroy the user's other
/// servers. (`Path::exists` cannot tell the two apart — it is `false` for a
/// denied stat too.)
fn classify_config_read(path: &Path, read: io::Result<String>) -> Result<ConfigFile> {
    match read {
        Ok(raw) => {
            let value = serde_json::from_str(&raw)
                .with_context(|| format!("invalid JSON in {}", path.display()))?;
            Ok(ConfigFile {
                value,
                raw: Some(raw),
            })
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(ConfigFile {
            value: json!({}),
            raw: None,
        }),
        Err(error) => bail!(
            "could not read {} ({error}), so Wenlan cannot tell what is in it; \
             nothing was changed",
            path.display()
        ),
    }
}

/// Fail unless `path` still holds exactly the bytes `expected` was parsed from
/// (`None`: still absent). The CLI's counterpart of the app's `back_up_parsed`:
/// without it a second writer's changes made while this edit was in flight are
/// silently replaced.
///
/// THE RESIDUAL: this narrows the window, it does not close it. Between this
/// check and the rename the file can still change, and closing that needs a
/// lock on another vendor's config file, which this codebase does not take.
fn ensure_unchanged(path: &Path, expected: Option<&str>) -> Result<()> {
    let unchanged = match (fs::read_to_string(path), expected) {
        (Ok(now), Some(parsed)) => now == parsed,
        (Err(error), None) if error.kind() == ErrorKind::NotFound => true,
        (Err(error), _) if error.kind() != ErrorKind::NotFound => {
            bail!(
                "could not re-read {} to confirm it had not changed ({error}); \
                 nothing was written",
                path.display()
            )
        }
        _ => false,
    };
    if !unchanged {
        bail!(
            "{} changed while Wenlan was updating it, so this update was built from \
             bytes that are no longer there; nothing was written. Try again.",
            path.display()
        );
    }
    Ok(())
}

/// Where a write to `path` must land. A config that is a symlink (a dotfiles
/// repo is the common case) is written THROUGH, so the link stays a link: the
/// temp file goes next to the real file, and renaming it over the real file
/// replaces the content the link points at instead of the link itself. A
/// dangling link resolves to the file it names.
fn resolve_write_target(path: &Path) -> Result<PathBuf> {
    const MAX_LINK_HOPS: usize = 40;
    let mut current = path.to_path_buf();
    for _ in 0..MAX_LINK_HOPS {
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                let link = fs::read_link(&current)
                    .with_context(|| format!("read symlink {}", current.display()))?;
                current = match current.parent() {
                    Some(parent) if link.is_relative() => parent.join(link),
                    _ => link,
                };
            }
            Ok(_) => return Ok(current),
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(current),
            Err(error) => bail!(
                "could not inspect {} ({error}); nothing was written",
                current.display()
            ),
        }
    }
    bail!(
        "too many levels of symbolic links at {}; nothing was written",
        path.display()
    )
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

/// Replace `path` (or the file it links to) with `value`. `expected` is what
/// the edit was parsed from; it is checked again immediately before the rename.
fn write_json_atomic(path: &Path, value: &Value, expected: Option<&str>) -> Result<()> {
    let target = resolve_write_target(path)?;
    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("config path has no parent: {}", target.display()))?;
    let file_name = target
        .file_name()
        .ok_or_else(|| anyhow!("config path has no file name: {}", target.display()))?
        .to_string_lossy();
    fs::create_dir_all(parent)
        .with_context(|| format!("create config directory {}", parent.display()))?;
    let tmp = parent.join(format!(".{file_name}.tmp.{}", std::process::id()));
    let body = format!("{}\n", serde_json::to_string_pretty(value)?);
    fs::write(&tmp, body).with_context(|| format!("write temp config {}", tmp.display()))?;

    let swapped = ensure_unchanged(path, expected).and_then(|()| {
        fs::rename(&tmp, &target)
            .with_context(|| format!("replace {} with {}", target.display(), tmp.display()))
    });
    if swapped.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    swapped
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

    // ---- Writing a client's JSON config -------------------------------------

    const OTHER_SERVERS: &str = r#"{"mcpServers":{"other":{"command":"keep-me"}}}"#;

    fn wenlan_server() -> ServerCommand {
        ServerCommand {
            command: "wenlan-mcp".to_string(),
            args: Vec::new(),
        }
    }

    fn connect(path: &Path) -> Result<()> {
        add_json_config(
            "cursor",
            path.to_path_buf(),
            "mcpServers",
            &wenlan_server(),
            false,
            true,
        )
    }

    /// Sorted entry names, so a test can say "nothing else was left behind".
    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .expect("list dir")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }

    #[test]
    fn only_not_found_means_the_config_is_absent() {
        let path = p("/home/u/.cursor/mcp.json");

        let absent = classify_config_read(&path, Err(io::Error::from(ErrorKind::NotFound)))
            .expect("not found is absent");
        assert_eq!(absent.raw, None);
        assert_eq!(absent.value, json!({}));

        for kind in [
            ErrorKind::PermissionDenied,
            ErrorKind::InvalidData,
            ErrorKind::Other,
        ] {
            let error = classify_config_read(&path, Err(io::Error::from(kind)))
                .err()
                .unwrap_or_else(|| panic!("{kind:?} must abort, not read as an empty config"));
            assert!(
                error.to_string().contains("nothing was changed"),
                "{kind:?}: {error}"
            );
        }

        let found = classify_config_read(&path, Ok(OTHER_SERVERS.to_string())).expect("parses");
        assert_eq!(found.raw.as_deref(), Some(OTHER_SERVERS));
        assert_eq!(found.value["mcpServers"]["other"]["command"], "keep-me");
    }

    #[test]
    fn connecting_keeps_the_other_servers_and_backs_up_the_original() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("mcp.json");
        fs::write(&path, OTHER_SERVERS).expect("seed");

        connect(&path).expect("connect");

        let written: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written["mcpServers"]["other"]["command"], "keep-me");
        assert_eq!(written["mcpServers"]["wenlan"]["command"], "wenlan-mcp");
        let backups: Vec<String> = names_in(dir.path())
            .into_iter()
            .filter(|name| name.starts_with("mcp.json.bak."))
            .collect();
        assert_eq!(backups.len(), 1, "{backups:?}");
        assert_eq!(
            fs::read_to_string(dir.path().join(&backups[0])).unwrap(),
            OTHER_SERVERS
        );
        assert_eq!(names_in(dir.path()).len(), 2, "no temp file left behind");
    }

    #[test]
    fn an_edit_built_from_stale_bytes_is_abandoned() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("mcp.json");
        fs::write(&path, OTHER_SERVERS).expect("seed");
        let concurrent = r#"{"mcpServers":{"other":{"command":"keep-me"},"added":{"command":"by-someone-else"}}}"#;

        let error = add_json_config_with(
            "cursor",
            path.clone(),
            "mcpServers",
            &wenlan_server(),
            false,
            true,
            || fs::write(&path, concurrent).expect("a second writer lands"),
        )
        .expect_err("the second writer's change must not be replaced");

        assert!(
            error
                .to_string()
                .contains("changed while Wenlan was updating"),
            "{error}"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), concurrent);
        assert_eq!(
            names_in(dir.path()),
            vec!["mcp.json".to_string()],
            "no backup of a stale parse and no temp file"
        );
    }

    #[test]
    fn a_config_created_while_updating_is_not_overwritten() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("mcp.json");
        let appeared = r#"{"mcpServers":{"added":{"command":"first"}}}"#;

        let error = add_json_config_with(
            "cursor",
            path.clone(),
            "mcpServers",
            &wenlan_server(),
            false,
            true,
            || fs::write(&path, appeared).expect("a second writer creates it"),
        )
        .expect_err("a file that appeared must not be clobbered");

        assert!(error.to_string().contains("changed while"), "{error}");
        assert_eq!(fs::read_to_string(&path).unwrap(), appeared);
        assert_eq!(names_in(dir.path()), vec!["mcp.json".to_string()]);
    }

    #[test]
    fn the_rename_is_guarded_by_a_check_made_just_before_it() {
        // `write_json_atomic` is where the window is narrowest: the bytes are
        // compared after the temp file exists and immediately before the
        // rename, so it must refuse on its own, not rely on the caller.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("mcp.json");
        fs::write(&path, "current").expect("seed");

        let error = write_json_atomic(&path, &json!({"a": 1}), Some("what-was-parsed"))
            .expect_err("stale expectation");

        assert!(error.to_string().contains("changed while"), "{error}");
        assert_eq!(fs::read_to_string(&path).unwrap(), "current");
        assert_eq!(names_in(dir.path()), vec!["mcp.json".to_string()]);

        write_json_atomic(&path, &json!({"a": 1}), Some("current")).expect("fresh expectation");
        assert!(fs::read_to_string(&path).unwrap().contains("\"a\": 1"));
    }

    #[cfg(unix)]
    mod unix {
        use super::*;
        use std::os::unix::fs::{symlink, PermissionsExt};

        #[test]
        fn a_config_that_cannot_be_read_is_never_replaced_by_an_empty_one() {
            let dir = tempfile::tempdir().expect("tempdir");
            let locked = dir.path().join("locked");
            fs::create_dir(&locked).expect("mkdir");
            let path = locked.join("mcp.json");
            fs::write(&path, OTHER_SERVERS).expect("seed");
            fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).expect("chmod");

            let outcome = if fs::read_dir(&locked).is_ok() {
                None // running as root: permissions do not bind, nothing to prove
            } else {
                Some(connect(&path))
            };

            fs::set_permissions(&locked, fs::Permissions::from_mode(0o700)).expect("restore");
            let Some(outcome) = outcome else { return };
            let error = outcome.expect_err("an unreadable config must abort");
            assert!(error.to_string().contains("nothing was changed"), "{error}");
            assert_eq!(fs::read_to_string(&path).unwrap(), OTHER_SERVERS);
            assert_eq!(names_in(&locked), vec!["mcp.json".to_string()]);
        }

        #[test]
        fn a_symlink_loop_aborts_instead_of_being_replaced() {
            // `Path::exists` is false for this (stat fails with ELOOP), which
            // used to send it down the new-file branch.
            let dir = tempfile::tempdir().expect("tempdir");
            let a = dir.path().join("a.json");
            let b = dir.path().join("b.json");
            symlink(&b, &a).expect("a -> b");
            symlink(&a, &b).expect("b -> a");

            let error = connect(&a).expect_err("a loop is not an absent file");

            assert!(error.to_string().contains("nothing was changed"), "{error}");
            assert!(
                fs::symlink_metadata(&a).unwrap().file_type().is_symlink(),
                "the link must still be a link"
            );
            assert_eq!(
                names_in(dir.path()),
                vec!["a.json".to_string(), "b.json".to_string()]
            );
        }

        #[test]
        fn a_symlinked_config_is_written_through_and_stays_a_link() {
            let dir = tempfile::tempdir().expect("tempdir");
            let dotfiles = dir.path().join("dotfiles");
            let config_dir = dir.path().join("config");
            fs::create_dir(&dotfiles).expect("mkdir");
            fs::create_dir(&config_dir).expect("mkdir");
            fs::write(dotfiles.join("mcp.json"), OTHER_SERVERS).expect("seed");
            let link = config_dir.join("mcp.json");
            symlink("../dotfiles/mcp.json", &link).expect("relative link");

            connect(&link).expect("connect");

            assert!(
                fs::symlink_metadata(&link)
                    .unwrap()
                    .file_type()
                    .is_symlink(),
                "the link was replaced by a regular file"
            );
            let through: Value =
                serde_json::from_str(&fs::read_to_string(dotfiles.join("mcp.json")).unwrap())
                    .unwrap();
            assert_eq!(through["mcpServers"]["other"]["command"], "keep-me");
            assert_eq!(through["mcpServers"]["wenlan"]["command"], "wenlan-mcp");
            // The temp file sat next to the real file and is gone; the backup
            // sits where the user keeps the config, not in the dotfiles repo.
            assert_eq!(names_in(&dotfiles), vec!["mcp.json".to_string()]);
            assert!(names_in(&config_dir)
                .iter()
                .any(|name| name.starts_with("mcp.json.bak.")));
        }

        #[test]
        fn a_dangling_symlink_creates_the_file_it_names() {
            let dir = tempfile::tempdir().expect("tempdir");
            let real = dir.path().join("real");
            let link = dir.path().join("mcp.json");
            symlink(real.join("mcp.json"), &link).expect("dangling link");

            connect(&link).expect("connect");

            assert!(fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink());
            let created: Value =
                serde_json::from_str(&fs::read_to_string(real.join("mcp.json")).unwrap()).unwrap();
            assert_eq!(created["mcpServers"]["wenlan"]["command"], "wenlan-mcp");
        }
    }
}
