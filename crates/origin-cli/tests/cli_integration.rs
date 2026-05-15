// SPDX-License-Identifier: Apache-2.0
//! Integration tests for the origin CLI. Offline (no daemon required).

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
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
        write_fake_launchctl(fake_bin.path());
        ensure_sibling_origin_server();
        Self {
            root,
            home,
            data,
            fake_bin,
        }
    }

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

fn write_fake_launchctl(fake_bin: &Path) {
    let path = fake_bin.join("launchctl");
    fs::write(
        &path,
        "#!/bin/sh\ncase \"$1\" in\n  list) exit 0 ;;\n  load|unload) echo \"fake launchctl $1 $2\"; exit 0 ;;\n  *) echo \"fake launchctl $@\"; exit 0 ;;\nesac\n",
    )
    .expect("write fake launchctl");
    let mut perms = fs::metadata(&path)
        .expect("fake launchctl metadata")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).expect("chmod fake launchctl");
}

fn ensure_sibling_origin_server() {
    let origin_bin = assert_cmd::cargo::cargo_bin("origin");
    let server = origin_bin
        .parent()
        .expect("origin binary has parent")
        .join("origin-server");
    if !server.exists() {
        fs::write(&server, "#!/bin/sh\nexit 0\n").expect("write fake origin-server sibling");
        let mut perms = fs::metadata(&server)
            .expect("fake origin-server metadata")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(server, perms).expect("chmod fake origin-server");
    }
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
    ] {
        cli().args([sub, "--help"]).assert().success();
    }
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
