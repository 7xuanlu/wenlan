#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("lint-e2e.py")
SPEC = importlib.util.spec_from_file_location("lint_e2e", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
lint_e2e = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(lint_e2e)


class ValidateReportTests(unittest.TestCase):
    def test_accepts_valid_totals_with_additive_fields(self) -> None:
        report = {
            "checks": [
                {
                    "check_id": "test.pass",
                    "outcome": "pass",
                    "gate_effect": "actionable",
                    "severity": "info",
                },
                {
                    "check_id": "test.warning",
                    "outcome": "finding",
                    "gate_effect": "advisory",
                    "severity": "warning",
                },
            ],
            "totals": {
                "checks": 2,
                "passed": 1,
                "findings": 1,
                "actionable_findings": 0,
                "advisory_findings": 1,
                "incomplete": 0,
                "future_counter": 3,
            },
            "complete": True,
        }

        lint_e2e.validate_report(report)

    def test_clean_fixture_keeps_gate_totals_consistent(self) -> None:
        report = route_finding_report()
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "source.json"
            destination = Path(directory) / "clean.json"
            source.write_text(json.dumps(report), encoding="utf-8")

            lint_e2e.clean_fixture(str(source), str(destination))

            clean = json.loads(destination.read_text(encoding="utf-8"))
            lint_e2e.validate_report(clean)
            self.assertEqual(clean["totals"]["actionable_findings"], 0)

    def test_precedence_fixture_keeps_gate_totals_consistent(self) -> None:
        finding = route_finding_report()
        incomplete = {
            "checks": [
                terminal_check("serving.route_scope_contracts"),
                terminal_check("test.still_incomplete"),
            ],
            "totals": {
                "checks": 2,
                "passed": 0,
                "findings": 0,
                "actionable_findings": 0,
                "advisory_findings": 0,
                "incomplete": 2,
            },
            "complete": False,
        }
        with tempfile.TemporaryDirectory() as directory:
            finding_source = Path(directory) / "finding.json"
            incomplete_source = Path(directory) / "incomplete.json"
            destination = Path(directory) / "precedence.json"
            finding_source.write_text(json.dumps(finding), encoding="utf-8")
            incomplete_source.write_text(json.dumps(incomplete), encoding="utf-8")

            lint_e2e.precedence_fixture(
                str(finding_source), str(incomplete_source), str(destination)
            )

            precedence = json.loads(destination.read_text(encoding="utf-8"))
            lint_e2e.validate_report(precedence)
            self.assertEqual(precedence["totals"]["actionable_findings"], 1)


def route_finding_report() -> dict[str, object]:
    return {
        "checks": [
            {
                "check_id": "serving.route_scope_contracts",
                "outcome": "finding",
                "gate_effect": "actionable",
                "severity": "error",
                "summary_code": "finding_detected",
                "recommendation_code": "review_finding",
            }
        ],
        "totals": {
            "checks": 1,
            "passed": 0,
            "findings": 1,
            "actionable_findings": 1,
            "advisory_findings": 0,
            "incomplete": 0,
        },
        "complete": True,
    }


def terminal_check(check_id: str) -> dict[str, object]:
    return {
        "check_id": check_id,
        "outcome": "failed_to_run",
        "gate_effect": "actionable",
        "severity": "error",
    }


if __name__ == "__main__":
    unittest.main()
