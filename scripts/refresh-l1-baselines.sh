#!/usr/bin/env bash
# Regenerate all L1 baselines. Builds binaries first, then runs each save_*_baseline
# test sequentially. Aborts on first failure.
#
# Required env:
#   ANTHROPIC_API_KEY  (for answer_quality variants only)
#   EVAL_MAX_USD_RUN   (recommended: 5)
#
# Optional env:
#   EVAL_MAX_WALL_SECS (default 14400 = 4h)
#   EVAL_BASELINES_DIR (default ~/.cache/origin-eval)

set -euo pipefail

cd "$(dirname "$0")/.."

echo "==> Building origin-core with eval-harness feature"
cargo build -p origin-core --features eval-harness

echo "==> Building compare-baselines binary"
cargo build --bin compare-baselines --features eval-harness

run_save() {
    local test_name="$1"
    echo "==> Running $test_name"
    cargo test -p origin-core --test eval_harness --features eval-harness \
        "$test_name" -- --ignored --nocapture
}

echo ""
echo "==> Step 1/3: Retrieval baselines (no judge cost)"
run_save save_locomo_baseline
run_save save_locomo_reranked_baseline
run_save save_longmemeval_baseline
run_save save_longmemeval_reranked_baseline

echo ""
echo "==> Step 2/3: Answer-quality baselines (judge cost)"
if [[ -z "${ANTHROPIC_API_KEY:-}" ]]; then
    echo "ERROR: ANTHROPIC_API_KEY not set; answer_quality saves skipped"
    exit 1
fi
if [[ -z "${EVAL_MAX_USD_RUN:-}" ]]; then
    echo "Warning: EVAL_MAX_USD_RUN not set; defaulting to 5"
    export EVAL_MAX_USD_RUN=5
fi
run_save save_locomo_answer_quality_baseline
run_save save_lme_answer_quality_baseline

echo ""
echo "==> Step 3/3: Verify outputs"
root="${EVAL_BASELINES_DIR:-$HOME/.cache/origin-eval}"
echo "Listing $root/baselines/l1_db/..."
find "$root/baselines/l1_db" -name "*.json" 2>/dev/null | sort

echo ""
echo "==> Done. Inspect with:"
echo "    jq '.env' $root/baselines/l1_db/locomo/base__*.json"
