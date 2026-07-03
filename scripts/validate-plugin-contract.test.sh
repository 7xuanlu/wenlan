#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

VALIDATOR="$PWD/scripts/validate-plugin-contract.py"
TMPDIR_TEST=$(mktemp -d)
trap "rm -rf $TMPDIR_TEST" EXIT

copy_fixture() {
    rm -rf "$TMPDIR_TEST/root"
    mkdir -p "$TMPDIR_TEST/root/crates/wenlan-mcp/src"
    cp -R plugin "$TMPDIR_TEST/root/plugin"
    cp -R plugin-codex "$TMPDIR_TEST/root/plugin-codex"
    cp -R .agents "$TMPDIR_TEST/root/.agents"
    cp plugin-contract.json "$TMPDIR_TEST/root/plugin-contract.json"
    cp crates/wenlan-mcp/src/main.rs "$TMPDIR_TEST/root/crates/wenlan-mcp/src/main.rs"
}

assert_rejects() {
    local name="$1"
    shift
    copy_fixture
    "$@"
    if python3 "$VALIDATOR" --root "$TMPDIR_TEST/root" 2>/dev/null; then
        echo "FAIL $name: validator accepted drift"
        exit 1
    fi
    echo "PASS $name"
}

copy_fixture
python3 "$VALIDATOR" --root "$TMPDIR_TEST/root"
echo "PASS valid plugin contract"

assert_rejects "codex skill autocomplete drift" \
    perl -0pi -e 's/user-invocable: true/user-invocable: false/' \
    "$TMPDIR_TEST/root/plugin-codex/skills/brief/SKILL.md"

assert_rejects "claude skill copied into codex" \
    bash -c 'rm -rf "$1"; cp -R "$2" "$1"' _ \
    "$TMPDIR_TEST/root/plugin-codex/skills/handoff" \
    "$TMPDIR_TEST/root/plugin/skills/handoff"

assert_rejects "missing destructive confirmation wording" \
    perl -0pi -e 's/cannot be undone/cannot be reversed/' \
    "$TMPDIR_TEST/root/plugin-codex/skills/forget/SKILL.md"

assert_rejects "missing curate ambiguity guardrail" \
    perl -0pi -e 's/Ambiguous replies do not mutate/Ambiguous replies are clarified/' \
    "$TMPDIR_TEST/root/plugin-codex/skills/curate/SKILL.md"

assert_rejects "debrief thin alias drift" \
    perl -0pi -e 's/Pending-captures preview/Pending review/g' \
    "$TMPDIR_TEST/root/plugin-codex/skills/debrief/SKILL.md"

assert_rejects "codex resolver parity drift" \
    perl -0pi -e 's/cwd-config-default/codex-default/' \
    "$TMPDIR_TEST/root/plugin-codex/bin/resolve-space.sh"

assert_rejects "marketplace source drift" \
    perl -0pi -e 's|"path": "./plugin-codex"|"path": "./plugin"|' \
    "$TMPDIR_TEST/root/.agents/plugins/marketplace.json"

assert_rejects "claude stdio default drift" \
    perl -0pi -e 's/"claude-code"/"codex"/' \
    "$TMPDIR_TEST/root/crates/wenlan-mcp/src/main.rs"
