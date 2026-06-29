#!/usr/bin/env bash
# PreToolUse guard for Edit/Write. Blocks an edit that adds an eval metric line
# (percent, or f1/accuracy/recall/precision/ndcg/mrr with a number) when the
# same edit carries no provenance token. Enforces the AGENTS.md single-run rule
# at the moment a number is about to land in a file.
#
# Blocks via the documented PreToolUse deny path: exit 0 with a
# hookSpecificOutput JSON object on stdout. Allows by exiting 0 with no output.
set -euo pipefail

INPUT="$(cat)"

# Edit carries new_string; Write carries content. MultiEdit is not matched here.
NEWTEXT="$(printf '%s' "$INPUT" | jq -r '.tool_input.new_string // .tool_input.content // empty')"
[ -z "$NEWTEXT" ] && exit 0

# Does the added text contain an eval-style metric?
# percent (e.g. 71.4%) OR a named metric followed within ~12 chars by a number.
METRIC_RE='([0-9]{1,3}(\.[0-9]+)?[[:space:]]*%)|((f1|accuracy|recall|precision|ndcg|mrr)[^0-9]{0,12}[0-9]+(\.[0-9]+)?)'
if ! printf '%s' "$NEWTEXT" | grep -qiE "$METRIC_RE"; then
  exit 0
fi

# Provenance present anywhere in the added text => allow.
# Multi-run (N>=3 + stddev) for headline claims, or an explicit scaffold tag +
# repro command for internal single-run snapshots.
PROVENANCE_RE='(N[[:space:]]*[=>][[:space:]]*[0-9]+)|(stddev)|(std dev)|(scaffold)|(single-run, treat as scaffold)|(repro:)'
if printf '%s' "$NEWTEXT" | grep -qiE "$PROVENANCE_RE"; then
  exit 0
fi

# Unicode escapes (>= and stddev sigma) checked separately so the ASCII-class
# regex above stays portable.
if printf '%s' "$NEWTEXT" | grep -qF '±'; then exit 0; fi
if printf '%s' "$NEWTEXT" | grep -qF '≥'; then exit 0; fi

jq -n '{
  hookSpecificOutput: {
    hookEventName: "PreToolUse",
    permissionDecision: "deny",
    permissionDecisionReason: "Eval-citation guard: this edit adds a metric (%, F1, accuracy, recall, precision, ndcg, mrr) with no provenance. AGENTS.md single-run rule requires inline methodology: N>=3 + stddev for headline claims, or an explicit \"scaffold\" tag + repro command for internal single-run snapshots. Add provenance or drop the number."
  }
}'
exit 0
