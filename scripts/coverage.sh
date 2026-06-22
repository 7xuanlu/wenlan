#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$REPO_ROOT"

echo "Generating Rust coverage report (wenlan-core + wenlan-server)..."
cargo llvm-cov --html -p wenlan-core -p wenlan-server
echo "  -> target/llvm-cov/html/index.html"

# Open report on macOS
if command -v open &> /dev/null; then
    open target/llvm-cov/html/index.html
fi
