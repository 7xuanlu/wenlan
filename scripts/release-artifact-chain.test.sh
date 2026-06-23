#!/usr/bin/env bash
set -euo pipefail

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

release_yml=".github/workflows/release.yml"
cli_runner="crates/wenlan-cli/npm/run.js"

expected_darwin_asset="wenlan-darwin-arm64.tar.gz"

grep -q "artifact_name: wenlan-darwin-arm64" "$release_yml" \
  || fail "release.yml does not produce wenlan-darwin-arm64"

grep -q "const ASSET = \"${expected_darwin_asset}\"" "$cli_runner" \
  || fail "npm wenlan runner does not download ${expected_darwin_asset}"

if grep -q '\${name}-\${TARGET}' "$cli_runner"; then
  fail "npm wenlan runner still downloads per-binary target-name assets"
fi

echo "PASS: npm wenlan runner consumes release.yml darwin-arm64 artifact"
