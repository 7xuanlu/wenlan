#!/usr/bin/env bash
# Refresh L2 (live-daemon HTTP) baselines.
#
# P2 status: the L2 runners (`run_locomo_l2`, `run_longmemeval_l2`) are
# scaffolded but NOT YET WIRED — they spawn the daemon, run smoke
# preflight, then return a sentinel `NOT YET WIRED` error. This script
# exists now so the wiring PR only has to swap the runner implementation;
# the orchestration + compare-baselines step is already in place.
#
# Until the runner lands, running this script will EXIT NON-ZERO with the
# sentinel error visible in the test output. That is the expected,
# documented behavior.
#
# Prereqs:
#   - L1 baselines already generated (see `scripts/refresh-l1-baselines.sh`)
#   - `cargo build -p origin-server` (this script does this for you)
#
# Override the source eval data dir:
#   ORIGIN_EVAL_ROOT=/path/to/app/eval ./scripts/refresh-l2-baselines.sh

set -euo pipefail
cd "$(dirname "$0")/.."

echo "==> Building origin-core + origin-server (eval-harness feature)"
cargo build -p origin-core -p origin-server --features origin-core/eval-harness

run_save() {
    local test_name="$1"
    echo ""
    echo "==> Running $test_name"
    cargo test -p origin-core --test eval_harness --features eval-harness \
        "$test_name" -- --ignored --nocapture --test-threads=1 || {
            echo "    (expected: NOT YET WIRED sentinel until follow-up wires the scoring loop)"
        }
}

# All four invocations expected to fail with NOT YET WIRED until wiring lands.
run_save save_locomo_l2_baseline_returns_not_wired_until_scoring_lands
run_save save_locomo_l2_reranked_baseline_returns_not_wired_until_scoring_lands
run_save save_longmemeval_l2_baseline_returns_not_wired_until_scoring_lands
run_save save_longmemeval_l2_reranked_baseline_returns_not_wired_until_scoring_lands

echo ""
echo "==> L2 baselines on disk (expected: none yet, runner NOT YET WIRED):"
find "${HOME}/.cache/origin-eval/baselines/l2_http" -name "*.json" 2>/dev/null | sort || true

# L1 vs L2 comparison gated on both files existing. Skipped silently until
# the wiring PR lands and L2 baselines start showing up under l2_http/.
echo ""
echo "==> L1 vs L2 comparison (per task / variant):"
for task in locomo longmemeval; do
    for variant in base reranked; do
        l1=$(ls "${HOME}/.cache/origin-eval/baselines/l1_db/${task}/${variant}__"*.json 2>/dev/null | head -1 || true)
        l2=$(ls "${HOME}/.cache/origin-eval/baselines/l2_http/${task}/${variant}__"*.json 2>/dev/null | head -1 || true)
        if [[ -n "${l1}" && -n "${l2}" ]]; then
            echo ""
            echo "--- ${task} / ${variant} ---"
            ./target/debug/compare-baselines "${l1}" "${l2}" || true
        fi
    done
done

echo ""
echo "Done. Once the wiring PR lands, re-run this script and L2 baselines"
echo "will appear under ~/.cache/origin-eval/baselines/l2_http/."
