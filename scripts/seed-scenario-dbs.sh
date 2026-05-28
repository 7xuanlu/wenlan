#!/usr/bin/env bash
# Repopulate ~/.cache/origin-eval/scenario_seeded/ from canonical
# fullpipeline_*.db.
#
# Used by `crates/origin-core/tests/cached_scenario_db_check.rs` and the
# PR-B page-channel eval runners
# (`save_locomo_v2_with_pages_baseline` / `save_longmemeval_v2_with_pages_baseline`).
#
# Idempotent. Skips files that already exist so reruns are cheap.
# Honors EVAL_BASELINES_DIR override; defaults to $HOME/.cache/origin-eval.
set -euo pipefail

CACHE="${EVAL_BASELINES_DIR:-$HOME/.cache/origin-eval}/scenario_seeded"
LOCOMO_SRC="${EVAL_BASELINES_DIR:-$HOME/.cache/origin-eval}/fullpipeline_locomo_tuples.db/origin_memory.db"
LME_SRC="${EVAL_BASELINES_DIR:-$HOME/.cache/origin-eval}/fullpipeline_lme_tuples.db/origin_memory.db"

mkdir -p "$CACHE/locomo_v1" "$CACHE/lme_v1"

if [ ! -f "$LOCOMO_SRC" ]; then
    echo "missing LoCoMo source: $LOCOMO_SRC" >&2
    echo "expected the canonical fullpipeline cache; rerun the LoCoMo fullpipeline harness or symlink the source" >&2
    exit 1
fi
if [ ! -f "$LME_SRC" ]; then
    echo "missing LME source: $LME_SRC" >&2
    echo "expected the canonical fullpipeline cache; rerun the LME fullpipeline harness or symlink the source" >&2
    exit 1
fi

[ -f "$CACHE/locomo_v1/origin_memory.db" ] || cp "$LOCOMO_SRC" "$CACHE/locomo_v1/origin_memory.db"
[ -f "$CACHE/lme_v1/origin_memory.db" ] || cp "$LME_SRC" "$CACHE/lme_v1/origin_memory.db"

echo "Seeded scenario DBs at $CACHE"
ls -la "$CACHE/locomo_v1/origin_memory.db" "$CACHE/lme_v1/origin_memory.db"
