#!/usr/bin/env python3
"""Update README benchmark snapshots from local gitignored JSON.

Source (gitignored): ${EVAL_BASELINES_DIR:-~/.cache/origin-eval}/readme_metrics.json
Targets: README files with blocks between EVAL_SNAPSHOT_START / EVAL_SNAPSHOT_END
"""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
README = ROOT / "README.md"
TRANSLATED_READMES = ("README.zh-Hans.md", "README.zh-Hant.md")
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
        (("longmemeval_oracle", "longmemeval"), "LME_Oracle (500 Q)"),
        (("longmemeval_s",), "LME_S (deep, 90 Q)"),
    ]

    rows = []
    for keys, fallback_label in known:
        key = next((candidate for candidate in keys if candidate in benchmarks), None)
        if key is None:
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


def replace_snapshot(path: Path, table: str) -> bool:
    text = path.read_text(encoding="utf-8")
    start = text.find(START)
    end = text.find(END)
    if start == -1 or end == -1:
        raise SystemExit(f"{path}: markers not found: EVAL_SNAPSHOT_START / EVAL_SNAPSHOT_END")

    end += len(END)
    updated = text[:start] + table + text[end:]
    if updated == text:
        return False
    path.write_text(updated, encoding="utf-8")
    return True


def normalize_generated_snapshot(text: str) -> str:
    start = text.find(START)
    end = text.find(END)
    if start == -1 or end == -1:
        raise SystemExit("README markers not found: EVAL_SNAPSHOT_START / EVAL_SNAPSHOT_END")
    end += len(END)
    return text[:start] + START + "\n" + END + text[end:]


def readme_sync_hash(root: Path) -> str:
    text = (root / "README.md").read_text(encoding="utf-8")
    normalized = normalize_generated_snapshot(text)
    return hashlib.sha256(normalized.encode("utf-8")).hexdigest()


def update_tree(root: Path, table: str) -> int:
    changed = int(replace_snapshot(root / "README.md", table))

    for rel in TRANSLATED_READMES:
        path = root / rel
        if not path.exists():
            continue
        changed += int(replace_snapshot(path, table))

    return changed


def main() -> None:
    if not METRICS.exists():
        raise SystemExit(
            f"Missing local metrics file: {METRICS}\n"
            "Create it from docs/eval/readme_metrics.example.json first."
        )

    data = json.loads(METRICS.read_text(encoding="utf-8"))
    table = build_table(data)
    changed = update_tree(ROOT, table)
    print(f"Updated {changed} README file(s) from {METRICS}")


if __name__ == "__main__":
    main()
