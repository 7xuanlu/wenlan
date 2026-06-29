#!/usr/bin/env bash
# competitor-radar - weekly scan for new/rising local-first AI-memory tools for coding agents.
# Why: the lane is crowded (sverklo 70*, ghost, n2n, pebble, vibemem...). You were blindsided by
# how contested this is. This makes the surprise impossible: it tells you, every week, who is new
# and who is rising, so you compete or differentiate on facts, not vibes.
#
# Uses only the public GitHub search API (no auth needed for low-rate use; set GH_TOKEN to raise limits).
# Output: a sorted markdown table to stdout (and to radar-YYYY-MM-DD.md if --write).
# Run: bash radar.sh            # print table
#      bash radar.sh --write    # also save dated snapshot for diffing week-over-week
set -euo pipefail

WRITE=0
[[ "${1:-}" == "--write" ]] && WRITE=1

# Search queries that surface this micro-category. Add/remove as the space shifts.
QUERIES=(
  "claude code memory mcp"
  "local-first agent memory"
  "git AI memory cursor"
  "persistent context coding agent mcp"
  "MCP memory server local"
)

AUTH=()
[[ -n "${GH_TOKEN:-}" ]] && AUTH=(-H "Authorization: Bearer ${GH_TOKEN}")

TMP="$(mktemp)"
trap 'rm -f "$TMP"' EXIT

for q in "${QUERIES[@]}"; do
  enc="$(printf '%s' "$q" | jq -sRr @uri)"
  # stars>=1 trims noise; sort by recently-updated to catch rising entrants.
  url="https://api.github.com/search/repositories?q=${enc}+stars:%3E%3D1&sort=updated&order=desc&per_page=30"
  curl -fsSL --max-time 20 "${AUTH[@]}" "$url" 2>/dev/null \
    | jq -r '.items[]? | [.full_name, (.stargazers_count|tostring), (.language // "?"), (.updated_at|split("T")[0]), (.description // "" | gsub("[\n|]"; " ") | .[0:90])] | @tsv' \
    >> "$TMP" || true
  sleep 2  # be polite to the unauthenticated rate limit
done

# Dedup by repo, keep the highest star count seen, sort by stars desc.
DATE="$(date +%Y-%m-%d)"
SELF="7xuanlu/origin"
{
  echo "# Competitor radar - ${DATE}"
  echo
  echo "| Repo | Stars | Lang | Updated | What it claims |"
  echo "|---|---:|---|---|---|"
  sort -u "$TMP" \
    | awk -F'\t' '{ if ($2+0 >= max[$1]+0) { max[$1]=$2; line[$1]=$0 } } END { for (k in line) print line[k] }' \
    | sort -t$'\t' -k2,2 -nr \
    | awk -F'\t' -v self="$SELF" '{ star = ($1==self) ? "**"$1"** (you)" : $1; printf "| %s | %s | %s | %s | %s |\n", star, $2, $3, $4, $5 }'
  echo
  echo "_Source: GitHub search API, ${DATE}. Sorted by stars. Rising = recently updated with climbing stars._"
} | tee "$( [[ $WRITE -eq 1 ]] && echo "radar-${DATE}.md" || echo /dev/null )"
