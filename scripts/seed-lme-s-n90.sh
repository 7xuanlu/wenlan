#!/usr/bin/env bash
# Runbook: seed the N=90 stratified deep-S LongMemEval P1+P2 substrate for the
# CE pool/model + page-channel decision (the "gate" at next-best-N below 150).
#
# P1 = classify, P2 = entity + page enrichment (canonical enrichment). NO P3
# answer / 9B, NO judge — this seeds the retrieval substrate only; the pool/model
# sweep reads it later (retrieval NDCG, deterministic, no GPU answer).
#
# VALIDATE-FIRST: `validate` mode seeds a small subset and GATES on TWO things
# before you commit to the multi-hour full run:
#   (1) full-speed batching — mean on-device n_seqs ≥ FILL_FLOOR (M=8 coalescer
#       actually filling, not running serial), via ORIGIN_BATCH_LOG telemetry.
#   (2) complete substrate  — every seeded DB has classify + entities + pages
#       live (the channels the page/CE/pool decision needs), via the enumerate-
#       based liveness probe (never COUNT(*) — see lesson_libsql_count bug).
# Only if BOTH pass should you run `full`.
#
# Usage:
#   scripts/seed-lme-s-n90.sh strat               # build the N=90 fixture only
#   scripts/seed-lme-s-n90.sh validate [N]        # seed+gate first N (default 3)
#   scripts/seed-lme-s-n90.sh full                # seed all 90 (resumable)
#
# GPU: needs Metal. Run OUTSIDE the command sandbox. If a daemon holds :7878 it
# competes for Metal — stop it (`origin stop` / kill) for a clean full-speed run.
set -uo pipefail

# ── Config (override via env) ────────────────────────────────────────────────
MODE="${1:-validate}"
REPO_ROOT="${REPO_ROOT:-/Users/lucian/Repos/origin}"
FIXTURE_SRC="${FIXTURE_SRC:-$REPO_ROOT/app/eval/data/longmemeval_s.json}"   # deep 500-Q
STRAT_FIXTURE="${STRAT_FIXTURE:-/tmp/claude/lme_s_90.json}"
# Dedicated N=90 substrate dir. Decoupled from the ambient EVAL_BASELINES_DIR
# (which often points at the shared ~/.cache/origin-eval and would mix this
# stratified set with unrelated DBs). Override only via POOL90_DIR.
CACHE="${POOL90_DIR:-$HOME/.cache/origin-eval-pool90}"
LOG_DIR="${LOG_DIR:-/tmp/claude/seed_n90}"
FILL_FLOOR="${FILL_FLOOR:-4.0}"          # mean n_seqs gate (M=8; serial=1.0)
PROBE="$(cd "$(dirname "$0")" && pwd)/probe-scenario-liveness.sh"

# Batched on-device config — the ONE set of knobs that makes P1+P2 fill the M=8
# continuous batch (within-question buffer_unordered coalesces into the worker).
# EVAL_SCENARIO_CONCURRENCY = cross-question fan-out. The on-device batch width
# CAPS AT M=8, and conc=4 (x EVAL_ENRICHMENT_CONCURRENCY=8 within-question)
# already SATURATES it — measured n_seqs=8 on every call. So the throughput
# ceiling is raw 4B prefill (~56 enrichments/min, ~15h for N=90 fresh), NOT
# concurrency. conc>4 gives ZERO throughput gain and only adds RAM pressure
# (~1.6GB/question working set); conc=16 SWAPPED here and ran SLOWER than serial.
# Default 4 = the saturation point. Raising it does not speed the seed up.
export EVAL_SCENARIO_CONCURRENCY="${EVAL_SCENARIO_CONCURRENCY:-4}"
export EVAL_ENRICHMENT_CONCURRENCY="${EVAL_ENRICHMENT_CONCURRENCY:-8}"
export ORIGIN_LLM_PARALLEL_SEQS="${ORIGIN_LLM_PARALLEL_SEQS:-8}"
export ORIGIN_LLM_CTX_SIZE="${ORIGIN_LLM_CTX_SIZE:-16384}"
export ORIGIN_LLM_COALESCE_MS="${ORIGIN_LLM_COALESCE_MS:-10}"

# ── Speedup levers (engine-level; all MERGED, all overridable, default ON here) ─
# These are default-OFF in the engine (opt-in) but ON for the seed: each is a
# throughput win with per-sequence semantics unchanged. To A/B a lever, override
# it to 0 on the command line. Authoritative table + safety rationale:
#   crates/origin-core/src/eval/AGENTS.md → "on-device perf levers".
#   BATCHED_POSTINGEST (WAY B): within-Q buffer_unordered extract → fills M=8.
#   SLOT_BACKFILL      (#276):  keeps M=8 slots full as ragged seqs finish (decode).
#   PREFIX_KV          (#278):  caches the shared KG-template prefill (prefill).
#   DISTILL_CLUSTER_CONCURRENCY: parallel distill across independent clusters.
export ORIGIN_SEED_BATCHED_POSTINGEST="${ORIGIN_SEED_BATCHED_POSTINGEST:-1}"
export ORIGIN_LLM_SLOT_BACKFILL="${ORIGIN_LLM_SLOT_BACKFILL:-1}"
export ORIGIN_LLM_PREFIX_KV_CACHE="${ORIGIN_LLM_PREFIX_KV_CACHE:-1}"
export DISTILL_CLUSTER_CONCURRENCY="${DISTILL_CLUSTER_CONCURRENCY:-2}"

export ORIGIN_BATCH_LOG=1                 # emit [batch_log] n_seqs=.. to stderr
export EVAL_ENRICHMENT="${EVAL_ENRICHMENT:-local}"   # on-device Qwen3-4B
export EVAL_BASELINES_DIR="$CACHE"
export LME_S_FIXTURE="$STRAT_FIXTURE"
export RUSTC_WRAPPER=

mkdir -p "$LOG_DIR" "$(dirname "$STRAT_FIXTURE")" "$CACHE"
TEST_BIN_GLOB="$REPO_ROOT/.claude/worktrees/adaptive-rerank-pool/target/debug/deps/eval_harness-*"

# Strat counts must match full LME-S proportions (15.6/26.7/11.1/5.6/14.4/26.7%).
STRAT_COUNTS='{"knowledge-update":14,"multi-session":24,"single-session-assistant":10,"single-session-preference":5,"single-session-user":13,"temporal-reasoning":24}'

# ── Helpers ──────────────────────────────────────────────────────────────────
hr() { printf -- '─%.0s' {1..70}; echo; }

# Print the active perf levers so a run is never silently mis-configured.
perf_banner() {
  echo "[perf] levers: BATCHED_POSTINGEST=$ORIGIN_SEED_BATCHED_POSTINGEST SLOT_BACKFILL=$ORIGIN_LLM_SLOT_BACKFILL PREFIX_KV=$ORIGIN_LLM_PREFIX_KV_CACHE DISTILL_CONC=$DISTILL_CLUSTER_CONCURRENCY"
  echo "[perf] batch:  conc=$EVAL_SCENARIO_CONCURRENCY within-Q=$EVAL_ENRICHMENT_CONCURRENCY M=$ORIGIN_LLM_PARALLEL_SEQS ctx=$ORIGIN_LLM_CTX_SIZE coalesce=${ORIGIN_LLM_COALESCE_MS}ms"
}

build_strat() {
  [ -f "$FIXTURE_SRC" ] || { echo "FATAL: deep fixture missing: $FIXTURE_SRC"; exit 1; }
  if [ -f "$STRAT_FIXTURE" ]; then
    echo "[strat] reuse existing $STRAT_FIXTURE ($(python3 -c "import json;print(len(json.load(open('$STRAT_FIXTURE'))))") Q)"
    return
  fi
  echo "[strat] building $STRAT_FIXTURE from $FIXTURE_SRC ..."
  python3 - "$FIXTURE_SRC" "$STRAT_FIXTURE" "$STRAT_COUNTS" <<'PY'
import json, sys
from collections import defaultdict
src, out, counts = sys.argv[1], sys.argv[2], json.loads(sys.argv[3])
d = json.load(open(src))
qs = d if isinstance(d, list) else d.get("questions", d)
by = defaultdict(list)
for q in qs:
    by[q.get("question_type", "?")].append(q)
sel = []
for t, n in counts.items():
    g = sorted(by.get(t, []), key=lambda q: q.get("question_id", ""))  # deterministic
    if len(g) < n:
        print(f"WARN {t}: only {len(g)} < {n} requested", file=sys.stderr)
    sel.extend(g[:n])
json.dump(sel, open(out, "w"))
print(f"[strat] wrote {len(sel)} questions -> {out}")
PY
}

# Warn if EVAL_SCENARIO_CONCURRENCY won't fit free RAM (~1.6GB/question working
# set). Thrash here is silent — it just collapses throughput — so surface it up front.
ram_warn() {
  local total_gb free_pct free_gb need_gb
  total_gb=$(( $(sysctl -n hw.memsize 2>/dev/null || echo 0) / 1073741824 ))
  free_pct=$(memory_pressure 2>/dev/null | grep -oE 'free percentage: [0-9]+' | grep -oE '[0-9]+' | head -1)
  [ -z "${free_pct:-}" ] && return 0
  free_gb=$(( total_gb * free_pct / 100 ))
  need_gb=$(awk "BEGIN{printf \"%d\", 1.6*$EVAL_SCENARIO_CONCURRENCY + 4}")
  if [ "$free_gb" -lt "$need_gb" ]; then
    echo "WARN: conc=$EVAL_SCENARIO_CONCURRENCY needs ~${need_gb}GB free but only ${free_gb}GB free"
    echo "      (~1.6GB/question + 4GB margin). It will likely SWAP and run SLOWER than serial."
    echo "      Free RAM (close apps) or lower EVAL_SCENARIO_CONCURRENCY."
  else
    echo "[ram] conc=$EVAL_SCENARIO_CONCURRENCY fits: ~${need_gb}GB needed, ${free_gb}GB free."
  fi
}

daemon_warn() {
  local pid; pid="$(lsof -ti :7878 2>/dev/null | head -1 || true)"
  if [ -n "${pid:-}" ]; then
    echo "WARN: a process holds :7878 (PID $pid) — it competes for Metal and will"
    echo "      depress fill/speed. Stop it for a clean full-speed run:  kill -9 $pid"
  fi
}

# Run the seed-only test for [skip, skip+limit). Streams progress + batch_log to
# a tee'd log; prints a live ETA line per question.
run_seed() {
  local limit="$1" skip="${2:-0}" log="$3"
  : > "$log"
  LME_LIMIT_QUESTIONS="$limit" LME_SKIP_QUESTIONS="$skip" \
  cargo test -p origin-core --test eval_harness --features eval-harness \
    enrich_fullpipeline_lme_only -- --ignored --nocapture 2>&1 \
  | tee "$log" \
  | awk -v total="$limit" '
      /\[lme_enrich_only\] [0-9]+\// {
        # parse "N/T idx=.. q=.. mem=.. enriched=a/b elapsed=Xs total=Ym"
        for (i=1;i<=NF;i++){
          if ($i ~ /^[0-9]+\/[0-9]+$/){ split($i,a,"/"); done=a[1] }
          if ($i ~ /^total=/){ sub("total=","",$i); sub("m$","",$i); mins=$i }
        }
        if (done>0 && mins>0){ eta=(mins/done)*(total-done); printf "  >> %d/%d done, %.1fm elapsed, ETA %.1fm\n", done, total, mins, eta }
      }
      /\[batch_log\]/ { next }   # batch_log lines stay in the tee log, not stdout
      { print }
    '
}

# Mean n_seqs + batching_rate from the tee'd log's [batch_log] lines.
report_fill() {
  local log="$1"
  python3 - "$log" "$FILL_FLOOR" <<'PY'
import sys, re
log, floor = sys.argv[1], float(sys.argv[2])
ns = [int(m) for m in re.findall(r"\[batch_log\] n_seqs=(\d+)", open(log, errors="ignore").read())]
if not ns:
    print("  fill: NO [batch_log] lines — ORIGIN_BATCH_LOG not honored or no on-device calls"); sys.exit(3)
mean = sum(ns)/len(ns)
batched = sum(1 for n in ns if n >= 2)/len(ns)
print(f"  fill: calls={len(ns)} mean_n_seqs={mean:.2f} batching_rate={batched:.0%} (floor {floor})")
sys.exit(0 if mean >= floor else 4)
PY
}

# ── Modes ────────────────────────────────────────────────────────────────────
case "$MODE" in
  strat)
    build_strat ;;

  validate)
    VN="${2:-8}"   # >= a fan-out wave so cross-question width is actually exercised
    hr; echo "VALIDATE: seed first $VN Q + gate on fill (≥$FILL_FLOOR) and substrate"; hr
    perf_banner; build_strat; ram_warn; daemon_warn
    LOG="$LOG_DIR/validate.log"
    echo "[validate] seeding $VN Q -> $CACHE (log: $LOG)"
    run_seed "$VN" 0 "$LOG"
    hr; echo "GATE 1 — batching fill:"; FILL_OK=1; report_fill "$LOG" || FILL_OK=0
    echo "GATE 2 — substrate liveness (enumerate, never COUNT):"
    PROBE_OUT="$(bash "$PROBE" "$CACHE/fullpipeline/lme" 2>/dev/null)"
    echo "$PROBE_OUT" | tail -3
    # The probe's penultimate line is "channels with ANY zero-DB: pages=N ents=M
    # event_date=K ...". N/M/K are COUNTS of starved DBs — pass iff pages & ents
    # are both 0 (no DB lacks that substrate). (event_date/episodes are not P1/P2,
    # so they don't gate the page/CE/pool decision.)
    SUB_OK=1
    zp="$(echo "$PROBE_OUT" | grep -oE 'pages=[0-9]+' | tail -1 | cut -d= -f2)"
    ze="$(echo "$PROBE_OUT" | grep -oE 'ents=[0-9]+' | tail -1 | cut -d= -f2)"
    [ "${zp:-1}" = 0 ] && [ "${ze:-1}" = 0 ] || SUB_OK=0
    hr
    if [ "$FILL_OK" = 1 ]; then echo "  ✓ fill ≥ floor — running full speed"; else echo "  ✗ fill BELOW floor — free the GPU (stop daemon) or raise concurrency, re-validate"; fi
    echo "  (review the probe table above: every DB must have mem/class/ents/pages > 0)"
    echo "PROCEED to 'full' only if fill passed AND the probe shows no zero columns."
    ;;

  full)
    hr; echo "FULL: seed all 90 Q -> $CACHE (resumable; cached Q are skipped)"; hr
    perf_banner; build_strat; ram_warn; daemon_warn
    LOG="$LOG_DIR/full_$(date +%Y%m%d_%H%M%S 2>/dev/null || echo run).log"
    echo "[full] log: $LOG   (tail -f to watch)"
    run_seed 90 0 "$LOG"
    hr; echo "FULL DONE. Fill summary:"; report_fill "$LOG" || true
    echo "Substrate:"; bash "$PROBE" "$CACHE/fullpipeline/lme" 2>/dev/null | tail -2
    ;;

  *)
    echo "usage: $0 {strat|validate [N]|full}"; exit 2 ;;
esac
