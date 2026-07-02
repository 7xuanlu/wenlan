#!/usr/bin/env bash
# Plain-bash test harness for plugin-codex/bin/resolve-space.sh.
# Each test sets up env then asserts stdout matches expected.
# Exits 1 on first failure.
#
# Spec: docs/superpowers/specs/2026-05-23-space-ux-gap-design.md
# Plan: docs/superpowers/plans/2026-05-23-space-plan-a-resolver.md
#
# Run from any cwd:
#   ./plugin-codex/bin/test/test-resolve-space.sh

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

# --- Test 1: bare invocation returns no resolved space
out="$(WENLAN_SPACE='' "$RESOLVER" --cwd /tmp 2>/dev/null)"
assert_eq 'bare invocation -> unscoped' \
    $'\tunscoped' \
    "$out"

# --- Test 2: --topic falls back to topic when no higher layer hits
out="$(WENLAN_SPACE='' "$RESOLVER" --cwd /tmp --topic 'career-research' 2>/dev/null)"
assert_eq 'topic fallback -> career-research/topic' \
    'career-research	topic' \
    "$out"

# --- Test 3: --cwd inside a git repo returns the repo basename
tmpdir="$(mktemp -d)"
cd "$tmpdir"
git init -q
out="$(WENLAN_SPACE='' "$RESOLVER" --cwd "$tmpdir" 2>/dev/null)"
expected_name="$(basename "$tmpdir")"
assert_eq 'cwd-repo inside git -> basename/cwd-repo' \
    "${expected_name}	cwd-repo" \
    "$out"
cd - >/dev/null
rm -rf "$tmpdir"

# --- Test 3b: --cwd inside a linked worktree returns the canonical repo basename
tmpbase="$(mktemp -d)"
repo="$tmpbase/canonical-repo"
worktree="$tmpbase/feature-worktree"
mkdir -p "$repo"
cd "$repo"
git init -q
printf 'seed\n' > README.md
git add README.md
git -c user.name='Wenlan Test' -c user.email='wenlan@example.com' commit -q -m init
git worktree add -q -b feature/test "$worktree"
out="$(WENLAN_SPACE='' "$RESOLVER" --cwd "$worktree" 2>/dev/null)"
assert_eq 'linked worktree cwd-repo -> canonical repo basename/cwd-repo' \
    'canonical-repo	cwd-repo' \
    "$out"
cd - >/dev/null
rm -rf "$tmpbase"

# --- Test 4: cwd inside a configured prefix returns the mapped space
out="$(SPACES_FILE="$SCRIPT_DIR/fixtures/spaces-basic.toml" WENLAN_SPACE='' "$RESOLVER" --cwd /tmp/origin-test/career/foo 2>/dev/null)"
assert_eq 'cwd-config prefix match -> career/cwd-config' \
    'career	cwd-config' \
    "$out"

# --- Test 5: cwd matching two prefixes returns the longest match
mkdir -p /tmp/origin-test/extra
cat > /tmp/origin-test/spaces-two.toml <<'EOF'
[[mapping]]
prefix = "/tmp/origin-test"
space  = "outer"

[[mapping]]
prefix = "/tmp/origin-test/extra"
space  = "inner"
EOF
out="$(SPACES_FILE=/tmp/origin-test/spaces-two.toml WENLAN_SPACE='' "$RESOLVER" --cwd /tmp/origin-test/extra/leaf 2>/dev/null)"
assert_eq 'cwd-config longest prefix wins -> inner/cwd-config' \
    'inner	cwd-config' \
    "$out"
rm -f /tmp/origin-test/spaces-two.toml

# --- Test 6: cwd outside any mapping + default key in TOML -> uses cwd-config-default
out="$(SPACES_FILE="$SCRIPT_DIR/fixtures/spaces-basic.toml" WENLAN_SPACE='' "$RESOLVER" --cwd /opt/somewhere-unmapped 2>/dev/null)"
assert_eq 'cwd-config no match with default key -> personal/cwd-config-default' \
    'personal	cwd-config-default' \
    "$out"

# --- Test 7: malformed TOML falls through to next layer; never crashes
out="$(SPACES_FILE="$SCRIPT_DIR/fixtures/spaces-malformed.toml" WENLAN_SPACE='' "$RESOLVER" --cwd /opt/no-repo-here 2>/dev/null)"
assert_eq 'malformed TOML -> falls through to unscoped' \
    $'\tunscoped' \
    "$out"

# --- Test 8: missing TOML file -> falls through to next layer
out="$(SPACES_FILE=/tmp/this-does-not-exist.toml WENLAN_SPACE='' "$RESOLVER" --cwd /opt/no-repo-here 2>/dev/null)"
assert_eq 'missing TOML file -> falls through to unscoped' \
    $'\tunscoped' \
    "$out"

# --- Test 9: WENLAN_SPACE env var overrides cwd-config + cwd-repo
out="$(SPACES_FILE="$SCRIPT_DIR/fixtures/spaces-basic.toml" WENLAN_SPACE='health' "$RESOLVER" --cwd /tmp/origin-test/career/foo 2>/dev/null)"
assert_eq 'env overrides cwd-config -> health/env' \
    'health	env' \
    "$out"

# --- Test 10: empty WENLAN_SPACE is treated as unset (does not override)
out="$(SPACES_FILE="$SCRIPT_DIR/fixtures/spaces-basic.toml" WENLAN_SPACE='' "$RESOLVER" --cwd /tmp/origin-test/career/foo 2>/dev/null)"
assert_eq 'empty env does NOT override -> career/cwd-config' \
    'career	cwd-config' \
    "$out"

# --- Test 11: --arg overrides env + cwd-config + cwd-repo
out="$(SPACES_FILE="$SCRIPT_DIR/fixtures/spaces-basic.toml" WENLAN_SPACE='health' "$RESOLVER" --cwd /tmp/origin-test/career/foo --arg ideas 2>/dev/null)"
assert_eq 'arg overrides all -> ideas/arg' \
    'ideas	arg' \
    "$out"

# --- Test 12: empty --arg is treated as unset
out="$(SPACES_FILE="$SCRIPT_DIR/fixtures/spaces-basic.toml" WENLAN_SPACE='' "$RESOLVER" --cwd /tmp/origin-test/career/foo --arg '' 2>/dev/null)"
assert_eq 'empty arg does NOT override -> career/cwd-config' \
    'career	cwd-config' \
    "$out"

# --- Test 13: full precedence ladder
# Set everything; arg should win.
out="$(SPACES_FILE="$SCRIPT_DIR/fixtures/spaces-basic.toml" WENLAN_SPACE='env-space' "$RESOLVER" --cwd /tmp/origin-test/career/foo --arg arg-space --topic topic-space 2>/dev/null)"
assert_eq 'precedence: arg beats env beats config -> arg-space/arg' \
    'arg-space	arg' \
    "$out"

# Remove arg; env should win.
out="$(SPACES_FILE="$SCRIPT_DIR/fixtures/spaces-basic.toml" WENLAN_SPACE='env-space' "$RESOLVER" --cwd /tmp/origin-test/career/foo --topic topic-space 2>/dev/null)"
assert_eq 'precedence: env beats config -> env-space/env' \
    'env-space	env' \
    "$out"

# Remove env; cwd-config should win.
out="$(SPACES_FILE="$SCRIPT_DIR/fixtures/spaces-basic.toml" WENLAN_SPACE='' "$RESOLVER" --cwd /tmp/origin-test/career/foo --topic topic-space 2>/dev/null)"
assert_eq 'precedence: cwd-config beats topic -> career/cwd-config' \
    'career	cwd-config' \
    "$out"

# --- Test 16: whitespace-only --arg falls through (does not produce whitespace space)
out="$(WENLAN_SPACE='' "$RESOLVER" --cwd /tmp --arg '   ' 2>/dev/null)"
assert_eq 'whitespace-only --arg falls through -> unscoped' \
    $'\tunscoped' \
    "$out"

# --- Test 17: whitespace-only WENLAN_SPACE falls through
out="$(WENLAN_SPACE='   ' "$RESOLVER" --cwd /tmp 2>/dev/null)"
assert_eq 'whitespace-only WENLAN_SPACE falls through -> unscoped' \
    $'\tunscoped' \
    "$out"

# --- Test 18: trailing whitespace in TOML values still produces a valid mapping
out="$(SPACES_FILE="$SCRIPT_DIR/fixtures/spaces-trailing-whitespace.toml" WENLAN_SPACE='' "$RESOLVER" --cwd /tmp/origin-trail-test/sub 2>/dev/null)"
assert_eq 'trailing whitespace in TOML still matches -> trail/cwd-config' \
    'trail	cwd-config' \
    "$out"

# --- Test 14: cwd outside mappings + default key -> uses cwd-config-default
out="$(SPACES_FILE="$SCRIPT_DIR/fixtures/spaces-basic.toml" WENLAN_SPACE='' "$RESOLVER" --cwd /opt/somewhere-unmapped 2>/dev/null)"
assert_eq 'cwd-config default key -> personal/cwd-config-default' \
    'personal	cwd-config-default' \
    "$out"

# --- Test 15: no-default fixture without mapping match -> falls through to unscoped
out="$(SPACES_FILE="$SCRIPT_DIR/fixtures/spaces-no-default.toml" WENLAN_SPACE='' "$RESOLVER" --cwd /opt/no-match-here 2>/dev/null)"
assert_eq 'no-default fixture no match -> unscoped' \
    $'\tunscoped' \
    "$out"

printf '\n%d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
