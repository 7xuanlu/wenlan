#!/usr/bin/env bash
# Regenerate L1 retrieval baselines. Builds binaries first, then runs each
# save_*_baseline retrieval test sequentially. Aborts on first failure.
#
# Optional env:
#   EVAL_MAX_WALL_SECS (default 14400 = 4h)
#   EVAL_BASELINES_DIR (default ~/.cache/origin-eval)
#
# Answer-quality baselines (LLM-judge + Anthropic batch) are NOT yet covered
# by this script — those test functions (save_*_answer_quality_baseline)
# don't exist in eval_harness.rs as of PR #192. Follow-up PR will add them
# together with the cost-tracker wiring.

set -euo pipefail

cd "$(dirname "$0")/.."

echo "==> Building wenlan-core with eval-harness feature"
cargo build -p wenlan-core --features eval-harness

echo "==> Building compare-baselines binary"
cargo build --bin compare-baselines --features eval-harness

run_save() {
    local test_name="$1"
    echo "==> Running $test_name"
    cargo test -p wenlan-core --test eval_harness --features eval-harness \
        "$test_name" -- --ignored --nocapture
}

echo ""
echo "==> Step 1/2: Retrieval baselines (no judge cost)"
run_save save_locomo_baseline
run_save save_locomo_reranked_baseline
run_save save_longmemeval_baseline
run_save save_longmemeval_reranked_baseline

echo ""
echo "==> Step 2/2: Verify outputs"
root="${EVAL_BASELINES_DIR:-$HOME/.cache/origin-eval}"
echo "Listing $root/baselines/l1_db/..."
find "$root/baselines/l1_db" -name "*.json" 2>/dev/null | sort

echo ""
echo "==> Done. Inspect with:"
echo "    jq '.env' $root/baselines/l1_db/locomo/base__*.json"
