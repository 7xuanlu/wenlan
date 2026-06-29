# The L1-L8 Test-Responsibility Model

A reusable framework for deciding where every check in your project runs.

Most repos have an implicit, accidental answer to "where does this test go?" Some checks block commits, some block pushes, some run in CI, some only ever run on one person's laptop, and nobody wrote down why. The result is friction in the wrong places (a slow coverage gate on every push) and gaps in the right ones (a quality suite that never actually runs). This model makes the placement deliberate.

It is generalized from a working Rust project, but nothing here is language-specific. The layers are about *cost, speed, and gate-vs-measure*, not about any toolchain.

---

## The decision: three questions

Before you place any check, answer three questions. They determine the layer.

1. **Can a hosted runner do this?** No special hardware (GPU, device), no secrets that cannot be exposed to a PR from a fork, no per-run dollar cost. If yes, it can live in CI. If no, it is a manual or pre-release layer.
2. **Is it under ~60 seconds on a cold cache?** Fast checks can gate the inner loop (commit, push). Slow checks cannot, or they tax every commit and people start using `--no-verify`.
3. **Does it gate correctness, or only measure quality?** Correctness gates block (lint, type errors, failing unit tests). Quality measures (coverage percent, benchmark scores, latency) inform but **never block**, because percentage gates rot: any new untestable surface drops the number and forces busywork unrelated to the change.

The third question is the one most teams get wrong. They turn a quality measure into a gate (a 90% coverage requirement on push) and then spend their lives feeding the gate instead of shipping.

---

## The eight layers

| Layer | What runs | Where | When | Time | Blocks? |
|---|---|---|---|---|---|
| **L1** dev loop | IDE / language server, inline diagnostics | Local | Every keystroke/save | <1s | No |
| **L2** pre-commit | Auto-format + lint on the staged units | Local | `git commit` | ~5s | Yes |
| **L3** pre-push | Full lint + the fast (unit/library) test tier | Local | `git push` | ~60-90s | Yes |
| **L4** CI on PR | The same checks repo-wide + fast tests | CI | Every PR | minutes | Yes (required) |
| **L5** coverage on PR | Coverage report | CI | Every PR | minutes | **No (informational)** |
| **L6** main canary | Post-merge quality/perf checks | CI | Push to main | minutes | No (post-merge) |
| **L7** manual local | Slow suites, special-hardware runs, paid-judge runs | Your machine | On demand | minutes-hours | No |
| **L8** pre-release | Full quality suite vs saved baseline, env-stamped snapshot | Your machine | Per release | hours | Soft gate |

### Layer by layer

**L1 dev loop.** The language server. Costs nothing, runs constantly, catches typos and type errors before you even save. Not a gate, just feedback.

**L2 pre-commit.** Auto-format the staged files (and re-stage them, so formatting can never reach CI) plus a lint scoped to the units you touched. Must be fast (~5s). This is a gate: a commit with lint errors should not exist. Keep it targeted, not repo-wide, so it stays cheap.

**L3 pre-push.** The full linter plus the fast test tier (unit/library tests, not integration). Target under 90 seconds. Skip it entirely on docs-only pushes, because those checks exist to catch broken code and a docs change cannot break code. This is the last local gate before your work is visible.

**L4 CI on PR.** Repeat L2 and L3 repo-wide on a clean runner, plus the fast test suites for every unit. This is the required gate that protects the main branch. It must be reproducible by anyone, which is why it cannot depend on your laptop's special hardware.

**L5 coverage on PR.** A coverage report, posted as information. **Not a gate.** Earlier versions of this model enforced a coverage percentage on push; it was slow (the instrumented rebuild took many minutes and overloaded memory), not mirrored in CI (so it added local friction without upstream protection), and prone to rot (new untestable surface drops the percentage and forces busywork). Coverage informs; it does not block.

**L6 main canary.** Quality and performance checks that run *after* merge, on the main branch. They cannot block the merge (they run after it) but they catch post-merge regressions early. Good home for the cheap end of your quality suite.

**L7 manual local.** Everything a hosted runner cannot do: GPU/device evals, runs that need a paid API judge, the full slow integration suite. These are `#[ignore]`d or otherwise gated off the automatic lanes so they do not run by accident. You run them on demand.

**L8 pre-release.** The full quality suite against a saved baseline, once per release. A soft gate: a regression here makes you stop and look, but the numbers are bound by the metric-citation discipline (see `eval-citation-discipline.md`). You commit a curated, env-stamped snapshot of headline numbers, not the per-run history.

---

## Why the split is shaped this way

Two principles drive every placement:

- **Friction belongs where it is cheap.** Fast, deterministic, hosted-runner-capable checks gate early and often (L2-L4). Slow or special checks move outward (L7-L8) so they never tax the inner loop.
- **Gates protect correctness; measures inform quality.** A gate that blocks on a quality measure (coverage, score) inverts this and produces busywork. Keep L5 informational. Keep L8 a soft gate you can read and judge, not a hard number that fails the build.

The corollary: do not add a gate without checking it against the three questions. If it is slow, it does not belong on push. If it measures quality, it does not block. If a hosted runner cannot run it, it is not CI, it is manual.

---

## Adapting this to your repo

You will not use all eight layers, and that is fine. A small library might be L1-L4 only. A project with a benchmark suite needs L7-L8. Fill in the concrete command for each layer that applies, delete the rest, and write down the *why* for any check you exclude from CI, so nobody later assumes a coverage you do not have.

---

*Generalized from the "Local vs CI test responsibilities" section of a working project's AGENTS.md (the Origin project), where the eight layers were the actual test-placement policy across a daemon, a CLI, and a benchmark harness.*
