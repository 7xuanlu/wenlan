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
    left_value = normalize(load(left))
    right_value = normalize(load(right))
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


def assert_report(args: argparse.Namespace) -> None:
    report = load(args.path)
    expected_complete = args.complete == "true"
    if report.get("complete") is not expected_complete:
        raise SystemExit(f"unexpected complete value in {args.path}")
    if report.get("scope", {}).get("kind") != args.scope:
        raise SystemExit(f"unexpected scope in {args.path}")
    producer = report.get("producer_receipt", {}).get("runtime_commit")
    expected_producer = None if args.producer == "null" else args.producer
    if producer != expected_producer:
        raise SystemExit(f"unexpected producer receipt in {args.path}: {producer!r}")
    outcomes = {check["check_id"]: check["outcome"] for check in report["checks"]}
    if args.finding and outcomes.get(args.finding) != "finding":
        raise SystemExit(f"missing expected finding {args.finding}")
    if args.incomplete and report.get("totals", {}).get("incomplete", 0) < 1:
        raise SystemExit("expected an incomplete report")


def assert_private(paths: list[str], canaries: list[str]) -> None:
    for path in paths:
        data = Path(path).read_bytes()
        for canary in canaries:
            if canary.encode() in data:
                raise SystemExit(f"privacy canary leaked in {path}: {canary}")


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
    with open(destination, "w", encoding="utf-8") as handle:
        json.dump(report, handle, separators=(",", ":"), sort_keys=True)


def serve_once(fixture: str, port_file: str) -> None:
    body = Path(fixture).read_bytes()

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
    report_cmd.add_argument("--finding")
    report_cmd.add_argument("--incomplete", action="store_true")
    private_cmd = commands.add_parser("assert-private")
    private_cmd.add_argument("--canary", action="append", required=True)
    private_cmd.add_argument("paths", nargs="+")
    clean_cmd = commands.add_parser("clean-fixture")
    clean_cmd.add_argument("source")
    clean_cmd.add_argument("destination")
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
    elif args.command == "clean-fixture":
        clean_fixture(args.source, args.destination)
    elif args.command == "serve-once":
        serve_once(args.fixture, args.port_file)


if __name__ == "__main__":
    main()
