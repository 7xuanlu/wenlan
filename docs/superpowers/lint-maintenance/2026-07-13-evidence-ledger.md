# Evidence-Driven Lint Maintenance Ledger

This ledger records redacted evidence for the campaign defined by
`docs/superpowers/specs/2026-07-13-evidence-driven-lint-repair-design.md`.
Raw database backups, Page archives, lint responses, and probe diagnostics stay
in the external SHA-keyed artifact store. This file contains no memory bodies,
Page prose, user-content paths, URLs, credentials, or raw database rows.

Allowed statuses: `candidate`, `reproduced`, `fixed`, `not_reproduced`,
`expected_state`, `semantic_review`, and `deferred`. Priority A cannot satisfy
the campaign completion gate while `candidate` or `deferred`.

## Run Receipts

No real-store probe has been accepted yet. Add only a stable manifest receipt,
its code commit, lint exit codes, completeness reason, and redacted aggregate
counts. Never copy raw artifact contents into this ledger.

## A1: Page Source Locator Ownership

| Field | Evidence |
|---|---|
| issue_id | `A1` |
| scenario | Cleanup evaluates Page provenance that names a memory by logical `source_id` or internal row `id`. |
| observed_live_exposure | Pending a stable real-store probe. |
| code_evidence | `crates/wenlan-core/src/db.rs`, `cleanup_orphaned_page_sources` currently validates only `memories.source_id`. |
| invariant | Either authorized memory identity preserves `page_sources` and memory `page_evidence`; only a missing owner is removable. |
| reproducer | Focused cleanup fixture for logical id, row id, and missing owner. |
| root_cause | Confirmed: both cleanup predicates recognized only logical `memories.source_id`, so a valid internal row-id locator was deleted. |
| repair | Both dual-write cleanup predicates now use correlated `NOT EXISTS` and preserve a non-episode memory matching either `source_id` or `id` in the existing transaction. |
| lint_coverage | Existing `pages.provenance.source_evidence_coverage` is the preferred check group. |
| cleanup_class | Unclassified until live exposure is measured. |
| verification | RED removed 2 rows instead of 1. GREEN: focused locator fixture 1/1, orphan-cleanup group 3/3, Page provenance adjacency 6/6. |
| follow_up_direction | Task 8 missing-owner lint coverage; Task 9 measures live residue without mutation. |
| status | `fixed` |

## A2: Entity Merge and Delete References

| Field | Evidence |
|---|---|
| issue_id | `A2` |
| scenario | Merge or delete an entity referenced by `memory_entities`, legacy memory ownership, aliases, relations, observations, or Pages. |
| observed_live_exposure | Pending a stable real-store probe. |
| code_evidence | `crates/wenlan-core/src/db.rs`, `merge_entities` and `delete_entity`; Page and canonical junction ownership are not handled as one transaction. |
| invariant | Merge transfers every surviving reference without duplicates; delete nulls nullable owners and rolls back all statements on failure. |
| reproducer | Junction collision, Page owner, and abort-trigger rollback fixtures. |
| root_cause | Confirmed: merge omitted `memory_entities` and `pages.entity_id`; delete omitted Page ownership and ran memory/alias/entity statements without one transaction. |
| repair | Merge now transfers canonical junction links with `INSERT OR IGNORE`, removes loser links, and re-points Pages inside its existing transaction. Delete now nulls memory/Page owners and deletes aliases/entity in one rollback-safe transaction; declared FK cascades remove junction/graph children. |
| lint_coverage | `memory_entities.integrity` already covers missing memory/entity owners; Page owner coverage depends on Task 7 evidence. |
| cleanup_class | Unclassified until live exposure is measured. |
| verification | RED: alias-only junction disappeared; abort left memory ownership cleared. GREEN: new merge 1/1, new delete rollback/retry 1/1, merge group 14/14, delete group 3/3. |
| follow_up_direction | Task 7/8 decides whether live dangling Page owners justify a canonical lint check. |
| status | `fixed` |

## A3: Document Upsert Rollback

| Field | Evidence |
|---|---|
| issue_id | `A3` |
| scenario | Replacement upsert fails after deleting the previous logical document. |
| observed_live_exposure | Transaction property; live residue is not sufficient proof. |
| code_evidence | `crates/wenlan-core/src/db.rs`, `upsert_documents_with_derived_channels`; the RED fault injection observed all 8 previous chunks disappear after an insert abort because fallible statements returned after `BEGIN` without rollback. |
| invariant | Failure preserves the previous document and derived rows; the same connection accepts the next write. |
| reproducer | `BEFORE INSERT` abort trigger after seeding an 8-chunk document plus narrative/structured-field child vectors; exact chunk and child inventories are compared before and after failure, followed by a same-connection retry. |
| root_cause | Confirmed: early `?` returns left the deletes visible in an open failed transaction and left the shared connection unable to begin a clean replacement transaction. |
| repair | Existing delete, insert, child-vector, and best-effort supersession statements now execute inside one explicit transaction-result boundary. Mutation or commit failure attempts `ROLLBACK` before returning. |
| lint_coverage | None; rollback and connection reuse are product-test invariants. |
| cleanup_class | `do_not_touch` until a specific residue owner is proven. |
| verification | RED: failed replacement changed the previous chunk population from 8 to 0. GREEN: rollback/reuse test 1/1, existing upsert group 9/9, child-vector replacement 1/1. |
| follow_up_direction | Real-store probe may discover historical residue, but this transaction property remains enforced by deterministic product tests rather than a lint finding. |
| status | `fixed`; live historical exposure remains unproven. |

## A4: Atomic Logical Memory Update

| Field | Evidence |
|---|---|
| issue_id | `A4` |
| scenario | One update request changes content and metadata for a logical memory that has secondary chunks, child vectors, or an episode. |
| observed_live_exposure | Pending a stable real-store probe; multi-chunk population is measured by the harness. |
| code_evidence | RED observed a content edit leave 7 chunks under one logical memory, and an HTTP request combining content/confirm with an invalid taxonomy returned 200 after sequential mutation. |
| invariant | One validated request updates primary content/metadata, removes stale secondary chunks, replaces derived children, and synchronizes episodes in one transaction while preserving untouched metadata. |
| reproducer | Invalid multi-field HTTP request; 7-chunk edit; exact head-metadata snapshot; FTS sentinel; content-backed and source-text-backed episodes; feature-off/word-gate deletion; child-delete abort trigger followed by same-connection retry. |
| root_cause | Confirmed: the server owned a sequence of independent mutations, while core content editing updated only chunk zero and rebuilt child vectors in a later transaction. Taxonomy was not validated at the update boundary. |
| repair | The route now validates `MemoryType`, resolves registered-space fallback, and calls one `post_write::update_memory` capability. One DB primitive prepares embeddings before `BEGIN`, updates the head in place, deletes stale chunks, replaces/deletes children, synchronizes episodes, applies requested metadata/confirmation, and rolls back every mutation on failure. |
| lint_coverage | Lifecycle integrity may detect stable stale-row shapes; transaction atomicity remains test-only. |
| cleanup_class | Unclassified until stale live owners are measured. |
| verification | RED: stale chunk population was 7 instead of 1; invalid multi-field request returned 200. GREEN: core update group 6/6, source-text episode 1/1, derived deletion 1/1, server update group 2/2, unknown-space fallback 1/1, core/server all-target Clippy clean. |
| follow_up_direction | Use the read-only real-store probe to classify any historical stale secondary chunks; do not infer cleanup ownership from the transaction test alone. |
| status | `fixed`; live historical exposure remains unclassified. |

## A5: Scope-Safe Page Growth and Wikilinks

| Field | Evidence |
|---|---|
| issue_id | `A5` |
| scenario | Automatic enrichment grows a Page or resolves a wikilink when equivalent titles/entities exist across spaces. |
| observed_live_exposure | Pending a stable real-store probe; semantic wrong-target exposure needs bounded samples, not raw rows here. |
| code_evidence | RED fixtures proved that Page growth could select an equivalent Page from another scope, ignored an entity linked during the current enrichment run, resolved duplicate titles by arbitrary global row order, and repaired every orphan sharing a label. |
| invariant | Automatic matching is deterministic within the source scope; same-scope ambiguity remains unresolved; intentional cross-space links are preserved. |
| reproducer | Duplicate cross-space titles/entities, same-scope ambiguity, and source-specific orphan-link fixtures. |
| root_cause | Confirmed: Page growth passed the pre-enrichment entity to a global matcher; entity-first matching fetched one global Page before checking scope; wikilink resolution omitted the source Page scope; orphan repair grouped updates by `label_key` alone. |
| repair | Re-read the final memory entity and scope before growth; query entity and embedding candidates within that scope; resolve titles only when exactly one active same-scope Page exists; repair orphan rows by `(source_page_id, label_key)` while leaving explicit targets untouched. |
| lint_coverage | Add only a deterministic wrong-scope detector with complete population; semantic relatedness remains Deep review. |
| cleanup_class | Likely `needs_semantic_review` for existing links; not classified before evidence. |
| verification | RED: `cargo test -p wenlan-core --lib page_growth_ -- --nocapture` failed 2/2 and `cargo test -p wenlan-core --lib wikilink_ -- --nocapture` failed the three automatic-resolution cases. GREEN: Page growth 2/2, direct growth 3/3, wikilink 21/21, scoped matcher 3/3, and all post-ingest tests 17/17. |
| follow_up_direction | Use the read-only real-store probe to inventory wrong-scope existing links as bounded semantic-review candidates; do not infer that a same-title cross-space link is wrong without evidence. |
| status | `fixed`; live historical exposure remains unclassified. |

## B1: KG and Dual-Pool Partial Commit

| Field | Evidence |
|---|---|
| issue_id | `B1` |
| scenario | Relation/observation or dual-pool resolution fails between owned writes. |
| observed_live_exposure | Pending the stable real-store probe; no live row is classified from the synthetic fault. |
| code_evidence | `resolve_supersede_existing` writes `incoming.supersedes` and suppresses the existing memory in separate statements; the refinement caller logs a helper error and continues. |
| invariant | Retry converges without duplicate, missing, or half-applied graph/lifecycle state. |
| reproducer | `supersede_existing_rolls_back_link_when_suppression_fails` aborts the second statement and asserts the first linkage rolls back. |
| root_cause | No transaction spans linkage and suppression. |
| repair | Linkage and suppression now share one explicit transaction with rollback on either statement or COMMIT failure. |
| lint_coverage | Existing KG/source/lifecycle groups first. |
| cleanup_class | Existing half-linked rows require live evidence and semantic review; do not auto-suppress from shape alone. |
| verification | RED: `cargo test -p wenlan-core --lib supersede_existing_rolls_back_link_when_suppression_fails -- --nocapture` left `incoming.supersedes` committed after suppression abort. GREEN: the same test 1/1 and `test_apply_invalidate_existing_soft_suppressed` 1/1. |
| follow_up_direction | Use the real-store probe to inventory half-linked historical rows without auto-suppressing them. |
| status | `fixed`; live historical exposure remains unclassified. |

## B2: Page Revision, Archive, Watcher, and Proposal Boundaries

| Field | Evidence |
|---|---|
| issue_id | `B2` |
| scenario | Page revision acceptance, archive, watcher replay, or proposal consumption fails between DB and projection writes. |
| observed_live_exposure | Pending the stable real-store probe. |
| code_evidence | Page version-CAS update commits before a separate pending-card consumption UPDATE; projection follows both. |
| invariant | CAS/version state and projection receipts converge or report incomplete; consumed work is not lost. |
| reproducer | `accept_page_revision_consume_failure_keeps_page_retryable` aborts card consumption after the Page CAS. |
| root_cause | Page mutation and work-item consumption have separate transaction boundaries. |
| repair | Reproduced; combined Page/card transaction specified in the Priority B addendum. |
| lint_coverage | Pages/projections state and non-atomic snapshot contracts first. |
| cleanup_class | Existing Page/card mismatches need version/content comparison before any proposal; default `needs_semantic_review`. |
| verification | RED: `cargo test -p wenlan-core --lib accept_page_revision_consume_failure_keeps_page_retryable -- --nocapture` committed proposed Page content while leaving the card pending. |
| follow_up_direction | Execute B2 addendum after narrower DB repairs. |
| status | `reproduced` |

## B3: Delete, Episode, and Source-ID Rebinding

| Field | Evidence |
|---|---|
| issue_id | `B3` |
| scenario | Ordinary/time-range delete or logical source-id rebinding leaves owned children, episodes, or provenance. |
| observed_live_exposure | Pending the stable real-store probe. |
| code_evidence | `rebind_source_id` updates primary memories first, then swallows enrichment checkpoint update errors; logical child keys have no update cascade. |
| invariant | Declared owners cascade/rebind; telemetry retention remains explicit and is not mislabeled orphan data. |
| reproducer | `rebind_source_id_rolls_back_when_checkpoint_rebind_fails` aborts checkpoint rebinding and inventories old/new owners. |
| root_cause | Rebinding is multi-statement, non-transactional, and treats an owned checkpoint write as best-effort. |
| repair | Primary rows, enrichment checkpoints, episode ownership, and child-vector ownership now rebind in one explicit transaction; owned-write errors and COMMIT failures roll back. |
| lint_coverage | Owner-integrity checks only where retention contract is deterministic. |
| cleanup_class | Stable old/new rename receipts may permit `deterministic_safe`; otherwise unclassified until live evidence. |
| verification | RED: `cargo test -p wenlan-core --lib rebind_source_id_rolls_back_when_checkpoint_rebind_fails -- --nocapture` returned success with memories on new and checkpoints on old. GREEN: fault/child ownership test 1/1 and daemon source rename route 1/1; core/server all-target Clippy passed. |
| follow_up_direction | Run the source rename route and classify any live old/new split owners from stable receipts. |
| status | `fixed`; live historical exposure remains unclassified. |

## B4: Checkpoint Ordering and Retry Convergence

| Field | Evidence |
|---|---|
| issue_id | `B4` |
| scenario | Enrichment or source-sync checkpoint reaches terminal state before its owned artifacts are durable. |
| observed_live_exposure | Pending the stable real-store probe. |
| code_evidence | Document enrichment calls `mark_done` before best-effort `record_sync_state`; same-hash enqueue does not reopen a done item. |
| invariant | Terminal receipt implies required writes exist; retry is idempotent and resumes from a valid checkpoint. |
| reproducer | `sync_receipt_failure_does_not_leave_same_hash_queue_terminal` aborts the receipt INSERT, removes the trigger, and re-enqueues the same hash. |
| root_cause | Terminal queue state is committed before its required receipt, and receipt failure is swallowed. |
| repair | Reproduced; receipt-before-terminal ordering specified in the Priority B addendum. |
| lint_coverage | Queue/runtime completeness checks first. |
| cleanup_class | Missing receipt with done queue is `environment_or_config` or retryable state; data cleanup waits for live source ownership. |
| verification | RED: `cargo test -p wenlan-core --lib sync_receipt_failure_does_not_leave_same_hash_queue_terminal -- --nocapture` observed `done` with no receipt and no same-hash recovery. |
| follow_up_direction | Execute B4 after source Page retry is failure-safe. |
| status | `reproduced` |

## B5: Source Page Replacement Failure

| Field | Evidence |
|---|---|
| issue_id | `B5` |
| scenario | Source Page replacement deletes the old projection before the new projection is durable. |
| observed_live_exposure | Pending the stable real-store probe. |
| code_evidence | `write_source_page` deletes the deterministic Page before a separate PageWrite create; Page source/evidence rows cascade on delete. |
| invariant | A failed replacement preserves the last valid Page and provenance or reports an incomplete projection. |
| reproducer | `source_page_replacement_failure_preserves_last_valid_page_and_provenance` aborts replacement Page insertion after an existing source Page is valid. |
| root_cause | Delete-then-create spans two commits; failed create has no last-known-good row to restore. |
| repair | Reproduced; atomic in-place source Page replacement specified in the Priority B addendum. |
| lint_coverage | Page DB/filesystem/state receipt agreement first. |
| cleanup_class | Lost historical Page prose/provenance is not reconstructable from shape alone; default `needs_semantic_review` or external-source replay. |
| verification | RED: `cargo test -p wenlan-core --lib source_page_replacement_failure_preserves_last_valid_page_and_provenance -- --nocapture` left the Page absent after insertion abort. |
| follow_up_direction | Execute B5 before queue retry ordering. |
| status | `reproduced` |

## B6: Concurrent Projection and Capture Writes

| Field | Evidence |
|---|---|
| issue_id | `B6` |
| scenario | Concurrent Page projection writes or identical captures race around version/dedup ownership. |
| observed_live_exposure | Pending the stable real-store probe. |
| code_evidence | Capture dedup is a read before independent ID allocation; there is no unique content key. Projection-manifest last-writer behavior is explicitly regenerable expected state. |
| invariant | CAS/dedup selects one deterministic survivor and no valid write is silently lost. |
| reproducer | Not executed: the real handler needs a deterministic barrier immediately after its dedup read; no existing seam can stop both requests there without production/test-hook work. |
| root_cause | Unproven concurrency window; static ordering alone is insufficient. |
| repair | None until the barrier fixture is separately approved and executed. |
| lint_coverage | Stable duplicate/version drift only; concurrency property remains test-owned. |
| cleanup_class | Duplicate captures would need semantic review; regenerable projection-manifest races are `expected_state`. |
| verification | Bounded code/test read only; existing batcher concurrency test uses a mock processor and cannot prove DB dedup. |
| follow_up_direction | Entry criterion: add a test-only post-dedup barrier and assert one durable head, one duplicate response, then connection reuse. |
| status | `deferred` |

## B7: Legacy Provenance and Deleted Link Targets

| Field | Evidence |
|---|---|
| issue_id | `B7` |
| scenario | Legacy Page provenance or a deleted wikilink target leaves stale locators/resolution state. |
| observed_live_exposure | Pending the stable real-store probe. |
| code_evidence | `page_links.target_page_id` intentionally has no FK; `delete_page` deletes only the target row and orphan repair scans only NULL targets. |
| invariant | Deleted targets become explicit unresolved links; valid provenance locator kinds remain distinguishable. |
| reproducer | `page_links_target_delete_becomes_orphan_and_reresolves` deletes a resolved target, recreates its title under a new ID, and runs orphan repair. |
| root_cause | Target deletion does not null inbound link targets, so stale non-NULL IDs never re-enter orphan resolution. |
| repair | Reproduced; transactional target-null plus Page delete specified in the Priority B addendum. |
| lint_coverage | Existing Pages/provenance and broken-link checks first. |
| cleanup_class | A target ID absent from Pages can be proposed as `deterministic_safe` orphaning; semantic rebinding remains separate. |
| verification | RED: `cargo test -p wenlan-core --lib page_links_target_delete_becomes_orphan_and_reresolves -- --nocapture` retained the deleted target ID. |
| follow_up_direction | Execute B7 addendum and keep same-scope ambiguity tests green. |
| status | `reproduced` |
