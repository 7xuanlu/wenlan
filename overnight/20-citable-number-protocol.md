# Protocol: One Honestly-Citable Headline Number for Origin

Goal: produce a single benchmark number for the field guide (`overnight/10-field-guide.md`)
that satisfies Origin's own "Eval Citation Discipline" (AGENTS.md): no single-run external
citation, N>=3 runs (ideally 10), reported as mean +/- stddev with the env receipt.

This document is the procedure to GET the number. It contains no benchmark number.

---

## 0. What the harness actually measures (read this first)

The LoCoMo runner does NOT compute an LLM-judged QA accuracy today. The field is a placeholder:

> `qa_accuracy: Option<f64>` ... "Currently None - requires an LLM to generate answers"
> [VERIFIED `crates/origin-core/src/eval/locomo.rs:321-323`]

What it DOES compute and serialize is retrieval quality. The primary headline metric is
`aggregate_ndcg_at_10` (NDCG@10), with `aggregate_mrr`, `aggregate_recall_at_5`,
`aggregate_hit_rate_at_1` alongside.
[VERIFIED `crates/origin-core/src/eval/locomo.rs:314-317`; primary flagged "<- primary" at
`crates/origin-core/src/eval/report.rs:339`]

Implication: the only number Origin can honestly cite from this harness right now is a
**retrieval-quality** number, not a "% questions answered correctly" number. Citing it as
"accuracy" would be the exact dishonesty the discipline forbids. State it as NDCG@10.

---

## 1. Which benchmark + variant for the headline

**Recommendation: LoCoMo, base variant, NDCG@10.**

Test: `save_locomo_baseline` [VERIFIED `crates/origin-core/tests/eval_harness.rs:427-444`]
Runner: `origin_core::eval::locomo::run_locomo_eval`
[VERIFIED `crates/origin-core/src/eval/locomo.rs:603`]

Why base, not reranked / expanded / cross-rerank:

- **Base is the honest floor.** It is embedding + FTS + RRF with no LLM in the loop. It needs
  no API key, no Qwen3.5-9B GPU model, and has the fewest moving parts. Per AGENTS.md "Design
  Philosophy" (minimize moving parts) and the eval-citation "Layer attribution" rule, the
  cleanest claim is the one with the least machinery to caveat.
- **Reranked / expanded require Qwen3.5-9B on Metal GPU** (`OnDeviceProvider::new_with_model(Some("qwen3.5-9b"))`,
  [VERIFIED `crates/origin-core/tests/eval_harness.rs:474-476, 520-522`]). That adds a provider
  class to the receipt and a model-availability dependency. Defensible later, not for the first
  honest headline.
- **Cross-rerank downloads ~600MB** and pins `ORIGIN_ENABLE_PAGE_CHANNEL=None`
  [VERIFIED `crates/origin-core/tests/eval_harness.rs:542-562`] - more caveats, not fewer.
- **LongMemEval** is a valid alternative (`save_longmemeval_baseline`,
  [VERIFIED `eval_harness.rs:446-463`]) but its fixture file (`longmemeval_oracle.json`) and the
  larger question set make it the slower, second number. Lead with LoCoMo base, add LongMemEval
  base as the second data point once the first is locked.

The variant is encoded into the baseline filename and the `env.variant` field automatically, so
the receipt self-documents which variant produced the number
[VERIFIED `crates/origin-core/src/eval/report.rs:305-318`, `locomo.rs:575`].

---

## 2. The N>=3 (ideally 10) run procedure

### 2a. Why naive reruns collide

Each `save_locomo_baseline` run writes to a filename built from
`base + variant + llm_provider_class + fixture_revision`
[VERIFIED `crates/origin-core/src/eval/report.rs:305-318`]. None of those vary between runs of
the same config, so run 2 overwrites run 1 on disk. You MUST copy each run's JSON to a unique
path before the next run. The in-file `env.run_id` already carries a nanosecond timestamp
[VERIFIED `locomo.rs:552-558`], so the runs are distinguishable inside the JSON even though the
default filename is not.

### 2b. The runs are independent by construction

Each LoCoMo conversation builds a fresh ephemeral DB and re-seeds all observations
[VERIFIED `locomo.rs:598-602` docstring: "Create fresh ephemeral DB / Seed ALL observations"].
There is no fixed RNG seed knob to vary; independence comes from fresh DB construction +
embedding/index nondeterminism per run, plus a fresh `run_id` each time. Do not fabricate a
`--seed` flag; none exists. Run the same command K times and capture each output separately.

### 2c. Exact commands

Set the cache dir once (AGENTS.md "Eval baselines cache"):

```bash
export EVAL_BASELINES_DIR="$HOME/.cache/origin-eval"
# baselines land in $EVAL_BASELINES_DIR/baselines/ per eval_harness.rs:12,22
```

Run the headline test K times, copying each output to a per-run file. The base test name and
invocation match AGENTS.md ("Generate eval baselines") exactly:

```bash
RUNS=10                                   # >=3 required, 10 ideal (AGENTS.md P1.5)
OUT="$EVAL_BASELINES_DIR/headline_runs"
mkdir -p "$OUT"

for i in $(seq 1 "$RUNS"); do
  echo "=== headline run $i/$RUNS ==="
  cargo test -p origin-core --test eval_harness \
    save_locomo_baseline -- --ignored --nocapture

  # The runner overwrites a fixed filename each run; snapshot it immediately.
  src=$(ls -t "$EVAL_BASELINES_DIR/baselines"/locomo__base__*.json | head -1)
  cp "$src" "$OUT/run_$i.json"
done
```

[VERIFIED test name + invocation: `eval_harness.rs:427-444`; AGENTS.md "Generate eval baselines
(slow, needs Qwen 3.5-9B on Metal GPU)" lists this exact line. Note: base variant does NOT need
the 9B model; only reranked/expanded do.]

### 2d. Stats script (mean +/- stddev + 95% CI + per-case breakdown)

Save as `scripts/headline-stats.py`. It reads the per-run JSONs and prints the citable line.

Formulas (stated explicitly):
- mean: `m = (1/K) * sum(x_i)`
- sample stddev (Bessel-corrected, N-1): `s = sqrt( sum((x_i - m)^2) / (K - 1) )`
- standard error: `se = s / sqrt(K)`
- 95% CI: `m +/- t * se`, with `t` the Student-t critical value at K-1 dof (two-sided 0.975).
  For K>=30 this approaches 1.96; for small K use the t table (provided in the script).

```python
#!/usr/bin/env python3
# Aggregate Origin LoCoMo headline runs -> mean +/- stddev (n=K) + per-category breakdown.
# Usage: python3 scripts/headline-stats.py ~/.cache/origin-eval/headline_runs/run_*.json
import sys, json, math

# Two-sided 0.975 Student-t critical values by dof (K-1). dof>=30 -> ~1.96.
T = {1:12.706,2:4.303,3:3.182,4:2.776,5:2.571,6:2.447,7:2.365,8:2.306,
     9:2.262,10:2.228,15:2.131,20:2.086,29:2.045}
def tcrit(dof):
    if dof in T: return T[dof]
    if dof >= 30: return 1.96
    keys = sorted(T); 
    return T[min(keys, key=lambda k: abs(k-dof))]

def stats(xs):
    k = len(xs); m = sum(xs)/k
    if k < 2: return m, 0.0, (m, m), k
    var = sum((x-m)**2 for x in xs)/(k-1)      # N-1 sample variance
    s = math.sqrt(var); se = s/math.sqrt(k)
    h = tcrit(k-1)*se
    return m, s, (m-h, m+h), k

paths = sys.argv[1:]
if not paths:
    print("usage: headline-stats.py run_1.json run_2.json ..."); sys.exit(1)

reports = [json.load(open(p)) for p in paths]

# Headline metric: aggregate_ndcg_at_10 (locomo.rs:314, the "<- primary" metric).
ndcg = [r["aggregate_ndcg_at_10"] for r in reports]
mrr  = [r["aggregate_mrr"] for r in reports]
rec5 = [r["aggregate_recall_at_5"] for r in reports]

# Env receipt from run 1 (identical across runs by config).
env = reports[0].get("env") or {}
print(f"n runs (K)            : {len(reports)}")
print(f"variant               : {env.get('variant')}")
print(f"schema_version        : {env.get('schema_version')}")
print(f"embedder_model        : {env.get('embedder_model')} ({env.get('embed_dim')}d)")
print(f"llm_provider_class    : {env.get('llm_provider_class')}")
print(f"retrieval_method      : {env.get('retrieval_method')}")
print(f"fixture_revision      : {env.get('fixture_revision')}")
print(f"is_single_run (any)   : {any((r.get('env') or {}).get('is_single_run') for r in reports)}")
print()

for name, xs in [("NDCG@10 (HEADLINE)", ndcg), ("MRR", mrr), ("Recall@5", rec5)]:
    m, s, (lo, hi), k = stats(xs)
    print(f"{name:20s}: {m*100:.1f}% +/- {s*100:.1f}  "
          f"(n={k}, 95% CI [{lo*100:.1f}, {hi*100:.1f}])")

# Per-category breakdown (AGENTS.md "Per-case visibility": aggregate hides regressions).
# Average each category's NDCG@10 across runs.
print("\nPer-category NDCG@10 (mean across runs):")
cats = {}
for r in reports:
    for c in r.get("per_category_aggregate", []):
        cats.setdefault((c["category"], c["name"]), []).append(c["ndcg_at_10"])
for (cid, cname), xs in sorted(cats.items()):
    m, s, _, k = stats(xs)
    print(f"  cat {cid} {cname:24s}: {m*100:.1f}% +/- {s*100:.1f} (n={k})")
```

[VERIFIED JSON keys: `aggregate_ndcg_at_10`, `aggregate_mrr`, `aggregate_recall_at_5`,
`per_category_aggregate`, `env` all serialize from `LocomoReport`
(`crates/origin-core/src/eval/locomo.rs:312-338`); category fields `category`/`name`/`ndcg_at_10`
at `locomo.rs:288-296`; env fields at `report.rs:8-67`.]

Run it:

```bash
python3 scripts/headline-stats.py "$EVAL_BASELINES_DIR"/headline_runs/run_*.json
```

This is the multi-run aggregation AGENTS.md calls the "P1.5 protocol (mean ± stddev over >=3
runs, ideally 10)". Note: the in-repo `compare-baselines` tool does NOT aggregate runs - it has
only `diff` and `paired-mcnemar` subcommands
[VERIFIED `crates/origin-core/src/bin/compare_baselines.rs:46-68`], and it explicitly tells you
to "Use the P1.5 multi-run protocol" for statistical comparison
[VERIFIED `compare_baselines.rs:189`]. So this script fills the gap; it does not duplicate a tool.

---

## 3. Honesty guardrails to report alongside the number

Each maps to a real `ReportEnv` field (`crates/origin-core/src/eval/report.rs:8-67`) or AGENTS.md
rule. All are emitted automatically into the baseline JSON and surfaced by the stats script above.

| Guardrail | Source field / rule | VERIFIED |
|---|---|---|
| schema_version | `env.schema_version` (=1) | report.rs:36-37; locomo.rs:581 |
| embedder | `env.embedder_model` = "BGE-Base-EN-v1.5-Q", `embed_dim`=768 | report.rs:10; locomo.rs:564,576 |
| provider class | `env.llm_provider_class` (base = no LLM; reranked = "qwen3.5-9b") | report.rs:13; locomo.rs:567 |
| fixture revision hash | `env.fixture_revision` via `fixture_revision_hash(path)` | report.rs:9; locomo.rs:549-550 |
| layer (L-level) | `env.layer = EvalLayer::L1Db` | report.rs:21; locomo.rs:573 |
| retrieval method/variant | `env.retrieval_method`, `env.variant` | report.rs:12,25; locomo.rs:566,575 |
| run count N | `env.n_runs` + the K JSONs you aggregate | report.rs:42-43; locomo.rs:551,584 |
| single-run flag | `env.is_single_run` (`n_runs == 1`) | report.rs:45; locomo.rs:585 |
| cost receipt | `env.total_cost_usd`, `total_wall_secs`; base LoCoMo cost ~= $0 (no API) | report.rs:60-63 |
| git sha / version | `env.git_sha` (set `ORIGIN_GIT_SHA`), `env.origin_version` | report.rs:51; locomo.rs:570,588 |

Single-run rule: because each base run sets `is_single_run = true` (n_runs == 1), any ONE JSON is
NOT externally citable [VERIFIED `locomo.rs:585`; AGENTS.md "Single-run rule"]. The citable object
is the aggregate over K>=3, which is what your stats script produces. Tag the committed snapshot
with its methodology inline per AGENTS.md "Commit policy - snapshot, not history" (commit the
curated headline value + repro command to a results doc; do NOT commit the per-run series - those
stay in the gitignored `$EVAL_BASELINES_DIR`).

---

## 4. Pre-flight sanity check (cheap, before the multi-hour full runs)

Use the subset knob to verify direction in ~minutes before committing to the full fixture
(AGENTS.md "Eval pre-flight subset"). `EVAL_LOCOMO_LIMIT` truncates the fixture in place
[VERIFIED `crates/origin-core/src/eval/locomo.rs:71-76`].

```bash
export EVAL_BASELINES_DIR="$HOME/.cache/origin-eval"
EVAL_LOCOMO_LIMIT=2 cargo test -p origin-core --test eval_harness \
  save_locomo_baseline -- --ignored --nocapture
```

Expect `aggregate_ndcg_at_10 > 0.0` and a sane terminal table. The harness already asserts NDCG
is positive in its non-ignored smoke test [VERIFIED `eval_harness.rs:127`]. If the subset run is
sane, drop the limit and run the full K-run loop from section 2c. Never cite a limited run - the
fixture is truncated, so the fixture_revision no longer reflects the full benchmark.

---

## 5. The exact sentence template for the field guide

Fill the bracketed values from the stats-script output. Do not hand-edit the numbers; paste what
the script prints.

> On LoCoMo (base retrieval, NDCG@10), Origin scores **[XX.X]% +/- [Y.Y]** (n=[K] runs, mean +/-
> sample stddev; schema v[1]; BGE-Base-EN-v1.5-Q 768d embedder; on-device, no LLM in the retrieval
> path; fixture rev [hash]; layer L1-DB; cost ~$0, [W]s wall). Repro:
> `EVAL_BASELINES_DIR=~/.cache/origin-eval cargo test -p origin-core --test eval_harness
> save_locomo_baseline -- --ignored --nocapture` (x[K]), then
> `python3 scripts/headline-stats.py ~/.cache/origin-eval/headline_runs/run_*.json`.

Honest-phrasing notes:
- Say **NDCG@10**, not "accuracy". There is no QA-accuracy number yet (`qa_accuracy = None`).
- Always carry **n=K** and **+/- stddev**. A bare percentage is the single-run violation.
- If you later run reranked/expanded, you must change the provider-class clause to name
  Qwen3.5-9B and re-stamp the cost receipt - those variants are not free.

---

## Flags / could-not-confirm

- [INFERRED] Student-t critical values in the script are standard textbook values, not pulled
  from the repo. The repo's `paired-mcnemar` path computes its own CIs
  (`compare_baselines.rs:328-339`) but does not expose a reusable t-table, so the script carries
  its own. Formula is stated; verify against any stats reference.
- [VERIFIED] No `--seed` / RNG knob exists for LoCoMo runs; independence is via fresh ephemeral
  DB per conversation + fresh `run_id`. Do not claim "fixed seed".
- [VERIFIED] `compare-baselines` cannot aggregate N runs into mean +/- stddev; only `diff` and
  `paired-mcnemar`. The Python script is required, not optional.
- [INFERRED] base-variant LoCoMo cost ~$0 / no GPU model: base runner is `run_locomo_eval` with no
  `LlmProvider` argument (`locomo.rs:603`), unlike reranked/expanded which take an `Arc<dyn
  LlmProvider>` (`eval_harness.rs:474-477`). Confirm `total_cost_usd` reads 0 in your JSON before
  citing "~$0".
- The AGENTS.md "Generate eval baselines" block annotates ALL save_* lines as "needs Qwen 3.5-9B".
  That annotation is over-broad: it is true for reranked/expanded (they construct the 9B provider)
  but the base `save_locomo_baseline` takes no provider. The base test runs CPU-only embedding +
  FTS. [VERIFIED by absence of provider arg at `locomo.rs:603` vs presence at `eval_harness.rs:474`.]
