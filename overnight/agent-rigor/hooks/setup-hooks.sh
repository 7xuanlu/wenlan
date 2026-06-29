#!/usr/bin/env bash
# agent-rigor hook installer.
# Generalized from a working project's scripts/setup-hooks.sh.
#
# Points git at the hooks/ directory in this kit. Run once after copying the kit
# into your repo. If you placed the hooks somewhere other than .githooks/, edit
# HOOKS_DIR below.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
HOOKS_DIR="$SCRIPT_DIR"

git config core.hooksPath "$HOOKS_DIR"
chmod +x "$HOOKS_DIR/pre-commit" "$HOOKS_DIR/pre-push"

echo "Git hooks configured (core.hooksPath -> $HOOKS_DIR)."
echo "  Pre-commit (L2): auto-format + targeted lint on changed units (fast)."
echo "  Pre-push   (L3): full lint + fast test tier, docs-only pushes skipped."
echo
echo "Next: edit the TODO blocks in pre-commit and pre-push for your toolchain."
