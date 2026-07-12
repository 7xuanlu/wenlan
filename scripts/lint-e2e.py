#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
import os
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
from typing import Any


VOLATILE_KEYS = {"duration_ms", "observed_at"}
VOLATILE_FILE_SUFFIXES = ("-shm",)
INCOMPLETE_OUTCOMES = {
    "not_run_prerequisite",
    "inconsistent_snapshot",
    "failed_to_run",
}
MAX_BODY_BYTES = 8 * 1024 * 1024


def normalize(value: Any) -> Any:
    if isinstance(value, dict):
        return {
            key: 0 if key in VOLATILE_KEYS else normalize(item)
            for key, item in value.items()
        }
    if isinstance(value, list):
        return [normalize(item) for item in value]
    return value


def load(path: str) -> Any:
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def compare(left: str, right: str) -> None:
    left_report = load(left)
    right_report = load(right)
    validate_report(left_report)
    validate_report(right_report)
    left_value = normalize(left_report)
    right_value = normalize(right_report)
    if left_value != right_value:
        raise SystemExit("normalized HTTP and CLI reports differ")


def hash_path(hasher: Any, root: Path, path: Path) -> None:
    relative = path.relative_to(root).as_posix().encode()
    hasher.update(relative)
    if path.is_symlink():
        hasher.update(b"L")
        hasher.update(os.readlink(path).encode())
    elif path.is_dir():
        hasher.update(b"D")
    elif path.is_file():
        hasher.update(b"F")
        with path.open("rb") as handle:
            for chunk in iter(lambda: handle.read(1024 * 1024), b""):
                hasher.update(chunk)


def fingerprint(paths: list[str]) -> str:
    hasher = hashlib.sha256()
    for raw in sorted(paths):
        root = Path(raw)
        hasher.update(str(root).encode())
        if not root.exists() and not root.is_symlink():
            hasher.update(b"MISSING")
            continue
        hash_path(hasher, root.parent, root)
        if root.is_dir():
            for path in sorted(root.rglob("*"), key=lambda item: item.as_posix()):
                if path.name.endswith(VOLATILE_FILE_SUFFIXES):
                    continue
                hash_path(hasher, root, path)
    return hasher.hexdigest()


def validate_report(report: Any) -> None:
    checks = report.get("checks")
    if not isinstance(checks, list):
        raise SystemExit("report checks must be an array")
    ids = [check.get("check_id") for check in checks]
    if any(not isinstance(check_id, str) for check_id in ids):
        raise SystemExit("every check must have a string check_id")
    if len(ids) != len(set(ids)) or ids != sorted(ids):
        raise SystemExit("check IDs must be unique and deterministically ordered")
    outcomes = [check.get("outcome") for check in checks]
    allowed = {"pass", "finding", *INCOMPLETE_OUTCOMES}
    if any(outcome not in allowed for outcome in outcomes):
        raise SystemExit("report contains an unknown outcome")
    if any(
        check.get("severity") != "info"
        for check in checks
        if check.get("outcome") == "pass"
    ):
        raise SystemExit("pass outcomes must remain informational")
    expected = {
        "checks": len(checks),
        "passed": outcomes.count("pass"),
        "findings": outcomes.count("finding"),
        "actionable_findings": sum(
            check.get("outcome") == "finding"
            and check.get("gate_effect") == "actionable"
            for check in checks
        ),
        "advisory_findings": sum(
            check.get("outcome") == "finding"
            and check.get("gate_effect") == "advisory"
            for check in checks
        ),
        "incomplete": sum(outcome in INCOMPLETE_OUTCOMES for outcome in outcomes),
    }
    totals = report.get("totals")
    if not isinstance(totals, dict) or any(
        totals.get(key) != value for key, value in expected.items()
    ):
        raise SystemExit(f"report totals do not match outcomes: {expected}")
    if report.get("complete") is not (expected["incomplete"] == 0):
        raise SystemExit("report completeness does not match outcomes")


def assert_report(args: argparse.Namespace) -> None:
    report = load(args.path)
    validate_report(report)
    expected_complete = args.complete == "true"
    if report.get("complete") is not expected_complete:
        raise SystemExit(f"unexpected complete value in {args.path}")
    if report.get("scope", {}).get("kind") != args.scope:
        raise SystemExit(f"unexpected scope in {args.path}")
    producer = report.get("producer_receipt", {}).get("runtime_commit")
    expected_producer = None if args.producer == "null" else args.producer
    if producer != expected_producer:
        raise SystemExit(f"unexpected producer receipt in {args.path}: {producer!r}")
    findings = sorted(
        check["check_id"] for check in report["checks"] if check["outcome"] == "finding"
    )
    if findings != sorted(args.finding):
        raise SystemExit(f"unexpected finding set in {args.path}: {findings}")
    if args.incomplete and report.get("totals", {}).get("incomplete", 0) < 1:
        raise SystemExit("expected an incomplete report")


def assert_private(paths: list[str], canaries: list[str]) -> None:
    for path in paths:
        data = Path(path).read_bytes()
        for canary in canaries:
            if canary.encode() in data:
                raise SystemExit(f"privacy canary leaked in {path}: {canary}")


def assert_error(http_path: str, cli_stderr_path: str) -> None:
    envelope = load(http_path)
    error = envelope.get("error") if isinstance(envelope, dict) else None
    if not isinstance(error, str):
        raise SystemExit("HTTP error envelope is not typed")
    stderr = Path(cli_stderr_path).read_text(encoding="utf-8")
    if stderr != f"wenlan lint: {error}\n":
        raise SystemExit("CLI diagnostic does not match HTTP error envelope")


def clean_fixture(source: str, destination: str) -> None:
    report = load(source)
    findings = [check for check in report["checks"] if check["outcome"] == "finding"]
    if [check["check_id"] for check in findings] != ["serving.route_scope_contracts"]:
        raise SystemExit("clean fixture requires the route-scope finding to be the only finding")
    if report["totals"]["incomplete"] != 0:
        raise SystemExit("clean fixture source must be complete")
    finding = findings[0]
    finding["outcome"] = "pass"
    finding["severity"] = "info"
    finding["summary_code"] = "check_passed"
    finding["recommendation_code"] = None
    report["totals"]["passed"] += 1
    report["totals"]["findings"] = 0
    report["complete"] = True
    validate_report(report)
    with open(destination, "w", encoding="utf-8") as handle:
        json.dump(report, handle, separators=(",", ":"), sort_keys=True)


def precedence_fixture(finding_source: str, incomplete_source: str, destination: str) -> None:
    finding_report = load(finding_source)
    findings = [
        check for check in finding_report["checks"] if check["outcome"] == "finding"
    ]
    if [check["check_id"] for check in findings] != ["serving.route_scope_contracts"]:
        raise SystemExit("precedence fixture requires the canonical route-scope finding")
    report = load(incomplete_source)
    if report["complete"] or report["totals"]["incomplete"] < 1:
        raise SystemExit("precedence fixture source must be incomplete")
    target = next(
        check
        for check in report["checks"]
        if check["check_id"] == "serving.route_scope_contracts"
    )
    if target["outcome"] == "finding":
        raise SystemExit("incomplete source unexpectedly retained the finding")
    if target["outcome"] == "pass":
        report["totals"]["passed"] -= 1
    else:
        report["totals"]["incomplete"] -= 1
    report["totals"]["findings"] += 1
    report["checks"][report["checks"].index(target)] = findings[0]
    validate_report(report)
    with open(destination, "w", encoding="utf-8") as handle:
        json.dump(report, handle, separators=(",", ":"), sort_keys=True)


def serve_once(fixture: str, port_file: str) -> None:
    body = Path(fixture).read_bytes()
    if len(body) > MAX_BODY_BYTES:
        raise SystemExit("fixture exceeds response-size bound")

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            if not self.path.startswith("/api/lint"):
                self.send_error(404)
                return
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, format: str, *args: Any) -> None:
            del format, args

    server = HTTPServer(("127.0.0.1", 0), Handler)
    Path(port_file).write_text(str(server.server_port), encoding="utf-8")
    server.handle_request()


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser()
    commands = root.add_subparsers(dest="command", required=True)
    compare_cmd = commands.add_parser("compare")
    compare_cmd.add_argument("left")
    compare_cmd.add_argument("right")
    fingerprint_cmd = commands.add_parser("fingerprint")
    fingerprint_cmd.add_argument("paths", nargs="+")
    report_cmd = commands.add_parser("assert-report")
    report_cmd.add_argument("path")
    report_cmd.add_argument("--complete", choices=["true", "false"], required=True)
    report_cmd.add_argument(
        "--scope", choices=["global", "registered", "uncategorized"], required=True
    )
    report_cmd.add_argument("--producer", required=True)
    report_cmd.add_argument("--finding", action="append", default=[])
    report_cmd.add_argument("--incomplete", action="store_true")
    private_cmd = commands.add_parser("assert-private")
    private_cmd.add_argument("--canary", action="append", required=True)
    private_cmd.add_argument("paths", nargs="+")
    error_cmd = commands.add_parser("assert-error")
    error_cmd.add_argument("http_path")
    error_cmd.add_argument("cli_stderr_path")
    clean_cmd = commands.add_parser("clean-fixture")
    clean_cmd.add_argument("source")
    clean_cmd.add_argument("destination")
    precedence_cmd = commands.add_parser("precedence-fixture")
    precedence_cmd.add_argument("finding_source")
    precedence_cmd.add_argument("incomplete_source")
    precedence_cmd.add_argument("destination")
    serve_cmd = commands.add_parser("serve-once")
    serve_cmd.add_argument("fixture")
    serve_cmd.add_argument("port_file")
    return root


def main() -> None:
    args = parser().parse_args()
    if args.command == "compare":
        compare(args.left, args.right)
    elif args.command == "fingerprint":
        print(fingerprint(args.paths))
    elif args.command == "assert-report":
        assert_report(args)
    elif args.command == "assert-private":
        assert_private(args.paths, args.canary)
    elif args.command == "assert-error":
        assert_error(args.http_path, args.cli_stderr_path)
    elif args.command == "clean-fixture":
        clean_fixture(args.source, args.destination)
    elif args.command == "precedence-fixture":
        precedence_fixture(
            args.finding_source, args.incomplete_source, args.destination
        )
    elif args.command == "serve-once":
        serve_once(args.fixture, args.port_file)


if __name__ == "__main__":
    main()
