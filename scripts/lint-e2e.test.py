#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
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


if __name__ == "__main__":
    unittest.main()
