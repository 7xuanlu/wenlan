// SPDX-License-Identifier: Apache-2.0
//! Integration tests for `origin space` subcommands.
//!
//! Decision: toml-only approach (no throwaway daemon).
//! Spawning a process-level daemon requires port management, wait-for-ready
//! polling, and drop-based cleanup — well over 30 lines of scaffolding with no
//! existing harness in origin-cli tests.  The unit tests in `origin-core` and
//! the origin-server HTTP integration tests already cover the server-side space
//! logic.  These tests cover the CLI surface that does NOT require a live
//! daemon: argument parsing (--help flags) and the toml-backed `space default`
//! round-trip.

use assert_cmd::Command;
use std::fs;

fn cli() -> Command {
    Command::cargo_bin("origin").expect("origin binary built")
}

// ---------------------------------------------------------------------------
// Argument parsing — all `space` subcommands must accept --help
// ---------------------------------------------------------------------------

#[test]
fn space_subcommands_help_exits_zero() {
    for args in [
        &["space", "--help"][..],
        &["space", "list", "--help"][..],
        &["space", "add", "--help"][..],
        &["space", "default", "--help"][..],
        &["space", "move", "--help"][..],
        &["space", "show", "--help"][..],
    ] {
        cli().args(args).assert().success();
    }
}

// ---------------------------------------------------------------------------
// `space default` toml round-trip — no daemon required
// ---------------------------------------------------------------------------

#[test]
fn space_default_set_and_read_back() {
    let tmp = tempfile::tempdir().expect("mktempdir");

    // Set the default.
    cli()
        .env("HOME", tmp.path())
        .args(["space", "default", "work"])
        .assert()
        .success();

    // Read it back — output should be exactly "work".
    cli()
        .env("HOME", tmp.path())
        .args(["space", "default"])
        .assert()
        .success()
        .stdout(predicates::str::contains("work"));
}

#[test]
fn space_default_toml_file_written_correctly() {
    let tmp = tempfile::tempdir().expect("mktempdir");

    cli()
        .env("HOME", tmp.path())
        .args(["space", "default", "personal"])
        .assert()
        .success();

    let toml_path = tmp.path().join(".origin/spaces.toml");
    let content = fs::read_to_string(&toml_path).expect("spaces.toml should exist");
    assert!(
        content.contains("default = \"personal\""),
        "toml content: {}",
        content
    );
}

#[test]
fn space_default_overwrite_existing_entry() {
    let tmp = tempfile::tempdir().expect("mktempdir");

    // First write.
    cli()
        .env("HOME", tmp.path())
        .args(["space", "default", "first"])
        .assert()
        .success();

    // Overwrite.
    cli()
        .env("HOME", tmp.path())
        .args(["space", "default", "second"])
        .assert()
        .success();

    // Only the new value survives.
    let toml_path = tmp.path().join(".origin/spaces.toml");
    let content = fs::read_to_string(&toml_path).expect("spaces.toml should exist");
    assert!(
        content.contains("default = \"second\""),
        "expected 'second' in toml: {}",
        content
    );
    assert!(
        !content.contains("default = \"first\""),
        "unexpected stale 'first' in toml: {}",
        content
    );
}

#[test]
fn space_default_no_default_set_prints_fallback_message() {
    let tmp = tempfile::tempdir().expect("mktempdir");

    // No default has been set — CLI should print the fallback note.
    cli()
        .env("HOME", tmp.path())
        .args(["space", "default"])
        .assert()
        .success()
        .stdout(predicates::str::contains("personal"));
}
