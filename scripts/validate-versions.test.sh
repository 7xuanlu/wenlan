#!/usr/bin/env bash
set -euo pipefail

TMPDIR_TEST=$(mktemp -d)
trap "rm -rf $TMPDIR_TEST" EXIT

mkdir -p \
    "$TMPDIR_TEST/crates/wenlan-mcp/npm" \
    "$TMPDIR_TEST/crates/wenlan-cli/npm" \
    "$TMPDIR_TEST/plugin/.claude-plugin" \
    "$TMPDIR_TEST/plugin-codex/.codex-plugin" \
    "$TMPDIR_TEST/plugin-codex/bin" \
    "$TMPDIR_TEST/plugin-codex/skills/setup"
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
name = "wenlan"
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
echo '{"version": "0.5.0+codex"}' > "$TMPDIR_TEST/plugin-codex/.codex-plugin/plugin.json"
cat > "$TMPDIR_TEST/plugin-codex/bin/wenlan-mcp-runner.sh" <<EOF
exec npx -y wenlan-mcp@^0.5.0 --agent-name "\${agent_name}" "\$@"
EOF
cat > "$TMPDIR_TEST/plugin-codex/README.md" <<EOF
Fallbacks to npx -y wenlan-mcp@^0.5.0 when no local runtime exists.
EOF
cat > "$TMPDIR_TEST/plugin-codex/skills/setup/SKILL.md" <<EOF
curl -fsSL https://raw.githubusercontent.com/7xuanlu/wenlan/v0.5.0/install.sh | bash
EOF

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

perl -0pi -e 's/name = "wenlan-core"\nversion = "0\.4\.9"/name = "wenlan-core"\nversion = "0.5.0"/' "$TMPDIR_TEST/Cargo.lock"
echo '{"version": "0.4.9+codex"}' > "$TMPDIR_TEST/plugin-codex/.codex-plugin/plugin.json"
if (cd "$TMPDIR_TEST" && RELEASE_TAG="v0.5.0" bash "$OLDPWD/scripts/validate-versions.sh") 2>/dev/null; then
    echo "FAIL test 5: should have detected Codex plugin manifest drift"
    exit 1
fi
echo "PASS test 5: Codex plugin manifest drift detected"

echo '{"version": "0.5.0+codex"}' > "$TMPDIR_TEST/plugin-codex/.codex-plugin/plugin.json"
perl -0pi -e 's/wenlan-mcp@\^0\.5\.0/wenlan-mcp@^0.4.9/g' "$TMPDIR_TEST/plugin-codex/bin/wenlan-mcp-runner.sh"
if (cd "$TMPDIR_TEST" && RELEASE_TAG="v0.5.0" bash "$OLDPWD/scripts/validate-versions.sh") 2>/dev/null; then
    echo "FAIL test 6: should have detected Codex runner pin drift"
    exit 1
fi
echo "PASS test 6: Codex runner pin drift detected"

perl -0pi -e 's/wenlan-mcp@\^0\.4\.9/wenlan-mcp@^0.5.0/g' "$TMPDIR_TEST/plugin-codex/bin/wenlan-mcp-runner.sh"
perl -0pi -e 's|/v0\.5\.0/install\.sh|/v0.4.9/install.sh|g' "$TMPDIR_TEST/plugin-codex/skills/setup/SKILL.md"
if (cd "$TMPDIR_TEST" && RELEASE_TAG="v0.5.0" bash "$OLDPWD/scripts/validate-versions.sh") 2>/dev/null; then
    echo "FAIL test 7: should have detected Codex setup install tag drift"
    exit 1
fi
echo "PASS test 7: Codex setup install tag drift detected"

perl -0pi -e 's|/v0\.4\.9/install\.sh|/v0.5.0/install.sh|g' "$TMPDIR_TEST/plugin-codex/skills/setup/SKILL.md"
perl -0pi -e 's/wenlan-mcp@\^0\.5\.0/wenlan-mcp@^0.4.9/g' "$TMPDIR_TEST/plugin-codex/README.md"
if (cd "$TMPDIR_TEST" && RELEASE_TAG="v0.5.0" bash "$OLDPWD/scripts/validate-versions.sh") 2>/dev/null; then
    echo "FAIL test 8: should have detected Codex README runner pin drift"
    exit 1
fi
echo "PASS test 8: Codex README runner pin drift detected"

cat > "$TMPDIR_TEST/plugin-codex/README.md" <<EOF
No package fallback is documented here.
EOF
if output=$(cd "$TMPDIR_TEST" && RELEASE_TAG="v0.5.0" bash "$OLDPWD/scripts/validate-versions.sh" 2>&1); then
    echo "FAIL test 9: should have detected missing Codex README runner pin"
    exit 1
fi
if ! printf '%s\n' "$output" | grep -q "Codex plugin release pin missing"; then
    echo "FAIL test 9: missing pin error was not reported"
    printf '%s\n' "$output"
    exit 1
fi
echo "PASS test 9: Codex README runner pin missing detected"
