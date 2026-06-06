# app/eval - Eval Discipline

Applies to agents working under `app/eval/`. Read alongside root `AGENTS.md`, which takes precedence on any topic not covered here.

---

## data/ - fixture management

**`app/eval/data/` is not committed.** It is shown as an untracked directory in `git status`. `.gitignore` only excludes `app/eval/data/longmemeval_*.json` by pattern; `locomo10.json` has no explicit gitignore rule but is also not tracked. Neither file ships in the repo. Every worktree must populate its own copy.

### What each file is

| File | Benchmark | Size | Source |
|---|---|---|---|
| `locomo10.json` | LoCoMo (10-conv subset) | ~5MB | https://github.com/snap-research/locomo |
| `longmemeval_oracle.json` | LongMemEval oracle split | ~15MB | https://huggingface.co/datasets/xiaowu0162/longmemeval-cleaned |

### How to populate a new worktree

Option A: copy from a sibling worktree that already has the files:

```bash
find . -name "locomo10.json" -not -path "*/target/*"
# then:
cp /path/to/found/locomo10.json app/eval/data/locomo10.json
```

Option B: download from source:

```bash
mkdir -p app/eval/data
# LoCoMo: obtain locomo10.json from the snap-research/locomo repo (see README there)
# LongMemEval oracle:
curl -sL 'https://huggingface.co/datasets/xiaowu0162/longmemeval-cleaned/resolve/main/longmemeval_oracle.json' \
  -o app/eval/data/longmemeval_oracle.json
```

No import script exists in this repo for the LoCoMo 10-conv subset. If one is added, reference it here.

### What silently SKIPs when a fixture is missing

Tests that call `eval_root().join("data/<fixture>.json")` print "SKIP" and return early if the file is absent. They do not fail: they silently produce no metrics. This is the bug that triggered this doc.

Tests that depend on `locomo10.json`:
- `test_locomo_benchmark`
- `test_locomo_gate_comparison`
- `save_locomo_baseline`
- `save_locomo_reranked_baseline`
- `save_locomo_expanded_baseline`
- `save_locomo_cross_rerank_baseline`
- `save_locomo_v2_with_pages_baseline`
- `benchmark_locomo_pipeline`
- `benchmark_context_path`
- `generate_e2e_context_tuples_locomo`
- `generate_e2e_context_tuples_locomo_api`
- `judge_e2e_context_locomo` (and variants)
- `generate_fullpipeline_locomo`
- `smoke_per_scenario_locomo` (and `_cli`, `_cli_batched`)

Tests that depend on `longmemeval_oracle.json`:
- `test_longmemeval_benchmark`
- `test_longmemeval_gate_comparison`
- `save_longmemeval_baseline`
- `save_longmemeval_reranked_baseline`
- `save_longmemeval_expanded_baseline`
- `save_longmemeval_cross_rerank_baseline`
- `save_longmemeval_v2_with_pages_baseline`
- `benchmark_longmemeval_pipeline`
- `benchmark_context_path_longmemeval`
- `generate_e2e_context_tuples_longmemeval`
- `generate_fullpipeline_lme`
- `smoke_per_scenario_lme`
- `judge_fullpipeline_lme` (and `_cli`)

### Sibling fixture trees

Three sibling subdirs live alongside `data/` and are tracked in git:

- `app/eval/kg_fixtures/*.toml` - hand-curated entity + relation ground-truth, consumed by `eval::kg_faithfulness`. See root `AGENTS.md` "KG-faithfulness bench".
- `app/eval/page_fixtures/*.toml` - hand-curated source memories + distilled page bodies, consumed by `eval::page_faithfulness`. See root `AGENTS.md` "Page-distillation faithfulness bench".
- `app/eval/fixtures/*.toml` - 41 LoCoMo/LME fixtures vendored from `7xuanlu/origin-app` (PR #148). Used by `eval_harness.rs` integration tests.

---

## baselines/ - artifact layout

`app/eval/baselines/` is gitignored. Two parallel artifact paths exist:

### Legacy path (single-file format)

```
app/eval/baselines/<benchmark>__<retrieval_method>__<hash>.json
```

Used by README citations and external references. Written by `report.save_baseline(&baseline_path)`. The `save_locomo_v2_with_pages_baseline` and `save_longmemeval_v2_with_pages_baseline` tests branch the filename suffix on `ORIGIN_ENABLE_PAGE_CHANNEL`:
- page-channel ON (`ORIGIN_ENABLE_PAGE_CHANNEL=1`): `...__with_pages.json`
- page-channel OFF (default, unset): `...__no_pages.json`

### Layered path (P0b schema, for compare-baselines)

```
~/.cache/origin-eval/baselines/l1_db/<task>/<variant_tag>__<comparable_hash>.json
```

Written by `save_full_report` via the `save_layered` helper in `eval_harness.rs`. Used by `compare-baselines`. The `comparable_hash` is a SHA-256[..8] over a fixed subset of `ReportEnv` fields (fixture revision, embedder revision, LLM provider class, LLM model, mcp_schema_hash, skill_prompt_hash, schema_version, schema_db_version, similarity_fn_name). The `variant_tag` string (not flags) is the load-bearing differentiator because `flags` is not yet included in the hash (Task #30).

### Dual-write convention

New PR-B tests call `save_baseline` for the legacy path AND `save_layered` for the layered path. Older tests (`save_locomo_baseline`, `save_longmemeval_baseline`, etc.) call `save_layered` too, via the same pattern.

Exception: `save_locomo_cross_rerank_baseline` and `save_longmemeval_cross_rerank_baseline` write legacy-only by design. This protects the pre-PR-B 0.684/0.883 disk artifacts as a stable reference (see Task 5 plan). New `__with_pages` variants dual-write.

---

## environment variables

| Variable | Purpose | Default | Consumed by |
|---|---|---|---|
| `SCENARIO_DB_ROOT` | Override root for cached scenario DBs | (auto-resolve) | `resolve_scenario_db_root_from_harness`, `cached_scenario_db_check.rs` |
| `EVAL_BASELINES_DIR` | Root of the `~/.cache/origin-eval` cache | `$HOME/.cache/origin-eval` | `baselines_root()`, `resolve_scenario_db_root_from_harness` |
| `EVAL_LOCOMO_LIMIT` | Truncate LoCoMo fixture to N samples | full fixture (10 conversations) | all `run_locomo_eval*` variants |
| `EVAL_LME_LIMIT` | Truncate LME fixture to N samples | full fixture | all `run_longmemeval_eval*` variants |
| `LOCOMO_LIMIT_CONVS` | (fullpipeline only) Limit to first N conversations | full fixture | `generate_fullpipeline_locomo` in `answer_quality.rs` |
| `EVAL_SCENARIO_CONCURRENCY` | Parallel scenario seeding (fullpipeline) | 1 | `generate_fullpipeline_locomo`, `generate_fullpipeline_lme` in `answer_quality.rs` |
| `EVAL_MAX_USD` | Cost cap for API-batch judge runs (pre-flight and per-batch) | none | `anthropic.rs` (`parse_eval_max_usd`, `submit_batch`, `estimate_batch_cost`) |
| `EVAL_MAX_WALL_SECS` | Wall-clock timeout for eval runs | 14400 (4h) | `wall_clock.rs` watchdog |
| `EVAL_ALLOW_WIPE` | Allow `clear_all_for_eval` to wipe DB | unset (refuses) | `open_or_seed_scenario_db` stale-cache recovery |
| `ORIGIN_ENABLE_PAGE_CHANNEL` | Enable page-channel in `search_memory_with_reranker` | unset (page-channel OFF) | `db.rs:search_memory_with_reranker`, `locomo.rs:run_locomo_eval_cross_rerank_from_db`, `longmemeval.rs:run_longmemeval_eval_cross_rerank_from_db`, suffix branching in `eval_harness.rs` |
| `ORIGIN_EVAL_ROOT` | Override `eval_root()` in test harness | `app/eval/` | `eval_root()` in `eval_harness.rs` |

---

## seed scripts and cached-DB workflow

### seed-scenario-dbs.sh

`scripts/seed-scenario-dbs.sh` copies `origin_memory.db` from the canonical fullpipeline DBs to the scenario_seeded layout:

```
~/.cache/origin-eval/scenario_seeded/locomo_v1/origin_memory.db
~/.cache/origin-eval/scenario_seeded/lme_v1/origin_memory.db
```

Run from the repo root:

```bash
bash scripts/seed-scenario-dbs.sh
```

Sources: `~/.cache/origin-eval/fullpipeline_locomo_tuples.db/origin_memory.db` and `~/.cache/origin-eval/fullpipeline_lme_tuples.db/origin_memory.db`. If those originals do not exist, run the fullpipeline harness first (`generate_fullpipeline_locomo` / `generate_fullpipeline_lme`).

### cached_scenario_db_check.rs

`crates/origin-core/tests/cached_scenario_db_check.rs` (L7 manual, `--ignored`) opens each scenario DB via `MemoryDB::new`, which replays migrations idempotently, then prints table counts and 3 sample pages per DB. Root resolution: `SCENARIO_DB_ROOT > EVAL_BASELINES_DIR/scenario_seeded > ~/.cache/origin-eval/scenario_seeded/`.

Run with:

```bash
cargo test -p origin-core --test cached_scenario_db_check -- --ignored --nocapture
```

---

## eval citation discipline

Numbers from `~/.cache/origin-eval/baselines/` carry guardrails that MUST be honored when citing them externally (Reddit, HN, Karpathy gist, vendor decks, README, blog).

- **Single-run rule.** Any baseline with `env.is_single_run = true` MUST NOT be cited externally. Internal team references are fine but must be flagged "single-run, treat as scaffold." Full citation requires the P1.5 multi-run protocol (mean ± stddev over ≥3 runs, ideally 10).
- **Schema-version rule.** Cross-`env.schema_version` comparisons are refused by `compare-baselines` (exit code 2). Public claims that compare numbers across schema versions MUST regenerate both sides via current `save_*_baseline` tests.
- **Receipt-only rule (extends cost-receipt).** Regression thresholds, latency claims, accuracy improvements must have a measured-stddev or N≥3-run backing. No "improved X%" or "regressed Y%" without `compare-baselines` output AND multi-run inputs.
- **Per-case visibility.** Aggregate accuracy claims must include per-case breakdown when available. Headline-only numbers hide regressions (LoCoMo adversarial-cat-5 contamination is the canonical trap; see `feature/eval-semantic-gaps` discussion).
- **Layer attribution.** Public numbers must specify L1 / L2 / L3 / L4. No cross-layer averages without explicit weighting.
- **Commit policy — snapshot, not history.** Metric *values* MAY be committed to git as a **curated, env-stamped snapshot** (the current headline numbers) in a results doc or README section, overwritten per release. Each committed value carries its methodology inline (model, dataset, run count, repro command); single-run results are tagged "scaffold" and headline/external claims still require the Single-run + Receipt-only gates above (N≥3 + stddev). Do **not** commit a per-run *history series* — that is what the gitignored `append_history` file is for. Raw per-run baseline JSONs under `~/.cache/origin-eval/baselines/` stay **gitignored** (artifacts, reproduced by re-running, not source). The repo is Apache-2.0; the older blanket "never commit numbers to git" rule is retired in favor of this snapshot policy.

---

## pre-flight checklist

Before running any eval test:

```
- [ ] app/eval/data/locomo10.json present. If missing, populate via sibling worktree cp or source download
- [ ] app/eval/data/longmemeval_oracle.json present. If missing, download via curl (see above)
- [ ] Cached scenario DBs present at ~/.cache/origin-eval/scenario_seeded/{locomo_v1,lme_v1}/origin_memory.db. If missing, run: bash scripts/seed-scenario-dbs.sh
- [ ] Branch is clean: git status --short shows only intentional changes
- [ ] Limit chosen: EVAL_LOCOMO_LIMIT=2 for ~1min wiring smoke; EVAL_LOCOMO_LIMIT=20 for ~30min subset; unset for full fixture
- [ ] For A/B comparison runs: identical env except the ONE variable being tested
```

---

## subset eval methodology (PR-B page-channel)

Page-channel impact is measured by running the same test twice:

```bash
# page-channel ON (opt-in):
ORIGIN_ENABLE_PAGE_CHANNEL=1 cargo test -p origin-core --test eval_harness save_locomo_v2_with_pages_baseline -- --ignored --nocapture

# page-channel OFF (default):
cargo test -p origin-core --test eval_harness save_locomo_v2_with_pages_baseline -- --ignored --nocapture
```

The cached consolidated scenario DB introduces cross-conversation noise compared to the per-conversation ephemeral DB used by `save_locomo_cross_rerank_baseline`. So numbers from `save_locomo_v2_with_pages_baseline` are NOT directly comparable to the published 0.684 LoCoMo bar. They are comparable to the OFF variant of the same runner (page-channel unset/default).

The `variant_tag` field (`cross_rerank_v2_pages` vs `cross_rerank_v2_no_pages`) is the load-bearing differentiator on the layered baseline path, because `comparable_env_hash` does not yet hash `flags` (pending Task #30).
