# Evidence-Driven Lint Repair Campaign Design

## Decision

Run a bounded evidence-driven repair campaign before designing a general
remediation platform. The campaign starts from Wenlan's real stored state and
the code paths that produce or mutate that state. Every product change must be
preceded by a deterministic reproducer and followed by lint and state-integrity
verification.

`wenlan lint` remains read-only. This phase does not add a repair API, automatic
repair, review-card emission, or a new lint product surface.

## Planning Layout

Store planning at two levels:

- The current phase has one design spec and one executable implementation plan
  under `docs/superpowers/specs/` and `docs/superpowers/plans/`.
- A multi-phase roadmap, when needed, is only an index of phase order, status,
  entry criteria, and links. It does not duplicate task details from phase
  plans.

No separate roadmap file is needed yet. A remediation platform remains a
deferred follow-up and receives its own spec only after this campaign produces
stable repair classes and demonstrates repeated user-facing repair needs.

## Goals

1. Reproduce and repair code paths that can create, preserve, or hide invalid
   memory, entity, graph, Page, source, queue, or projection state.
2. Use the live store to discover which theoretical defects have real exposure
   and which historical defects left residue.
3. Add read-only lint checks when a stable, bounded detector can prevent a
   repaired invariant from silently regressing.
4. Produce a redacted evidence ledger that distinguishes product defects,
   historical residue, semantic review work, expected telemetry, and
   environment/configuration issues.
5. Finish with an explicit cleanup proposal, not an automatic cleanup.

## Non-Goals

- No generic `explain -> plan -> approve -> apply` remediation platform.
- No `--fix` mode, repair agent, repair skill, or mutating lint endpoint.
- No direct SQL mutation of the live store.
- No automatic deletion, merge, relink, reclassification, supersession, Page
  rewrite, or review-card creation.
- No attempt to make every advisory or inventory count zero.
- No broad refactor of ingest, refinery, Page, or database modules.

## Evidence Model

Each investigated issue has one ledger entry with these fields:

```text
issue_id
scenario
observed_live_exposure
code_evidence
invariant
reproducer
root_cause
repair
lint_coverage
cleanup_class
verification
follow_up_direction
status
```

Allowed statuses are `candidate`, `reproduced`, `fixed`, `not_reproduced`,
`expected_state`, `semantic_review`, and `deferred`. A code smell or plausible
race remains `candidate` until a deterministic test or data invariant proves
it.

The committed ledger contains counts, reason codes, hashes, opaque identifiers,
and source locations only. It must not contain memory bodies, Page prose, local
paths from user content, URLs, credentials, or raw database rows.

## Artifact Layout

- Design: this file.
- Executable plan:
  `docs/superpowers/plans/2026-07-13-evidence-driven-lint-repair.md`.
- Redacted campaign ledger:
  `docs/superpowers/lint-maintenance/2026-07-13-evidence-ledger.md`.
- Raw database snapshots, SQL output, and regenerable probe artifacts:
  `$REPO_DATA_ROOT/wenlan/lint-maintenance/<commit-sha>/<run-id>/`, with
  `REPO_DATA_ROOT` defaulting to `~/.local/share/repo-data`.

Each external run directory has a `manifest.json` containing the code commit,
dirty-tree flag, run id, source database fingerprint, snapshot method, probe
version, timestamps, and artifact hashes. Raw snapshots remain outside every
worktree, use owner-only permissions, and are never committed.

## Investigation Matrix

The campaign follows user-visible state transitions rather than only current
lint groups.

### Memory Write and Enrichment

- initial store/upsert, chunk replacement, rollback, and connection reuse;
- concurrent identical captures and batch-local deduplication;
- classify, structured extraction, tags, and enrichment status transitions;
- entity extraction/linking and stale asynchronous work;
- Page growth after entity linking;
- dual-pool deduplication, contradiction, supersession, and temporal evolution;
- cross-space isolation for Page and memory mutations.

### Entity and Graph

- supplied entity identifiers and missing owners;
- entity create, alias, link, observation, and relation commit atomicity;
- entity merge/delete reference transfer;
- self-relations, duplicate relations, wrong-space links, and dangling links;
- legacy `memories.entity_id` and canonical `memory_entities` agreement.

### Pages and Projections

- Page create, refresh, revision acceptance, archive, merge, and source cleanup;
- `page_sources`, typed `page_evidence`, legacy `source_memory_ids`, canonical
  Markdown, state, and manifest agreement;
- workspace-aware wikilink resolution and duplicate-title ambiguity;
- archived Page behavior in the file watcher;
- concurrent projection writes and version/CAS behavior;
- source Page replacement failure and provenance retention.

### Update, Delete, and Rebinding

- atomic multi-field memory updates and validation order;
- multi-chunk content replacement and child-vector rebuilding;
- ordinary delete, time-range delete, episode ownership, and logical children;
- source-id rebinding across every table keyed by the logical memory id;
- telemetry and audit retention, which must not be classified as orphan data
  without an explicit retention contract.

### Queues, Refinery, and Retry

- checkpoint ordering relative to persisted summaries;
- terminal queue state relative to source sync receipts;
- retry convergence and idempotency after partial failure;
- Page revision/proposal consumption atomicity;
- keep/archive and proposal resolution consistency;
- refinery actions that cross DB and projection boundaries.

## Initial Priorities

The following are investigation priorities, not pre-approved fixes.

### Priority A: Reproduce First

1. Page source cleanup must preserve a valid source referenced by either the
   logical source id or the internal memory row id.
2. Entity merge/delete must not lose `memory_entities` links or leave dangling
   Page entity references.
3. Failed document upsert must roll back and leave the shared connection
   reusable.
4. Multi-field and multi-chunk memory update must be atomic and must not retain
   stale chunks.
5. Cross-space enrichment and wikilink resolution must not mutate or bind to an
   unrelated scope.

### Priority B: Continue Discovery

- KG and dual-pool partial commit behavior;
- Page revision, archive, watcher, and proposal transaction boundaries;
- ordinary/time-range delete coverage, including episodes and logical children;
- enrichment and source-sync checkpoint ordering;
- source Page delete-then-create failure;
- concurrent projection writes and concurrent identical captures;
- legacy Page provenance drift and resolver behavior after target deletion.

Priority B issues move into repair work only after deterministic reproduction
and impact classification. This keeps the campaign from becoming a speculative
rewrite.

## Campaign Flow

### Wave 0: Baseline and Ledger

1. Capture the database through its supported backup/snapshot mechanism and
   capture the Page projection tree with before/after receipts. Because the two
   stores are not observed atomically, any receipt drift marks the run
   inconsistent and disqualifies it from repair conclusions.
2. Record before fingerprints and the canonical General and Deep report
   receipts. Deep without an available judge remains incomplete, not clean.
3. Run redacted aggregate probes across the investigation matrix.
4. Seed the ledger with current lint findings, live residue, and code-derived
   candidates without treating them as equivalent severities.

### Wave 1: Priority A RED Tests and Repairs

For each issue, add one focused failing test, confirm the failure reason, make
the smallest production repair through the existing ownership boundary, and
rerun the focused test. Do not combine unrelated defects in one patch merely
because they share `db.rs`.

### Wave 2: Priority B Discovery

Use fault injection, barriers, and small synthetic fixtures to prove or reject
the remaining candidates. Promote only reproduced issues into repair tasks.
Record rejected hypotheses because they prevent future agents from repeating
the same speculative investigation.

### Wave 3: Lint Coverage

Add a lint check only when all of the following hold:

- the invariant is product-owned rather than semantic preference;
- the detector can enumerate its authorized population deterministically;
- a finding has a clear severity and remediation class;
- incomplete observation cannot be mistaken for clean;
- evidence remains bounded and redacted;
- the check stays read-only and shares canonical snapshot infrastructure.

### Wave 4: Live Verification and Cleanup Proposal

1. Run the repaired binary against a fresh database copy and Page projection
   copy whose capture receipts remained stable first.
2. Re-run General and applicable Deep diagnostics and compare receipts.
3. Verify database and Page projection non-mutation for lint itself.
4. Produce a cleanup manifest grouped into:
   `deterministic_safe`, `needs_semantic_review`, `historical_telemetry`,
   `environment_or_config`, and `do_not_touch`.
5. Stop before live mutation. Applying the manifest requires a separate user
   approval and a rollback artifact.

## Error Handling and Safety

- A failed or unstable snapshot invalidates that run's conclusions.
- A test that fails for an unrelated reason does not count as a reproducer.
- Fault-injection tests must prove both rollback and subsequent connection or
  queue reuse.
- Cleanup candidates must use durable owner identifiers and expected versions,
  never lint report ordinals.
- Semantic ambiguity is surfaced as unresolved; it is not converted into a
  deterministic repair for convenience.
- Live daemon replacement, restart, migration, or data mutation is outside the
  campaign's standing approval and must be surfaced before execution.

## Verification Gates

For each repaired issue:

1. Focused RED test observed before the fix.
2. Focused test passes after the fix.
3. Adjacent module tests pass.
4. `cargo fmt --all -- --check` passes.
5. `cargo clippy` passes for every changed crate and target required by the
   repository gate.
6. Workspace library tests pass before PR publication.
7. Relevant lint check or aggregate probe detects the pre-fix fixture and is
   clean on the repaired fixture.
8. Before/after fingerprints prove lint made no database or Page projection
   mutation.

The campaign is complete when every Priority A issue is fixed or explicitly
rejected by evidence, every Priority B candidate has a recorded disposition,
the redacted ledger is current, and the cleanup proposal has been produced.
Live data does not need to be mutated for the campaign itself to be complete.

## Deferred Remediation Platform Trigger

A separate remediation-platform design becomes justified only when this
campaign demonstrates at least two recurring repair classes that:

1. are useful to ordinary users;
2. cannot be safely handled by an idempotent migration or existing canonical
   writer;
3. need the same explain, approval, CAS, audit, and verify lifecycle; and
4. have stable typed targets that survive lint reruns.

Until then, preserve the requirement but do not build the platform.
