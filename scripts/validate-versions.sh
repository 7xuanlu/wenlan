#!/usr/bin/env bash
# Pre-flight: assert all version sources match RELEASE_TAG.
set -euo pipefail

[[ -n "${RELEASE_TAG:-}" ]] || { echo "ERROR: RELEASE_TAG env var required"; exit 1; }
TAG_VER="${RELEASE_TAG#v}"

VTXT_VER=$(cat version.txt | tr -d '[:space:]')
WS_VER=$(grep -E '^version = ' Cargo.toml | head -1 | sed -E 's/version = "([^"]+)".*/\1/')
MCP_NPM_VER=$(jq -r .version crates/origin-mcp/npm/package.json)
ORIGIN_NPM_VER=$(jq -r .version crates/origin-cli/npm/package.json)
PLUGIN_VER=$(jq -r .version plugin/.claude-plugin/plugin.json)

echo "Tag:         $TAG_VER"
echo "version.txt: $VTXT_VER"
echo "Cargo:       $WS_VER"
echo "origin-mcp npm: $MCP_NPM_VER"
echo "@7xuanlu/origin npm: $ORIGIN_NPM_VER"
echo "Plugin:      $PLUGIN_VER"

if [[ "$VTXT_VER" != "$TAG_VER" || "$WS_VER" != "$TAG_VER" || "$MCP_NPM_VER" != "$TAG_VER" || "$ORIGIN_NPM_VER" != "$TAG_VER" || "$PLUGIN_VER" != "$TAG_VER" ]]; then
    echo "ERROR: version drift — bump-version.sh likely failed in release-please.yml"
    exit 1
fi

echo "All versions consistent: $TAG_VER"
