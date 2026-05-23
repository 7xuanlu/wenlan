#!/usr/bin/env bash
# Plain-bash test harness for resolve-space.sh.
# Each test sets up env then asserts stdout matches expected.
# Exits 1 on first failure.

set -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESOLVER="$SCRIPT_DIR/../resolve-space.sh"

pass=0
fail=0

assert_eq() {
    local name="$1"
    local expected="$2"
    local actual="$3"
    if [ "$expected" = "$actual" ]; then
        printf 'PASS  %s\n' "$name"
        pass=$((pass + 1))
    else
        printf 'FAIL  %s\n  expected: %q\n  actual:   %q\n' \
            "$name" "$expected" "$actual" >&2
        fail=$((fail + 1))
    fi
}

# --- Test 1: bare invocation returns default "personal" from "default" layer
out="$(ORIGIN_SPACE='' "$RESOLVER" --cwd /tmp 2>/dev/null)"
assert_eq 'bare invocation -> personal/default' \
    'personal	default' \
    "$out"

# --- Test 2: --topic falls back to topic when no higher layer hits
out="$(ORIGIN_SPACE='' "$RESOLVER" --cwd /tmp --topic 'career-research' 2>/dev/null)"
assert_eq 'topic fallback -> career-research/topic' \
    'career-research	topic' \
    "$out"

# --- Test 3: --cwd inside a git repo returns the repo basename
tmpdir="$(mktemp -d)"
cd "$tmpdir"
git init -q
out="$(ORIGIN_SPACE='' "$RESOLVER" --cwd "$tmpdir" 2>/dev/null)"
expected_name="$(basename "$tmpdir")"
assert_eq 'cwd-repo inside git -> basename/cwd-repo' \
    "${expected_name}	cwd-repo" \
    "$out"
cd - >/dev/null
rm -rf "$tmpdir"

printf '\n%d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
