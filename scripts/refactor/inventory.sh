#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
OUT="$ROOT/docs/superpowers/refactor/wenlan-app-inventory"
SG=(npx -y -p @ast-grep/cli sg)

mkdir -p "$OUT"

count_matches() {
  local pattern="$1"
  shift
  { rg -n "$pattern" "$@" || true; } | wc -l | tr -d ' '
}

count_inventory_calls() {
  local file="$1"
  { rg "^.+:[0-9]+:.*\\binvoke(<[^>]+>)?\\(" "$file" || true; } | wc -l | tr -d ' '
}

"${SG[@]}" outline "$ROOT/src/lib/tauri.ts" > "$OUT/tauri-ts-outline.txt"
"${SG[@]}" outline "$ROOT/app/src/api.rs" > "$OUT/api-rs-outline.txt"
"${SG[@]}" outline "$ROOT/app/src/search.rs" > "$OUT/search-rs-outline.txt"
{
  for lang in ts tsx; do
    "${SG[@]}" run -p 'invoke($CMD)' -l "$lang" "$ROOT/src" || true
    "${SG[@]}" run -p 'invoke($CMD, $$$ARGS)' -l "$lang" "$ROOT/src" || true
    "${SG[@]}" run -p 'invoke<$TYPE>($CMD)' -l "$lang" "$ROOT/src" || true
    "${SG[@]}" run -p 'invoke<$TYPE>($CMD, $$$ARGS)' -l "$lang" "$ROOT/src" || true
  done
} | LC_ALL=C sort -u -t: -k1,1 -k2,2n > "$OUT/frontend-invokes.txt"
"${SG[@]}" run -p 'pub struct $NAME { $$$FIELDS }' -l rs "$ROOT/app/src" \
  | LC_ALL=C sort -t: -k1,1 -k2,2n > "$OUT/rust-structs.txt"

{
  echo "# Wenlan App Structural Inventory"
  echo
  echo "- branch: $(git branch --show-current)"
  echo
  echo "## Counts"
  printf -- "- frontend invoke calls: "
  count_inventory_calls "$OUT/frontend-invokes.txt"
  printf -- "- registered Tauri commands: "
  count_matches "search::" "$ROOT/app/src/lib.rs"
  printf -- "- origin_types references in Rust app code: "
  count_matches "origin_types::|use origin_types" "$ROOT/app/src"
  printf -- "- runtime identity references: "
  count_matches "origin-server|origin-mcp|Origin|com\\.origin|origin-relay|originmemory|\\.config/origin-mcp" \
    "$ROOT/app" "$ROOT/src" "$ROOT/package.json" "$ROOT/README.md" "$ROOT/Cargo.toml"
  printf -- "- stale taxonomy references: "
  count_matches "\\bconcept\\b|\\bgoal\\b|\\bdomain\\b" "$ROOT/src" "$ROOT/app/src" "$ROOT/app/tests" "$ROOT/package.json" "$ROOT/README.md"
  printf -- "- source files under app/src and src: "
  rg --files "$ROOT/app/src" "$ROOT/src" | wc -l | tr -d ' '
  echo
  echo "## Artifacts"
  echo "- tauri-ts-outline.txt"
  echo "- api-rs-outline.txt"
  echo "- search-rs-outline.txt"
  echo "- frontend-invokes.txt"
  echo "- rust-structs.txt"
} > "$OUT/summary.md"

echo "Inventory written to $OUT"
