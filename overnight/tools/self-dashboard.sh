#!/usr/bin/env bash
# self-dashboard.sh — a weekly mirror for a solo builder.
# Shows where effort ACTUALLY went, and the one ratio that matters: inward vs outward.
# Run from the repo root: bash overnight/tools/self-dashboard.sh [days]
# Default window: 7 days.
set -euo pipefail

DAYS="${1:-7}"
SINCE="$(date -d "${DAYS} days ago" +%Y-%m-%d 2>/dev/null || date -v-"${DAYS}"d +%Y-%m-%d)"

# Pull subjects once.
SUBJECTS="$(git log --since="${SINCE}" --format='%s')"
TOTAL="$(printf '%s\n' "${SUBJECTS}" | grep -c . || true)"

if [[ "${TOTAL}" -eq 0 ]]; then
  echo "No commits in the last ${DAYS} days. That is also a signal."
  exit 0
fi

count() { printf '%s\n' "${SUBJECTS}" | grep -ciE "$1" || true; }

# Outward = work a user could notice: feat (non-eval), plugin, cli UX, install, docs that are user-facing.
# Inward  = eval, ci, release plumbing, refactor, deps, chore.
EVAL=$(count 'eval|locomo|longmem|baseline|judge|faithful')
CI=$(count '^[a-z]+\(ci\)|^ci|release\.yml|release-please|workflow|deps|cargo\.lock')
DOCS=$(count '^docs|readme|seo')
REFACTOR=$(count '^refactor|^chore')
FEAT=$(count '^feat')
FIX=$(count '^fix')

INWARD=$(( EVAL + CI + REFACTOR ))
# crude outward proxy: feat + fix that are NOT eval/ci/refactor flavored
OUTWARD=$(( TOTAL - INWARD - DOCS ))
(( OUTWARD < 0 )) && OUTWARD=0

# User-signal scan: did anything this week reference a real human on the other end?
USERSIG=$(printf '%s\n' "${SUBJECTS}" | grep -ciE 'user|feedback|report|onboard|install fix|first-run|signup|waitlist' || true)

# Days since last user-facing feat (feat not eval/ci).
LAST_FEAT_DATE="$(git log --format='%ad|%s' --date=short \
  | grep -iE '\|feat' | grep -viE 'eval|ci\(|infra|harness' | head -1 | cut -d'|' -f1 || true)"
if [[ -n "${LAST_FEAT_DATE}" ]]; then
  DAYS_SINCE_FEAT=$(( ( $(date +%s) - $(date -d "${LAST_FEAT_DATE}" +%s 2>/dev/null || date -j -f %Y-%m-%d "${LAST_FEAT_DATE}" +%s) ) / 86400 ))
else
  DAYS_SINCE_FEAT="?"
fi

bar() { # bar <count> <total>
  local n=$1 t=$2 width=24 filled
  (( t == 0 )) && t=1
  filled=$(( n * width / t ))
  printf '%s%s' "$(printf '#%.0s' $(seq 1 $filled 2>/dev/null))" "$(printf '.%.0s' $(seq 1 $((width-filled)) 2>/dev/null))"
}

echo "=================================================="
echo " SELF-DASHBOARD  ·  last ${DAYS} days  (since ${SINCE})"
echo "=================================================="
printf ' commits total      %3d\n' "${TOTAL}"
printf ' feat / fix         %3d / %3d\n' "${FEAT}" "${FIX}"
echo "--------------------------------------------------"
printf ' eval     %3d  [%s]\n' "${EVAL}"     "$(bar ${EVAL} ${TOTAL})"
printf ' ci/rel   %3d  [%s]\n' "${CI}"       "$(bar ${CI} ${TOTAL})"
printf ' docs/seo %3d  [%s]\n' "${DOCS}"     "$(bar ${DOCS} ${TOTAL})"
printf ' refactor %3d  [%s]\n' "${REFACTOR}" "$(bar ${REFACTOR} ${TOTAL})"
echo "--------------------------------------------------"
printf ' INWARD  (eval+ci+refactor)  %3d  (%d%%)\n' "${INWARD}" "$(( INWARD*100/TOTAL ))"
printf ' OUTWARD (everything else)   %3d  (%d%%)\n' "${OUTWARD}" "$(( OUTWARD*100/TOTAL ))"
echo "--------------------------------------------------"
printf ' user/feedback mentions      %3d   <- the number that should not be 0\n' "${USERSIG}"
printf ' days since user-facing feat  %s\n' "${DAYS_SINCE_FEAT}"
echo "=================================================="
if (( INWARD*100/TOTAL > 50 )); then
  echo " VERDICT: inward-dominant week. You optimized things only you can see."
elif (( USERSIG == 0 )); then
  echo " VERDICT: no human appeared in your week. Ship to one this week."
else
  echo " VERDICT: balanced. Keep the outward thread alive."
fi
