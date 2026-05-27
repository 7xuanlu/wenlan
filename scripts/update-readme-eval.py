#!/usr/bin/env python3
"""Update README benchmark snapshot from local gitignored JSON.

Source (gitignored): ${EVAL_BASELINES_DIR:-~/.cache/origin-eval}/readme_metrics.json
Target: README.md block between EVAL_SNAPSHOT_START / EVAL_SNAPSHOT_END
"""

from __future__ import annotations

import json
import os
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
README = ROOT / "README.md"
BASELINES_DIR = Path(
    os.environ.get("EVAL_BASELINES_DIR", str(Path.home() / ".cache" / "origin-eval"))
).expanduser()
METRICS = BASELINES_DIR / "readme_metrics.json"
START = "<!-- EVAL_SNAPSHOT_START -->"
END = "<!-- EVAL_SNAPSHOT_END -->"


def pct(v: float | None) -> str:
    if v is None:
        return "-"
    return f"{v * 100:.1f}%"


def score(v: float | None) -> str:
    if v is None:
        return "-"
    return f"{v:.3f}"


def benchmark_rows(data: dict) -> list[dict]:
    benchmarks = data.get("benchmarks", {})
    known = [
        ("longmemeval", "LongMemEval (oracle, 500 Q)"),
        ("locomo", "LoCoMo (locomo10)"),
        ("locomo_plus", "LoCoMo-Plus"),
        ("lifebench", "LifeBench"),
    ]

    rows = []
    for key, fallback_label in known:
        if key not in benchmarks:
            continue
        row = dict(benchmarks[key])
        row.setdefault("label", fallback_label)
        rows.append(row)
    return rows


def build_table(data: dict) -> str:
    lines = [
        START,
        "| Benchmark | Recall@5 | MRR | NDCG@10 |",
        "|---|---:|---:|---:|",
    ]
    for row in benchmark_rows(data):
        lines.append(
            f"| {row['label']} | {pct(row.get('recall_at_5'))} | {score(row.get('mrr'))} | "
            f"{score(row.get('ndcg_at_10'))} |"
        )
    lines.append(END)
    return "\n".join(lines)


def main() -> None:
    if not METRICS.exists():
        raise SystemExit(
            f"Missing local metrics file: {METRICS}\n"
            "Create it from docs/eval/readme_metrics.example.json first."
        )

    data = json.loads(METRICS.read_text(encoding="utf-8"))
    readme = README.read_text(encoding="utf-8")
    start = readme.find(START)
    end = readme.find(END)
    if start == -1 or end == -1:
        raise SystemExit("README markers not found: EVAL_SNAPSHOT_START / EVAL_SNAPSHOT_END")

    end += len(END)
    table = build_table(data)
    updated = readme[:start] + table + readme[end:]
    README.write_text(updated, encoding="utf-8")
    print(f"Updated {README} from {METRICS}")


if __name__ == "__main__":
    main()
