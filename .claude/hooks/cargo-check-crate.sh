#!/usr/bin/env bash
# PostToolUse(Edit|Write): per-crate `cargo check` after a .rs edit.
# Runs on an ISOLATED target dir (~/.cache/origin-hook-target — legacy 'origin' name kept to preserve the warm cache) so it never fights
# rust-analyzer for the workspace build lock. Surfaces compile errors to Claude.
set -euo pipefail

export CARGO_TARGET_DIR="${HOME}/.cache/origin-hook-target"

input="$(cat)"
# Loud, not silent: jq failure surfaces to Claude (PostToolUse exit 2 shows
# stderr) instead of a fail-open crash vanishing the compile check.
file_path="$(echo "$input" | jq -r '.tool_input.file_path // empty')" || {
  echo "cargo-check hook: could not parse tool input (jq missing?) — compile check skipped." >&2
  exit 2
}

# Only .rs files under crates/<name>/
[[ "$file_path" == *.rs ]] || exit 0
[[ "$file_path" =~ /crates/([^/]+)/ ]] || exit 0
crate="${BASH_REMATCH[1]}"
# Dir name != package name for the CLI: crates/wenlan-cli ships package `wenlan`.
[[ "$crate" == "wenlan-cli" ]] && crate="wenlan"

cd "$CLAUDE_PROJECT_DIR" || exit 0

# `if ! ...` so `set -e` doesn't exit at the assignment before we read the output.
if ! output="$(cargo check -p "$crate" 2>&1)"; then
  echo "cargo check -p $crate failed:" >&2
  echo "$output" | tail -30 >&2
  exit 2  # blocking: Claude sees stderr and must fix
fi

exit 0
