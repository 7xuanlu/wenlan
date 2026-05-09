#!/usr/bin/env bash
# Pre-flight: assert all version sources match RELEASE_TAG.
set -euo pipefail

[[ -n "${RELEASE_TAG:-}" ]] || { echo "ERROR: RELEASE_TAG env var required"; exit 1; }
TAG_VER="${RELEASE_TAG#v}"

VTXT_VER=$(cat version.txt | tr -d '[:space:]')
WS_VER=$(grep -E '^version = ' Cargo.toml | head -1 | sed -E 's/version = "([^"]+)".*/\1/')
NPM_VER=$(jq -r .version crates/origin-mcp/npm/package.json)
PLUGIN_VER=$(jq -r .version .claude-plugin/plugin.json)

echo "Tag:         $TAG_VER"
echo "version.txt: $VTXT_VER"
echo "Cargo:       $WS_VER"
echo "npm:         $NPM_VER"
echo "Plugin:      $PLUGIN_VER"

if [[ "$VTXT_VER" != "$TAG_VER" || "$WS_VER" != "$TAG_VER" || "$NPM_VER" != "$TAG_VER" || "$PLUGIN_VER" != "$TAG_VER" ]]; then
    echo "ERROR: version drift — bump-version.sh likely failed in release-please.yml"
    exit 1
fi

echo "All versions consistent: $TAG_VER"
