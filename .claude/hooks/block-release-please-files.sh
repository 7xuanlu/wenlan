#!/usr/bin/env bash
# Block edits to files owned by release-please.
# - CHANGELOG.md (generated)
# - Cargo.toml version lines (marked `# x-release-please-version`)
# Manual edits break the next release PR.

set -euo pipefail

# Hook receives tool input as JSON on stdin.
input="$(cat)"

# Fail CLOSED: a jq failure (missing binary, bad JSON) previously killed the
# script under `set -euo pipefail` with a non-2 exit — which Claude Code
# treats as allow, silently disabling this guard.
file_path="$(echo "$input" | jq -r '.tool_input.file_path // empty')" || {
  echo "BLOCKED: release-please guard could not parse tool input (jq missing?) — failing closed." >&2
  exit 2
}

if [[ -z "$file_path" ]]; then
  exit 0
fi

# Block CHANGELOG.md anywhere
if [[ "$file_path" == */CHANGELOG.md || "$file_path" == CHANGELOG.md ]]; then
  echo "BLOCKED: $file_path is managed by release-please. Update by merging a PR with a conventional commit." >&2
  exit 2
fi

# Block Cargo.toml ONLY if it has release-please version marker
if [[ "$file_path" == */Cargo.toml || "$file_path" == Cargo.toml ]]; then
  if [[ -f "$file_path" ]] && grep -q 'x-release-please-version' "$file_path" 2>/dev/null; then
    echo "BLOCKED: $file_path has release-please-managed version line. Bump via release-please PR, not direct edit." >&2
    exit 2
  fi
fi

exit 0
