// SPDX-License-Identifier: Apache-2.0
use assert_cmd::Command;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::mpsc::{self, Receiver};
use std::thread;

fn cli() -> Command {
    Command::cargo_bin("wenlan").expect("wenlan binary built")
}

fn check(id: &str, outcome: &str) -> Value {
    let (severity, applicability, precondition, summary, recommendation) = match outcome {
        "pass" => ("info", "applicable", "ready", "check_passed", None),
        "finding" => (
            "warning",
            "applicable",
            "ready",
            "finding_detected",
            Some("review_finding"),
        ),
        "not_run_prerequisite" => (
            "error",
            "not_applicable",
            "missing_prerequisite",
            "prerequisite_unavailable",
            Some("restore_prerequisite"),
        ),
        other => panic!("unsupported fixture outcome: {other}"),
    };
    json!({
        "check_id": id,
        "outcome": outcome,
        "severity": severity,
        "applicability": applicability,
        "precondition": precondition,
        "coverage": {
            "method": "full_enumeration", "authorized_denominator": 0,
            "evaluated": 0, "evidence_cap": 100, "truncated": false,
            "evidence_returned": 0
        },
        "metrics": [], "summary_code": summary,
        "recommendation_code": recommendation, "evidence": [], "duration_ms": 1
    })
}

fn report(checks: Vec<Value>) -> Value {
    let findings = checks
        .iter()
        .filter(|item| item["outcome"] == "finding")
        .count();
    let incomplete = checks
        .iter()
        .filter(|item| item["outcome"] == "not_run_prerequisite")
        .count();
    json!({
        "report_schema_version": 1, "check_catalog_version": 1,
        "profile": "deterministic", "scope": {"kind": "global"},
        "capability_context": "daemon_operator_endpoint_unauthenticated_unverified",
        "snapshots": {
            "db": {"mode": "transactional_read_only", "analysis_digest": "0000000000000001", "post_run_digest": "0000000000000001"},
            "pages": {"mode": "best_effort", "before_scan_digest": "0000000000000002", "after_scan_digest": "0000000000000002"}
        },
        "config_fingerprint": "0000000000000003",
        "producer_receipt": {"runtime_commit": null},
        "totals": {
            "checks": checks.len(),
            "passed": checks.len() - findings - incomplete,
            "findings": findings, "incomplete": incomplete
        },
        "complete": incomplete == 0, "checks": checks
    })
}

fn spawn_report(body: &Value) -> (String, Receiver<String>) {
    spawn_body(&body.to_string())
}

fn spawn_body(body: &str) -> (String, Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture");
    let base = format!("http://{}", listener.local_addr().expect("fixture address"));
    let response_body = body.to_string();
    let (sent, received) = mpsc::channel();
    thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept request");
        let mut reader = BufReader::new(stream);
        let mut request_line = String::new();
        reader.read_line(&mut request_line).expect("request line");
        let _ = sent.send(request_line);
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).expect("request header");
            if line == "\r\n" || line.is_empty() {
                break;
            }
        }
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_body.len(), response_body
        );
        reader
            .get_mut()
            .write_all(response.as_bytes())
            .expect("write response");
    });
    (base, received)
}

#[test]
fn lint_json_accepts_global_flags_before_command_and_uses_remote_scope() {
    let expected = report(vec![check("memories.sample", "pass")]);
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

    assert_eq!(serde_json::from_slice::<Value>(&output).unwrap(), expected);
    assert_eq!(
        request.recv().unwrap(),
        "GET /api/lint?space=work HTTP/1.1\r\n"
    );
}

#[test]
fn lint_json_accepts_global_flags_after_command_and_quiet_keeps_json() {
    let expected = report(vec![check("pages.sample", "finding")]);
    let (base, _) = spawn_report(&expected);

    let output = cli()
        .env("WENLAN_HOST", base)
        .args(["lint", "--format", "json", "--quiet"])
        .assert()
        .code(1)
        .stderr("")
        .get_output()
        .stdout
        .clone();

    assert_eq!(serde_json::from_slice::<Value>(&output).unwrap(), expected);
}

#[test]
fn lint_human_clean_report_summarizes_groups_and_exits_zero() {
    let expected = report(vec![check("memories.sample", "pass")]);
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
    let expected = report(vec![check("pages.sample", "finding")]);
    let (base, _) = spawn_report(&expected);

    cli()
        .env("WENLAN_HOST", base)
        .args(["lint", "--format", "table"])
        .assert()
        .code(1)
        .stdout(predicates::str::contains("Findings (1):"))
        .stdout(predicates::str::contains("pages.sample: finding_detected"))
        .stderr("");
}

#[test]
fn lint_incomplete_precedes_findings_and_exits_two() {
    let expected = report(vec![
        check("pages.sample", "finding"),
        check("runtime.sample", "not_run_prerequisite"),
    ]);
    let (base, _) = spawn_report(&expected);

    cli()
        .env("WENLAN_HOST", base)
        .args(["lint", "--format", "table"])
        .assert()
        .code(2)
        .stdout(predicates::str::contains("Findings (1):"))
        .stdout(predicates::str::contains("Incomplete (1):"))
        .stdout(predicates::str::contains(
            "runtime.sample: prerequisite_unavailable",
        ))
        .stderr("");
}

#[test]
fn lint_quiet_suppresses_human_output_without_changing_exit() {
    let expected = report(vec![check("pages.sample", "finding")]);
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
    cli()
        .env("WENLAN_HOST", "http://127.0.0.1:9")
        .arg("lint")
        .assert()
        .code(2)
        .stdout("")
        .stderr(predicates::str::contains(
            "GET http://127.0.0.1:9/api/lint failed",
        ));

    let (base, _) = spawn_body(r#"{"report_schema_version":999}"#);
    cli()
        .env("WENLAN_HOST", base)
        .arg("lint")
        .assert()
        .code(2)
        .stdout("")
        .stderr(predicates::str::contains("parsing /api/lint response"));
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
