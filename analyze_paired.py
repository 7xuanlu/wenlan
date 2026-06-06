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

Pure stdlib (no numpy/scipy). Wilcoxon uses the normal approximation, which is
adequate for n >= ~10; for tiny n the p-value is conservative/approximate and is
flagged in the output. Recommendation column:

  FLIP-ON      iff CI_low(Δndcg) > +0.005 AND BH-significant
                   AND no category mean Δndcg < -cat_se AND latency-neutral
  RE-SEED-NEEDED  for temporal_filter (self-seeds; see harness note)
  KEEP-OFF     otherwise

Usage:
  python3 analyze_paired.py [--dir EVAL_OUT] [--q 0.10] [--boot 10000]
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
    }


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


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dir", default=os.environ.get("EVAL_OUT", "/tmp/eval_paired"))
    ap.add_argument("--q", type=float, default=0.10)
    ap.add_argument("--boot", type=int, default=10000)
    ap.add_argument("--json", default=None)
    ap.add_argument("--md", default=None)
    args = ap.parse_args()

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

    # markdown table
    lines = []
    lines.append(f"# Paired A/B analysis (apparatus v2)\n")
    lines.append(f"- input dir: `{args.dir}`")
    lines.append(f"- BH-FDR q = {args.q}, bootstrap B = {args.boot}")
    lines.append(f"- Wilcoxon = normal approx (tie+continuity corrected); small-n p-values approximate\n")
    hdr = (
        "| feature | bench | n | n_touched | meanΔndcg(all) | 95% CI | Wilcoxon p | BH-sig | worst-cat Δ | ΔP99 lat(ms) | skip% | rec |"
    )
    sep = "|" + "---|" * 13
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
            "| {f} | {b} | {n} | {nt} | {md:+.4f} | [{lo:+.4f},{hi:+.4f}] | {p:.4f} | {sig} | {wc} | {dp99:+.2f} | {skip} | {rec} |".format(
                f=r["feature"], b=r["bench"], n=r["n_paired"], nt=r["n_touched"],
                md=r["mean_dndcg_all"], lo=ci[0], hi=ci[1], p=r["wilcoxon_p"],
                sig="yes" if r["bh_significant"] else "no", wc=wc,
                dp99=r["latency"]["d_p99"], skip=skip, rec=r["recommendation"],
            )
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
