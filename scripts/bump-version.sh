#!/usr/bin/env bash
set -euo pipefail

# bump-version.sh — update the version string across all project files
# Usage: bash scripts/bump-version.sh <NEW_VERSION>
# Example: bash scripts/bump-version.sh 0.2.0
# Example: bash scripts/bump-version.sh 0.2.0-alpha.1

NEW_VERSION="${1:-}"

# Validate argument
if [[ -z "$NEW_VERSION" ]]; then
  echo "Error: version argument required" >&2
  echo "Usage: bash scripts/bump-version.sh <N.N.N[-PRERELEASE]>" >&2
  exit 1
fi

# Validate semver format (N.N.N or N.N.N-prerelease)
if ! [[ "$NEW_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then
  echo "Error: version must be in N.N.N or N.N.N-prerelease format (e.g. 0.2.0 or 0.2.0-alpha.1), got: $NEW_VERSION" >&2
  exit 1
fi

# Resolve repo root relative to this script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Files to update — daemon workspace crates only.
# The Tauri desktop app moved to https://github.com/7xuanlu/origin-app and
# manages its own version bumps independently.
CARGO_TOMLS=(
  "$REPO_ROOT/crates/origin-cli/Cargo.toml"
  "$REPO_ROOT/crates/origin-core/Cargo.toml"
  "$REPO_ROOT/crates/origin-server/Cargo.toml"
  "$REPO_ROOT/crates/origin-types/Cargo.toml"
)

echo "Bumping version to $NEW_VERSION"
echo ""

# Update Cargo.toml files — only the bare `version = "..."` line (the [package] version).
# Dependency version lines use inline table syntax ({ version = "..." }) and are not affected
# by this pattern which anchors to start-of-line.
for f in "${CARGO_TOMLS[@]}"; do
  sed -i '' -E 's/^version = "[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]*)?"/version = "'"$NEW_VERSION"'"/' "$f"
  echo "  Updated $f"
done

echo ""
echo "Regenerating Cargo.lock..."
cargo generate-lockfile
echo "  Updated Cargo.lock"

echo ""
echo "Changed files:"
git diff --stat crates/*/Cargo.toml Cargo.lock 2>/dev/null || true

echo ""
echo "Done. Verify with:"
echo '  grep -rn "^version" crates/*/Cargo.toml'
