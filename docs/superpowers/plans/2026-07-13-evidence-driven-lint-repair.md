# Evidence-Driven Lint Repair Campaign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` to execute this plan task-by-task. Use `superpowers:test-driven-development` for every product defect and `superpowers:systematic-debugging` when a RED test fails for an unexpected reason.

**Goal:** Repair the five evidence-backed producer defects that can create or preserve invalid Wenlan state, discover the remaining high-risk lifecycle defects with deterministic reproducers, and leave a redacted live-store evidence ledger plus a non-mutating cleanup proposal.

**Architecture:** Keep `wenlan lint` as the one read-only diagnostic runner. Repair data producers at their existing core ownership boundaries, add lint coverage only for stable enumerable invariants, and use an external read-only probe to compare canonical lint receipts with DB and Page projection fingerprints. No repair API, `--fix`, review-card emission, or live-store mutation is introduced.

**Tech Stack:** Rust 2021, libSQL/SQLite, Tokio, Axum, Clap, Bash, `sqlite3`, `jq`, `shasum`, Cargo tests.

## Global Constraints

- Work only in `/Users/lucian/.codex/worktrees/072f/wenlan` on `codex/lint-evidence-repair-design`.
- Never mutate the live Wenlan database or Page tree during this campaign. Live observations are read-only snapshots and diagnostics.
- Never restart or replace the live daemon without a separate explicit approval.
- Keep raw snapshots and raw probe output outside every worktree under `${REPO_DATA_ROOT:-$HOME/.local/share/repo-data}/wenlan/lint-maintenance/<commit-sha>/<run-id>/` with mode `0700`.
- Commit only redacted counts, reason codes, hashes, opaque IDs, and code references. Never commit memory bodies, Page prose, user-content paths, URLs, credentials, or raw rows.
- Treat DB/Page receipt drift, unavailable Deep provider, timeout, truncation, malformed output, or failed candidate generation as incomplete. Never convert incomplete observation into clean.
- Do not add a lint catalog ID for a transaction property. Transaction rollback and connection reuse belong in deterministic product tests.
- Do not add `pages.db.integrity` unless Task 7 or the live probe proves a stable user-visible population not already covered by an existing check. If added, update the catalog version, profile counts, and catalog contract tests in the same commit.
- Run Cargo commands serially in this worktree.
- After every task: inspect `git diff`, run `git diff --check`, and commit only that task's files.

---

## Task 1: Build the Read-Only Evidence Harness and Ledger

**Files:**
- Create: `scripts/lint-maintenance-probe.sh`
- Create: `scripts/lint-maintenance-probe.sql`
- Create: `scripts/lint-maintenance-probe.test.sh`
- Create: `crates/wenlan-core/tests/lint_maintenance_probe_e2e.rs`
- Create: `docs/superpowers/lint-maintenance/2026-07-13-evidence-ledger.md`
- Reference: `scripts/cleanup-legacy-captures.sh`
- Reference: `crates/wenlan-types/src/lint_contract.rs`

**Step 1: Write the failing harness test**

Create `scripts/lint-maintenance-probe.test.sh` with temporary fixtures that assert:

1. `--help` documents a read-only probe and the external artifact root.
2. A tiny SQLite fixture produces only named aggregate counters, never inserted sentinel content.
3. The run directory and artifacts are owner-only.
4. A stable DB/Page fixture produces `complete: true` and matching before/after fingerprints.
5. A test hook that changes the Page fixture between receipts produces `complete: false` and `reason: inconsistent_snapshot`.
6. The source fixture database and Page tree hashes are unchanged after the run.
7. A symlinked output root, DB, or Page root is rejected before any artifact is created.
8. The Page archive extracted from the run hashes to the same root-relative
   receipt captured inside the stable observation envelope.
9. A libSQL-backed fixture with vector indexes compares `COUNT(*)` with row
   enumeration for `memories`, `entities`, and `pages`; any disagreement marks
   the run incomplete and the aggregate result unusable.

Run:

```bash
bash scripts/lint-maintenance-probe.test.sh
```

Expected RED: the test exits non-zero because the probe script and SQL do not exist.

**Step 2: Add bounded aggregate SQL**

Implement `scripts/lint-maintenance-probe.sql` as one opaque metric name per
matching row, never `COUNT(*)` and never free-text columns. The shell driver
counts those enumerated names. Cover at minimum:

```text
foreign_key_violations
page_sources_missing_owner
page_evidence_memory_missing_owner
pages_dangling_entity
pending_revision_missing_target
enrichment_steps_missing_owner
legacy_page_sources_missing_owner
relation_self_edges
episodes_missing_parent
broken_nonnull_page_links
done_queue_missing_sync_receipt
multi_chunk_memory_sources
content_hash_missing_heads
```

For Page memory ownership, treat either identity as valid:

```sql
m.source != 'episode'
AND (m.source_id = locator OR m.id = locator)
```

Guard schema-version-dependent queries through table/column detection in the
shell driver. A missing optional table becomes a recorded `not_applicable`
field, not a fabricated zero. Separately compare `COUNT(*)` to enumeration for
core vector-indexed tables as a differential oracle. A mismatch sets
`complete=false`, `reason=count_oracle_mismatch`; no live conclusion may use
that run even though the real metrics themselves use enumeration.

**Step 3: Implement the conservative snapshot protocol**

Implement this CLI:

```text
scripts/lint-maintenance-probe.sh \
  --db <origin_memory.db> \
  --pages-root <canonical-pages-root> \
  [--wenlan-bin <wenlan>] \
  [--output-root <external-root>] \
  [--test-after-first-receipt-hook <command>]
```

Required behavior:

1. `set -euo pipefail`, `umask 077`, bounded command output, explicit tool checks.
2. Canonicalize inputs without following an input symlink; reject path escape and output roots inside a git worktree.
3. Create a run directory keyed by commit SHA and UTC run ID.
4. Fingerprint the Page tree deterministically from root-relative regular files only; reject symlinks and sort with `LC_ALL=C`.
5. Use SQLite `.backup` to create `db-before.sqlite` and `db-after.sqlite`; fingerprint both backups.
6. Create `pages-snapshot.tar` between the Page before/after receipts, verify
   its extracted root-relative receipt equals the Page-before receipt, and keep
   it inside the same owner-only external run directory as the DB backups.
7. Run aggregate SQL only against the backup, never the live DB.
8. If a daemon CLI is supplied, capture `wenlan --format json lint` and `wenlan --format json lint --profile deep` while preserving exit codes `0/1/2` as evidence rather than treating `1` as script failure.
9. Re-fingerprint DB and Page tree after diagnostics. Any drift sets top-level `complete=false`, `reason=inconsistent_snapshot`.
10. Write `manifest.json` with commit, dirty flag, run ID, probe version, timestamps, snapshot method, source receipts, count-oracle receipt, lint exit codes, completeness, and SHA-256 for every artifact.
11. Print only the manifest path and a bounded summary to stdout.

**Step 4: Make the harness pass**

Run:

```bash
bash scripts/lint-maintenance-probe.test.sh
```

Expected GREEN: all stable, drift, redaction, permissions, symlink, and non-mutation cases pass.

**Step 5: Seed the redacted ledger schema**

Create the ledger with one table per issue using these fields:

```text
issue_id | scenario | live_exposure | code_evidence | invariant | reproducer |
root_cause | repair | lint_coverage | cleanup_class | verification |
follow_up_direction | status
```

Seed `A1` through `A5` as `candidate`, and the Priority B families as `candidate`. Add an explicit rule that raw artifact paths point to the external run manifest and are not copied into the ledger.

**Step 6: Verify and commit**

```bash
bash scripts/lint-maintenance-probe.test.sh
cargo test -p wenlan-core --test lint_maintenance_probe_e2e -- --nocapture
bash -n scripts/lint-maintenance-probe.sh
git diff --check
git add scripts/lint-maintenance-probe.sh \
  scripts/lint-maintenance-probe.sql \
  scripts/lint-maintenance-probe.test.sh \
  crates/wenlan-core/tests/lint_maintenance_probe_e2e.rs
git add -f docs/superpowers/lint-maintenance/2026-07-13-evidence-ledger.md
git commit -m "test: add read-only lint maintenance evidence harness"
```

Acceptance gate: the harness proves redaction, stable/inconsistent observation,
owner-only artifacts, same-envelope DB/Page capture, libSQL-safe enumeration,
count-oracle failure behavior, and zero source mutation before it is allowed to
inspect the real store.

---

## Task 2: Preserve Both Valid Page Source Locator Forms

**Files:**
- Modify: `crates/wenlan-core/src/db.rs` near `cleanup_orphaned_page_sources`
- Test: `crates/wenlan-core/src/db.rs` near existing Page source cleanup tests

**Step 1: Add RED fixtures for both locator forms**

Add focused tests that seed one memory and Page provenance rows using:

- `memories.source_id` as locator;
- `memories.id` as locator;
- a missing locator as the actual orphan.

Cover both `page_sources.memory_source_id` and `page_evidence.locator`. Assert cleanup keeps the first two and removes only the orphan.

Run:

```bash
cargo test -p wenlan-core --lib cleanup_orphaned_page_sources -- --nocapture
```

Expected RED: rows keyed by `memories.id` are incorrectly deleted.

**Step 2: Repair the ownership predicate inside the existing transaction**

Change both deletion predicates to preserve a locator when a non-episode memory matches either logical or internal identity:

```sql
NOT EXISTS (
  SELECT 1
  FROM memories m
  WHERE m.source != 'episode'
    AND (
      m.source_id = page_sources.memory_source_id
      OR m.id = page_sources.memory_source_id
    )
)
```

Use the corresponding table/column for `page_evidence`. Do not add a second cleanup runner.

**Step 3: Verify**

```bash
cargo test -p wenlan-core --lib cleanup_orphaned_page_sources -- --nocapture
cargo test -p wenlan-core --lib pages::provenance -- --nocapture
cargo fmt --all -- --check
git diff --check
```

**Step 4: Update ledger and commit**

Record `A1` as `fixed`, including RED/GREEN commands and code references. Do not claim live exposure until Task 9 runs the real-store probe.

```bash
git add crates/wenlan-core/src/db.rs
git add -f docs/superpowers/lint-maintenance/2026-07-13-evidence-ledger.md
git commit -m "fix: preserve valid page source locators"
```

Acceptance gate: cleanup keeps both authorized locator representations and removes a truly missing owner atomically.

---

## Task 3: Make Entity Merge and Delete Reference-Safe

**Files:**
- Modify: `crates/wenlan-core/src/db.rs` near `merge_entities` and `delete_entity`
- Test: `crates/wenlan-core/src/db.rs` near entity merge/delete tests

**Step 1: Add RED merge coverage**

Seed canonical and loser entities with:

- distinct `memory_entities` rows;
- one duplicate `(memory_id, entity_id)` collision after merge;
- a Page whose `entity_id` points to the loser;
- existing relations, observations, aliases, and legacy `memories.entity_id` coverage.

Assert merge transfers all surviving references to the canonical entity without duplicate junction rows.

Expected RED command:

```bash
cargo test -p wenlan-core --lib merge_entities -- --nocapture
```

Expected failure: loser junction links disappear through cascade and Page ownership remains dangling or stale.

**Step 2: Add RED delete and rollback coverage**

Seed a Page and memory references to an entity. Assert delete nulls nullable Page/legacy memory references and removes canonical junction links through FK cascade. Add a temporary `BEFORE DELETE ON entities` trigger that raises `ABORT`; assert all pre-delete references remain unchanged after the error.

Expected RED command:

```bash
cargo test -p wenlan-core --lib delete_entity -- --nocapture
```

Expected failure: Page references are not cleared and the multi-statement delete is not atomic.

**Step 3: Implement the minimal transactional transfer**

Inside `merge_entities`' existing transaction:

```sql
INSERT OR IGNORE INTO memory_entities (memory_id, entity_id)
SELECT memory_id, ?canonical_id
FROM memory_entities
WHERE entity_id = ?loser_id;

DELETE FROM memory_entities WHERE entity_id = ?loser_id;
UPDATE pages SET entity_id = ?canonical_id WHERE entity_id = ?loser_id;
```

Preserve every existing junction metadata column exactly; inspect the actual schema before writing the `SELECT` list. Keep relation, observation, alias, and legacy memory handling inside the same transaction.

Wrap `delete_entity` in `BEGIN`/`COMMIT` with explicit rollback on every error. Null `pages.entity_id` and legacy `memories.entity_id`, remove aliases, then delete the entity. Let declared cascades remove canonical junction rows.

**Step 4: Verify and commit**

```bash
cargo test -p wenlan-core --lib merge_entities -- --nocapture
cargo test -p wenlan-core --lib delete_entity -- --nocapture
cargo test -p wenlan-core --lib entity -- --nocapture
cargo fmt --all -- --check
git diff --check
git add crates/wenlan-core/src/db.rs
git add -f docs/superpowers/lint-maintenance/2026-07-13-evidence-ledger.md
git commit -m "fix: preserve entity references across merge and delete"
```

Acceptance gate: success transfers/nulls every owned reference; injected delete failure restores every pre-operation reference and the connection remains usable.

---

## Task 4: Roll Back Failed Document Upserts and Reuse the Connection

**Files:**
- Modify: `crates/wenlan-core/src/db.rs` near `upsert_documents_with_derived_channels`
- Test: `crates/wenlan-core/src/db.rs` near document upsert tests

**Step 1: Add a deterministic RED fault injection test**

1. Seed an existing document with multiple chunks and derived child vectors.
2. Install `BEFORE INSERT ON memories WHEN NEW.source_id = 'upsert_rollback'` that raises `ABORT`.
3. Upsert replacement content under the same source id and assert an error.
4. Assert the original memory rows and derived rows remain byte-for-byte/logically unchanged.
5. Drop the trigger, execute another upsert on the same `MemoryDB`, and assert success.

Run:

```bash
cargo test -p wenlan-core --lib upsert_documents_rolls_back_and_reuses_connection -- --nocapture
```

Expected RED: the old rows were deleted and/or the shared connection remains in an open failed transaction.

**Step 2: Add an explicit transaction outcome boundary**

Keep preparation outside the lock. After `BEGIN`, evaluate all mutation steps in one result boundary:

```rust
conn.execute("BEGIN", ()).await?;
let mutation = async {
    // existing delete, insert, child-vector, and supersession statements
    Ok::<_, WenlanError>(result)
}
.await;

match mutation {
    Ok(value) => {
        if let Err(error) = conn.execute("COMMIT", ()).await {
            let _ = conn.execute("ROLLBACK", ()).await;
            return Err(error.into());
        }
        Ok(value)
    }
    Err(error) => {
        let _ = conn.execute("ROLLBACK", ()).await;
        Err(error)
    }
}
```

Do not refactor unrelated SQL or change best-effort supersession semantics in this task.

**Step 3: Verify and commit**

```bash
cargo test -p wenlan-core --lib upsert_documents_rolls_back_and_reuses_connection -- --nocapture
cargo test -p wenlan-core --lib upsert_documents -- --nocapture
cargo fmt --all -- --check
git diff --check
git add crates/wenlan-core/src/db.rs
git add -f docs/superpowers/lint-maintenance/2026-07-13-evidence-ledger.md
git commit -m "fix: roll back failed document upserts"
```

Acceptance gate: injected failure preserves the previous document and a subsequent write succeeds through the same connection.

---

## Task 5: Make Memory Updates Validated, Atomic, and Multi-Chunk Safe

**Files:**
- Modify: `crates/wenlan-types/src/requests.rs` only if the existing request shape cannot express the final core command cleanly
- Modify: `crates/wenlan-core/src/db.rs` near `update_memory` and child-vector rebuild
- Modify: `crates/wenlan-core/src/post_write.rs`
- Modify: `crates/wenlan-server/src/memory_routes.rs` near `update_memory`
- Test: `crates/wenlan-core/src/db.rs`
- Test: `crates/wenlan-server/src/memory_routes.rs` or focused server integration tests
- Reference: `crates/wenlan-types/src/sources.rs` (`MemoryType`, `RawDocument`)

**Step 1: Lock the current metadata contract before choosing an implementation**

Read the current memory/chunk schema and update call graph. Write a short code comment only if needed to explain which fields are head-owned versus chunk-owned. The implementation must preserve at least:

```text
id, source_id, source, title, summary, url, source_agent,
structured_fields, retrieval_cue, source_text, confidence, importance,
event_date, event_end, version, changelog, created_at, access counters,
supersession/revision state, enrichment state, content_hash, and provenance
```

Do not route through `RawDocument` if doing so drops fields that `RawDocument` cannot represent.

**Step 2: Add RED tests for validation order and all-or-nothing updates**

Cover:

1. invalid `memory_type` is rejected before any field changes;
2. registered-space resolution is completed before the core mutation and the
   existing unknown-space fallback contract remains explicit in its focused test;
3. `confirmed=Some(true)` keeps the existing confirm behavior and
   `confirmed=Some(false)` keeps the existing no-op behavior;
4. a multi-field request that fails one validation changes no fields;
5. injected SQL failure after a head/content change rolls back metadata, chunks, and child vectors;
6. the same connection succeeds on a later update.

Run the narrow server/core test names and observe RED before production edits.

**Step 3: Add RED multi-chunk replacement coverage**

Seed one logical memory with at least three chunks and child vectors. Update its content through the memory-edit endpoint. Assert:

- every secondary chunk is removed and the complete edited content lives in
  the primary `chunk_index = 0` row;
- child vectors correspond only to the new content;
- `source_id`, `created_at`, event/lifecycle metadata, version/changelog,
  provenance, and access state are preserved unchanged;
- FTS reflects the primary-row update through existing triggers.

Expected RED: current `update_memory` updates only `chunk_index = 0` and rebuilds child vectors in a separate transaction.

**Step 4: Implement one core-owned update transaction**

Add one public capability in `post_write` and one DB transaction primitive:

```rust
pub struct MemoryUpdate<'a> {
    pub content: Option<&'a str>,
    pub space: Option<Option<&'a str>>,
    pub confirm: bool,
    pub memory_type: Option<&'a str>,
}

pub async fn post_write::update_memory(
    db: &MemoryDB,
    source_id: &str,
    update: MemoryUpdate<'_>,
) -> Result<(), WenlanError>;
```

`post_write::update_memory` calls the DB primitive once. The DB method owns:

1. embeddings and child-vector text/embeddings prepared before `BEGIN`;
2. one explicit `BEGIN`/rollback/`COMMIT` boundary;
3. an in-place primary-row update limited to requested fields plus content's
   embedding and word count;
4. deletion of stale `chunk_index > 0` rows for the same logical source when
   content changes, without routing through upsert or the document chunker;
5. child-vector replacement inside the same transaction, including deletion
   when the fact channel is disabled;
6. episode synchronization inside the same transaction: preserve a
   `source_text`-backed verbatim episode, otherwise re-run the existing
   `derive_episode` rule and update or delete the content-backed episode;
7. one returned updated memory view if the existing route response requires it.

Do not explicitly rewrite FTS; existing update/delete triggers own it. Extract
the existing embedding-text and child-vector-text builders into small pure
helpers only where needed to avoid duplicating upsert behavior.

The server handler must parse `MemoryType::from_str`, complete the existing registered-space/fallback resolution before mutation, and call the core method once. It must not issue sequential field updates. Keep the HTTP response shape stable.

**Step 5: Verify and commit**

```bash
cargo test -p wenlan-core --lib update_memory -- --nocapture
cargo test -p wenlan-server update_memory -- --nocapture
cargo test -p wenlan-server --test space_header_fallback -- --nocapture
cargo fmt --all -- --check
cargo clippy -p wenlan-core -p wenlan-server --all-targets -- -D warnings
git diff --check
git add crates/wenlan-types/src/requests.rs \
  crates/wenlan-core/src/db.rs \
  crates/wenlan-core/src/post_write.rs \
  crates/wenlan-server/src/memory_routes.rs
git add -f docs/superpowers/lint-maintenance/2026-07-13-evidence-ledger.md
git commit -m "fix: make memory updates atomic and multi-chunk safe"
```

Stage only files actually changed. Preserve the existing `space_header_fallback` contract in this task; changing it requires a separate product decision and focused design update.

Acceptance gate: one request either validates and atomically updates the complete logical memory or leaves all storage/derived channels untouched; no stale chunks survive.

---

## Task 6: Scope Page Growth and Automatic Wikilink Resolution

**Files:**
- Modify: `crates/wenlan-core/src/post_ingest.rs`
- Modify: `crates/wenlan-core/src/db.rs` near Page matching/link resolution
- Modify: `crates/wenlan-core/src/synthesis/wikilinks.rs`
- Test: focused tests in the same modules

**Step 1: Add RED Page-growth scope tests**

Create identical entity/title candidates in two spaces. Ingest a memory in one space and assert automatic Page growth updates only the same-space Page. Also assert Page growth uses the entity linked during the current enrichment run, not only the entity id present before enrichment.

Expected RED:

```bash
cargo test -p wenlan-core --lib post_ingest::tests -- --nocapture
```

Current global `find_matching_page` may select the unrelated Page, and `grow_page` receives the stale initial entity id.

**Step 2: Add RED wikilink scope and ambiguity tests**

Cover:

1. same title in two spaces resolves to the source Page's scope;
2. same-scope duplicate active titles remain unresolved;
3. an orphan link is repaired only for its source Page, not every row sharing the same label;
4. an explicitly stored cross-space link is not deleted by the automatic resolver.

Expected RED:

```bash
cargo test -p wenlan-core --lib wikilink -- --nocapture
```

**Step 3: Thread scope through existing ownership seams**

- Pass the current workspace/space to `grow_page` and use `find_matching_page_scoped`.
- Re-read the linked entity after extraction completes, immediately before
  Page growth, and pass that final value rather than the pre-enrichment
  `entity_id` or the pre-extraction `current_entity_id`.
- Resolve automatic wikilinks from the source Page's workspace/space.
- Return a target only when exactly one active same-scope Page matches.
- Process orphan links by `(source_page_id, label_key)`, never by global label alone.

Do not forbid intentional cross-space links globally. This task constrains only automatic matching and repair.

**Step 4: Verify and commit**

```bash
cargo test -p wenlan-core --lib post_ingest::tests -- --nocapture
cargo test -p wenlan-core --lib wikilink -- --nocapture
cargo test -p wenlan-core --lib find_matching_page_scoped -- --nocapture
cargo fmt --all -- --check
git diff --check
git add crates/wenlan-core/src/post_ingest.rs \
  crates/wenlan-core/src/db.rs \
  crates/wenlan-core/src/synthesis/wikilinks.rs
git add -f docs/superpowers/lint-maintenance/2026-07-13-evidence-ledger.md
git commit -m "fix: scope page growth and wikilink resolution"
```

Acceptance gate: automatic Page mutation and target resolution are deterministic within the source scope; ambiguity remains visible and no unrelated Page is changed.

---

## Task 7: Run a Bounded Priority B Reproducer Sweep

**Files:**
- Modify: existing focused test modules only when adding a reproducer
- Modify: `docs/superpowers/lint-maintenance/2026-07-13-evidence-ledger.md`
- Create only when needed: one narrowly named integration test under `crates/wenlan-core/tests/`

**Step 1: Investigate by state transition, not by module inventory**

For each family below, read the write path and its existing tests, then choose one highest-risk fault point. Do not edit production code until a deterministic test proves an invariant violation.

```text
B1 KG relation/observation and dual-pool partial commit
B2 Page revision accept, archive, watcher, and proposal consumption
B3 ordinary/time-range delete, episode children, and source-id rebinding
B4 enrichment/source-sync checkpoint ordering and retry convergence
B5 source Page delete-then-create failure and provenance retention
B6 concurrent projection writes and concurrent identical captures
B7 legacy Page provenance and resolver behavior after target deletion
```

**Step 2: Use deterministic fault controls**

Prefer SQLite abort triggers, Tokio barriers, fixed clocks/IDs, and tiny synthetic fixtures. Do not use sleeps as concurrency proof. Each reproducer must assert both invariant damage and subsequent connection/queue reuse.

**Step 3: Give every candidate one disposition**

Allowed dispositions:

- `reproduced`: promote to a focused repair commit using RED/GREEN TDD;
- `not_reproduced`: record the test and why the current code is safe;
- `expected_state`: document the retention/telemetry contract;
- `semantic_review`: deterministic repair would be unsafe;
- `deferred`: bounded reason and entry criterion, not merely lack of time.

Do not require all seven families to produce a code fix. The deliverable is complete evidence and no unresolved `candidate` status.

**Step 4: Stop at discovery and route reproduced defects**

Do not edit production code in Task 7. For each `reproduced` item, add a
separately reviewed plan addendum containing its exact test location, expected
RED, ownership seam, adjacent gates, and commit boundary. Execute that addendum
only after the current Priority A repairs remain green. This prevents the
bounded discovery wave from silently growing into an unreviewed repair batch.

**Step 5: Verify the sweep**

```bash
rg -n '\| candidate \|' docs/superpowers/lint-maintenance/2026-07-13-evidence-ledger.md
cargo fmt --all -- --check
git diff --check
```

Expected: no Priority B issue row remains `candidate`. Prose that explains the allowed status vocabulary may still contain the word.

Acceptance gate: every Priority B family has an explicit evidence-backed
disposition, no production file changed during the discovery task, and every
`reproduced` item links to a separately reviewed repair addendum.

---

## Task 8: Add Stable Lint Coverage Without Duplicating the Runner

**Files:**
- Modify: `crates/wenlan-core/src/lint/pages/provenance_checks/source.rs`
- Modify only if justified: `crates/wenlan-core/src/lint/**`
- Modify only if a new check is justified: `crates/wenlan-types/src/lint_contract.rs`
- Test: existing lint contract and Page provenance tests

**Step 1: Add a RED missing-owner provenance test**

Construct a Page source locator whose memory owner is absent but whose evidence shape would otherwise look valid. Assert the existing `pages.provenance.source_evidence_coverage` check emits a hard actionable finding. Cover both missing `source_id` and missing internal row id owner forms.

Run:

```bash
cargo test -p wenlan-core --lib pages::provenance -- --nocapture
```

Expected RED: current matching evidence can mask the missing source owner.

**Step 2: Extend the existing check, not the product surface**

Add `owner_present` to the existing source population and make a missing owner a hard finding in `pages.provenance.source_evidence_coverage`. Preserve:

- `memory`, `external_file`, `external_url`, and `authored` semantics;
- wrong/multi-kind as drift;
- extra legal evidence as inventory;
- citation NULL/empty/non-empty and verified/scope partitions;
- deterministic ordering, complete validation population, and bounded samples.

Do not add `/api/wiki/check`, a second runner, a second report, or mutating repair behavior.

**Step 3: Decide whether dangling Page entity ownership needs a check**

Use Task 1/7 evidence:

- If `pages_dangling_entity > 0` is reproduced as a product-owned invariant and is not already reported, add one core Page integrity check and update catalog/profile counts and report schema fixtures.
- Otherwise keep it as an aggregate maintenance probe plus transaction regression tests. Record the decision in the ledger.

**Step 4: Verify catalog and non-mutation contracts**

```bash
cargo test -p wenlan-core --lib lint:: -- --nocapture
cargo test -p wenlan-types lint_contract -- --nocapture
cargo fmt --all -- --check
cargo clippy -p wenlan-types -p wenlan-core --all-targets -- -D warnings
git diff --check
```

Update before/after test fingerprints for DB and the full Page projection tree. A check that cannot observe a complete population must return incomplete, never clean.

**Step 5: Commit**

```bash
git add crates/wenlan-core/src/lint crates/wenlan-types/src/lint_contract.rs
git add -f docs/superpowers/lint-maintenance/2026-07-13-evidence-ledger.md
git commit -m "fix: surface missing page provenance owners in lint"
```

Stage only files actually changed.

Acceptance gate: one canonical `wenlan lint` runner reports the stable repaired invariants with existing outcome/severity/complete semantics and remains provably read-only.

---

## Task 9: Fresh-Copy Verification and Cleanup Proposal

**Files:**
- Modify: `docs/superpowers/lint-maintenance/2026-07-13-evidence-ledger.md`
- Create: `docs/superpowers/lint-maintenance/2026-07-13-cleanup-proposal.md`

**Step 1: Run the tested probe against the real store, read-only**

Resolve the current platform data paths from code/runtime configuration; do not assume them. Run Task 1's probe with the current daemon CLI. Record the manifest path, General/Deep exit codes, completeness, and redacted counts in the ledger.

If receipts drift, preserve the run as `inconsistent_snapshot`, do not draw clean/fail conclusions, and retry at most once after a quiet interval. Do not stop or restart the daemon.

**Step 2: Build an isolated fresh-copy environment**

Only from a run with stable DB/Page receipts:

1. extract `pages-snapshot.tar` and copy `db-before.sqlite` from the same stable
   Task 1 manifest into an external owner-only verification directory;
2. set `SCRATCH_HOME=<verification>/home` and
   `SCRATCH_DATA=<verification>/data`, place Pages at
   `$SCRATCH_HOME/.wenlan/pages` and the DB at
   `$SCRATCH_DATA/memorydb/origin_memory.db`;
3. preflight `realpath` for scratch home, data, config, DB, and Pages and reject
   any path equal to or nested under the resolved live HOME, live data root, or
   live `~/.wenlan` root;
4. launch only with
   `HOME="$SCRATCH_HOME" WENLAN_DATA_DIR="$SCRATCH_DATA" WENLAN_BIND_ADDR=127.0.0.1:0 WENLAN_PORT_FILE=<verification>/port <repaired-wenlan-server>`;
5. verify the published port is non-7878, `/api/health` reports healthy, and
   the process has not created or changed any preflighted live-root receipt;
6. never register the scratch daemon as a service.

Run canonical General and Deep profiles through `/api/lint` and the thin CLI. Deep with no configured/available provider must prove `complete=false`, not falsely clean.

**Step 3: Prove user experience and non-mutation**

Capture exact commands and bounded report summaries for:

```bash
HOME="$SCRATCH_HOME" WENLAN_DATA_DIR="$SCRATCH_DATA" \
  WENLAN_HOST="http://127.0.0.1:$SCRATCH_PORT" \
  <repaired-wenlan> --format json lint
HOME="$SCRATCH_HOME" WENLAN_DATA_DIR="$SCRATCH_DATA" \
  WENLAN_HOST="http://127.0.0.1:$SCRATCH_PORT" \
  <repaired-wenlan> --format json lint --profile deep
```

Verify exit semantics:

```text
0 = complete and clean
1 = complete with actionable findings
2 = incomplete, execution failure, or invalid invocation
```

Fingerprint the isolated DB and full Page tree before/after each profile. Any mutation is a release blocker.

**Step 4: Produce a proposal, not an apply tool**

Create a redacted cleanup proposal grouped into:

```text
deterministic_safe
needs_semantic_review
historical_telemetry
environment_or_config
do_not_touch
```

Every proposed action must include durable owner identity, expected version/CAS precondition, evidence reason, rollback artifact requirement, and post-apply lint assertion. Do not include raw prose or execute any action.

**Step 5: Run final repository gates**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --lib
bash scripts/lint-maintenance-probe.test.sh
git diff --check
```

If product changes touched a daemon-facing flow, run the relevant hermetic integration and black-box smoke tests. Do not substitute a live-model smoke for deterministic CI-safe tests.

**Step 6: Final high-accuracy review**

Run one Sol-level integrated review against:

- the approved design spec;
- this plan;
- the full branch diff;
- RED/GREEN and final gate evidence;
- the real-store manifest and redacted ledger;
- the cleanup proposal.

Review for data loss, transaction leaks, scope crossing, false-clean lint behavior, report/catalog drift, raw-data leakage, and accidental mutation. Address every actionable finding, rerun affected gates, and record the final verdict.

**Step 7: Commit the evidence artifacts**

```bash
git add -f \
  docs/superpowers/lint-maintenance/2026-07-13-evidence-ledger.md \
  docs/superpowers/lint-maintenance/2026-07-13-cleanup-proposal.md
git commit -m "docs: record lint maintenance evidence and cleanup proposal"
```

Acceptance gate: the repaired branch is green, canonical lint is demonstrated through daemon and CLI against an isolated copy, non-mutation is proven, every Priority A/B row has a disposition, and no cleanup has touched live data.

---

## Campaign Completion Checklist

- [ ] Priority A1-A5 are `fixed` or `not_reproduced` with a deterministic rejection; none remain `candidate` or `deferred`.
- [ ] Every Priority B family has a deterministic disposition.
- [ ] All committed evidence is redacted; raw artifacts remain in the external SHA-keyed store.
- [ ] General lint completes locally; Deep accurately reports provider availability and judged population.
- [ ] `/api/lint` and the thin CLI return the same versioned report contract.
- [ ] Lint DB and Page projection before/after fingerprints match.
- [ ] Focused tests, adjacent tests, workspace format, clippy, and library tests pass.
- [ ] Final integrated review has no unresolved actionable finding.
- [ ] Cleanup exists only as a proposal with CAS and rollback requirements; live data is unchanged.
