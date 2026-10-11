#!/usr/bin/env bash
# Bash 3.2-compatible contract tests for resolve-space.sh.

set -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESOLVER="$SCRIPT_DIR/../resolve-space.sh"
tmpbase="$(mktemp -d)"
trap 'rm -rf "$tmpbase"' EXIT

pass=0
fail=0

assert_eq() {
    name="$1"
    expected="$2"
    actual="$3"
    if [ "$expected" = "$actual" ]; then
        printf 'PASS  %s\n' "$name"
        pass=$((pass + 1))
    else
        printf 'FAIL  %s\n  expected: %q\n  actual:   %q\n' \
            "$name" "$expected" "$actual" >&2
        fail=$((fail + 1))
    fi
}

fake_bin="$tmpbase/bin"
mkdir -p "$fake_bin"
cat > "$fake_bin/wenlan" <<'EOF'
#!/usr/bin/env bash
if [ "${1:-}" = "spaces" ] && [ "${2:-}" = "show" ]; then
    case "${3:-}" in
        registered-repo|MyProject)
            printf '{"name":"%s"}\n' "$3"
            exit 0
            ;;
    esac
fi
exit 1
EOF
chmod +x "$fake_bin/wenlan"

config="$tmpbase/spaces.toml"
cat > "$config" <<EOF
default = "legacy-default-must-be-ignored"

[[mapping]]
prefix = "$tmpbase"
space = "outer"

[[mapping]]
prefix = "$tmpbase/nested"
space = "mapped"
EOF

out="$(WENLAN_BIN="$fake_bin/wenlan" WENLAN_SPACE='  locked  context  ' WENLAN_DEFAULT_SPACE='fallback' \
    "$RESOLVER" --cwd "$tmpbase/nested" --arg explicit 2>/dev/null)"
assert_eq 'strict pin preserves internal spaces and beats explicit and fallback' 'locked  context	locked-env' "$out"

out="$(WENLAN_BIN="$fake_bin/wenlan" WENLAN_SPACE='' WENLAN_DEFAULT_SPACE='fallback' \
    "$RESOLVER" --cwd "$tmpbase/nested" --arg explicit 2>/dev/null)"
assert_eq 'explicit beats fallback' 'explicit	arg' "$out"

out="$(WENLAN_BIN="$fake_bin/wenlan" WENLAN_SPACE='' WENLAN_DEFAULT_SPACE='fallback' \
    "$RESOLVER" --cwd "$tmpbase/nested" 2>/dev/null)"
assert_eq 'default env beats cwd mapping' 'fallback	default-env' "$out"

out="$(SPACES_FILE="$config" WENLAN_BIN="$fake_bin/wenlan" WENLAN_SPACE='' \
    WENLAN_DEFAULT_SPACE='' "$RESOLVER" --cwd "$tmpbase/nested/leaf" 2>/dev/null)"
assert_eq 'longest cwd mapping wins' 'mapped	cwd-config' "$out"

out="$(SPACES_FILE="$config" WENLAN_BIN="$fake_bin/wenlan" WENLAN_SPACE='' \
    WENLAN_DEFAULT_SPACE='' "$RESOLVER" --cwd /opt/no-match --topic topic 2>/dev/null)"
assert_eq 'legacy default and topic are ignored' $'\tunscoped' "$out"

repo="$tmpbase/registered-repo"
mkdir -p "$repo"
git -C "$repo" init -q
out="$(SPACES_FILE=/does/not/exist WENLAN_BIN="$fake_bin/wenlan" WENLAN_SPACE='' \
    WENLAN_DEFAULT_SPACE='' "$RESOLVER" --cwd "$repo" 2>/dev/null)"
assert_eq 'registered repo basename is accepted' 'registered-repo	cwd-repo' "$out"

unknown_repo="$tmpbase/unregistered-repo"
mkdir -p "$unknown_repo"
git -C "$unknown_repo" init -q
out="$(SPACES_FILE=/does/not/exist WENLAN_BIN="$fake_bin/wenlan" WENLAN_SPACE='' \
    WENLAN_DEFAULT_SPACE='' "$RESOLVER" --cwd "$unknown_repo" 2>/dev/null)"
assert_eq 'unregistered repo basename is skipped without the flag' $'\tunscoped' "$out"

out="$(SPACES_FILE=/does/not/exist WENLAN_BIN="$fake_bin/wenlan" WENLAN_SPACE='' \
    WENLAN_DEFAULT_SPACE='' "$RESOLVER" --cwd "$unknown_repo" --new-repo-fallback 2>/dev/null)"
assert_eq 'unregistered repo basename falls back with --new-repo-fallback' \
    'unregistered-repo	cwd-repo-new' "$out"

space_repo="$tmpbase/My Project"
mkdir -p "$space_repo"
git -C "$space_repo" init -q
out="$(SPACES_FILE=/does/not/exist WENLAN_BIN="$fake_bin/wenlan" WENLAN_SPACE='' \
    WENLAN_DEFAULT_SPACE='' "$RESOLVER" --cwd "$space_repo" 2>/dev/null)"
assert_eq 'repo basename requires exact registered Space name' $'\tunscoped' "$out"

# --args skill mode: strips the first space:<name> token, adds a third field
# for the remainder, and reports the resolution on stderr.
out="$(WENLAN_BIN="$fake_bin/wenlan" WENLAN_SPACE='' WENLAN_DEFAULT_SPACE='' \
    "$RESOLVER" --cwd "$tmpbase/nested" --args 'space:foo find the thing' 2>/dev/null)"
assert_eq '--args leading token is the arg layer and is stripped from rest' \
    'foo	arg	find the thing' "$out"

out="$(WENLAN_BIN="$fake_bin/wenlan" WENLAN_SPACE='' WENLAN_DEFAULT_SPACE='' \
    "$RESOLVER" --cwd "$tmpbase/nested" --args 'find space:foo the thing' 2>/dev/null)"
assert_eq '--args token in the middle is stripped' 'foo	arg	find the thing' "$out"

out="$(WENLAN_BIN="$fake_bin/wenlan" WENLAN_SPACE='' WENLAN_DEFAULT_SPACE='' \
    "$RESOLVER" --cwd "$tmpbase/nested" --args 'find the thing space:foo' 2>/dev/null)"
assert_eq '--args token at the end is stripped' 'foo	arg	find the thing' "$out"

out="$(SPACES_FILE="$config" WENLAN_BIN="$fake_bin/wenlan" WENLAN_SPACE='' WENLAN_DEFAULT_SPACE='' \
    "$RESOLVER" --cwd "$tmpbase/nested/leaf" --args '  find the thing  ' 2>/dev/null)"
assert_eq '--args without a token falls through to cwd mapping with trimmed rest' \
    'mapped	cwd-config	find the thing' "$out"

out="$(WENLAN_BIN="$fake_bin/wenlan" WENLAN_SPACE='locked' WENLAN_DEFAULT_SPACE='' \
    "$RESOLVER" --cwd "$tmpbase/nested" --args 'space:foo find the thing' 2>/dev/null)"
assert_eq '--args strict pin wins and the token is still stripped' \
    'locked	locked-env	find the thing' "$out"

out="$(SPACES_FILE=/does/not/exist WENLAN_BIN="$fake_bin/wenlan" WENLAN_SPACE='' WENLAN_DEFAULT_SPACE='' \
    "$RESOLVER" --cwd /opt/no-match --args 'just words' 2>/dev/null)"
assert_eq '--args unscoped keeps an empty space field' $'\tunscoped\tjust words' "$out"

out="$(WENLAN_BIN="$fake_bin/wenlan" WENLAN_SPACE='' WENLAN_DEFAULT_SPACE='' \
    "$RESOLVER" --cwd "$tmpbase/nested" --args 'space:foo' 2>/dev/null)"
assert_eq '--args with only a token leaves rest empty' $'foo\targ\t' "$out"

err="$(WENLAN_BIN="$fake_bin/wenlan" WENLAN_SPACE='' WENLAN_DEFAULT_SPACE='' \
    "$RESOLVER" --cwd "$tmpbase/nested" --args 'space:foo x' 2>&1 >/dev/null)"
assert_eq '--args stderr reports the resolved space' 'Resolved space: foo (from arg)' "$err"

err="$(SPACES_FILE=/does/not/exist WENLAN_BIN="$fake_bin/wenlan" WENLAN_SPACE='' WENLAN_DEFAULT_SPACE='' \
    "$RESOLVER" --cwd /opt/no-match --args 'x' 2>&1 >/dev/null)"
assert_eq '--args stderr reports unscoped' 'Resolved space: none (unscoped)' "$err"

err="$(WENLAN_BIN="$fake_bin/wenlan" WENLAN_SPACE='' WENLAN_DEFAULT_SPACE='' \
    "$RESOLVER" --cwd "$tmpbase/nested" --arg explicit 2>&1 >/dev/null)"
assert_eq 'stderr is empty without --args' '' "$err"

printf '\n%d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
