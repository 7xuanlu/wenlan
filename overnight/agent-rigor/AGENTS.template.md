# AGENTS.md

This file guides any coding agent working in this repository: Claude Code, Cursor, Codex, GitHub Copilot, Zed, Aider, and similar. It is the canonical agent-instruction file. Vendor-specific files (such as `CLAUDE.md`) re-import from here so the rules stay in sync. The format follows the [agents.md](https://agents.md/) spec.

> This template is a project-agnostic distillation of a working AGENTS.md from a real, shipped codebase (the Origin project, a Rust monorepo). Every rule below traces to a section of that file. The `[VERIFIED AGENTS.md section]` tags mark which one. Replace the bracketed `<PLACEHOLDERS>` with your project's specifics. Delete sections that do not apply. Do not add discipline you do not actually practice; an aspirational AGENTS.md the team ignores is worse than a short one it follows.

---

## Project Summary

`<ONE PARAGRAPH: what this repo is, what ships from it, where the public surface lives.>`

`<If a monorepo: list each package/module and its single responsibility in one line.>`

---

## Design Philosophy

These are the rules the agent applies to every change. They come first because they override convenience.

- **Simple and elegant over clever.** Prefer the straightforward solution. Reads-like-easy-to-write usually is right.
- **Use existing packages.** Check for a well-maintained library before custom implementation. Do not reinvent the wheel.
- **Minimize moving parts.** Fewer abstractions, layers, indirections. Complexity must justify itself.
- **Standard idioms first.** Follow ecosystem conventions. Surprising code is usually wrong code.
- **No speculative surface.** No features beyond what was asked, no abstractions for single-use code, no "flexibility" or "configurability" that was not requested, no error handling for impossible scenarios.
- **Surgical changes.** Touch only what the task requires. No "while I'm here" refactors, no adjacent-code cleanups, no formatting fixes outside the diff. Match existing style. Remove only imports/vars your own change made unused; pre-existing dead code needs an explicit ask.
- **Challenge assumptions.** Do not follow user framing uncritically. If multiple interpretations exist, present them rather than pick silently. Push back when the approach is wrong.
- **Verify before claiming done.** Run tests, check the build, confirm behavior. Evidence before assertions.

`[VERIFIED AGENTS.md section: "Design Philosophy"]` — these eight rules are lifted verbatim from the source file's Karpathy-derived philosophy block. They are project-agnostic and copy as-is.

---

## Build & Dev Commands

List the exact commands an agent should run. Be specific: an agent that has to guess the build command guesses wrong.

```bash
# Build the whole project
<BUILD COMMAND>

# Fast per-package build for iteration
<PER-PACKAGE BUILD COMMAND>

# Run the test suite
<TEST COMMAND>

# Run a single test
<SINGLE-TEST COMMAND>

# Set up git hooks (one-time)
<HOOK SETUP COMMAND, e.g. bash scripts/setup-hooks.sh>
```

State plainly what the hooks do, so the agent knows what will run before its commit/push lands:

> Pre-commit auto-formats and runs the linter on changed packages. Pre-push runs the full linter plus the fast test suite.

`[VERIFIED AGENTS.md section: "Build & Dev Commands" and "Git hooks (auto-activated)"]` — the source file lists exact commands and states what each hook does. Keep the structure, swap the commands.

---

## Test Layers (L1-L8)

Every check runs at exactly one layer. A check's layer is decided by three questions:

1. **Can a hosted CI runner do this?** (no special hardware, no secret keys, no per-run cost)
2. **Is it under ~60 seconds on a cold cache?**
3. **Does it gate correctness, or only measure quality?** Quality measures never gate.

| Layer | What runs | Where | When | Time | Blocks? |
|---|---|---|---|---|---|
| **L1 dev loop** | IDE / language server | Local | Every save | <1s | No |
| **L2 pre-commit** | Format + lint on changed files | Local | `git commit` | ~5s | Yes |
| **L3 pre-push** | Full lint + fast test suite | Local | `git push` | ~60-90s | Yes |
| **L4 CI on PR** | Full lint + full test suite | CI | Every PR | minutes | Yes (required) |
| **L5 coverage on PR** | Coverage report | CI | Every PR | minutes | **No (informational)** |
| **L6 main canary** | Post-merge slow/quality checks | CI | Push to main | minutes | No (post-merge) |
| **L7 manual local** | Expensive suites (hardware, paid APIs, long runs) | Laptop | On demand | minutes-hours | No |
| **L8 pre-release** | Full quality suite vs saved baseline; curated snapshot | Laptop | Per release | hours | Soft gate |

Rules that fall out of the model:

- **Quality measures never gate a merge.** A coverage percentage or benchmark score dropping is a signal, not a block. Percentage gates rot: every new untestable surface forces busywork without protecting anything.
- **A local gate that CI does not mirror is friction without protection.** If pre-push enforces something CI never checks, the gate only slows the author. Either CI mirrors it, or it moves to L5/L7.
- **State what does NOT run in CI and why.** Hardware-bound, paid-API, and desktop/platform-specific suites are explicitly excluded so nobody assumes CI covered them. Mark them `[ignore]`/skipped so they cannot run by accident.

`[VERIFIED AGENTS.md section: "Local vs CI test responsibilities", "What does NOT run in CI and why", "Why pre-push doesn't run coverage"]` — the three-question rule, the L1-L8 table, and the no-rotting-gates reasoning are lifted directly. Renumber or collapse layers your project does not have; keep the three questions, they are the reusable engine. Full framework in `docs/test-layers.md`.

---

## Eval / Metric Citation Discipline

Any number you report (in a README, a PR description, a blog post, a deck, an issue) carries provenance obligations. These rules keep the project honest about its own measurements.

- **Single-run ban.** A metric from a single run MUST NOT be cited externally. Internal references are fine but must be flagged "single-run, treat as scaffold." External or headline claims require N>=3 runs reported as mean +/- stddev (ideally 10).
- **Schema/version refusal.** Do not compare numbers produced under different fixture/schema/config versions. The comparison tool refuses cross-version diffs. Public claims that span versions must regenerate both sides under the current setup.
- **Receipt-only.** No "improved X%" or "regressed Y%" without a comparison-tool output AND multi-run inputs behind it. Regression thresholds and latency claims need measured stddev or N>=3 backing.
- **Per-case visibility.** Aggregate accuracy claims must ship with a per-case breakdown when one exists. Headline-only numbers hide regressions in individual cases.
- **Layer attribution.** Public numbers must say which test layer (L1/L2/L3/...) produced them. No cross-layer averages without explicit weighting.
- **Commit policy: snapshot, not history.** Metric values MAY be committed to git as a curated, environment-stamped snapshot (the current headline numbers), overwritten per release, each value carrying its methodology inline (model, dataset, run count, repro command). Do NOT commit a per-run history series, and keep raw per-run artifacts gitignored; they are reproduced by re-running, not stored as source.

`[VERIFIED AGENTS.md section: "Conventions > Eval Citation Discipline"]` — all six rules map one-to-one to the source file's bullets (Single-run, Schema-version, Receipt-only, Per-case visibility, Layer attribution, Commit policy). Standalone guide in `docs/eval-citation-discipline.md`.

---

## Module / Package Boundaries

State the dependency rules that keep packages from rotting into a ball of mud. The pattern: name a package, name the dependency it MUST NOT take, give a one-line command that proves the rule holds.

- **`<CORE PACKAGE>` must have NO `<FRAMEWORK>` dependency.** Verify: `grep -rn "use <framework>" <core path>` expects zero hits. Cross-boundary concerns go through a trait/interface, not a direct dependency.
- **`<SHARED TYPES PACKAGE>` stays lightweight.** Only `<allowed minimal deps>`. Adding heavy deps here forces them on every downstream consumer.
- **Do not add business logic to `<THIN LAYER, e.g. HTTP/CLI layer>`.** That layer does framing/transport only; it calls into the core with plain values.
- **`<BOUNDARY-CROSSING CODE>` deserializes into typed structs, never an untyped blob.** Typed deserialization fails loud on shape drift; untyped silently passes whatever it gets.

`[VERIFIED AGENTS.md section: "Conventions > Crate boundaries"]` — the "no-framework-in-core, verify by grep," "keep shared types light," "no logic in the thin layer," and "typed deserialization at boundaries" rules are generalized from the source file's crate-boundary bullets. Each rule keeps its proof command; that is what makes it enforceable rather than aspirational.

---

## Concurrency / Resource Rules

`<If your project has hot-path locking, connection, or async hazards, state them as do/don't rules with the reasoning.>`

Examples of the shape (from the source project; replace with yours):

- **Never hold a lock guard across an `await`/blocking call.** Snapshot what you need into a scoped block that ends before the slow call, then call with the cloned values.
- **`<X>` is the single writer.** Only one component touches the data store directly; everything else goes through its API.

`[VERIFIED AGENTS.md section: "Conventions > Async and locking"]` — generalized. Drop this whole section if your project has no shared-resource hazards; do not invent rules to fill it.

---

## Data / Safety Rules

`<Language- and store-specific footguns, stated as rules with a one-line rationale.>`

Examples of the shape:

- **Parameterize all queries.** Never interpolate user input into a query string.
- **Distinguish null from empty.** Store an absent value as NULL, not an empty string, so `IS NULL` filters work.
- **Encoding safety.** `<Your language's string/byte-index footgun and the safe idiom.>`

`[VERIFIED AGENTS.md section: "Conventions > SQL, strings, data"]` — generalized from the source file's SQL/string/data bullets. Keep only the ones your stack can actually trip on.

---

## Version-Control Hygiene

### Surgical commits

Restated from Design Philosophy because it is the rule most often broken under agent autonomy: one change per commit, no drive-by edits, no reformatting outside the diff.

`[VERIFIED AGENTS.md section: "Design Philosophy > Surgical changes"]`

### Branch and worktree cleanup after squash-merge

Squash-merge bundles a PR's commits into one new commit on the main branch with a fresh SHA. The original branch commits keep their old SHAs and look "unmerged" even though their content shipped. Three traps follow:

- **SHA-based "is it merged?" checks lie.** Tools that compare commit SHAs mark squashed commits as unmerged. Verify by content (read the squash commit body, or grep main's history for the files/keywords the branch added), not by SHA.
- **Stale worktrees/branches accumulate.** They are not auto-removed on merge. After confirming the content is on main, remove the worktree, force-delete the branch (force is needed because the SHA check thinks it is unmerged), and prune.
- **Per-checkout gitignored artifacts.** Files under gitignored paths live per-worktree. If a worktree is the only host of a large gitignored artifact, back it up to the shared location before deleting the worktree.

Run this hygiene pass on a schedule (roughly weekly, or whenever the worktree list gets long).

`[VERIFIED AGENTS.md section: "Worktree cleanup after squash-merge"]` — lifted and de-Rust-ified. The SHA-lies trap and the force-delete reason are the load-bearing insights; keep them.

---

## Releasing

`<Describe your release automation. The reusable discipline, regardless of tool:>`

- **State which commit-message prefixes bump which version component**, with examples. If a prefix has a surprising effect (e.g. a "feature" prefix bumps minor not patch), call it out in bold so nobody triggers it by accident.
- **Squash-merge commit messages matter.** The squash commit message often defaults to the PR title. Review PR titles before merging so the release automation reads the bump you intend.
- **List the version files that must stay in sync** and how they are kept in sync.

`[VERIFIED AGENTS.md section: "Releasing", "Release pipeline gotchas (learned the hard way)"]` — the reusable core is "prefixes control bumps, squash titles become commit messages, keep version files in sync." The specific tool is yours to fill in.

---

## Conventions Catch-All

`<Project-specific conventions that did not fit above: env vars, log levels, data directories, naming. Keep each to one line with its rationale.>`

`[VERIFIED AGENTS.md section: "Conventions > Misc"]`

---

## Subdirectory AGENTS.md

Per the agents.md hierarchical convention, an `AGENTS.md` in a subdirectory applies additively when an agent works under that subtree. Use this for area-specific rules (an eval harness, a generated-code dir) so the root file stays general.

`[VERIFIED AGENTS.md section: the source file references subdir AGENTS.md files that "apply per the agents.md hierarchical-instruction convention"]`

---

## Internal Consistency Checklist

Before committing changes to this file, confirm:

- [ ] Every rule states what to do AND why, or carries a one-line proof command.
- [ ] No rule contradicts another (e.g. a metric committed to git vs. the gitignore-raw-artifacts rule: snapshot is committed, per-run history is not).
- [ ] Every boundary rule has a verification command an agent can run.
- [ ] The test-layer table assigns each real check to exactly one layer.
- [ ] No aspirational rules: everything here is actually practiced and enforced.
