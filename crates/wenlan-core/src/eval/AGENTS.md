# crates/origin-core/src/eval - Rust runner conventions

Applies to agents working under `crates/origin-core/src/eval/`. Read alongside root `AGENTS.md` and `app/eval/AGENTS.md` for fixture + artifact context.

---

## runner shapes

Two runner families exist. Choose based on what question you are answering.

### Ephemeral-per-conversation runners

Create a `tempdir` + `MemoryDB::new` + upsert observations per conversation. Each conversation gets a fresh isolated DB. No page distillation runs because pages are a background task that requires a full enrichment pipeline.

Examples: `run_locomo_eval`, `run_locomo_eval_cross_rerank`, `run_locomo_eval_expanded`, and their LME equivalents.

Use when: measuring the published baseline bar, or any retrieval-only benchmark where pages should not be a factor.

### From-cached-DB runners (PR-B)

Accept a pre-seeded `&MemoryDB` for a consolidated scenario DB that already contains memories, KG entities, and distilled pages. Skips ingest entirely. The DB was seeded once via the fullpipeline harness.

Examples: `run_locomo_eval_cross_rerank_from_db`, `run_longmemeval_eval_cross_rerank_from_db`.

Use when: measuring page-channel impact, or any question that requires pages to already exist in the DB.

Tradeoff: consolidated DB introduces cross-conversation noise because all conversations share one DB. Results are comparable to the OFF variant of the same runner, not to the ephemeral-per-conversation bar.

---

## source_id conventions

Source IDs must match exactly between the seeder and the evaluator. A mismatch produces silent zero NDCG with no error.

### LoCoMo

```rust
source_id: format!("locomo_{}_obs_{}", sample.sample_id, i)
```

Used in: ephemeral runner seed paths and the fullpipeline harness. The `run_locomo_eval_cross_rerank_from_db` runner re-derives this mapping from the fixture file, so the cached DB must have been seeded with the same format.

### LongMemEval

```rust
// Use the helper, not a hand-rolled format!:
memory_source_id(question_id, session_idx, turn_idx)
// defined in longmemeval.rs
```

The helper exists precisely so the format is defined once. Do not inline an equivalent `format!` call elsewhere.

---

## build_*_env stamping

`build_locomo_env` and `build_lme_env` produce a `ReportEnv` struct that stamps every baseline. Key fields:

- `variant`: the `variant_tag` string, a human-readable differentiator for the layered baseline path. As of Task #30, `comparable_env_hash` also includes `flags`, so the hash itself now distinguishes flag-only differences; `variant` is retained for readability.
- `flags`: human-readable `Vec<String>` with `"key=value"` entries. Wired into `comparable_env_hash` (Task #30, sorted so order is insignificant) and used for audit. Always populate it.
- `is_single_run`: always `true` for single-run tests. Any baseline with `is_single_run = true` must not be cited externally (see root AGENTS.md Eval Citation Discipline).

### When adding a new A/B variant

1. Branch `variant_tag` on the distinguishing env var. Pattern from `locomo.rs`:

```rust
let page_channel_state = if crate::db::page_channel_enabled() {
    "on"
} else {
    "off"
};
let variant_tag = if page_channel_state == "on" {
    "cross_rerank_v2_pages"
} else {
    "cross_rerank_v2_no_pages"
};
```

2. Push the state into `flags`:

```rust
env_stamp.flags.push(format!("page_channel={}", page_channel_state));
env_stamp.flags.push("scenario_db=consolidated".to_string());
```

3. Mirror the filename suffix in `eval_harness.rs` (legacy path). The harness branches `__with_pages` vs `__no_pages` on `ORIGIN_ENABLE_PAGE_CHANNEL` (opt-in, default OFF).

---

## filename suffix on legacy path

`eval_harness.rs` branches the `app/eval/baselines/` filename suffix on `ORIGIN_ENABLE_PAGE_CHANNEL`:

```rust
let suffix = if wenlan_core::db::page_channel_enabled() {
    "__with_pages"
} else {
    "__no_pages"
};
```

When adding a new A/B variant driven by a different env var, add an analogous suffix branch so page-ON and page-OFF artifacts don't overwrite each other at the legacy path.

---

## smoke vs full run sizing

| `EVAL_*_LIMIT` | Time | Use case |
|---|---|---|
| `2` | ~1 min | Wiring smoke: verify the test reaches the eval loop and saves a baseline |
| `20` | ~30 min | Subset eval: direction check for A/B comparison |
| unset | ~30 min-3h | Full fixture: for results worth citing |

Reranker first-run downloads the model weights (~1.1GB for the default bge-reranker-base; ~600MB-2.27GB for others). Account for that on cold caches.

---

## pages_count sanity gate (PR-B convention)

When using a cached scenario DB, check that pages exist BEFORE the eval loop. The PR-B runners SKIP with a clear message when the table is empty:

```rust
let pages_count = db.count_active_pages().await.expect("count_active_pages failed");
if pages_count == 0 {
    println!(
        "SKIP: cached scenario DB has 0 active pages at {}. Run scripts/seed-scenario-dbs.sh from the repo root then verify with cached_scenario_db_compat_check.",
        db_dir.display()
    );
    return;
}
```

SKIP semantics (rather than panic) match the surrounding fixture-missing branches so a contributor without seeded DBs gets actionable output instead of a thread panic. Without this gate, a corrupt or empty page table silently produces page-OFF metrics stamped with the page-ON variant tag. The mislabeling would contaminate any external citation.

---

## ephemeral vs cached: decision table

| Question | Runner shape | DB |
|---|---|---|
| What is the baseline NDCG for retrieval-only? | ephemeral-per-conv | tempdir |
| Does page-channel improve NDCG? | from-cached-DB, run twice (ON/OFF) | scenario_seeded |
| Does a new retrieval signal help? | ephemeral-per-conv (no pages needed) | tempdir |
| Does distillation quality affect retrieval? | from-cached-DB (pages already distilled) | scenario_seeded |

---

## seed completeness: one route, one contract (no drift)

Seeding a cached scenario DB is ONE orchestrator, not a scatter of STEP tests: run `seed_scenario_dbs_complete` (`tests/eval_harness.rs`). It chains event_date inject → classify → `memory_entities` sweep → episodes → distill, then asserts `SeedExpectations::complete()`. Never hand-run the individual `seed_*` STEP tests; they are its internals.

`seed_contract.rs` is the single liveness contract, gating BOTH ends:
- **Producer:** `seed_scenario_dbs_complete` asserts `complete()` — hard-fails on `memory_entities=0` (graph), `event_date=0` (temporal), or `pages=0` active (page channel; matches `MemoryDB::count_active_pages`). Presence checks, not percentages (percentages rot).
- **Consumer:** every per-query collector calls `assert_feature_substrate_live(conn, feature)` at entry — a graph/temporal/page A/B over an empty substrate **errors ("EVAL REFUSED")** rather than emitting a null that reads as "doesn't help". A starved-substrate lie is structurally impossible.

Adding a channel with an A/B: add its step to the orchestrator, its floor to `SeedExpectations`, its key to `assert_feature_substrate_live`, and a `seed_contract.rs` unit test. See root `AGENTS.md` "Eval seed + eval read: ONE route, ONE contract".

---

## scenario DB cache env flags

Three env flags govern how `open_or_seed_scenario_db` handles a stale or mismatched cached scenario DB.

| Flag | Default | Effect |
|---|---|---|
| `EVAL_ALLOW_WIPE` | unset (refuses) | Permits wiping + reseeding from scratch: a partial-state DB via `clear_all_for_eval`, or a cache-env-mismatched DB by removing its `origin_memory.db` file (`std::fs::remove_file`). |
| `EVAL_PARALLEL_OK` | unset (refuses) | Allows concurrent access to a locked scenario DB (results may be corrupted). |
| `EVAL_MIGRATE_STALE` | unset (refuses) | Migrates a schema-stale cached scenario DB IN PLACE, without wipe or re-seed. |

### EVAL_MIGRATE_STALE details

`EVAL_MIGRATE_STALE=1` triggers the migrate path when `open_or_seed_scenario_db` detects that the DB's `cache_env.json` has a schema/migrations stamp mismatch but the enricher provenance (`enricher_provider` + `enricher_model`) matches the current run. It refuses cloud-enriched or unstamped-legacy DBs (anti-laundering: a DB seeded by a different model cannot be silently promoted as if it were seeded by the current one). The migrate guard requires the DB to already be fully enriched (`enriched == mem_count`, no partial state) and substrate-live (temporal, graph, pages). If either check fails, the path returns an error and you must re-seed (or set `EVAL_ALLOW_WIPE=1`).

After the migration guard passes, execution falls through into the shared Phase-1 classification backfill (the same block that runs on a normal cache hit). This ensures that a migrated OLD DB that predates the classification pass gets `importance`/`quality` backfilled before the eval loop sees it, preventing T8/T11/T15 training-serving skew. There is exactly one `write_cache_env_stamp` + `return Ok(db)` for both the migrate and cache-hit paths (at the bottom of the shared block).

## paired A/B reading: the G3 gate (analyze_paired.py)

`analyze_paired.py` (repo root) applies the Eval-Trust v3 G3 "A/A-floored attributed liveness" gate on top of Wilcoxon + BH-FDR. Any `<feature>_aa_<bench>.jsonl` in the input dir is an A/A no-op control (flag OFF on both arms) and sets that bench's noise floor (aggregate + per-category |meanΔndcg|). Every other feature gets a `G3` verdict: `SIGNAL` (above floor, right direction, attributed, BH-sig), `WEAK` (same minus BH-sig), `NOISE-FLOOR`, `WRONG-DIR`, `UNATTRIBUTED` (<90% of moved queries carry the channel's touch flag — precedence: `channel_touched` (generic, highest) > `temporal_touched` > `graph_skipped`; features with a wired probe: `graph_stream` arms — probe is whether the stream would contribute ≥1 linked memory (non-person anchors under hub cap with linked memories, mirrors `MemoryDB::graph_stream_touches`); `rerank_skip_pref` — probe is `is_preference_query`, the bypass IS the channel; `rerank`/`rerank_model_*` — probe is CE top-10 differing from base top-10, collectors fetch base ids outside the latency window; `rerank_graph_stack` — stream probe (arm retired on graph-stream-default-on branch); page/episode/fact/global_prelude carry NO probe intentionally — verdicts stay `*` until an internals probe exists), `NO-FLOOR` (no A/A run for the bench — emit one, e.g. the `rerank_window_aa` pattern, before trusting verdicts). Per-category verdicts are conditions 1+2 only (no per-category p-value). Selftest: `python3 analyze_paired.py --selftest`.

Trust calibration: a `*` suffix (`SIGNAL*`/`WEAK*`) means the rows carried no touch flag, so attribution was vacuous — `graph_stream`, `rerank_skip_pref`, `rerank`/`rerank_model_*`, and `rerank_graph_stack` arms now emit `channel_touched` probes (condition 3 is real for those); page/episode/fact/global_prelude intentionally carry no probe yet — their `*` verdicts are honest, not a bug, and stay until an internals probe is wired. Floors are bench-keyed, not path-keyed: don't co-locate a CE-path A/A with base-path feature files (noise characteristics differ; the CE-path A/A measured NON-deterministic — a full-ndcg flip between identical OFF arms at smoke n — which the analyzer surfaces as a `WARNING` when a floor source has `n_touched > 0`). `paired_ab_emit` itself emits no `_aa` file; co-locate an A/A from the matching path/bench or every verdict reads `NO-FLOOR`.

## cross-reference

- Fixture population, env vars, seed scripts, baseline layout: see `app/eval/AGENTS.md`.
- Citation rules (single-run, schema-version, receipt-only): see root `AGENTS.md` "Eval Citation Discipline".
