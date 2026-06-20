#!/usr/bin/env bash
# Local weekly drift audit. Runs the read-only doc-drift-auditor agent headless
# and writes a timestamped report. Schedule via cron/launchd or `/loop 7d`.
# Requires: claude CLI on PATH, run from anywhere inside the repo.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
OUT="docs/superpowers/drift-reports"
mkdir -p "$OUT"
STAMP="$(date +%Y-%m-%d)"
claude -p "Use the doc-drift-auditor agent to audit this repo. Print the full findings report." \
  > "$OUT/drift-$STAMP.md" 2>&1
echo "drift report -> $OUT/drift-$STAMP.md"
