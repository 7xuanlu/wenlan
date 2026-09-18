#!/usr/bin/env bash
# Stop hook: advisory scan for high-signal unfinished work.
# - Scope drift (diff stat shown for context, not blocking)
# - High-signal unfinished markers (assert!(true), todo!(), unimplemented!(), FIXME)
# This hook is advisory only: dirty source may belong to another actor or a
# previous run, so it must not claim completion proof or block a stop.

set -eo pipefail

REPO="${CLAUDE_PROJECT_DIR:-$(pwd)}"
cd "$REPO"

[ -f "$REPO/Cargo.toml" ] || exit 0

# Collect changed source files (staged + unstaged + untracked, Rust and TypeScript)
FILES_LIST=$({
  git diff --name-only --diff-filter=ACMR -- '*.rs' '*.ts' '*.tsx' 2>/dev/null
  git diff --name-only --cached --diff-filter=ACMR -- '*.rs' '*.ts' '*.tsx' 2>/dev/null
  git ls-files --others --exclude-standard -- '*.rs' '*.ts' '*.tsx' 2>/dev/null
} | sort -u)

PROBLEMS=()
FILES=()
if [ -n "$FILES_LIST" ]; then
  while IFS= read -r f; do
    [ -n "$f" ] && FILES+=("$f")
  done <<< "$FILES_LIST"
fi

if [ "${#FILES[@]}" -gt 0 ]; then
  # High-signal "fake / unfinished" tells only. Look at NEW lines (added in diff).
  # Deliberately NOT flagged (too many legit uses → false-positive noise):
  #   #[ignore] (87 legit GPU/eval tests), let _ = (189 legit discards),
  #   unreachable!() (exhaustive match arms), #[allow(dead_code)] (test scaffolding).
  PATTERNS=(
    'assert!\s*\(\s*true\s*\)'
    'assert_eq!\s*\(\s*true\s*,\s*true\s*\)'
    'todo!\s*\('
    'unimplemented!\s*\('
    '^[+ ]*\s*//.*FIXME'
    'assert[[:space:]]*\([[:space:]]*true[[:space:]]*\)'
    'expect[[:space:]]*\([[:space:]]*true[[:space:]]*\)[[:space:]]*\.toBe[[:space:]]*\([[:space:]]*true[[:space:]]*\)'
    '(^|[^[:alnum:]_])(it|test)\.skip[[:space:]]*\('
    '(^|[^[:alnum:]_])xit[[:space:]]*\('
  )

  # Added lines from both unstaged and staged changes. Untracked files are counted
  # whole-file below.
  ADDED_LINES="$({
    git diff --unified=0 -- "${FILES[@]}" 2>/dev/null
    git diff --cached --unified=0 -- "${FILES[@]}" 2>/dev/null
  } | grep -E '^\+[^+]' || true)"
  # For untracked source files, include all lines
  for f in "${FILES[@]}"; do
    if git ls-files --error-unmatch "$f" >/dev/null 2>&1; then continue; fi
    [ -f "$f" ] && ADDED_LINES+=$'\n'"$(sed 's/^/+ /' "$f")"
  done

  for pat in "${PATTERNS[@]}"; do
    if HIT=$(printf '%s\n' "$ADDED_LINES" | grep -nE "$pat" || true); [ -n "$HIT" ]; then
      PROBLEMS+=("⚠ pattern '$pat' in changes:")
      while IFS= read -r line; do PROBLEMS+=("    $line"); done <<< "$HIT"
    fi
  done
fi

# NOTE: no cargo invocation here by design. A workspace test-compile on every Stop
# starved on the target/ build lock, timed out, and false-blocked. Verification scope
# belongs to the task, explicit repo commands, and CI; this hook is only an instant
# pattern scan.

if [ "${#PROBLEMS[@]}" -gt 0 ]; then
  {
    echo "⚠ pre-stop-gate advisory — possible unfinished work in changed source:"
    echo
    git diff --stat 2>/dev/null | tail -20
    echo
    for line in "${PROBLEMS[@]}"; do echo "$line"; done
    echo
    echo "Review these findings if they belong to this task; this scan is advisory only."
  } >&2
  printf '%s\n' '{"systemMessage":"pre-stop-gate advisory: possible unfinished work in changed source; review findings if they belong to this task"}'
fi

exit 0
