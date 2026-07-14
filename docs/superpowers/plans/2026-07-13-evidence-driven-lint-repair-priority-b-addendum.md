# Priority B Evidence Repair Addendum

**Parent plan:** `docs/superpowers/plans/2026-07-13-evidence-driven-lint-repair.md`

**Scope:** Repair only the six deterministic defects reproduced by Task 7. Keep
lint read-only. Do not add repair APIs, review cards, provider routing, or a new
runner. Each item has its own RED/GREEN boundary and commit.

## B1: Atomic Dual-Pool Supersession

- RED: `crates/wenlan-core/src/db.rs` test
  `supersede_existing_rolls_back_link_when_suppression_fails`.
- Observed RED: the suppression UPDATE aborted, but
  `incoming.supersedes` remained committed.
- Ownership seam: `MemoryDB::resolve_supersede_existing`.
- Repair: execute linkage and suppression inside one explicit transaction;
  rollback on either statement or COMMIT failure.
- Adjacent gates: successful dual-pool invalidation, incoming-expired path,
  connection reuse, full `refinement_queue` tests.
- Commit: `fix: make dual-pool supersession atomic`.

## B2: Atomic Page Revision Acceptance

- RED: `crates/wenlan-core/src/post_write.rs` test
  `accept_page_revision_consume_failure_keeps_page_retryable`.
- Observed RED: Page content/version committed before revision-card consumption;
  the card stayed pending and its version CAS could not converge on retry.
- Ownership seam: one DB capability called by `accept_page_revision_card` that
  combines the existing version-CAS Page update, source/evidence replacement,
  and exact pending-card consumption in one transaction.
- Repair: precompute content, source set, citations, and changelog as today;
  perform Page mutation and `pending_revision` consumption atomically. Treat a
  zero-row card consume as conflict and rollback. Projection remains after the
  DB commit and remains best-effort.
- Adjacent gates: happy-path Page card acceptance, stale-version conflict,
  legacy card without `page_version`, source/evidence reconciliation, Page
  changelog, connection reuse.
- Commit: `fix: make page revision acceptance atomic`.

## B3: Atomic Source-ID Rebinding

- RED: `crates/wenlan-core/src/db.rs` test
  `rebind_source_id_rolls_back_when_checkpoint_rebind_fails`.
- Observed RED: primary memories moved to the new ID, enrichment checkpoints
  stayed on the old ID, and the helper returned success.
- Ownership seam: `MemoryDB::rebind_source_id`.
- Repair: transactionally rebind primary memory rows and enrichment checkpoints;
  do not swallow checkpoint errors. In the same capability, rebind logical
  children owned by the source ID (`episodes.episode_of/source_id` and
  `child_vectors.parent_id`) where those rows exist.
- Adjacent gates: source rename route, successful no-child rebind, abort
  rollback, child/episode ownership, connection reuse.
- Commit: `fix: make source id rebinding atomic`.

## B4: Terminal Queue State Requires Sync Receipt

- RED: `crates/wenlan-core/src/document_enrichment.rs` test
  `sync_receipt_failure_does_not_leave_same_hash_queue_terminal`.
- Observed RED: sync receipt INSERT failed, queue was already `done`, and a
  same-hash re-enqueue did not reopen it.
- Ownership seam: `record_sync_state` plus terminal transitions in
  `run_document_enrichment`.
- Repair: make receipt persistence return `Result`; write the receipt before
  `mark_done`. On receipt failure, mark the entry paused/retryable and return a
  non-terminal outcome. A crash after receipt but before `done` may repeat work
  but cannot create a false terminal receipt.
- Adjacent gates: no-LLM completion, full LLM completion, empty/unsupported
  document terminal paths, changed/vanished file handling, same-hash retry.
- Commit: `fix: require source sync receipts before queue completion`.

## B5: Preserve Source Page Across Replacement Failure

- RED: `crates/wenlan-core/src/document_enrichment.rs` test
  `source_page_replacement_failure_preserves_last_valid_page_and_provenance`.
- Observed RED: delete committed first; failed insert left no Page and cascaded
  away `page_sources` and `page_evidence`.
- Ownership seam: `write_source_page` through the canonical PageWrite/DB write
  capability.
- Repair: replace an existing machine-owned source Page without delete-first.
  Content, summary, source ids, typed evidence, version, and timestamps must
  commit atomically; creation remains the existing PageWrite create path.
- Adjacent gates: stub-to-digest refresh, deterministic ID reuse, typed
  `external_file` evidence, failed replacement rollback, exactly one Page.
- Commit: `fix: preserve source pages across replacement failure`.

## B6: Concurrent Identical Capture

- Disposition: `deferred`.
- Reason: the real handler performs dedup read before ID allocation, but there
  is no deterministic synchronization seam after that read. Adding a
  production/test hook is outside Task 7's discovery-only write scope; sleeps
  are explicitly disallowed and cannot prove the race.
- Entry criterion: approve a test-only `tokio::sync::Barrier` seam immediately
  after the handler dedup read, then run two real identical capture requests and
  assert one durable head plus one duplicate response. Projection-manifest lost
  updates remain documented regenerable state and are not conflated with this
  capture invariant.

## B7: Deleted Wikilink Targets Become Orphans

- RED: `crates/wenlan-core/src/db.rs` test
  `page_links_target_delete_becomes_orphan_and_reresolves`.
- Observed RED: target deletion left a stale non-NULL target ID, so orphan
  resolution could never bind the later replacement Page.
- Ownership seam: `MemoryDB::delete_page`.
- Repair: in one transaction, set inbound `page_links.target_page_id` to NULL,
  then delete the target Page. Source-page deletion continues to use its FK
  cascade; explicit links are preserved until their actual target is deleted.
- Adjacent gates: source-delete cascade, orphan-then-create resolution,
  same-scope resolution, same-title ambiguity, connection reuse.
- Commit: `fix: orphan wikilinks when targets are deleted`.

## Execution Order

1. B1 and B3 small DB transactions.
2. B7 target deletion transaction.
3. B5 source Page replacement capability.
4. B4 receipt-before-terminal ordering, after B5 makes retries safe.
5. B2 Page/card atomic capability, the widest DB boundary.
6. Run all six focused tests together, then full core lint and library gates.

No item may weaken CAS, convert a write failure into inventory, or mutate live
data. Task 9's real-store work remains read-only and starts only after all
non-deferred RED tests are green.
