#!/usr/bin/env bash
# P0a pool-expansion sweep: baseline + treatment-A + treatment-B for the
# LoCoMo cross-encoder rerank variant. Each run takes ~3h on Metal GPU
# (M2/M3 Pro) for the full 1540-question fixture; ~30min with
# EVAL_LOCOMO_LIMIT set to a small subset for preflight.
#
# Locked criteria: docs/superpowers/p0a-pool-expansion-acceptance-criteria-2026-05-25.md
#
# Output baselines land at $EVAL_BASELINES_DIR/baselines/l1_db/locomo/ —
# the test writes cross_rerank__<hash>.json. We rename after each run so
# the three variants don't overwrite each other.
#
# Usage:
#   bash scripts/p0a-pool-expansion-sweep.sh                # full sweep (~9h)
#   EVAL_LOCOMO_LIMIT=20 bash scripts/p0a-pool-expansion-sweep.sh  # preflight
#
# Optional:
#   EVAL_BASELINES_DIR  default ~/.cache/origin-eval
#   SKIP_BUILD=1        skip the cargo build step

set -euo pipefail

cd "$(dirname "$0")/.."

ROOT="${EVAL_BASELINES_DIR:-$HOME/.cache/origin-eval}"
LOCOMO_DIR="$ROOT/baselines/l1_db/locomo"
DECISION_LOG="$ROOT/baselines/l1_db/locomo/p0a-pool-expansion-sweep.log"

mkdir -p "$LOCOMO_DIR"
exec > >(tee -a "$DECISION_LOG") 2>&1

echo "==> P0a pool-expansion sweep starting at $(date -u +%FT%TZ)"
echo "    EVAL_BASELINES_DIR=$ROOT"
echo "    EVAL_LOCOMO_LIMIT=${EVAL_LOCOMO_LIMIT:-(full fixture)}"

if [[ "${SKIP_BUILD:-0}" != "1" ]]; then
    echo "==> Building"
    cargo build -p wenlan-core --features eval-harness
    cargo build --bin compare-baselines --features eval-harness
fi

run_save() {
    local label="$1"
    local mult="${2:-}"
    local floor="${3:-}"

    echo ""
    echo "==> Variant: $label (MULT=${mult:-unset} FLOOR=${floor:-unset})"
    echo "    Started: $(date -u +%FT%TZ)"

    # Clear any pre-existing cross_rerank baseline so the rename step
    # cleanly captures the fresh output.
    rm -f "$LOCOMO_DIR"/cross_rerank__*.json.tmp.*
    local fresh_marker
    fresh_marker="$LOCOMO_DIR/.marker-$$-$(date +%s)"
    : > "$fresh_marker"

    (
        if [[ -n "$mult" ]]; then export RERANK_POOL_MULTIPLIER="$mult"; fi
        if [[ -n "$floor" ]]; then export RERANK_POOL_FLOOR="$floor"; fi
        cargo test -p wenlan-core --test eval_harness --features eval-harness \
            save_locomo_cross_rerank_baseline -- --ignored --nocapture
    )

    # Find the newest cross_rerank__*.json under l1_db (the one this run wrote)
    local fresh
    fresh="$(find "$LOCOMO_DIR" -name 'cross_rerank__*.json' -not -name '*__pool*' \
                                -newer "$fresh_marker" -print 2>/dev/null | head -1)"
    rm -f "$fresh_marker"
    if [[ -z "$fresh" ]]; then
        echo "    !! no fresh baseline detected for $label — aborting sweep"
        exit 1
    fi
    local renamed="${fresh%.json}__pool_${label}.json"
    mv "$fresh" "$renamed"
    echo "    Saved → $renamed"
    echo "    Finished: $(date -u +%FT%TZ)"
}

run_save "baseline"
run_save "treatment_A" 3 50
run_save "treatment_B" 5 100

echo ""
echo "==> All three variants saved. Listing:"
ls -la "$LOCOMO_DIR"/cross_rerank__*__pool_*.json

cat <<EOM

==> Next step: paired-McNemar comparison
$ ./target/debug/compare-baselines paired-mcnemar \\
      $LOCOMO_DIR/cross_rerank__*__pool_baseline.json \\
      $LOCOMO_DIR/cross_rerank__*__pool_treatment_A.json \\
      --category multi-hop

  Then repeat with --category temporal / open-domain / single-hop and with
  treatment_B as the second arg. Locked decision rule:

    SHIP if a treatment achieves mid-p < 0.05 AND Δ accuracy CI lower bound > 0
    on cat=1 (multi-hop) AND no category regresses more than -2.0pp.

  See docs/superpowers/p0a-pool-expansion-acceptance-criteria-2026-05-25.md
EOM

echo ""
echo "==> Sweep complete at $(date -u +%FT%TZ)"
