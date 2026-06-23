#!/usr/bin/env bash
# Pre-flight: assert all version sources match RELEASE_TAG.
set -euo pipefail

[[ -n "${RELEASE_TAG:-}" ]] || { echo "ERROR: RELEASE_TAG env var required"; exit 1; }
TAG_VER="${RELEASE_TAG#v}"

VTXT_VER=$(cat version.txt | tr -d '[:space:]')
WS_VER=$(grep -E '^version = ' Cargo.toml | head -1 | sed -E 's/version = "([^"]+)".*/\1/')
WENLAN_TYPES_DEP_VER=$(grep -E '^wenlan-types[[:space:]]+=' Cargo.toml | sed -E 's/.*version = "([^"]+)".*/\1/')
WENLAN_CORE_DEP_VER=$(grep -E '^wenlan-core[[:space:]]+=' Cargo.toml | sed -E 's/.*version = "([^"]+)".*/\1/')
LOCK_VERSIONS=$(awk '
  $0 == "[[package]]" { in_pkg=1; name=""; version=""; next }
  in_pkg && $1 == "name" && $2 == "=" {
    name=$3
    gsub(/"/, "", name)
    next
  }
  in_pkg && $1 == "version" && $2 == "=" {
    version=$3
    gsub(/"/, "", version)
    if (name == "wenlan" || name == "wenlan-core" || name == "wenlan-mcp" || name == "wenlan-server" || name == "wenlan-types") {
      print name ":" version
    }
    in_pkg=0
  }
' Cargo.lock | sort)
MCP_NPM_VER=$(jq -r .version crates/wenlan-mcp/npm/package.json)
WENLAN_NPM_VER=$(jq -r .version crates/wenlan-cli/npm/package.json)
PLUGIN_VER=$(jq -r .version plugin/.claude-plugin/plugin.json)

echo "Tag:         $TAG_VER"
echo "version.txt: $VTXT_VER"
echo "Cargo:       $WS_VER"
echo "wenlan-types dep: $WENLAN_TYPES_DEP_VER"
echo "wenlan-core dep:  $WENLAN_CORE_DEP_VER"
echo "Cargo.lock:"
printf '%s\n' "$LOCK_VERSIONS" | sed 's/^/  /'
echo "wenlan-mcp npm: $MCP_NPM_VER"
echo "wenlan npm: $WENLAN_NPM_VER"
echo "Plugin:      $PLUGIN_VER"

if [[ "$VTXT_VER" != "$TAG_VER" || "$WS_VER" != "$TAG_VER" || "$WENLAN_TYPES_DEP_VER" != "$TAG_VER" || "$WENLAN_CORE_DEP_VER" != "$TAG_VER" || "$MCP_NPM_VER" != "$TAG_VER" || "$WENLAN_NPM_VER" != "$TAG_VER" || "$PLUGIN_VER" != "$TAG_VER" ]]; then
    echo "ERROR: version drift — bump-version.sh likely failed in release-please.yml"
    exit 1
fi

for crate in wenlan wenlan-core wenlan-mcp wenlan-server wenlan-types; do
    if ! printf '%s\n' "$LOCK_VERSIONS" | grep -qx "${crate}:${TAG_VER}"; then
        echo "ERROR: Cargo.lock drift — ${crate} is not ${TAG_VER}"
        exit 1
    fi
done

echo "All versions consistent: $TAG_VER"
