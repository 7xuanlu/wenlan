// SPDX-License-Identifier: Apache-2.0
//! Integration tests for the origin CLI. Offline (no daemon required).

use assert_cmd::Command;
use predicates::prelude::*;

fn cli() -> Command {
    Command::cargo_bin("origin").expect("origin binary built")
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
    for sub in ["status", "search", "recall", "store", "list", "agents"] {
        cli().args([sub, "--help"]).assert().success();
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
