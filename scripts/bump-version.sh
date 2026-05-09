#!/usr/bin/env bash
# Sync version from version.txt to Cargo.toml workspace.package + npm + plugin manifests.
# Called by release-please.yml on the release branch after release-please opens a release PR.
# Usage: bash scripts/bump-version.sh
#   (no arguments — version is read from version.txt in the repo root)
set -euo pipefail

NEW_VERSION="$(cat version.txt | tr -d '[:space:]')"
[[ -n "$NEW_VERSION" ]] || { echo "ERROR: version.txt is empty"; exit 1; }

# Validate semver format (N.N.N or N.N.N-prerelease)
if ! [[ "$NEW_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then
  echo "ERROR: version.txt must contain N.N.N or N.N.N-prerelease format, got: $NEW_VERSION" >&2
  exit 1
fi

echo "Syncing version from version.txt: ${NEW_VERSION}"
echo ""

# 1. Cargo.toml workspace.package version (propagates to all crates via inheritance)
# Portability: BSD sed (macOS) needs empty -i arg; GNU sed (Linux CI) doesn't.
if [[ "$(uname)" == "Darwin" ]]; then
    sed -i '' -E "s/^(version = \")[^\"]+(\".*x-release-please-version)/\1${NEW_VERSION}\2/" Cargo.toml
else
    sed -i -E "s/^(version = \")[^\"]+(\".*x-release-please-version)/\1${NEW_VERSION}\2/" Cargo.toml
fi
echo "  Updated Cargo.toml (workspace.package.version)"

# 2. npm wrapper package.json
(cd crates/origin-mcp/npm && npm version "$NEW_VERSION" --no-git-tag-version --allow-same-version >/dev/null)
echo "  Updated crates/origin-mcp/npm/package.json"

# 3. Claude plugin manifest
jq ".version = \"$NEW_VERSION\"" .claude-plugin/plugin.json > .claude-plugin/plugin.json.tmp
mv .claude-plugin/plugin.json.tmp .claude-plugin/plugin.json
echo "  Updated .claude-plugin/plugin.json"

echo ""
echo "Versions synced from version.txt (${NEW_VERSION}) to Cargo.toml + npm + plugin manifests."
