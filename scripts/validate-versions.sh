#!/usr/bin/env bash
# Pre-flight: assert all version sources match RELEASE_TAG.
set -euo pipefail

[[ -n "${RELEASE_TAG:-}" ]] || { echo "ERROR: RELEASE_TAG env var required"; exit 1; }
TAG_VER="${RELEASE_TAG#v}"

VTXT_VER=$(cat version.txt | tr -d '[:space:]')
WS_VER=$(grep -E '^version = ' Cargo.toml | head -1 | sed -E 's/version = "([^"]+)".*/\1/')
ORIGIN_TYPES_DEP_VER=$(grep -E '^origin-types[[:space:]]+=' Cargo.toml | sed -E 's/.*version = "([^"]+)".*/\1/')
ORIGIN_CORE_DEP_VER=$(grep -E '^origin-core[[:space:]]+=' Cargo.toml | sed -E 's/.*version = "([^"]+)".*/\1/')
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
    if (name == "origin" || name == "origin-core" || name == "origin-mcp" || name == "origin-server" || name == "origin-types") {
      print name ":" version
    }
    in_pkg=0
  }
' Cargo.lock | sort)
MCP_NPM_VER=$(jq -r .version crates/origin-mcp/npm/package.json)
ORIGIN_NPM_VER=$(jq -r .version crates/origin-cli/npm/package.json)
PLUGIN_VER=$(jq -r .version plugin/.claude-plugin/plugin.json)

echo "Tag:         $TAG_VER"
echo "version.txt: $VTXT_VER"
echo "Cargo:       $WS_VER"
echo "origin-types dep: $ORIGIN_TYPES_DEP_VER"
echo "origin-core dep:  $ORIGIN_CORE_DEP_VER"
echo "Cargo.lock:"
printf '%s\n' "$LOCK_VERSIONS" | sed 's/^/  /'
echo "origin-mcp npm: $MCP_NPM_VER"
echo "@7xuanlu/origin npm: $ORIGIN_NPM_VER"
echo "Plugin:      $PLUGIN_VER"

if [[ "$VTXT_VER" != "$TAG_VER" || "$WS_VER" != "$TAG_VER" || "$ORIGIN_TYPES_DEP_VER" != "$TAG_VER" || "$ORIGIN_CORE_DEP_VER" != "$TAG_VER" || "$MCP_NPM_VER" != "$TAG_VER" || "$ORIGIN_NPM_VER" != "$TAG_VER" || "$PLUGIN_VER" != "$TAG_VER" ]]; then
    echo "ERROR: version drift — bump-version.sh likely failed in release-please.yml"
    exit 1
fi

for crate in origin origin-core origin-mcp origin-server origin-types; do
    if ! printf '%s\n' "$LOCK_VERSIONS" | grep -qx "${crate}:${TAG_VER}"; then
        echo "ERROR: Cargo.lock drift — ${crate} is not ${TAG_VER}"
        exit 1
    fi
done

echo "All versions consistent: $TAG_VER"
