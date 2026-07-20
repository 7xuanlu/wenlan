# Ambient RB-01 target-Mac evidence

Date: 2026-07-19
Branch: `feature/ambient-enrichment-scheduler`
Status: IN PROGRESS — RB-01 remains open

## Evidence boundary

- The live Wenlan database was opened read-only for aggregate counts only. No content was selected, copied, or migrated.
- The user's config has no everyday/synthesis pin and no on-device model choice. The profiler therefore does not start the daemon or alter config. A manual ignored test uses the already-cached registry default `qwen3-4b`, synthetic content, and a temp database only.
- Deterministic slice/convergence tests and real-model thermal measurements are reported separately. A fast mock is not thermal proof; one real inference is not backlog-convergence proof.
- The profile refuses to start unless the model is already cached, macOS thermal state is nominal, free memory is at least 15%, and two consecutive 30-second aggregate CPU samples are at or below 20%. The Rust runner additionally enforces the scheduler's 2 GiB memory floor.
- Real profiles take an exclusive atomic lock before the first admission window
  and retain it through build, the second admission window, and the measured
  child. A failed child cannot replace the pre-armed 5,700-second cooldown with
  a shorter value.
- The shipped upgrade source is `origin/main` schema 77; the read-only user
  database observed here is schema 75. Schemas 78–83 exist only on this
  unshipped feature lineage. Its schema-82 origin rows cannot be reconstructed
  into service classes: that code wrote both ordinary interactive stores and
  raw imports into the same table, and an all-false origin is valid for either.
  Migration 83 therefore does not invent a bulk/interactive backfill. A
  production schema-77 upgrade creates an empty origin table with
  `service_class` already present, then classifies every later write at its
  known entry point.

## Current live aggregate

Read-only query against the current schema-75 database on 2026-07-19. The
database has neither `enrichment_origin` nor `enrichment_steps.input_version`;
the query did not run migrations.

| Metric | Count |
|---|---:|
| Memory rows, including chunks | 5,582 |
| Memory heads (`chunk_index=0`) | 2,877 |
| Active Pages | 133 |
| Pending/in-progress/paused document queue rows | 0 |
| Non-pending memory heads with no primary entity | 1,283 |
| Pending-revision memory heads | 0 |
| Legacy enrichment receipts | 9,536 |
| Active Pages missing citation processing | 1 |
| Missing-citation Pages with source evidence | 0 |
| Classification-eligible legacy heads | 2,441 |
| Page-growth-eligible legacy heads | 2,395 |
| Title-enrichment candidates | 1,850 |
| Entity candidates | 2,785 |
| Entity candidates with an authoritative link | 1,521 |
| Entity candidates without an authoritative link | 1,264 |

Legacy entity-related receipt counts:

| Step/status | Count |
|---|---:|
| `entity_extract / ok` | 71 |
| `entity_extract / skipped` | 1,466 |
| `entity_extract / failed` | 1 |
| `entity_link / ok` | 768 |
| `entity_link / skipped` | 770 |

These are upper bounds, not 1:1 inference counts. A missing primary entity can auto-link or terminate empty without inference, and new fixed-stage eligibility also applies origin, pending, version, and receipt filters.

## Backlog convergence risk

The current live schema predates the fixed-lane migration. Exact post-migration
selector predicates, applied read-only to the current data, yield 2,441
classification-eligible legacy heads, 2,395 Page-growth-eligible heads, 1,850
title candidates, and 2,785 entity candidates. Of the entity candidates, 1,521 already have an
authoritative entity link and can repair their receipt without inference;
1,264 have no authoritative link and may still auto-link before inference.
There are also 94 accepted revisions inside the classification count.

Classification always spends one provider call for a selected row. At the
ten-minute minimum thermal cooldown, its 2,441-row one-pass envelope is 16.95
days. Title generation adds up to 12.85 days, entity extraction up to 8.78 days
before auto-link savings, and the 2,395 selected Page-growth turns add another
16.63 days because production currently charges the thermal turn even for a
no-match result with zero LLM calls. Together, 7,950 successful one-pass
thermal turns imply a 55.21-day floor for that upper-envelope catch-up; retries
or a request longer than 31.6 seconds extend it. Zero-call structured-field
receipts and the one citation Page without evidence do not add thermal turns.

The earlier 40.2-day classification estimate was wrong because it counted
every chunk as a separate scheduler candidate. Fixed memory lanes select only
`chunk_index=0`; this document now keeps row, head, and inference envelopes
separate.

The existing oldest-first selectors would put a newly stored memory behind that
legacy catch-up. That violates the UX target even if the thermal duty cycle is
safe: background work could remain invisible while fresh data stays
unenriched for weeks.

The release-candidate convergence contract is therefore:

1. Every fixed memory lane uses explicit FIFO service classes:
   accepted revision, interactive/current work, raw bulk import, then legacy
   catch-up. It never relies on NULL collation.
2. A large raw import cannot enter the interactive class, and FIFO ordering
   avoids newest-first starvation inside a class.
3. A post-migration user mutation promotes a bulk or legacy memory into the
   interactive FIFO. For a missing legacy origin, it materializes the existing
   all-protected fallback without guessing which fields were originally
   explicit; an existing import retains its stored provenance flags.
4. Accepted revisions retain their existing highest priority.
5. Bulk rows and then legacy rows remain selectable after current interactive
   work advances; the change
   must not hide or discard catch-up.
6. Each slice retains its existing one-durable-item and at-most-one-provider-call
   bound. This is prioritization, not semantic batching.

A cross-lane RED regression uses distinct `ACCEPTED_REVISION`,
`FRESH_AUTOMATIC`, `KNOWN_BULK_IMPORT`, and `LEGACY_BACKFILL` markers across
classification, structured extraction, title, entity, and Page growth. The
service-class group passes 4/4; migrations 80–83 pass 4/4; both collision-lineage
migrations pass 2/2. The four existing deterministic slice bounds each pass
1/1 for Document, Entity, Reconcile, and Citation.

## Source-grounded controller facts

- Admission is checked before a scheduler turn. It requires two 30-second healthy samples, aggregate CPU at or below 20%, and available memory at or above both 2 GiB and 15%.
- One ambient turn selects at most one durable item and `AmbientBudgetProvider` forwards at most one LLM request.
- A selected thermal turn receives `max(10 minutes, 19 × elapsed)` recovery.
- A selected Page-growth turn currently receives that recovery even when it
  finds no matching Page and forwards zero LLM calls. This remains provisional
  until the added no-match profile measures its actual CPU/thermal cost.
- An already-started on-device request is not preempted when foreground activity begins. The pinned `llama.cpp` C API contains an abort callback, but `llama-cpp-2` 0.1.143 does not expose a safe context-level cancellation method.
- The existing continuous-batch engine only batches requests that are submitted concurrently. The serialized ambient controller currently submits one request, so its one-request invariant is not secretly a multi-item physical batch.

## RED/GREEN profiler gate

- RED: `rtk cargo test -j 2 -p wenlan-server rb01_profile_admission --lib -- --nocapture`
  failed at compile time with 13 expected missing-symbol errors for the profile admission type, block reasons, and explicit opt-in gate.
- GREEN: the same command passed the two admission/opt-in tests.
- Current head, after adding the gated real-model fixture and Page-growth lane,
  passes `3 passed, 1 ignored, 282 filtered out` for `rb01_profile_`.
- `rtk bash -n scripts/profile-ambient-rb01.sh` passed.
- RED: `rtk bash scripts/test-profile-ambient-rb01.sh` failed with
  `expected exactly two real-profile preflight calls, found 1`; the script
  would compile before checking whether the Mac was already busy.
- GREEN: the same shell regression passed after the real path became
  `preflight -> bounded -j 2 build -> preflight -> inference`. The second
  admission window ensures build heat cannot flow directly into model load.
- RED: the shell regression then failed with
  `expected preflight to enforce model working-set headroom exactly once`.
  On this 16 GiB Mac, the cached GGUF is 2,497,281,120 bytes, so a bare 15%
  pre-load check could pass and immediately enter compression after model load.
- GREEN: preflight now also requires estimated free bytes to cover the cached
  model file plus `max(2 GiB, 15% total memory)`. The static safety regression
  and syntax check both pass.
- RED: the shell regression then failed with
  `expected cooldown to use 19x measured job time with a timeout-sized fail-safe`.
  The script had always written 600 seconds even though production uses
  `max(600 seconds, 19 × job elapsed)`.
- GREEN: before the child can load a model, the script now arms a conservative
  `19 × 300-second timeout` recovery window. A complete JSON report replaces it
  with `max(600 seconds, 19 × report_elapsed_ms)`; an interrupt or missing
  report leaves the fail-safe in place. The manifest records which source won.
- RED: the static harness rejected manifests that identified only `git_head`
  while the measured source tree could still be dirty.
- GREEN: every real-profile manifest now binds the exact scheduler, DB selector,
  and profiler script bytes with SHA-256 values.
- RED: both the Rust lane parser and shell surface rejected `page-growth`.
- GREEN: `page-growth` now profiles one synthetic memory with no Pages. It must
  create a durable `page_growth` receipt with zero provider calls, and
  measures the CPU-only embedding/search path that production currently charges
  as a full thermal turn.
- RED after Opus review: the generic batch-import path used the atomic origin
  writer but fresh rows entered service class 0. The review's explanation that
  every folder row was selector-excluded was source-inaccurate; the production
  caller is `importer.rs`, not directory sync.
- GREEN: fresh generic imports now enter class 1. Conflict updates preserve the
  existing class, so re-import cannot demote a user-promoted class-0 memory.
- RED after the first admitted run: `raw.log` contained only RTK's one-line
  Cargo summary, so the JSON measurement and measured cooldown could not be
  recovered. The run remained successful partial evidence, but not a complete
  profile row.
- GREEN: Cargo now runs only the bounded `--no-run` build with JSON metadata.
  The script extracts the exact `wenlan_server` test executable, hashes it, and
  invokes that binary directly through `rtk proxy`, so `--nocapture` JSON is not
  filtered by Cargo/RTK. The failed first parse correctly left the already-armed
  5,700-second fail-safe cooldown intact.
- RED: the profiler promised peak/current RSS but emitted only point-in-time
  baseline/model-loaded/before/after values.
- GREEN: a 50 ms test-only sampler now refreshes only the current PID through
  pinned `sysinfo` 0.33.1 and emits `rss_peak_during_slice_bytes`; it does not
  call the system-wide `refresh_all()` loop while the slice runs.
- RED after independent review: two concurrent scripts could both pass the
  cooldown check before either armed it.
- GREEN: an atomic `/private/tmp/wenlan-rb01-profile.lock` is acquired before
  the first preflight and held until the child and manifest are finalized.
- RED after independent review: a nonzero child could print a JSON row before
  its assertions failed, allowing the shell to shorten the fail-safe from that
  invalid report.
- GREEN: `cooldown_from_report` is reachable only when the exact child exits
  zero. Nonzero, interrupted, timed-out, or unparsable runs retain the
  timeout-sized recovery window.
- RED after independent review: source hashes did not identify the actual test
  executable, dependency lock, or cached model blob.
- GREEN: the manifest now adds SHA-256 for `Cargo.lock`, the exact test binary,
  the tracked worktree diff, and the scheduler/DB/profiler sources, plus the
  resolved content-addressed Hugging Face model blob hash. A bounded no-run
  build proved Cargo selects one `wenlan_server` test executable, and invoking
  that exact binary passed 3 profiler harness tests with the real-model test
  ignored.
- RED on the current Mac: the hardened script used `/usr/bin/jq`, but Homebrew
  provides `jq` at `/opt/homebrew/bin/jq`.
- GREEN: `jq` is resolved from `PATH` and absence fails before compilation. The
  shell syntax/safety regression passes with the installed path.
- RED after final Opus review:
  `drift_guard::behavioral_flags_are_documented` failed on
  `WENLAN_RB01_PROFILE`, `WENLAN_RB01_LANE`, and the existing
  `WENLAN_TEST_STARTUP_SIGNAL_BARRIER`. The guard scans source text and does not
  exempt `cfg(test)` reads.
- GREEN: all three process-only test controls are explicitly documented in
  `FLAG_ALLOWLIST` with test-only reasons. The same fail-closed drift-guard test
  passes. Opus reported no other blocker in the service-order, one-call,
  lock/cooldown, SQL, or profiler provenance changes.

## Target-Mac preflight refusals

`rtk scripts/profile-ambient-rb01.sh --preflight-only` correctly refused to start
twice:

```text
refusing profile: aggregate CPU sample 1/2 was 36.79% (>20%)
refusing profile: aggregate CPU sample 1/2 was 41.42% (>20%)
refusing profile: aggregate CPU sample 1/2 was 41.5% (>20%)
refusing profile: aggregate CPU sample 1/2 was 43.68% (>20%)
refusing profile: aggregate CPU sample 1/2 was 23.4% (>20%)
refusing profile: aggregate CPU sample 1/2 was 23.3% (>20%)
refusing profile: aggregate CPU sample 1/2 was 33.3% (>20%)
refusing profile: aggregate CPU sample 1/2 was 67.26% (>20%)
refusing profile: aggregate CPU sample 1/2 was 33.02% (>20%)
refusing profile: aggregate CPU sample 2/2 was 54.29% (>20%)
```

No model was loaded and no inference ran. These are positive-control results
for the foreground-protection gate, not thermal measurements. A later
metadata-only snapshot also found unrelated `rustc` processes consuming
approximately 78–87% CPU each, so neither the focused RED test nor the real
profile was layered on top. A bounded five-attempt waiter subsequently
reacquired the complete two-sample gate on every attempt and stopped with exit
75 after all five refusals; it never reached its single focused Cargo command.

## Measurements pending

| Lane | Real-model wall time | LLM calls | RSS before/after | CPU before/after | Thermal before/after | Durable progress |
|---|---:|---:|---:|---:|---:|---|
| Document | pending | pending | pending | pending | pending | pending |
| Entity | pending | pending | pending | pending | pending | pending |
| Page Growth (no match) | not captured; test process 72.85 s | 0 (asserted) | not captured | not captured | 0 → 0 | yes (asserted) |
| Reconcile | pending | pending | pending | pending | pending | pending |
| Citation | pending | pending | pending | pending | pending | pending |

The partial Page-growth run is bound to
`/private/tmp/wenlan-rb01-profile-20260720T021512Z-page-growth`; its manifest
records the source hashes used by that run, exit status 0, thermal 0
before/after, and memory-free percentage 83 before/after. It predates the exact
binary/Cargo/model provenance fields above and must not be relabeled with them
after the fact. The test assertions prove selection, durable progress, zero
provider calls, no panic, and nominal end thermal state. They do not recover the
missing wall/RSS/CPU JSON, so the row must be rerun after the 5,700-second
fail-safe cooldown and fresh admission.

RB-01 cannot close until the target-Mac rows above and representative convergence arithmetic are complete. A busy-machine refusal or fail-safe cooldown must never be bypassed merely to finish the table.
