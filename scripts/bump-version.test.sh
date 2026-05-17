#!/usr/bin/env bash
# Test bump-version.sh syncs all manifests from version.txt
set -euo pipefail

# Setup: temp workspace mimicking real layout
# Use ${TMPDIR:-/tmp} so this works both inside Claude Code sandbox and in CI.
TMPDIR_TEST=$(mktemp -d "${TMPDIR:-/tmp}/bump-version-test.XXXXXX")
trap "rm -rf $TMPDIR_TEST" EXIT

mkdir -p "$TMPDIR_TEST/crates/origin-mcp/npm"
mkdir -p "$TMPDIR_TEST/crates/origin-cli/npm"
mkdir -p "$TMPDIR_TEST/plugin/.claude-plugin"
mkdir -p "$TMPDIR_TEST/plugin/bin"
mkdir -p "$TMPDIR_TEST/plugin/skills/init"

cat > "$TMPDIR_TEST/version.txt" <<EOF
0.5.0
EOF

cat > "$TMPDIR_TEST/Cargo.toml" <<EOF
[workspace.package]
version = "0.4.1"   # x-release-please-version

[workspace.dependencies]
origin-types = { path = "crates/origin-types", version = "0.4.1" }
origin-core  = { path = "crates/origin-core",  version = "0.4.1" }
EOF

cat > "$TMPDIR_TEST/crates/origin-mcp/npm/package.json" <<EOF
{"name": "origin-mcp", "version": "0.4.1"}
EOF

cat > "$TMPDIR_TEST/crates/origin-cli/npm/package.json" <<EOF
{"name": "@7xuanlu/origin", "version": "0.4.1"}
EOF

cat > "$TMPDIR_TEST/plugin/.claude-plugin/plugin.json" <<EOF
{"name": "origin", "version": "0.4.1"}
EOF

cat > "$TMPDIR_TEST/plugin/bin/origin-mcp-runner.sh" <<EOF
exec npx -y origin-mcp@^0.4.1 "\$@"
EOF

cat > "$TMPDIR_TEST/plugin/skills/init/SKILL.md" <<EOF
Bash: curl -fsSL https://raw.githubusercontent.com/7xuanlu/origin/v0.4.1/install.sh | bash
EOF

# Run script in temp dir
(cd "$TMPDIR_TEST" && bash "$OLDPWD/scripts/bump-version.sh")

# Assert all manifests bumped to 0.5.0
WS_VER=$(grep -E '^version = ' "$TMPDIR_TEST/Cargo.toml" | sed -E 's/version = "([^"]+)".*/\1/')
ORIGIN_TYPES_DEP_VER=$(grep -E '^origin-types[[:space:]]+=' "$TMPDIR_TEST/Cargo.toml" | sed -E 's/.*version = "([^"]+)".*/\1/')
ORIGIN_CORE_DEP_VER=$(grep -E '^origin-core[[:space:]]+=' "$TMPDIR_TEST/Cargo.toml" | sed -E 's/.*version = "([^"]+)".*/\1/')
MCP_NPM_VER=$(jq -r .version "$TMPDIR_TEST/crates/origin-mcp/npm/package.json")
ORIGIN_NPM_VER=$(jq -r .version "$TMPDIR_TEST/crates/origin-cli/npm/package.json")
PLUGIN_VER=$(jq -r .version "$TMPDIR_TEST/plugin/.claude-plugin/plugin.json")

[[ "$WS_VER" == "0.5.0" ]]   || { echo "FAIL: Cargo.toml not bumped (got $WS_VER)"; exit 1; }
[[ "$ORIGIN_TYPES_DEP_VER" == "0.5.0" ]] || { echo "FAIL: origin-types dep not bumped (got $ORIGIN_TYPES_DEP_VER)"; exit 1; }
[[ "$ORIGIN_CORE_DEP_VER" == "0.5.0" ]] || { echo "FAIL: origin-core dep not bumped (got $ORIGIN_CORE_DEP_VER)"; exit 1; }
[[ "$MCP_NPM_VER" == "0.5.0" ]]  || { echo "FAIL: origin-mcp npm not bumped (got $MCP_NPM_VER)"; exit 1; }
[[ "$ORIGIN_NPM_VER" == "0.5.0" ]]  || { echo "FAIL: @7xuanlu/origin npm not bumped (got $ORIGIN_NPM_VER)"; exit 1; }
[[ "$PLUGIN_VER" == "0.5.0" ]] || { echo "FAIL: plugin not bumped (got $PLUGIN_VER)"; exit 1; }
grep -q 'origin-mcp@\^0.5.0' "$TMPDIR_TEST/plugin/bin/origin-mcp-runner.sh" || { echo "FAIL: runner pin not bumped"; exit 1; }
grep -q '/v0.5.0/install.sh' "$TMPDIR_TEST/plugin/skills/init/SKILL.md" || { echo "FAIL: init skill installer not bumped"; exit 1; }
echo "PASS: bump-version.sh syncs all manifests"
