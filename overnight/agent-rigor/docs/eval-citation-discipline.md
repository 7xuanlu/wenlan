# Eval Citation Discipline

A standalone guide to honest metric reporting for agent and model evaluation.

Eval-driven development became a named discipline in early 2026. The labs publish on it, and "quality" is the most-cited blocker to shipping agents. But naming a discipline does not make people honest with their numbers. The category is full of single cherry-picked figures, cross-version comparisons dressed up as progress, and headline accuracy claims with no per-case backing. This guide is the rule set one real project used to keep itself honest. It is provider-agnostic and benchmark-agnostic; it applies to any number you produce and intend to show someone.

The core idea: **a metric is a claim, and a claim carries provenance obligations.** The moment a number leaves your laptop (into a README, a PR, a blog post, a deck, an issue, a tweet), it must be able to answer: how many runs, under what version, at which test layer, with what spread, reproducible how. The six rules below are the obligations.

---

## 1. Single-run ban

**A metric from a single run MUST NOT be cited externally.**

One run is a scaffold, not a result. Benchmark scores move run-to-run from sampling, nondeterminism, and cache state. A single number tells you nothing about whether a difference is real or noise.

- Internal use of single-run numbers is fine, but flag them: "single-run, treat as scaffold."
- External or headline claims require **N >= 3 runs, reported as mean +/- stddev.** Ten is better than three.
- Tag single-run artifacts in metadata (e.g. an `is_single_run` flag) so tooling can refuse to cite them and humans cannot forget.

If you only have one run and a deadline, the honest move is to report the number with the scaffold flag attached, not to drop the flag.

---

## 2. Schema / version refusal

**Do not compare numbers produced under different fixture, schema, or config versions.**

A score is only meaningful relative to the exact setup that produced it: the fixture revision, the embedder/model weights, the provider class, the scoring config. Change any of them and the two numbers are measuring different things. Subtracting them produces a difference that looks like progress and is actually an artifact.

- Make the comparison tool **refuse** cross-version diffs (a hard error, not a warning). A refusal you can override is a refusal you will override.
- Stamp every result with its version fields so the refusal can fire.
- A public claim that legitimately needs to span versions must **regenerate both sides** under the current setup. There is no shortcut.

This is the rule people break most, because version drift is invisible. The number from three weeks ago looks comparable. It is not.

---

## 3. Receipt-only

**No "improved X%" or "regressed Y%" without a comparison-tool receipt AND multi-run inputs behind it.**

A delta claim is two claims: the two endpoints, and the spread that says the gap is real. Both endpoints must be N >= 3. The gap must clear the combined noise.

- Regression thresholds, latency claims, and accuracy-improvement claims all need measured stddev or N >= 3 backing.
- The "receipt" is the tool output that compared two properly-stamped, multi-run baselines. If you cannot produce the receipt, you cannot make the delta claim.
- "It feels faster" and "looks like an improvement" are not receipts.

---

## 4. Per-case visibility

**Aggregate accuracy claims must ship with a per-case breakdown when one exists.**

A headline average hides regressions in individual cases. The canonical trap: one contaminated or adversarial category quietly inflates (or tanks) the mean while every other case moved the other way. The aggregate looks fine; the system got worse where it matters.

- Publish the per-case table alongside the headline number.
- Watch for one category dominating the average. If a single case moves the mean more than the rest combined, lead with that, do not bury it.

---

## 5. Layer attribution

**Public numbers must say which test layer produced them.**

A number from a fast smoke check and a number from the full quality suite are not the same kind of evidence, and averaging across them is meaningless. (See `test-layers.md` for the L1-L8 model.)

- Tag every public number with its layer: L1/L2/L3/L4/...
- No cross-layer averages without explicit, stated weighting. "Our accuracy is 92%" without a layer is unattributable and therefore uncitable.

---

## 6. Commit policy: snapshot, not history

**Metric values MAY be committed to git as a curated, environment-stamped snapshot. A per-run history series MAY NOT.**

The distinction:

- **Snapshot (commit it):** the current headline numbers, in a results doc or README section, overwritten each release. Each value carries its methodology inline: model, dataset, run count, repro command. Single-run values in the snapshot are tagged "scaffold" and are still bound by rules 1 and 3 for any external claim.
- **History series (do not commit):** the per-run time series of every baseline you ever produced. That belongs in a gitignored append-only file or artifact store.
- **Raw per-run artifacts (do not commit):** the raw baseline outputs. They are reproduced by re-running, not stored as source. Gitignore them.

The reasoning: a snapshot is documentation (here is where we are, here is how to reproduce it). A history series committed to git turns the repo into a metrics database, bloats it, and invites cross-version comparison (rule 2 violation) every time someone diffs an old commit.

---

## Putting it together: a citable claim

A number is ready to show the world when all of these are true:

- It is the mean of N >= 3 runs, with stddev reported. *(rule 1)*
- Every number it is compared against was produced under the same version. *(rule 2)*
- Any delta has a comparison-tool receipt behind it. *(rule 3)*
- The aggregate ships with its per-case breakdown. *(rule 4)*
- It is tagged with the test layer that produced it. *(rule 5)*
- It is reproducible from the methodology committed beside it. *(rule 6)*

Miss one and the number is internal-only. That is not pedantry. It is the difference between a measurement and a marketing figure, and in a category drowning in marketing figures, the measurement is the rare thing.

---

*Extracted from the AGENTS.md of a working eval harness (the Origin project). Generalized so it applies to any eval workflow, not just that one.*
