#!/usr/bin/env bash
# Probe substrate liveness of cached per-question scenario DBs — for manual eval
# sanity before trusting a sweep. PREVENTION pattern: ENUMERATE rows, never
# `SELECT COUNT(*)`. The sqlite3 CLI mis-plans COUNT(*) against libsql
# vector-index shadows (e.g. `pages`) and returns 0 even when rows exist — which
# once produced a false "pages=0 starved" finding. See memory
# lesson_libsql_count_vector_index_bug + crates/wenlan-core/src/eval/AGENTS.md.
#
# Usage: probe-scenario-liveness.sh <dir-of-per-question-DBs>
#   e.g. probe-scenario-liveness.sh ~/.cache/origin-eval-pool31/fullpipeline/lme
set -uo pipefail
ROOT="${1:?usage: probe-scenario-liveness.sh <dir containing <qid>/origin_memory.db>}"

# enumerate(db, sql) -> row count via wc -l (NOT COUNT(*))
enum() { sqlite3 "$1" "$2" 2>/dev/null | wc -l | tr -d ' '; }

n=0; tot_mem=0; tot_cls=0; tot_ent=0; tot_pg=0; tot_ed=0; tot_ep=0
zero_pg=0; zero_ent=0; zero_ed=0
printf '%-12s %6s %6s %6s %6s %6s %6s\n' qid mem class ents pages evdate episod
printf -- '------------------------------------------------------------\n'
for d in "$ROOT"/*/; do
  D="$d/origin_memory.db"; [ -f "$D" ] || continue; n=$((n+1))
  qid=$(basename "$d")
  m=$(enum "$D" "SELECT id FROM memories WHERE source='memory';")
  c=$(enum "$D" "SELECT id FROM memories WHERE memory_type IS NOT NULL AND memory_type!='';")
  e=$(enum "$D" "SELECT rowid FROM memory_entities;")
  p=$(enum "$D" "SELECT id FROM pages WHERE status='active';")
  ed=$(enum "$D" "SELECT id FROM memories WHERE event_date IS NOT NULL;")
  ep=$(enum "$D" "SELECT id FROM memories WHERE episode_of IS NOT NULL;")
  tot_mem=$((tot_mem+m)); tot_cls=$((tot_cls+c)); tot_ent=$((tot_ent+e))
  tot_pg=$((tot_pg+p)); tot_ed=$((tot_ed+ed)); tot_ep=$((tot_ep+ep))
  [ "$p" = 0 ] && zero_pg=$((zero_pg+1)); [ "$e" = 0 ] && zero_ent=$((zero_ent+1)); [ "$ed" = 0 ] && zero_ed=$((zero_ed+1))
  printf '%-12s %6s %6s %6s %6s %6s %6s\n' "$qid" "$m" "$c" "$e" "$p" "$ed" "$ep"
done
printf -- '------------------------------------------------------------\n'
echo "DBs=$n | totals mem=$tot_mem class=$tot_cls ents=$tot_ent pages=$tot_pg evdate=$tot_ed episodes=$tot_ep"
echo "channels with ANY zero-DB: pages=$zero_pg ents=$zero_ent event_date=$zero_ed  (episodes empty by design unless WENLAN_ENABLE_EPISODE_CHANNEL seed ran)"
