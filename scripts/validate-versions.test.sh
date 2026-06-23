#!/usr/bin/env bash
set -euo pipefail

TMPDIR_TEST=$(mktemp -d)
trap "rm -rf $TMPDIR_TEST" EXIT

mkdir -p "$TMPDIR_TEST/crates/wenlan-mcp/npm" "$TMPDIR_TEST/crates/wenlan-cli/npm" "$TMPDIR_TEST/plugin/.claude-plugin"
echo "0.5.0" > "$TMPDIR_TEST/version.txt"
cat > "$TMPDIR_TEST/Cargo.toml" <<EOF
[workspace.package]
version = "0.5.0"   # x-release-please-version

[workspace.dependencies]
wenlan-types = { path = "crates/wenlan-types", version = "0.5.0" }
wenlan-core  = { path = "crates/wenlan-core",  version = "0.5.0" }
EOF
cat > "$TMPDIR_TEST/Cargo.lock" <<EOF
[[package]]
name = "origin"
version = "0.5.0"

[[package]]
name = "wenlan-core"
version = "0.5.0"

[[package]]
name = "wenlan-mcp"
version = "0.5.0"

[[package]]
name = "wenlan-server"
version = "0.5.0"

[[package]]
name = "wenlan-types"
version = "0.5.0"
EOF
echo '{"version": "0.5.0"}' > "$TMPDIR_TEST/crates/wenlan-mcp/npm/package.json"
echo '{"version": "0.5.0"}' > "$TMPDIR_TEST/crates/wenlan-cli/npm/package.json"
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

# Test 3: internal workspace dependency mismatch → exit 1
echo '{"version": "0.5.0"}' > "$TMPDIR_TEST/plugin/.claude-plugin/plugin.json"
perl -0pi -e 's/wenlan-core  = \{ path = "crates\/wenlan-core",  version = "0\.5\.0" \}/wenlan-core  = { path = "crates\/wenlan-core",  version = "0.4.9" }/' "$TMPDIR_TEST/Cargo.toml"
if (cd "$TMPDIR_TEST" && RELEASE_TAG="v0.5.0" bash "$OLDPWD/scripts/validate-versions.sh") 2>/dev/null; then
    echo "FAIL test 3: should have detected internal dependency drift"
    exit 1
fi
echo "PASS test 3: internal dependency drift detected"

# Test 4: Cargo.lock mismatch → exit 1
perl -0pi -e 's/wenlan-core  = \{ path = "crates\/wenlan-core",  version = "0\.4\.9" \}/wenlan-core  = { path = "crates\/wenlan-core",  version = "0.5.0" }/' "$TMPDIR_TEST/Cargo.toml"
perl -0pi -e 's/name = "wenlan-core"\nversion = "0\.5\.0"/name = "wenlan-core"\nversion = "0.4.9"/' "$TMPDIR_TEST/Cargo.lock"
if (cd "$TMPDIR_TEST" && RELEASE_TAG="v0.5.0" bash "$OLDPWD/scripts/validate-versions.sh") 2>/dev/null; then
    echo "FAIL test 4: should have detected Cargo.lock drift"
    exit 1
fi
echo "PASS test 4: Cargo.lock drift detected"
