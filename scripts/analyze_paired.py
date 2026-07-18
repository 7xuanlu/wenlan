#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Paired-stats post-processor for the apparatus-v2 per-query JSONL emitter.

Reads `<feature>_<bench>.jsonl` files produced by the `paired_ab_emit` Rust test.
Each file holds per-query rows for BOTH flag arms (`flag_state` in {off,on}).
The analyzer joins the OFF and ON arms by `(bench, query_id)` to form per-query
paired deltas, then computes, per (feature, bench):

  - n_touched: queries where ndcg_on != ndcg_off
  - mean Δndcg / Δrecall over the TOUCHED subset
  - Wilcoxon signed-rank p-value (normal approx, tie + continuity corrected)
  - bootstrap 95% CI of mean Δndcg over ALL paired queries (B=10000, percentile)
  - per-category mean Δ + n
  - latency P50/P99 for on vs off + Δ
  - graph-gate skip rate (fraction of ON-arm queries with graph_skipped=true)

Across the whole family, Benjamini-Hochberg FDR at q=0.10 flags significance.

G3 — "A/A-floored attributed liveness" gate (Eval-Trust v3): any `<feature>_aa`
file in the dir is an A/A no-op control (flag OFF on both arms) and becomes the
per-bench noise floor (aggregate + per-category |mean Δndcg|, max over A/A
runs). Every other feature on that bench gets a G3 verdict:

  SIGNAL        |Δ| > floor AND Δ > 0 AND attributed AND BH-significant
  WEAK          conditions hold but not BH-significant
  NOISE-FLOOR   |Δ| inside the A/A floor — indistinguishable from no-op noise
  WRONG-DIR     above floor but negative
  UNATTRIBUTED  <90% of moved queries carry the channel's touch flag
                (channel_touched / temporal_touched / not graph_skipped); movement the channel
                disclaims is confound, not lever
  NO-FLOOR      bench has no A/A run — verdict unavailable, not a pass
  FLOOR-SRC     the A/A run itself

A per-category verdict table (conditions 1+2 only — no per-category p-value)
makes per-category deltas readable as above-floor vs noise. Run `--selftest`
for the synthetic-scenario asserts.

G3 trust calibration (read before acting on a verdict):
- A `*` suffix (SIGNAL*/WEAK*) = the rows carried no touch flag, so the
  attribution condition was VACUOUS. Today channel arms emit channel_touched
  (highest precedence), temporal arms emit temporal_touched, and graph arms
  run with ORIGIN_ENABLE_GRAPH_GATE on emit graph_skipped; for every other
  feature attribution is unchecked, not passed.
- The floor is keyed by bench only. A/A controls exist for the CE/cross-rerank
  path; do NOT co-locate a CE-path A/A with base-path feature files and trust
  the boundary — path noise characteristics differ. A deterministic base-path
  A/A floors at 0.0000 (any nonzero delta reads above-floor: intended — a
  deterministic pipeline has no noise floor).
- A floor-source with n_touched > 0 triggers a WARNING: the "no-op" control
  was not a no-op (CE path measured non-deterministic), and at smoke n a
  single outlier inflates the |mean Δ| floor. Re-run the A/A at full n before
  trusting NOISE-FLOOR boundaries near it.

Pure stdlib (no numpy/scipy). Wilcoxon uses the normal approximation, which is
adequate for n >= ~10; for tiny n the p-value is conservative/approximate and is
flagged in the output. Recommendation column:

  FLIP-ON      iff CI_low(Δndcg) > +0.005 AND BH-significant
                   AND no category mean Δndcg < -cat_se AND latency-neutral
  RE-SEED-NEEDED  for temporal_filter (self-seeds; see harness note)
  KEEP-OFF     otherwise

Usage:
  python3 scripts/analyze_paired.py [--dir EVAL_OUT] [--q 0.10] [--boot 10000]
                            [--json out.json] [--md out.md]
Defaults: --dir $EVAL_OUT or /tmp/eval_paired, prints markdown to stdout.
"""
import argparse
import glob
import json
import math
import os
import random
import statistics
import sys
from collections import defaultdict

random.seed(1234)  # reproducible bootstrap


def load_rows(path):
    rows = []
    with open(path) as f:
        for line in f:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def normal_cdf(x):
    return 0.5 * (1.0 + math.erf(x / math.sqrt(2.0)))


def wilcoxon_signed_rank_p(deltas):
    """Two-sided Wilcoxon signed-rank p-value via normal approximation.

    Drops zero deltas (standard Wilcoxon). Applies tie correction to the
    variance and a continuity correction. Returns (p, n_nonzero, W)."""
    nz = [d for d in deltas if d != 0.0]
    n = len(nz)
    if n == 0:
        return (1.0, 0, 0.0)
    absd = sorted((abs(d), d) for d in nz)
    # rank with ties averaged
    ranks = [0.0] * n
    i = 0
    while i < n:
        j = i
        while j + 1 < n and absd[j + 1][0] == absd[i][0]:
            j += 1
        avg_rank = (i + 1 + j + 1) / 2.0  # 1-based ranks i..j
        for k in range(i, j + 1):
            ranks[k] = avg_rank
        i = j + 1
    w_plus = sum(r for r, (_, d) in zip(ranks, absd) if d > 0)
    w_minus = sum(r for r, (_, d) in zip(ranks, absd) if d < 0)
    W = min(w_plus, w_minus)
    mean_w = n * (n + 1) / 4.0
    # tie correction
    from collections import Counter
    tie_counts = Counter(a for a, _ in absd)
    tie_term = sum(t**3 - t for t in tie_counts.values())
    var_w = (n * (n + 1) * (2 * n + 1) - tie_term / 2.0) / 24.0
    if var_w <= 0:
        return (1.0, n, W)
    z = (W - mean_w)
    # continuity correction toward the mean
    if z < 0:
        z += 0.5
    else:
        z -= 0.5
    z = z / math.sqrt(var_w)
    p = 2.0 * normal_cdf(-abs(z))
    return (min(1.0, p), n, W)


def bootstrap_ci_mean(values, B=10000, alpha=0.05):
    """Percentile bootstrap CI of the mean."""
    n = len(values)
    if n == 0:
        return (float("nan"), float("nan"))
    means = []
    for _ in range(B):
        s = 0.0
        for _ in range(n):
            s += values[random.randrange(n)]
        means.append(s / n)
    means.sort()
    lo = means[int((alpha / 2.0) * B)]
    hi = means[min(B - 1, int((1.0 - alpha / 2.0) * B))]
    return (lo, hi)


def percentile(sorted_vals, p):
    """Nearest-rank percentile matching the production Rust estimator in
    crates/origin-core/src/eval/latency.rs::latency_summary (the source of truth
    for P50/P99): floor each sample to integer milliseconds, sort, then pick
    index ``(n*p - 1) // 100`` (saturating at 0).

    Deliberately NOT linear interpolation. Using the SAME estimator as
    latency.rs lets this paired report's P50/P99 reconcile with the baseline
    EvalReport.latency numbers instead of diverging on identical samples — the
    old linear-interp put P99 strictly below the max at small n (e.g. n=20)
    while latency.rs returns the max."""
    if not sorted_vals:
        return float("nan")
    vals_ms = sorted(int(v) for v in sorted_vals)  # floor to int ms, as latency.rs
    n = len(vals_ms)
    idx = max(0, n * p - 1) // 100
    return vals_ms[min(idx, n - 1)]


def benjamini_hochberg(pvals, q=0.10):
    """Return a set of indices that are significant under BH at level q."""
    m = len(pvals)
    if m == 0:
        return set()
    order = sorted(range(m), key=lambda i: pvals[i])
    sig = set()
    thresh_rank = -1
    for rank, i in enumerate(order, start=1):
        if pvals[i] <= (rank / m) * q:
            thresh_rank = rank
    if thresh_rank >= 0:
        for rank, i in enumerate(order, start=1):
            if rank <= thresh_rank:
                sig.add(i)
    return sig


def analyze_pair(feature, bench, rows):
    off = {}
    on = {}
    for r in rows:
        key = r["query_id"]
        if r["flag_state"] == "off":
            off[key] = r
        elif r["flag_state"] == "on":
            on[key] = r
    keys = sorted(set(off) & set(on))
    paired = [(off[k], on[k]) for k in keys]
    if not paired:
        return None

    dndcg = [o2["ndcg10"] - o1["ndcg10"] for o1, o2 in paired]
    drecall = [o2["recall5"] - o1["recall5"] for o1, o2 in paired]
    dmrr = [o2["mrr"] - o1["mrr"] for o1, o2 in paired]

    # For page_channel feature: compute marginal coverage delta
    # marginal = coverage_on (expanded) - coverage_off (blind).
    # Compares: ON arm with page->source expansion vs OFF arm with no pages.
    d_marginal_cov = []
    for o1, o2 in paired:
        if feature == "page_channel" and \
           o1.get("cov_blind") is not None and o1.get("cov_expanded") is not None and \
           o2.get("cov_blind") is not None and o2.get("cov_expanded") is not None:
            # o1 (OFF arm): cov_blind (no page expansion)
            # o2 (ON arm): cov_expanded (page->source expansion)
            # marginal = on_coverage - off_coverage
            d_marginal_cov.append(o2["cov_expanded"] - o1["cov_blind"])
        else:
            d_marginal_cov.append(0.0)

    touched_idx = [i for i, d in enumerate(dndcg) if d != 0.0]
    n_touched = len(touched_idx)
    mean_dndcg_touched = (
        statistics.fmean(dndcg[i] for i in touched_idx) if touched_idx else 0.0
    )
    mean_drecall_touched = (
        statistics.fmean(drecall[i] for i in touched_idx) if touched_idx else 0.0
    )

    p, n_nonzero, _ = wilcoxon_signed_rank_p(dndcg)
    ci_lo, ci_hi = bootstrap_ci_mean(dndcg)

    # per-category
    cat = defaultdict(list)
    for (o1, o2), d in zip(paired, dndcg):
        cat[o1["category"]].append(d)
    per_cat = {}
    worst_cat = None
    worst_cat_mean = 0.0
    for c, ds in sorted(cat.items()):
        m = statistics.fmean(ds)
        se = (statistics.pstdev(ds) / math.sqrt(len(ds))) if len(ds) > 1 else 0.0
        per_cat[c] = {"n": len(ds), "mean_dndcg": m, "se": se}
        if m < worst_cat_mean:
            worst_cat_mean = m
            worst_cat = c

    # latency
    lat_off = sorted(o1["latency_ms"] for o1, _ in paired)
    lat_on = sorted(o2["latency_ms"] for _, o2 in paired)
    lat = {
        "p50_off": percentile(lat_off, 50),
        "p99_off": percentile(lat_off, 99),
        "p50_on": percentile(lat_on, 50),
        "p99_on": percentile(lat_on, 99),
    }
    lat["d_p50"] = lat["p50_on"] - lat["p50_off"]
    lat["d_p99"] = lat["p99_on"] - lat["p99_off"]

    # graph-gate skip rate (ON arm)
    skipped = [1 for _, o2 in paired if o2.get("graph_skipped") is True]
    skip_rate = (len(skipped) / len(paired)) if paired else 0.0
    has_skip = any(o2.get("graph_skipped") is not None for _, o2 in paired)

    # G3 attribution: of the queries the flag flip actually moved (Δndcg != 0),
    # what fraction does the channel itself claim to have touched? Movement on
    # a query the channel disclaims (channel not touched / temporal cue did not
    # fire / graph gate skipped) is unattributed — noise or confound, not the
    # lever. None when the rows carry no flag (vacuously attributed; the gate
    # never blocks what it cannot check). channel_touched has highest precedence,
    # then temporal_touched, then graph_skipped.
    changed_idx = [i for i, d in enumerate(dndcg) if d != 0.0]
    has_channel_flag = any(
        o2.get("channel_touched") is not None for _, o2 in paired
    )
    has_temporal_flag = any(
        o2.get("temporal_touched") is not None for _, o2 in paired
    )
    attribution_src = None
    attribution_ratio = None
    if has_channel_flag:
        attribution_src = "channel_touched"
        claimed = [
            i for i in changed_idx if paired[i][1].get("channel_touched") is True
        ]
        attribution_ratio = (len(claimed) / len(changed_idx)) if changed_idx else 1.0
    elif has_temporal_flag:
        attribution_src = "temporal_touched"
        claimed = [
            i for i in changed_idx if paired[i][1].get("temporal_touched") is True
        ]
        attribution_ratio = (len(claimed) / len(changed_idx)) if changed_idx else 1.0
    elif has_skip:
        attribution_src = "graph_skipped"
        claimed = [
            i for i in changed_idx if paired[i][1].get("graph_skipped") is False
        ]
        attribution_ratio = (len(claimed) / len(changed_idx)) if changed_idx else 1.0

    # T4a per-touched subset: when the ON arm carries a temporal_touched flag,
    # restrict the delta to the queries whose temporal cue actually fired. The
    # full-set delta is dominated by no-op queries (~96.6%), so this isolates the
    # signal the feature can possibly move. Additive — does not change full-set.
    touched_temporal_idx = [
        i for i, (_, o2) in enumerate(paired) if o2.get("temporal_touched") is True
    ]
    has_temporal = any(o2.get("temporal_touched") is not None for _, o2 in paired)
    per_touched = None
    if has_temporal:
        per_touched = {
            "n": len(touched_temporal_idx),
            "mean_dndcg": (
                statistics.fmean(dndcg[i] for i in touched_temporal_idx)
                if touched_temporal_idx
                else 0.0
            ),
            "mean_drecall": (
                statistics.fmean(drecall[i] for i in touched_temporal_idx)
                if touched_temporal_idx
                else 0.0
            ),
            "mean_dmrr": (
                statistics.fmean(dmrr[i] for i in touched_temporal_idx)
                if touched_temporal_idx
                else 0.0
            ),
        }

    # Marginal coverage stats (page_channel feature only)
    marginal_cov_stats = None
    if feature == "page_channel" and any(d != 0.0 for d in d_marginal_cov):
        marginal_cov_stats = {
            "mean_marginal_cov": statistics.fmean(d_marginal_cov),
            "raw_values": d_marginal_cov,
        }

    return {
        "feature": feature,
        "bench": bench,
        "n_paired": len(paired),
        "n_touched": n_touched,
        "n_nonzero_ndcg": n_nonzero,
        "mean_dndcg_touched": mean_dndcg_touched,
        "mean_drecall_touched": mean_drecall_touched,
        "mean_dndcg_all": statistics.fmean(dndcg),
        "mean_dmrr_all": statistics.fmean(dmrr),
        "wilcoxon_p": p,
        "ci95_dndcg_all": [ci_lo, ci_hi],
        "per_category": per_cat,
        "worst_category": worst_cat,
        "worst_category_mean_dndcg": worst_cat_mean,
        "latency": lat,
        "graph_skip_rate": skip_rate if has_skip else None,
        "per_touched_temporal": per_touched,
        "attribution_src": attribution_src,
        "attribution_ratio": attribution_ratio,
        "marginal_coverage": marginal_cov_stats,
    }


def is_aa_feature(feature):
    """A/A no-op control runs (flag OFF on both arms) follow the `<feature>_aa`
    naming convention (e.g. rerank_window_aa). They are the noise-floor source."""
    return feature.endswith("_aa")


def compute_aa_floors(results):
    """Per-bench noise floor from the A/A (OFF-vs-OFF) runs in the family: the
    largest |mean Δndcg| any A/A run produced, aggregate and per-category.
    Multiple A/A features for one bench take the max (conservative). A bench
    with no A/A run gets no floor — its features read NO-FLOOR rather than
    being gated against an invented number."""
    floors = {}
    for r in results:
        if not is_aa_feature(r["feature"]):
            continue
        f = floors.setdefault(r["bench"], {"agg": 0.0, "per_cat": {}, "sources": []})
        f["agg"] = max(f["agg"], abs(r["mean_dndcg_all"]))
        for c, info in r["per_category"].items():
            f["per_cat"][c] = max(f["per_cat"].get(c, 0.0), abs(info["mean_dndcg"]))
        f["sources"].append(r["feature"])
    return floors


def g3_verdict(res, floors):
    """G3 "A/A-floored attributed liveness" verdict for the aggregate Δndcg.

    SIGNAL requires ALL of: (1) |Δ| strictly above the bench's A/A noise floor,
    (2) right direction (Δ > 0), (3) attribution — when the channel emits a
    touch flag, ≥90% of the moved queries must carry it — plus BH significance.
    WEAK = conditions 1-3 pass but the delta is not BH-significant.
    A/A runs themselves read FLOOR-SRC; a bench without an A/A run reads
    NO-FLOOR (verdict unavailable, not a pass).

    A trailing `*` (SIGNAL* / WEAK*) means the rows carried NO touch flag, so
    condition 3 was vacuous — channel arms emit channel_touched (highest
    precedence), temporal arms emit temporal_touched, graph arms run with
    ORIGIN_ENABLE_GRAPH_GATE on emit graph_skipped; everything else is
    unchecked, and the star keeps that visible instead of letting an unchecked
    verdict read as a fully-gated one."""
    if is_aa_feature(res["feature"]):
        return "FLOOR-SRC"
    floor = floors.get(res["bench"])
    if floor is None:
        return "NO-FLOOR"
    d = res["mean_dndcg_all"]
    if abs(d) <= floor["agg"]:
        return "NOISE-FLOOR"
    if d < 0:
        return "WRONG-DIR"
    ratio = res.get("attribution_ratio")
    if ratio is not None and ratio < 0.9:
        return "UNATTRIBUTED"
    star = "" if ratio is not None else "*"
    return ("SIGNAL" if res.get("bh_significant") else "WEAK") + star


def aa_warnings(results):
    """Sanity warnings on the floor sources themselves. A no-op control should
    touch nothing; an A/A with n_touched > 0 means the path is non-deterministic
    (measured: the CE path flips a query's full ndcg between identical OFF
    arms), and at smoke sizes a single such outlier inflates the |mean Δ| floor
    enough to mask genuine feature signal as NOISE-FLOOR. Surface it rather
    than gate silently on a suspect floor."""
    warns = []
    for r in results:
        if not is_aa_feature(r["feature"]):
            continue
        if r["n_touched"] > 0:
            warns.append(
                f"A/A {r['feature']}/{r['bench']}: n_touched={r['n_touched']} of "
                f"{r['n_paired']} (a no-op control should touch nothing) — the "
                f"path is non-deterministic and the floor "
                f"({abs(r['mean_dndcg_all']):.4f}) may be outlier-inflated at "
                f"small n; treat verdicts near the floor as suspect."
            )
    return warns


def g3_per_category(res, floors):
    """Per-category G3 reading: conditions 1+2 only (above the per-category A/A
    floor, right direction). There is no per-category p-value, so no
    SIGNAL/WEAK split — this is directional readability, not a
    multiplicity-corrected claim. A category absent from the A/A data falls
    back to the aggregate floor."""
    floor = floors.get(res["bench"])
    out = {}
    for c, info in res["per_category"].items():
        if floor is None:
            out[c] = "NO-FLOOR"
            continue
        cat_floor = floor["per_cat"].get(c, floor["agg"])
        m = info["mean_dndcg"]
        if abs(m) <= cat_floor:
            out[c] = "NOISE-FLOOR"
        elif m < 0:
            out[c] = "WRONG-DIR"
        else:
            out[c] = "ABOVE-FLOOR"
    return out


def recommend(res, bh_sig):
    if res["feature"] == "temporal_filter":
        return "RE-SEED-NEEDED"
    ci_lo = res["ci95_dndcg_all"][0]
    cat_ok = True
    for c, info in res["per_category"].items():
        if info["mean_dndcg"] < -max(info["se"], 1e-9):
            cat_ok = False
            break
    latency_neutral = res["latency"]["d_p99"] <= 5.0  # ms tolerance
    if ci_lo > 0.005 and bh_sig and cat_ok and latency_neutral:
        return "FLIP-ON"
    return "KEEP-OFF"


def _selftest_rows(feature, bench, deltas, categories, touched=None, skipped=None,
                   channel_touched=None):
    """Build synthetic OFF/ON row pairs. deltas[i] = ndcg_on - ndcg_off for
    query qi; categories[i] its category; touched/skipped optional per-query
    ON-arm flags (temporal_touched / graph_skipped / channel_touched)."""
    rows = []
    for i, d in enumerate(deltas):
        base = 0.5
        common = {"feature": feature, "bench": bench, "query_id": f"q{i}",
                  "category": categories[i], "recall5": base, "mrr": base,
                  "latency_ms": 100.0}
        off = dict(common, flag_state="off", ndcg10=base)
        on = dict(common, flag_state="on", ndcg10=base + d)
        if touched is not None:
            on["temporal_touched"] = touched[i]
        if skipped is not None:
            on["graph_skipped"] = skipped[i]
        if channel_touched is not None:
            on["channel_touched"] = channel_touched[i]
        rows.append(off)
        rows.append(on)
    return rows


def selftest():
    """G3 gate selftest: synthetic scenarios, hard asserts. Run via --selftest."""
    cats = ["tr", "tr", "ms", "ms"] * 5  # 20 queries, 2 categories

    # --- A/A floor source: |mean Δ| = 0.002 agg
    aa_deltas = [0.002, -0.002, 0.006, 0.002] * 5
    aa = analyze_pair("foo_aa", "lme", _selftest_rows("foo_aa", "lme", aa_deltas, cats))
    assert is_aa_feature("foo_aa") and not is_aa_feature("foo")
    floors = compute_aa_floors([aa])
    assert "lme" in floors, "A/A feature must produce a floor for its bench"
    assert abs(floors["lme"]["agg"] - 0.002) < 1e-12, floors["lme"]["agg"]
    # per-cat floors: tr mean = (0.002-0.002)*... tr deltas alternate 0.002/-0.002 → 0.0;
    # ms deltas alternate 0.006/0.002 → 0.004
    assert abs(floors["lme"]["per_cat"]["tr"] - 0.0) < 1e-12
    assert abs(floors["lme"]["per_cat"]["ms"] - 0.004) < 1e-12

    # --- NOISE-FLOOR: |Δ| below agg floor
    small = analyze_pair("tiny", "lme", _selftest_rows("tiny", "lme", [0.001] * 20, cats))
    small["bh_significant"] = True
    assert g3_verdict(small, floors) == "NOISE-FLOOR", g3_verdict(small, floors)

    # --- WRONG-DIR: above floor but negative
    neg = analyze_pair("worse", "lme", _selftest_rows("worse", "lme", [-0.05] * 20, cats))
    neg["bh_significant"] = True
    assert g3_verdict(neg, floors) == "WRONG-DIR", g3_verdict(neg, floors)

    # --- UNATTRIBUTED: positive, above floor, but the channel says it did not
    # touch the moved queries (temporal_touched=false on changed rows)
    unattr = analyze_pair(
        "temporal_x", "lme",
        _selftest_rows("temporal_x", "lme", [0.05] * 20, cats, touched=[False] * 20),
    )
    unattr["bh_significant"] = True
    assert unattr["attribution_ratio"] == 0.0
    assert g3_verdict(unattr, floors) == "UNATTRIBUTED", g3_verdict(unattr, floors)

    # --- SIGNAL: above floor, right direction, attributed, BH-significant
    sig = analyze_pair(
        "temporal_y", "lme",
        _selftest_rows("temporal_y", "lme", [0.05] * 20, cats, touched=[True] * 20),
    )
    sig["bh_significant"] = True
    assert sig["attribution_ratio"] == 1.0
    assert g3_verdict(sig, floors) == "SIGNAL", g3_verdict(sig, floors)

    # --- WEAK: all three G3 conditions pass but not BH-significant
    weak = analyze_pair(
        "temporal_y2", "lme",
        _selftest_rows("temporal_y2", "lme", [0.05] * 20, cats, touched=[True] * 20),
    )
    weak["bh_significant"] = False
    assert g3_verdict(weak, floors) == "WEAK", g3_verdict(weak, floors)

    # --- graph_skipped attribution: a changed row on a graph-skipped query is
    # movement the channel disclaims → unattributed
    gskip = analyze_pair(
        "graph_z", "lme",
        _selftest_rows("graph_z", "lme", [0.05] * 20, cats, skipped=[True] * 20),
    )
    gskip["bh_significant"] = True
    assert gskip["attribution_ratio"] == 0.0
    assert g3_verdict(gskip, floors) == "UNATTRIBUTED", g3_verdict(gskip, floors)
    # inverse: graph ran on every changed row → attributed
    gok = analyze_pair(
        "graph_ok", "lme",
        _selftest_rows("graph_ok", "lme", [0.05] * 20, cats, skipped=[False] * 20),
    )
    gok["bh_significant"] = True
    assert gok["attribution_ratio"] == 1.0
    assert g3_verdict(gok, floors) == "SIGNAL", g3_verdict(gok, floors)

    # --- channel_touched attribution: highest-precedence flag (overrides temporal/graph)
    # (a) every moved query has channel_touched=True → attributed, no star, SIGNAL
    ch_sig = analyze_pair(
        "chan_a", "lme",
        _selftest_rows("chan_a", "lme", [0.05] * 20, cats, channel_touched=[True] * 20),
    )
    ch_sig["bh_significant"] = True
    assert ch_sig["attribution_src"] == "channel_touched", ch_sig.get("attribution_src")
    assert ch_sig["attribution_ratio"] == 1.0, ch_sig["attribution_ratio"]
    assert g3_verdict(ch_sig, floors) == "SIGNAL", g3_verdict(ch_sig, floors)
    # (b) no moved query has channel_touched=True → ratio 0.0, UNATTRIBUTED
    ch_unattr = analyze_pair(
        "chan_b", "lme",
        _selftest_rows("chan_b", "lme", [0.05] * 20, cats, channel_touched=[False] * 20),
    )
    ch_unattr["bh_significant"] = True
    assert ch_unattr["attribution_src"] == "channel_touched", ch_unattr.get("attribution_src")
    assert ch_unattr["attribution_ratio"] == 0.0, ch_unattr["attribution_ratio"]
    assert g3_verdict(ch_unattr, floors) == "UNATTRIBUTED", g3_verdict(ch_unattr, floors)
    # (c) precedence: both channel_touched and temporal_touched present → channel wins
    ch_prec = analyze_pair(
        "chan_c", "lme",
        _selftest_rows("chan_c", "lme", [0.05] * 20, cats,
                       touched=[True] * 20, channel_touched=[True] * 20),
    )
    ch_prec["bh_significant"] = True
    assert ch_prec["attribution_src"] == "channel_touched", ch_prec.get("attribution_src")

    # --- no flag data at all → vacuously attributed (gate never blocks what it
    # can't check), but the verdict carries a star so unchecked attribution
    # stays visible
    plain = analyze_pair("plain", "lme", _selftest_rows("plain", "lme", [0.05] * 20, cats))
    plain["bh_significant"] = True
    assert plain["attribution_ratio"] is None
    assert g3_verdict(plain, floors) == "SIGNAL*", g3_verdict(plain, floors)
    plain["bh_significant"] = False
    assert g3_verdict(plain, floors) == "WEAK*", g3_verdict(plain, floors)

    # --- A/A floor-source sanity warning: the synthetic A/A above has nonzero
    # per-query deltas (n_touched > 0) → must warn; a truly-deterministic A/A
    # (all deltas zero) must not
    warns = aa_warnings([aa])
    assert len(warns) == 1 and "n_touched" in warns[0], warns
    aa_clean = analyze_pair(
        "quiet_aa", "lme", _selftest_rows("quiet_aa", "lme", [0.0] * 20, cats)
    )
    assert aa_warnings([aa_clean]) == [], aa_warnings([aa_clean])

    # --- NO-FLOOR: bench without any A/A run
    other = analyze_pair("plain2", "locomo", _selftest_rows("plain2", "locomo", [0.05] * 20, cats))
    other["bh_significant"] = True
    assert g3_verdict(other, floors) == "NO-FLOOR", g3_verdict(other, floors)

    # --- A/A feature itself is labeled as the floor source
    assert g3_verdict(aa, floors) == "FLOOR-SRC"

    # --- per-category verdicts: ms floor is 0.004 → a +0.003 ms delta is noise,
    # tr floor 0.0 → +0.003 tr delta is above floor
    mixed = analyze_pair("mix", "lme", _selftest_rows("mix", "lme", [0.003] * 20, cats))
    pc = g3_per_category(mixed, floors)
    assert pc["tr"] == "ABOVE-FLOOR", pc
    assert pc["ms"] == "NOISE-FLOOR", pc
    negcat = analyze_pair("negcat", "lme", _selftest_rows("negcat", "lme", [-0.05] * 20, cats))
    pcn = g3_per_category(negcat, floors)
    assert pcn["tr"] == "WRONG-DIR" and pcn["ms"] == "WRONG-DIR", pcn

    print("selftest OK")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dir", default=os.environ.get("EVAL_OUT", "/tmp/eval_paired"))
    ap.add_argument("--q", type=float, default=0.10)
    ap.add_argument("--boot", type=int, default=10000)
    ap.add_argument("--json", default=None)
    ap.add_argument("--md", default=None)
    ap.add_argument("--selftest", action="store_true",
                    help="run the G3 gate selftest and exit")
    args = ap.parse_args()

    if args.selftest:
        selftest()
        return

    files = sorted(glob.glob(os.path.join(args.dir, "*.jsonl")))
    if not files:
        print(f"No *.jsonl files in {args.dir}", file=sys.stderr)
        sys.exit(1)

    results = []
    for path in files:
        base = os.path.basename(path)[: -len(".jsonl")]
        # split feature_bench: bench is last underscore token
        if "_" not in base:
            continue
        feature, bench = base.rsplit("_", 1)
        rows = load_rows(path)
        res = analyze_pair(feature, bench, rows)
        if res:
            results.append(res)

    # BH FDR across the family (using Δndcg Wilcoxon p)
    pvals = [r["wilcoxon_p"] for r in results]
    sig_idx = benjamini_hochberg(pvals, q=args.q)
    for i, r in enumerate(results):
        r["bh_significant"] = i in sig_idx
        r["recommendation"] = recommend(r, r["bh_significant"])

    # G3: A/A-floored attributed liveness gate
    floors = compute_aa_floors(results)
    for r in results:
        r["g3"] = g3_verdict(r, floors)
        r["g3_per_category"] = (
            g3_per_category(r, floors) if not is_aa_feature(r["feature"]) else None
        )

    # markdown table
    lines = []
    lines.append(f"# Paired A/B analysis (apparatus v2)\n")
    lines.append(f"- input dir: `{args.dir}`")
    lines.append(f"- BH-FDR q = {args.q}, bootstrap B = {args.boot}")
    lines.append(f"- Wilcoxon = normal approx (tie+continuity corrected); small-n p-values approximate\n")
    hdr = (
        "| feature | bench | n | n_touched | meanΔndcg(all) | 95% CI | Wilcoxon p | BH-sig | worst-cat Δ | ΔP99 lat(ms) | skip% | rec | G3 |"
    )
    sep = "|" + "---|" * 14
    lines.append(hdr)
    lines.append(sep)
    for r in results:
        ci = r["ci95_dndcg_all"]
        skip = (
            f"{100*r['graph_skip_rate']:.0f}%" if r["graph_skip_rate"] is not None else "-"
        )
        wc = (
            f"{r['worst_category']}={r['worst_category_mean_dndcg']:+.4f}"
            if r["worst_category"]
            else "-"
        )
        lines.append(
            "| {f} | {b} | {n} | {nt} | {md:+.4f} | [{lo:+.4f},{hi:+.4f}] | {p:.4f} | {sig} | {wc} | {dp99:+.2f} | {skip} | {rec} | {g3} |".format(
                f=r["feature"], b=r["bench"], n=r["n_paired"], nt=r["n_touched"],
                md=r["mean_dndcg_all"], lo=ci[0], hi=ci[1], p=r["wilcoxon_p"],
                sig="yes" if r["bh_significant"] else "no", wc=wc,
                dp99=r["latency"]["d_p99"], skip=skip, rec=r["recommendation"],
                g3=r["g3"],
            )
        )

    # G3 noise floors + per-category verdicts (only when an A/A run exists)
    if floors:
        lines.append("\n## G3 noise floor (from A/A OFF-vs-OFF runs)\n")
        lines.append("| bench | agg floor | per-category floors | sources |")
        lines.append("|" + "---|" * 4)
        for b, f in sorted(floors.items()):
            cats = " ".join(
                f"{c}={v:.4f}" for c, v in sorted(f["per_cat"].items())
            ) or "-"
            lines.append(
                f"| {b} | {f['agg']:.4f} | {cats} | {', '.join(f['sources'])} |"
            )
        for w in aa_warnings(results):
            lines.append(f"\n**WARNING:** {w}")
        lines.append(
            "\n## G3 per-category verdicts\n"
        )
        lines.append(
            "Conditions 1+2 only (above per-category A/A floor, right direction); "
            "no per-category p-value, so directional readability — not a "
            "multiplicity-corrected claim. Attribution ratio is feature-level.\n"
        )
        lines.append("| feature | bench | category | meanΔndcg | cat floor | verdict |")
        lines.append("|" + "---|" * 6)
        for r in results:
            pc = r.get("g3_per_category")
            if not pc or r["bench"] not in floors:
                continue
            f = floors[r["bench"]]
            for c in sorted(pc):
                cat_floor = f["per_cat"].get(c, f["agg"])
                m = r["per_category"][c]["mean_dndcg"]
                lines.append(
                    f"| {r['feature']} | {r['bench']} | {c} | {m:+.4f} "
                    f"| {cat_floor:.4f} | {pc[c]} |"
                )
    # T4a per-touched subset (temporal_filter and any future cue-gated feature):
    # the full-set delta above averages over mostly-no-op queries, so additionally
    # report the delta over only the queries whose cue actually fired.
    touched_results = [r for r in results if r.get("per_touched_temporal") is not None]
    if touched_results:
        lines.append("\n## Per-touched subset (temporal cue fired)\n")
        lines.append(
            "Delta over only the ON-arm queries whose temporal cue fired "
            "(`temporal_touched == true`); the full-set table above dilutes this "
            "with the no-op majority.\n"
        )
        lines.append("| feature | bench | n_touched | meanΔndcg | meanΔrecall | meanΔmrr |")
        lines.append("|" + "---|" * 6)
        for r in touched_results:
            pt = r["per_touched_temporal"]
            lines.append(
                "| {f} | {b} | {n} | {dn:+.4f} | {dr:+.4f} | {dm:+.4f} |".format(
                    f=r["feature"], b=r["bench"], n=pt["n"],
                    dn=pt["mean_dndcg"], dr=pt["mean_drecall"], dm=pt["mean_dmrr"],
                )
            )

    md = "\n".join(lines) + "\n"
    print(md)

    if args.md:
        with open(args.md, "w") as f:
            f.write(md)
    if args.json:
        with open(args.json, "w") as f:
            json.dump(results, f, indent=2)
        print(f"wrote JSON -> {args.json}", file=sys.stderr)


if __name__ == "__main__":
    main()
