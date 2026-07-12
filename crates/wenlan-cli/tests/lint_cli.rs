// SPDX-License-Identifier: Apache-2.0
use assert_cmd::Command;
use serde_json::{json, Value};
use wenlan_types::lint::{LintGateEffect, LintOutcome};

#[path = "lint_cli/support.rs"]
mod support;
use support::{
    closed_host, report, report_with_gates, spawn_error, spawn_oversized, spawn_report, spawn_value,
};

fn cli() -> Command {
    Command::cargo_bin("wenlan").expect("wenlan binary built")
}

#[test]
fn lint_json_accepts_global_flags_before_command_and_uses_remote_scope() {
    let expected = report(&[("memories.sample", LintOutcome::Pass)]);
    let (base, request) = spawn_report(&expected);

    let output = cli()
        .env("WENLAN_HOST", base)
        .args(["--format", "json", "lint", "--space", "work"])
        .assert()
        .code(0)
        .stderr("")
        .get_output()
        .stdout
        .clone();

    assert_eq!(
        serde_json::from_slice::<Value>(&output).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
    assert_eq!(
        request.recv().unwrap(),
        "GET /api/lint?space=work HTTP/1.1\r\n"
    );
}

#[test]
fn lint_deep_profile_is_forwarded_as_the_canonical_query() {
    let expected = report(&[("memories.sample", LintOutcome::Pass)]);
    let (base, request) = spawn_report(&expected);

    cli()
        .env("WENLAN_HOST", base)
        .args(["--format", "json", "lint", "--profile", "deep"])
        .assert()
        .code(0)
        .stderr("");

    assert_eq!(
        request.recv().unwrap(),
        "GET /api/lint?profile=deep HTTP/1.1\r\n"
    );
}

#[test]
fn lint_quiet_json_suppresses_output_without_changing_exit() {
    let expected = report(&[("pages.sample", LintOutcome::Finding)]);
    let (base, _) = spawn_report(&expected);

    cli()
        .env("WENLAN_HOST", base)
        .args(["lint", "--format", "json", "--quiet"])
        .assert()
        .code(1)
        .stdout("")
        .stderr("");
}

#[test]
fn lint_human_clean_report_summarizes_groups_and_exits_zero() {
    let expected = report(&[("memories.sample", LintOutcome::Pass)]);
    let (base, _) = spawn_report(&expected);

    cli()
        .env("WENLAN_HOST", base)
        .args(["lint", "--format", "table"])
        .assert()
        .code(0)
        .stdout(predicates::str::contains(
            "memories: 1 check, 0 findings, 0 incomplete",
        ))
        .stdout(predicates::str::contains("Findings: none"))
        .stdout(predicates::str::contains("Incomplete: none"))
        .stderr("");
}

#[test]
fn lint_human_finding_is_actionable_and_exits_one() {
    let expected = report(&[("pages.sample", LintOutcome::Finding)]);
    let (base, _) = spawn_report(&expected);

    cli()
        .env("WENLAN_HOST", base)
        .args(["lint", "--format", "table"])
        .assert()
        .code(1)
        .stdout(predicates::str::contains("Findings (1):"))
        .stdout(predicates::str::contains(
            "pages.sample: finding_detected; recommendation: review_finding",
        ))
        .stderr("");
}

#[test]
fn lint_advisory_only_is_visible_and_exits_zero() {
    let expected = report_with_gates(&[(
        "semantic.contradiction",
        LintOutcome::Finding,
        LintGateEffect::Advisory,
    )]);
    let (base, _) = spawn_report(&expected);

    cli()
        .env("WENLAN_HOST", base)
        .args(["lint", "--format", "table"])
        .assert()
        .code(0)
        .stdout(predicates::str::contains(
            "0 actionable findings, 1 advisory",
        ))
        .stdout(predicates::str::contains("Advisories (1):"))
        .stderr("");
}

#[test]
fn lint_incomplete_precedes_findings_and_exits_two() {
    let expected = report(&[
        ("pages.sample", LintOutcome::Finding),
        ("runtime.prerequisite", LintOutcome::NotRunPrerequisite),
        ("runtime.snapshot", LintOutcome::InconsistentSnapshot),
        ("runtime.execution", LintOutcome::FailedToRun),
    ]);
    let (base, _) = spawn_report(&expected);

    cli()
        .env("WENLAN_HOST", base)
        .args(["lint", "--format", "table"])
        .assert()
        .code(2)
        .stdout(predicates::str::contains("Findings (1):"))
        .stdout(predicates::str::contains("Incomplete (3):"))
        .stdout(predicates::str::contains(
            "runtime.prerequisite: prerequisite_unavailable; recommendation: restore_prerequisite",
        ))
        .stdout(predicates::str::contains(
            "runtime.snapshot: snapshot_inconsistent; recommendation: rerun_after_snapshot_stabilizes",
        ))
        .stdout(predicates::str::contains(
            "runtime.execution: execution_failed; recommendation: inspect_runtime",
        ))
        .stderr("");
}

#[test]
fn lint_quiet_suppresses_human_output_without_changing_exit() {
    let expected = report(&[("pages.sample", LintOutcome::Finding)]);
    let (base, _) = spawn_report(&expected);

    cli()
        .env("WENLAN_HOST", base)
        .args(["--quiet", "lint", "--format", "table"])
        .assert()
        .code(1)
        .stdout("")
        .stderr("");
}

#[test]
fn lint_transport_and_schema_failures_exit_two_on_stderr_only() {
    let host = closed_host();
    cli()
        .env("WENLAN_HOST", &host)
        .arg("lint")
        .assert()
        .code(2)
        .stdout("")
        .stderr(predicates::str::contains(format!(
            "GET {host}/api/lint failed"
        )));

    let mut unsupported = serde_json::to_value(report(&[("memories.sample", LintOutcome::Pass)]))
        .expect("serialize canonical report");
    unsupported["report_schema_version"] = json!(999);
    let (base, _) = spawn_value(&unsupported);
    cli()
        .env("WENLAN_HOST", base)
        .arg("lint")
        .assert()
        .code(2)
        .stdout("")
        .stderr(predicates::str::contains("unsupported_lint_report_schema"));
}

#[test]
fn lint_invalid_scope_preserves_typed_daemon_diagnostic() {
    let (base, _) = spawn_error(422, &json!({"error": "invalid_scope"}));

    cli()
        .env("WENLAN_HOST", base)
        .args(["--format", "json", "lint", "--space", "missing"])
        .assert()
        .code(2)
        .stdout("")
        .stderr("wenlan lint: invalid_scope\n");
}

#[test]
fn lint_rejects_oversized_daemon_response() {
    let (base, _) = spawn_oversized();

    cli()
        .env("WENLAN_HOST", base)
        .args(["--format", "json", "lint"])
        .assert()
        .code(2)
        .stdout("")
        .stderr(predicates::str::contains(
            "lint response exceeds 8388608 bytes",
        ));
}

#[test]
fn lint_does_not_create_wiki_command_or_change_pages_doctor_query() {
    cli()
        .args(["wiki", "check"])
        .assert()
        .code(2)
        .stdout("")
        .stderr(predicates::str::contains("unrecognized subcommand 'wiki'"));

    let home = tempfile::tempdir().expect("isolated home");
    cli()
        .env("HOME", home.path())
        .env("WENLAN_DATA_DIR", home.path())
        .args(["pages", "doctor", "--format", "table"])
        .assert()
        .code(0)
        .stdout("")
        .stderr(predicates::str::contains("no page matches: doctor"));
}
