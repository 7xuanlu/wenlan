// SPDX-License-Identifier: Apache-2.0
//! Integration tests for `wenlan spaces` subcommands.
//!
//! Decision: toml-only approach (no throwaway daemon).
//! Spawning a process-level daemon requires port management, wait-for-ready
//! polling, and drop-based cleanup — well over 30 lines of scaffolding with no
//! existing harness in wenlan CLI tests.  The unit tests in `wenlan-core` and
//! the wenlan-server HTTP integration tests already cover the server-side space
//! logic.  These tests cover the CLI surface that does NOT require a live
//! daemon: argument parsing (--help flags) and the toml-backed `spaces default`
//! round-trip.

use assert_cmd::Command;
use std::fs;

fn cli() -> Command {
    Command::cargo_bin("wenlan").expect("origin binary built")
}

// ---------------------------------------------------------------------------
// Argument parsing — all `spaces` subcommands must accept --help
// ---------------------------------------------------------------------------

#[test]
fn spaces_subcommands_help_exits_zero() {
    for args in [
        &["spaces", "--help"][..],
        &["spaces", "list", "--help"][..],
        &["spaces", "add", "--help"][..],
        &["spaces", "default", "--help"][..],
        &["spaces", "move", "--help"][..],
        &["spaces", "show", "--help"][..],
    ] {
        cli().args(args).assert().success();
    }
}

// ---------------------------------------------------------------------------
// `spaces default` toml round-trip — no daemon required
// ---------------------------------------------------------------------------

#[test]
fn spaces_default_set_and_read_back() {
    let tmp = tempfile::tempdir().expect("mktempdir");

    // Set the default.
    cli()
        .env("HOME", tmp.path())
        .args(["spaces", "default", "work"])
        .assert()
        .success();

    // Read it back — output should be exactly "work".
    cli()
        .env("HOME", tmp.path())
        .args(["spaces", "default"])
        .assert()
        .success()
        .stdout(predicates::str::contains("work"));
}

#[test]
fn spaces_default_toml_file_written_correctly() {
    let tmp = tempfile::tempdir().expect("mktempdir");

    cli()
        .env("HOME", tmp.path())
        .args(["spaces", "default", "personal"])
        .assert()
        .success();

    let toml_path = tmp.path().join(".wenlan/spaces.toml");
    let content = fs::read_to_string(&toml_path).expect("spaces.toml should exist");
    assert!(
        content.contains("default = \"personal\""),
        "toml content: {}",
        content
    );
}

#[test]
fn spaces_default_overwrite_existing_entry() {
    let tmp = tempfile::tempdir().expect("mktempdir");

    // First write.
    cli()
        .env("HOME", tmp.path())
        .args(["spaces", "default", "first"])
        .assert()
        .success();

    // Overwrite.
    cli()
        .env("HOME", tmp.path())
        .args(["spaces", "default", "second"])
        .assert()
        .success();

    // Only the new value survives.
    let toml_path = tmp.path().join(".wenlan/spaces.toml");
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
fn spaces_default_no_default_set_prints_fallback_message() {
    let tmp = tempfile::tempdir().expect("mktempdir");

    // No default has been set — CLI should print the unscoped fallback note.
    cli()
        .env("HOME", tmp.path())
        .args(["spaces", "default"])
        .assert()
        .success()
        .stdout(predicates::str::contains("unscoped"));
}
