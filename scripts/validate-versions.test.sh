#!/usr/bin/env bash
set -euo pipefail

TMPDIR_TEST=$(mktemp -d)
trap "rm -rf $TMPDIR_TEST" EXIT

mkdir -p "$TMPDIR_TEST/crates/origin-mcp/npm" "$TMPDIR_TEST/plugin/.claude-plugin"
echo "0.5.0" > "$TMPDIR_TEST/version.txt"
cat > "$TMPDIR_TEST/Cargo.toml" <<EOF
[workspace.package]
version = "0.5.0"   # x-release-please-version
EOF
echo '{"version": "0.5.0"}' > "$TMPDIR_TEST/crates/origin-mcp/npm/package.json"
echo '{"version": "0.5.0"}' > "$TMPDIR_TEST/plugin/.claude-plugin/plugin.json"

# Test 1: all match → exit 0
(cd "$TMPDIR_TEST" && RELEASE_TAG="v0.5.0" bash "$OLDPWD/scripts/validate-versions.sh")
echo "PASS test 1: all matching"

# Test 2: mismatch → exit 1
echo '{"version": "0.4.9"}' > "$TMPDIR_TEST/plugin/.claude-plugin/plugin.json"
if (cd "$TMPDIR_TEST" && RELEASE_TAG="v0.5.0" bash "$OLDPWD/scripts/validate-versions.sh") 2>/dev/null; then
    echo "FAIL test 2: should have detected drift"
    exit 1
fi
echo "PASS test 2: drift detected"
