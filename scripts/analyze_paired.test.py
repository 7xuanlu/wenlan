#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Reference tests for analyze_paired.py — the apparatus-v2 stats post-processor.

Locks the decision-gating statistics with hand-computed reference values:
  - percentile reconciliation against crates/origin-core/src/eval/latency.rs
  - Benjamini-Hochberg FDR (the gate with no Rust twin)
  - bootstrap CI of the mean (the FLIP-ON gate keys on its lower bound)
  - Wilcoxon signed-rank

The analyzer is a manual L7 tool (not in cargo CI), so this is its safety net.
Pure stdlib. Run before trusting a paired report:

    python3 scripts/analyze_paired.test.py
"""
import analyze_paired as a


def _latency_rs_percentile(samples, p):
    """Independent re-derivation of latency.rs::latency_summary's index pick:
    floor to int ms, sort, idx = (n*p - 1)//100 (saturating at 0)."""
    vals = sorted(int(v) for v in samples)
    n = len(vals)
    idx = max(0, n * p - 1) // 100
    return vals[min(idx, n - 1)]


def test_percentile_matches_latency_rs():
    # n=20, values 1..20 ms: latency.rs P99 -> idx (20*99-1)//100=19 -> max=20;
    # P50 -> idx 999//100=9 -> sorted[9]=10.
    vals = sorted(range(1, 21))
    assert a.percentile(vals, 99) == 20, a.percentile(vals, 99)
    assert a.percentile(vals, 50) == 10, a.percentile(vals, 50)
    # n=100, 1..100: P99 idx (9900-1)//100=98 -> 99; P50 idx 4999//100=49 -> 50.
    vals = sorted(range(1, 101))
    assert a.percentile(vals, 99) == 99
    assert a.percentile(vals, 50) == 50
    # floors to int ms like latency.rs (us/1000 floor): [1.9,2.9] -> [1,2],
    # P50 idx (2*50-1)//100=0 -> 1.
    assert a.percentile(sorted([1.9, 2.9]), 50) == 1
    # n=1
    assert a.percentile([7.4], 99) == 7
    # breadth: cross-check against the independent re-derivation over many n.
    for n in (1, 5, 10, 20, 37, 100, 250):
        samples = [(i * 7 + 3) % 97 + 0.5 for i in range(n)]
        for p in (50, 99):
            assert a.percentile(sorted(samples), p) == _latency_rs_percentile(samples, p), (n, p)


def test_percentile_is_not_linear_interp():
    # Regression guard: the old bug used linear interpolation, which put P99
    # strictly below the max at small n. Nearest-rank must return the max.
    vals = sorted(range(1, 21))
    assert a.percentile(vals, 99) == max(vals)


def test_benjamini_hochberg():
    # Classic BH: p=[.01,.02,.03,.04,.05], q=.05, m=5 -> every rank k passes
    # p_k <= (k/5)*.05, so all five are significant.
    assert a.benjamini_hochberg([0.01, 0.02, 0.03, 0.04, 0.05], q=0.05) == {0, 1, 2, 3, 4}
    # One clear winner, rest null: only index 0 significant at q=0.10.
    assert a.benjamini_hochberg([0.001, 0.5, 0.5, 0.5, 0.5], q=0.10) == {0}
    # All large p -> empty.
    assert a.benjamini_hochberg([0.4, 0.5, 0.6], q=0.10) == set()
    # Empty input -> empty set.
    assert a.benjamini_hochberg([], q=0.10) == set()
    # Step-up property: the largest passing rank rescues lower-ranked indices.
    # p=[.04,.01], m=2, q=.05: .01<=.025 (rank1) and .04<=.05 (rank2) -> both sig.
    assert a.benjamini_hochberg([0.04, 0.01], q=0.05) == {0, 1}


def test_bootstrap_ci_mean():
    # Constant sample -> every resample has the same mean -> degenerate CI.
    # (lo == hi exactly; value ~0.1 modulo float-sum accumulation.)
    lo, hi = a.bootstrap_ci_mean([0.1] * 30)
    assert lo == hi and abs(lo - 0.1) < 1e-9, (lo, hi)
    # Empty -> (nan, nan).
    lo, hi = a.bootstrap_ci_mean([])
    assert lo != lo and hi != hi  # nan != nan
    # Clearly-positive spread: CI lower bound > 0 (deterministic seed).
    import random
    random.seed(0)
    lo, hi = a.bootstrap_ci_mean([0.2, 0.25, 0.3, 0.22, 0.28, 0.26, 0.24] * 5, B=2000)
    assert lo > 0.0 and hi > lo, (lo, hi)


def test_wilcoxon_signed_rank_p():
    # No differences -> not significant (p == 1.0, n_nonzero == 0).
    p, n, _ = a.wilcoxon_signed_rank_p([0.0] * 10)
    assert p == 1.0 and n == 0
    # Consistent positive shift -> significant (small p).
    p, n, _ = a.wilcoxon_signed_rank_p(
        [0.1, 0.2, 0.15, 0.3, 0.25, 0.18, 0.22, 0.27, 0.12, 0.2]
    )
    assert n == 10 and p < 0.05, (p, n)
    # Two-sided: flipping every sign yields the same p (W = min(w+, w-)).
    deltas = [0.1, -0.05, 0.2, 0.15, -0.03, 0.08, 0.12, 0.3]
    p_pos, _, _ = a.wilcoxon_signed_rank_p(deltas)
    p_neg, _, _ = a.wilcoxon_signed_rank_p([-d for d in deltas])
    assert abs(p_pos - p_neg) < 1e-12


def main():
    tests = [v for k, v in sorted(globals().items()) if k.startswith("test_")]
    failed = 0
    for t in tests:
        try:
            t()
            print(f"PASS {t.__name__}")
        except AssertionError as e:
            failed += 1
            print(f"FAIL {t.__name__}: {e}")
    if failed:
        print(f"\n{failed}/{len(tests)} FAILED")
        raise SystemExit(1)
    print(f"\nall {len(tests)} passed")


if __name__ == "__main__":
    main()
