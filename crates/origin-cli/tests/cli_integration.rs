// SPDX-License-Identifier: Apache-2.0
//! Integration tests for the origin CLI. Offline (no daemon required).

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn cli() -> Command {
    Command::cargo_bin("origin").expect("origin binary built")
}

fn cli_with_isolated_runtime(runtime: &IsolatedRuntime) -> Command {
    let mut cmd = cli();
    cmd.env("HOME", runtime.home.path())
        .env("ORIGIN_DATA_DIR", runtime.data.path())
        .env("ORIGIN_HOST", "http://127.0.0.1:9")
        .env(
            "PATH",
            format!(
                "{}:{}",
                runtime.fake_bin.path().display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        );
    cmd
}

struct IsolatedRuntime {
    root: TempDir,
    home: TempDir,
    data: TempDir,
    fake_bin: TempDir,
}

impl IsolatedRuntime {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("temp root");
        let home = tempfile::tempdir_in(root.path()).expect("temp home");
        let data = tempfile::tempdir_in(root.path()).expect("temp data");
        let fake_bin = tempfile::tempdir_in(root.path()).expect("temp fake bin");
        #[cfg(target_os = "macos")]
        write_fake_launchctl(fake_bin.path());
        ensure_sibling_origin_server();
        ensure_sibling_origin_mcp();
        Self {
            root,
            home,
            data,
            fake_bin,
        }
    }

    #[cfg(target_os = "macos")]
    fn plist_path(&self) -> PathBuf {
        self.home
            .path()
            .join("Library/LaunchAgents/com.origin.server.plist")
    }

    fn config_path(&self) -> PathBuf {
        self.data.path().join("config.json")
    }

    fn root_exists(&self) -> bool {
        self.root.path().exists()
    }
}

#[cfg(target_os = "macos")]
fn write_fake_launchctl(fake_bin: &Path) {
    let path = fake_bin.join("launchctl");
    fs::write(
        &path,
        "#!/bin/sh\ncase \"$1\" in\n  list) exit 0 ;;\n  load|unload) echo \"fake launchctl $1 $2\"; exit 0 ;;\n  *) echo \"fake launchctl $@\"; exit 0 ;;\nesac\n",
    )
    .expect("write fake launchctl");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&path)
            .expect("fake launchctl metadata")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).expect("chmod fake launchctl");
    }
}

fn write_fake_command(fake_bin: &Path, name: &str) {
    let path = fake_bin.join(name);
    fs::write(
        &path,
        "#!/bin/sh\nprintf '%s' \"${0##*/}\" >> \"$ORIGIN_TEST_CLI_LOG\"\nfor arg in \"$@\"; do printf '\\t%s' \"$arg\" >> \"$ORIGIN_TEST_CLI_LOG\"; done\nprintf '\\n' >> \"$ORIGIN_TEST_CLI_LOG\"\nexit 0\n",
    )
    .expect("write fake command");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&path)
            .expect("fake command metadata")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).expect("chmod fake command");
    }
}

fn ensure_sibling_origin_server() {
    let origin_bin = assert_cmd::cargo::cargo_bin("origin");
    let server = origin_bin
        .parent()
        .expect("origin binary has parent")
        .join("origin-server");
    if !server.exists() {
        fs::write(&server, "#!/bin/sh\nexit 0\n").expect("write fake origin-server sibling");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&server)
                .expect("fake origin-server metadata")
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(server, perms).expect("chmod fake origin-server");
        }
    }
}

fn ensure_sibling_origin_mcp() {
    let path = origin_mcp_sibling();
    if !path.exists() {
        fs::write(&path, "#!/bin/sh\nexit 0\n").expect("write fake origin-mcp sibling");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&path)
                .expect("fake origin-mcp metadata")
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(path, perms).expect("chmod fake origin-mcp");
        }
    }
}

fn origin_mcp_sibling() -> PathBuf {
    let origin_bin = assert_cmd::cargo::cargo_bin("origin");
    origin_bin
        .parent()
        .expect("origin binary has parent")
        .join("origin-mcp")
}

fn origin_mcp_sibling_arg() -> String {
    origin_mcp_sibling().display().to_string()
}

#[test]
fn top_level_help() {
    cli()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Origin CLI"));
}

#[test]
fn version_flag() {
    cli()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("origin"));
}

#[test]
fn each_subcommand_has_help() {
    for sub in [
        "status",
        "search",
        "recall",
        "store",
        "list",
        "agents",
        "setup",
        "install",
        "uninstall",
        "doctor",
        "model",
        "key",
        "mcp",
    ] {
        cli().args([sub, "--help"]).assert().success();
    }
}

#[test]
fn mcp_subcommands_have_help() {
    for args in [&["mcp", "--help"][..], &["mcp", "add", "--help"][..]] {
        cli().args(args).assert().success();
    }
}

#[test]
fn mcp_add_claude_code_dry_run_explains_tools_only() {
    let runtime = IsolatedRuntime::new();

    cli_with_isolated_runtime(&runtime)
        .args(["mcp", "add", "claude-code", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "claude mcp add -s user origin -- {}",
            origin_mcp_sibling_arg()
        )))
        .stdout(predicate::str::contains("MCP tools only"))
        .stdout(predicate::str::contains("/brief"))
        .stdout(predicate::str::contains("/handoff"))
        .stdout(predicate::str::contains("/distill"))
        .stdout(predicate::str::contains("/init"));
}

#[test]
fn mcp_add_native_clients_run_add_without_destructive_remove() {
    let origin_mcp = origin_mcp_sibling_arg();
    let cases = [
        (
            "claude-code",
            "claude",
            format!("claude\tmcp\tadd\t-s\tuser\torigin\t--\t{origin_mcp}\n"),
        ),
        (
            "codex",
            "codex",
            format!("codex\tmcp\tadd\torigin\t--\t{origin_mcp}\n"),
        ),
        (
            "gemini",
            "gemini",
            format!("gemini\tmcp\tadd\t-s\tuser\torigin\t{origin_mcp}\n"),
        ),
    ];

    for (client, binary, expected_log) in cases {
        let runtime = IsolatedRuntime::new();
        write_fake_command(runtime.fake_bin.path(), binary);
        let log = runtime.root.path().join(format!("{client}.log"));

        cli_with_isolated_runtime(&runtime)
            .env("ORIGIN_TEST_CLI_LOG", &log)
            .args(["mcp", "add", client])
            .assert()
            .success()
            .stdout(predicate::str::contains("Configured Origin MCP"));

        assert_eq!(
            fs::read_to_string(log).expect("fake client log"),
            expected_log.as_str()
        );
    }
}

#[test]
fn mcp_add_cursor_preserves_existing_servers_and_backs_up_changed_origin() {
    let runtime = IsolatedRuntime::new();
    let config_path = runtime.home.path().join(".cursor/mcp.json");
    fs::create_dir_all(config_path.parent().expect("cursor config parent")).unwrap();
    fs::write(
        &config_path,
        r#"{"mcpServers":{"origin":{"command":"old-origin"},"other":{"command":"other-cmd"}}}"#,
    )
    .unwrap();

    cli_with_isolated_runtime(&runtime)
        .args(["mcp", "add", "cursor"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Updated"));

    let updated = fs::read_to_string(&config_path).expect("updated cursor config");
    assert!(updated.contains(r#""other""#), "{updated}");
    assert!(
        updated.contains(&format!(r#""command": "{}""#, origin_mcp_sibling_arg())),
        "{updated}"
    );
    assert!(updated.contains("origin-mcp"), "{updated}");

    let backups: Vec<_> = fs::read_dir(config_path.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("mcp.json.bak.")
        })
        .collect();
    assert_eq!(backups.len(), 1, "expected one backup");
    let backup = fs::read_to_string(backups[0].path()).expect("backup content");
    assert!(backup.contains("old-origin"), "{backup}");
}

#[test]
fn mcp_add_cursor_dry_run_prints_only_origin_block() {
    let runtime = IsolatedRuntime::new();
    let config_path = runtime.home.path().join(".cursor/mcp.json");
    fs::create_dir_all(config_path.parent().expect("cursor config parent")).unwrap();
    fs::write(
        &config_path,
        r#"{"mcpServers":{"private":{"command":"secret-tool","env":{"API_KEY":"SECRET_TOKEN"}}}}"#,
    )
    .unwrap();

    cli_with_isolated_runtime(&runtime)
        .args(["mcp", "add", "cursor", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("mcpServers.origin"))
        .stdout(predicate::str::contains("origin-mcp"))
        .stdout(predicate::str::contains("SECRET_TOKEN").not())
        .stdout(predicate::str::contains("secret-tool").not());

    let unchanged = fs::read_to_string(&config_path).expect("cursor config unchanged");
    assert!(unchanged.contains("SECRET_TOKEN"), "{unchanged}");
}

#[cfg(target_os = "macos")]
#[test]
fn mcp_add_json_clients_write_expected_config_shapes() {
    let runtime = IsolatedRuntime::new();

    cli_with_isolated_runtime(&runtime)
        .args(["mcp", "add", "claude-desktop"])
        .assert()
        .success();
    let claude = fs::read_to_string(
        runtime
            .home
            .path()
            .join("Library/Application Support/Claude/claude_desktop_config.json"),
    )
    .expect("claude desktop config");
    assert!(claude.contains(r#""mcpServers""#), "{claude}");
    assert!(claude.contains("origin-mcp"), "{claude}");

    cli_with_isolated_runtime(&runtime)
        .current_dir(runtime.root.path())
        .args(["mcp", "add", "vscode"])
        .assert()
        .success();
    let vscode = fs::read_to_string(runtime.root.path().join(".vscode/mcp.json"))
        .expect("vscode workspace config");
    assert!(vscode.contains(r#""servers""#), "{vscode}");
    assert!(vscode.contains("origin-mcp"), "{vscode}");
}

#[test]
fn mcp_add_invalid_json_fails_without_modifying_file() {
    let runtime = IsolatedRuntime::new();
    let config_path = runtime.home.path().join(".cursor/mcp.json");
    fs::create_dir_all(config_path.parent().expect("cursor config parent")).unwrap();
    fs::write(&config_path, "{not json").unwrap();

    cli_with_isolated_runtime(&runtime)
        .args(["mcp", "add", "cursor"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid JSON"));

    assert_eq!(
        fs::read_to_string(&config_path).expect("cursor config still exists"),
        "{not json"
    );
}

#[test]
fn setup_subcommands_have_help() {
    for args in [
        &["setup", "--help"][..],
        &["model", "list", "--help"][..],
        &["model", "install", "--help"][..],
        &["model", "status", "--help"][..],
        &["key", "status", "--help"][..],
        &["key", "set", "--help"][..],
        &["key", "clear", "--help"][..],
    ] {
        cli().args(args).assert().success();
    }
}

#[test]
fn invalid_subcommand_fails() {
    cli().arg("nonexistent-command").assert().failure();
}

#[test]
fn store_text_and_file_conflict_bails() {
    // text=Some, file=Some -> bail at runtime (mutual exclusion)
    cli()
        .args(["store", "some text", "--file", "/dev/null"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("either"));
}

#[test]
fn agents_edit_no_flags_bails() {
    cli()
        .args(["agents", "edit", "dummy-agent-name"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("No fields to update"));
}

#[test]
fn status_json_succeeds_when_daemon_is_unreachable() {
    cli()
        .env("ORIGIN_HOST", "http://127.0.0.1:9")
        .args(["status", "--format", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"unreachable\""));
}

#[test]
fn status_table_uses_origin_host_for_health_probe() {
    let runtime = IsolatedRuntime::new();

    cli_with_isolated_runtime(&runtime)
        .args(["status", "--format", "table"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Health: not reachable"))
        .stdout(predicate::str::contains("http://127.0.0.1:9/api/health"));
}

#[test]
fn doctor_uses_origin_host_for_health_probe() {
    cli()
        .env("ORIGIN_HOST", "http://127.0.0.1:9")
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Daemon: not reachable on http://127.0.0.1:9/api/health",
        ));
}

#[cfg(target_os = "macos")]
#[test]
fn setup_install_status_uninstall_roundtrip_isolated() {
    let runtime = IsolatedRuntime::new();

    cli_with_isolated_runtime(&runtime)
        .args(["setup", "--basic"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Origin is set up for local memory",
        ))
        .stdout(predicate::str::contains("Distill cycles stay off"));

    let config = fs::read_to_string(runtime.config_path()).expect("config written");
    assert!(config.contains(r#""setup_completed": true"#));
    assert!(config.contains(r#""on_device_model": null"#));
    assert!(config.contains(r#""anthropic_api_key": null"#));

    cli_with_isolated_runtime(&runtime)
        .arg("install")
        .assert()
        .success()
        .stdout(predicate::str::contains("Wrote"))
        .stdout(predicate::str::contains("Loaded com.origin.server"));

    let plist = fs::read_to_string(runtime.plist_path()).expect("plist written");
    assert!(plist.contains("<string>com.origin.server</string>"));
    assert!(plist.contains("origin-server"));
    assert!(!plist.contains("origin</string>"));

    cli_with_isolated_runtime(&runtime)
        .args(["status", "--format", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"unreachable\""));

    cli_with_isolated_runtime(&runtime)
        .arg("uninstall")
        .assert()
        .success()
        .stdout(predicate::str::contains("daemon will no longer auto-start"));

    assert!(
        !runtime.plist_path().exists(),
        "uninstall removes launchd plist"
    );
    assert!(
        runtime.root_exists(),
        "temp runtime remains alive until test end"
    );
}
