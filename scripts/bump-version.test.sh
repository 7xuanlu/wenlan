#!/usr/bin/env bash
# Test bump-version.sh syncs all manifests from version.txt
set -euo pipefail

# Setup: temp workspace mimicking real layout
# Use ${TMPDIR:-/tmp} so this works both inside Claude Code sandbox and in CI.
TMPDIR_TEST=$(mktemp -d "${TMPDIR:-/tmp}/bump-version-test.XXXXXX")
trap "rm -rf $TMPDIR_TEST" EXIT

mkdir -p "$TMPDIR_TEST/crates/origin-mcp/npm"
mkdir -p "$TMPDIR_TEST/.claude-plugin"

cat > "$TMPDIR_TEST/version.txt" <<EOF
0.5.0
EOF

cat > "$TMPDIR_TEST/Cargo.toml" <<EOF
[workspace.package]
version = "0.4.1"   # x-release-please-version
EOF

cat > "$TMPDIR_TEST/crates/origin-mcp/npm/package.json" <<EOF
{"name": "origin-mcp", "version": "0.4.1"}
EOF

cat > "$TMPDIR_TEST/.claude-plugin/plugin.json" <<EOF
{"name": "origin", "version": "0.4.1"}
EOF

# Run script in temp dir
(cd "$TMPDIR_TEST" && bash "$OLDPWD/scripts/bump-version.sh")

# Assert all manifests bumped to 0.5.0
WS_VER=$(grep -E '^version = ' "$TMPDIR_TEST/Cargo.toml" | sed -E 's/version = "([^"]+)".*/\1/')
NPM_VER=$(jq -r .version "$TMPDIR_TEST/crates/origin-mcp/npm/package.json")
PLUGIN_VER=$(jq -r .version "$TMPDIR_TEST/.claude-plugin/plugin.json")

[[ "$WS_VER" == "0.5.0" ]]   || { echo "FAIL: Cargo.toml not bumped (got $WS_VER)"; exit 1; }
[[ "$NPM_VER" == "0.5.0" ]]  || { echo "FAIL: npm not bumped (got $NPM_VER)"; exit 1; }
[[ "$PLUGIN_VER" == "0.5.0" ]] || { echo "FAIL: plugin not bumped (got $PLUGIN_VER)"; exit 1; }
echo "PASS: bump-version.sh syncs all manifests"
