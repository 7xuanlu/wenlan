// SPDX-License-Identifier: AGPL-3.0-only
use crate::error::AppError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpClient {
    pub name: String,
    pub client_type: String,
    pub config_path: String,
    pub detected: bool,
    pub already_configured: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WenlanMcpEntry {
    pub command: String,
    pub args: Vec<String>,
}

/// Returns the expected config file path for each MCP client on macOS.
pub fn client_config_path(client_type: &str) -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    match client_type {
        "claude_desktop" => {
            // macOS: ~/Library/Application Support/Claude/claude_desktop_config.json
            dirs::config_dir().map(|d| d.join("Claude").join("claude_desktop_config.json"))
        }
        "cursor" => Some(home.join(".cursor").join("mcp.json")),
        "claude_code" => Some(home.join(".claude.json")),
        "gemini_cli" => Some(home.join(".gemini").join("settings.json")),
        "codex_cli" => Some(home.join(".codex").join("config.toml")),
        _ => None,
    }
}

const MCP_SERVER_KEY: &str = "wenlan";
const LEGACY_MCP_SERVER_KEY: &str = "origin";

/// Check if a JSON config string already has a Wenlan entry or legacy Origin entry.
fn has_configured_entry(json_str: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(json_str)
        .ok()
        .and_then(|v| {
            let servers = v.get("mcpServers")?;
            Some(
                servers.get(MCP_SERVER_KEY).is_some()
                    || servers.get(LEGACY_MCP_SERVER_KEY).is_some(),
            )
        })
        .unwrap_or(false)
}

/// TOML variant for Codex CLI (`[mcp_servers.*]` tables).
fn has_configured_entry_toml(toml_str: &str) -> bool {
    toml_str
        .parse::<toml_edit::DocumentMut>()
        .ok()
        .and_then(|doc| {
            let servers = doc.get("mcp_servers")?;
            Some(
                servers.get(MCP_SERVER_KEY).is_some()
                    || servers.get(LEGACY_MCP_SERVER_KEY).is_some(),
            )
        })
        .unwrap_or(false)
}

/// Detect installed MCP-compatible tools and whether Wenlan is already configured.
pub fn detect_mcp_clients() -> Vec<McpClient> {
    let clients = [
        ("Cursor", "cursor"),
        ("Claude Code", "claude_code"),
        ("Claude Desktop", "claude_desktop"),
        ("Gemini CLI", "gemini_cli"),
        ("Codex CLI", "codex_cli"),
    ];

    clients
        .iter()
        .filter_map(|(name, client_type)| {
            let config_path = client_config_path(client_type)?;
            let config_path_str = config_path.to_string_lossy().to_string();

            let is_toml = *client_type == "codex_cli";
            let config_has_entry = || {
                config_path.exists()
                    && std::fs::read_to_string(&config_path)
                        .map(|s| {
                            if is_toml {
                                has_configured_entry_toml(&s)
                            } else {
                                has_configured_entry(&s)
                            }
                        })
                        .unwrap_or(false)
            };

            let (detected, already_configured) = if client_type == &"cursor" {
                // Cursor: detect by app bundle, not config file
                let app_exists = std::path::Path::new("/Applications/Cursor.app").exists()
                    || dirs::home_dir()
                        .map(|h| h.join("Applications/Cursor.app").exists())
                        .unwrap_or(false);
                (app_exists, config_has_entry())
            } else {
                // Everything else: detect by config file existence
                (config_path.exists(), config_has_entry())
            };

            Some(McpClient {
                name: name.to_string(),
                client_type: client_type.to_string(),
                config_path: config_path_str,
                detected,
                already_configured,
            })
        })
        .collect()
}

/// Search for a local wenlan-mcp binary (dev fallback).
fn find_wenlan_mcp_binary() -> Option<PathBuf> {
    let candidates = [
        dirs::home_dir().map(|h| h.join(".cargo/bin/wenlan-mcp")),
        dirs::home_dir().map(|h| h.join("Repos/wenlan/target/release/wenlan-mcp")),
        dirs::home_dir().map(|h| h.join("Repos/wenlan/target/debug/wenlan-mcp")),
        Some(PathBuf::from("/usr/local/bin/wenlan-mcp")),
    ];
    candidates.into_iter().flatten().find(|p| p.exists())
}

/// The MCP config entry Wenlan writes into client config files.
/// Default: npx (production path, requires wenlan-mcp published to npm).
/// Dev fallback: uses local binary if found on disk.
pub fn wenlan_mcp_entry() -> WenlanMcpEntry {
    // Dev fallback: use local binary if available
    if let Some(binary_path) = find_wenlan_mcp_binary() {
        return WenlanMcpEntry {
            command: binary_path.to_string_lossy().to_string(),
            args: Vec::new(),
        };
    }
    // Production default
    WenlanMcpEntry {
        command: "npx".to_string(),
        args: vec!["-y".to_string(), "wenlan-mcp".to_string()],
    }
}

/// Write the Wenlan MCP server entry into a client's config file.
/// Existing legacy `origin` entries are preserved and still detected.
/// If `is_claude_code` is true and the file doesn't exist, returns an error
/// (Claude Code manages its own config file).
pub fn write_wenlan_entry(
    config_path: &std::path::Path,
    is_claude_code: bool,
) -> Result<(), AppError> {
    let mut root = if config_path.exists() {
        // Back up existing file
        let backup_path = config_path.with_extension("json.bak");
        std::fs::copy(config_path, &backup_path)?;

        let contents = std::fs::read_to_string(config_path)?;
        serde_json::from_str::<serde_json::Value>(&contents).map_err(|e| {
            AppError::Generic(format!("Invalid JSON in {}: {}", config_path.display(), e))
        })?
    } else if is_claude_code {
        return Err(AppError::Generic(
            "Claude Code config file not found — Claude Code manages this file internally".into(),
        ));
    } else {
        // Create minimal skeleton for Claude Desktop / Cursor
        serde_json::json!({})
    };

    // Ensure mcpServers key exists
    if root.get("mcpServers").is_none() {
        root["mcpServers"] = serde_json::json!({});
    }
    root["mcpServers"][MCP_SERVER_KEY] =
        serde_json::to_value(wenlan_mcp_entry()).map_err(|e| AppError::Generic(e.to_string()))?;

    // Write back with pretty formatting
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let formatted =
        serde_json::to_string_pretty(&root).map_err(|e| AppError::Generic(e.to_string()))?;
    std::fs::write(config_path, formatted)?;

    Ok(())
}

/// Upsert the Wenlan entry into a Codex CLI `config.toml` — format-preserving:
/// user comments, key order, and unrelated tables survive byte-for-byte
/// (toml_edit round-trips everything it didn't touch).
pub fn write_wenlan_entry_toml(config_path: &std::path::Path) -> Result<(), AppError> {
    use toml_edit::{DocumentMut, Item, Table};

    let mut doc: DocumentMut = if config_path.exists() {
        let backup_path = config_path.with_extension("toml.bak");
        std::fs::copy(config_path, &backup_path)?;
        let contents = std::fs::read_to_string(config_path)?;
        contents.parse().map_err(|e| {
            AppError::Generic(format!("Invalid TOML in {}: {}", config_path.display(), e))
        })?
    } else {
        DocumentMut::new()
    };

    if doc.get("mcp_servers").is_none() {
        let mut parent = Table::new();
        parent.set_implicit(true); // render only [mcp_servers.wenlan], no bare [mcp_servers]
        doc.insert("mcp_servers", Item::Table(parent));
    }

    let entry = wenlan_mcp_entry();
    let mut server = Table::new();
    server.insert("command", toml_edit::value(entry.command));
    let mut args = toml_edit::Array::new();
    for a in entry.args {
        args.push(a);
    }
    server.insert("args", toml_edit::value(args));
    doc["mcp_servers"][MCP_SERVER_KEY] = Item::Table(server);

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(config_path, doc.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_config_path_claude_desktop() {
        let path = client_config_path("claude_desktop").unwrap();
        assert!(path.to_string_lossy().contains("Claude"));
        assert!(path
            .to_string_lossy()
            .ends_with("claude_desktop_config.json"));
    }

    #[test]
    fn test_client_config_path_cursor() {
        let path = client_config_path("cursor").unwrap();
        assert!(path.to_string_lossy().contains(".cursor"));
        assert!(path.to_string_lossy().ends_with("mcp.json"));
    }

    #[test]
    fn test_client_config_path_claude_code() {
        let path = client_config_path("claude_code").unwrap();
        assert!(path.to_string_lossy().ends_with(".claude.json"));
    }

    #[test]
    fn test_client_config_path_unknown() {
        assert!(client_config_path("unknown").is_none());
    }

    #[test]
    fn test_check_already_configured_finds_legacy_origin() {
        let json =
            r#"{"mcpServers": {"origin": {"command": "npx", "args": ["-y", "origin-mcp"]}}}"#;
        assert!(has_configured_entry(json));
    }

    #[test]
    fn test_check_already_configured_finds_wenlan() {
        let json =
            r#"{"mcpServers": {"wenlan": {"command": "npx", "args": ["-y", "wenlan-mcp"]}}}"#;
        assert!(has_configured_entry(json));
    }

    #[test]
    fn test_check_already_configured_not_found() {
        let json = r#"{"mcpServers": {"other-server": {}}}"#;
        assert!(!has_configured_entry(json));
    }

    #[test]
    fn test_check_already_configured_no_mcp_servers_key() {
        let json = r#"{"theme": "dark"}"#;
        assert!(!has_configured_entry(json));
    }

    #[test]
    fn test_check_already_configured_invalid_json() {
        assert!(!has_configured_entry("not json"));
    }

    #[test]
    fn test_wenlan_mcp_entry_is_typed_command_args() {
        let entry = wenlan_mcp_entry();

        assert!(!entry.command.is_empty());
        if entry.command == "npx" {
            assert_eq!(entry.args, vec!["-y".to_string(), "wenlan-mcp".to_string()]);
        } else {
            assert!(entry.command.ends_with("wenlan-mcp"));
            assert!(entry.args.is_empty());
        }
    }

    #[test]
    fn test_write_wenlan_entry_creates_new_file() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("config.json");
        write_wenlan_entry(&config_path, false).unwrap();
        let contents = std::fs::read_to_string(&config_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap();
        assert_eq!(
            parsed["mcpServers"]["wenlan"],
            serde_json::to_value(wenlan_mcp_entry()).unwrap()
        );
        // Command is either a local binary path or npx (depending on host)
        let cmd = parsed["mcpServers"]["wenlan"]["command"].as_str().unwrap();
        assert!(
            cmd == "npx" || cmd.ends_with("wenlan-mcp"),
            "expected npx or path to wenlan-mcp, got: {cmd}"
        );
        assert!(parsed["mcpServers"]["origin"].is_null());
    }

    #[test]
    fn test_write_wenlan_entry_preserves_existing_servers() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("config.json");
        let existing = r#"{"mcpServers": {"other": {"command": "other-cmd"}}}"#;
        std::fs::write(&config_path, existing).unwrap();
        write_wenlan_entry(&config_path, false).unwrap();
        let contents = std::fs::read_to_string(&config_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap();
        assert!(parsed["mcpServers"]["other"].is_object());
        assert!(parsed["mcpServers"]["wenlan"].is_object());
    }

    #[test]
    fn test_write_wenlan_entry_preserves_legacy_origin_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("config.json");
        let existing =
            r#"{"mcpServers": {"origin": {"command": "npx", "args": ["-y", "origin-mcp"]}}}"#;
        std::fs::write(&config_path, existing).unwrap();
        write_wenlan_entry(&config_path, false).unwrap();
        let contents = std::fs::read_to_string(&config_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap();
        assert_eq!(
            parsed["mcpServers"]["origin"]["args"],
            serde_json::json!(["-y", "origin-mcp"])
        );
        assert!(parsed["mcpServers"]["wenlan"].is_object());
    }

    #[test]
    fn test_write_wenlan_entry_creates_mcp_servers_key() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("config.json");
        std::fs::write(&config_path, r#"{"theme": "dark"}"#).unwrap();
        write_wenlan_entry(&config_path, false).unwrap();
        let contents = std::fs::read_to_string(&config_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap();
        assert_eq!(parsed["theme"], "dark");
        assert!(parsed["mcpServers"]["wenlan"].is_object());
    }

    #[test]
    fn test_write_wenlan_entry_creates_backup() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("config.json");
        std::fs::write(&config_path, r#"{"original": true}"#).unwrap();
        write_wenlan_entry(&config_path, false).unwrap();
        let backup = tmp.path().join("config.json.bak");
        assert!(backup.exists());
        let backup_contents = std::fs::read_to_string(&backup).unwrap();
        assert!(backup_contents.contains("original"));
    }

    #[test]
    fn test_write_wenlan_entry_errors_on_invalid_json() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("config.json");
        std::fs::write(&config_path, "not valid json").unwrap();
        let result = write_wenlan_entry(&config_path, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_write_wenlan_entry_refuses_create_for_claude_code() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("claude.json");
        // is_claude_code = true, file doesn't exist → should error
        let result = write_wenlan_entry(&config_path, true);
        assert!(result.is_err());
    }

    #[test]
    fn test_client_config_path_gemini_cli() {
        let path = client_config_path("gemini_cli").unwrap();
        assert!(path.to_string_lossy().ends_with(".gemini/settings.json"));
    }

    #[test]
    fn test_client_config_path_codex_cli() {
        let path = client_config_path("codex_cli").unwrap();
        assert!(path.to_string_lossy().ends_with(".codex/config.toml"));
    }

    #[test]
    fn test_detect_includes_new_clients() {
        let types: Vec<String> = detect_mcp_clients()
            .into_iter()
            .map(|c| c.client_type)
            .collect();
        for expected in [
            "cursor",
            "claude_code",
            "claude_desktop",
            "gemini_cli",
            "codex_cli",
        ] {
            assert!(types.contains(&expected.to_string()), "missing {expected}");
        }
    }

    #[test]
    fn test_has_configured_entry_toml() {
        assert!(has_configured_entry_toml(
            "[mcp_servers.wenlan]\ncommand = \"npx\"\nargs = [\"-y\", \"wenlan-mcp\"]\n"
        ));
        assert!(has_configured_entry_toml(
            "[mcp_servers.origin]\ncommand = \"npx\"\n"
        ));
        assert!(!has_configured_entry_toml(
            "[mcp_servers.other]\ncommand = \"x\"\n"
        ));
        assert!(!has_configured_entry_toml("model = \"gpt-5.5\"\n"));
        assert!(!has_configured_entry_toml("not toml ["));
    }

    #[test]
    fn test_write_wenlan_entry_toml_creates_new_file() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("config.toml");
        write_wenlan_entry_toml(&config_path).unwrap();
        let contents = std::fs::read_to_string(&config_path).unwrap();
        assert!(has_configured_entry_toml(&contents));
        let parsed: toml::Value = toml::from_str(&contents).unwrap();
        let wenlan = &parsed["mcp_servers"]["wenlan"];
        assert!(wenlan.get("command").is_some());
    }

    #[test]
    fn test_write_wenlan_entry_toml_preserves_formatting_byte_for_byte() {
        // Council change (d): a user's hand-edited config must survive the
        // upsert byte-for-byte — comments, spacing, key order, other tables.
        let fixture = r#"# my codex config — do not touch
model = "gpt-5.5"   # inline comment

[profiles.fast]
model   = "gpt-5.5-mini"

[mcp_servers.other]
command = "other-cmd"  # keep me
args = ["--flag"]
"#;
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("config.toml");
        std::fs::write(&config_path, fixture).unwrap();
        write_wenlan_entry_toml(&config_path).unwrap();
        let contents = std::fs::read_to_string(&config_path).unwrap();
        // Everything that existed before is preserved verbatim; the wenlan
        // table is appended after it.
        assert!(
            contents.starts_with(fixture),
            "existing content was reformatted:\n{contents}"
        );
        assert!(has_configured_entry_toml(&contents));
    }

    #[test]
    fn test_write_wenlan_entry_toml_upsert_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("config.toml");
        write_wenlan_entry_toml(&config_path).unwrap();
        let first = std::fs::read_to_string(&config_path).unwrap();
        write_wenlan_entry_toml(&config_path).unwrap();
        let second = std::fs::read_to_string(&config_path).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn test_write_wenlan_entry_toml_creates_backup() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("config.toml");
        std::fs::write(&config_path, "model = \"gpt-5.5\"\n").unwrap();
        write_wenlan_entry_toml(&config_path).unwrap();
        let backup = tmp.path().join("config.toml.bak");
        assert!(backup.exists());
        assert!(std::fs::read_to_string(&backup)
            .unwrap()
            .contains("gpt-5.5"));
    }

    #[test]
    fn test_write_wenlan_entry_toml_errors_on_invalid_toml() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("config.toml");
        std::fs::write(&config_path, "not toml [").unwrap();
        assert!(write_wenlan_entry_toml(&config_path).is_err());
    }
}
