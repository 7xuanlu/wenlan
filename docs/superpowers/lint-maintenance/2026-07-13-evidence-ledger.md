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
| observed_live_exposure | Pending probe and reproducer. |
| code_evidence | Pending Task 7 bounded path read. |
| invariant | Retry converges without duplicate, missing, or half-applied graph/lifecycle state. |
| reproducer | One abort-trigger fault point selected in Task 7. |
| root_cause | Unknown. |
| repair | Discovery only; reproduced defects require a reviewed addendum. |
| lint_coverage | Existing KG/source/lifecycle groups first. |
| cleanup_class | Unclassified. |
| verification | Not run. |
| follow_up_direction | Task 7 disposition. |
| status | `candidate` |

## B2: Page Revision, Archive, Watcher, and Proposal Boundaries

| Field | Evidence |
|---|---|
| issue_id | `B2` |
| scenario | Page revision acceptance, archive, watcher replay, or proposal consumption fails between DB and projection writes. |
| observed_live_exposure | Pending probe and reproducer. |
| code_evidence | Pending Task 7 bounded path read. |
| invariant | CAS/version state and projection receipts converge or report incomplete; consumed work is not lost. |
| reproducer | One deterministic failure boundary selected in Task 7. |
| root_cause | Unknown. |
| repair | Discovery only; reproduced defects require a reviewed addendum. |
| lint_coverage | Pages/projections state and non-atomic snapshot contracts first. |
| cleanup_class | Unclassified. |
| verification | Not run. |
| follow_up_direction | Task 7 disposition. |
| status | `candidate` |

## B3: Delete, Episode, and Source-ID Rebinding

| Field | Evidence |
|---|---|
| issue_id | `B3` |
| scenario | Ordinary/time-range delete or logical source-id rebinding leaves owned children, episodes, or provenance. |
| observed_live_exposure | Pending probe and reproducer. |
| code_evidence | Pending Task 7 bounded path read. |
| invariant | Declared owners cascade/rebind; telemetry retention remains explicit and is not mislabeled orphan data. |
| reproducer | One delete or rebind fixture selected in Task 7. |
| root_cause | Unknown. |
| repair | Discovery only; reproduced defects require a reviewed addendum. |
| lint_coverage | Owner-integrity checks only where retention contract is deterministic. |
| cleanup_class | Unclassified. |
| verification | Not run. |
| follow_up_direction | Task 7 disposition. |
| status | `candidate` |

## B4: Checkpoint Ordering and Retry Convergence

| Field | Evidence |
|---|---|
| issue_id | `B4` |
| scenario | Enrichment or source-sync checkpoint reaches terminal state before its owned artifacts are durable. |
| observed_live_exposure | Pending probe and reproducer. |
| code_evidence | Pending Task 7 bounded path read. |
| invariant | Terminal receipt implies required writes exist; retry is idempotent and resumes from a valid checkpoint. |
| reproducer | One queue/checkpoint abort fixture selected in Task 7. |
| root_cause | Unknown. |
| repair | Discovery only; reproduced defects require a reviewed addendum. |
| lint_coverage | Queue/runtime completeness checks first. |
| cleanup_class | Unclassified. |
| verification | Not run. |
| follow_up_direction | Task 7 disposition. |
| status | `candidate` |

## B5: Source Page Replacement Failure

| Field | Evidence |
|---|---|
| issue_id | `B5` |
| scenario | Source Page replacement deletes the old projection before the new projection is durable. |
| observed_live_exposure | Pending probe and reproducer. |
| code_evidence | Pending Task 7 bounded path read. |
| invariant | A failed replacement preserves the last valid Page and provenance or reports an incomplete projection. |
| reproducer | One projection-write fault selected in Task 7. |
| root_cause | Unknown. |
| repair | Discovery only; reproduced defects require a reviewed addendum. |
| lint_coverage | Page DB/filesystem/state receipt agreement first. |
| cleanup_class | Unclassified. |
| verification | Not run. |
| follow_up_direction | Task 7 disposition. |
| status | `candidate` |

## B6: Concurrent Projection and Capture Writes

| Field | Evidence |
|---|---|
| issue_id | `B6` |
| scenario | Concurrent Page projection writes or identical captures race around version/dedup ownership. |
| observed_live_exposure | Pending probe and reproducer. |
| code_evidence | Pending Task 7 bounded path read. |
| invariant | CAS/dedup selects one deterministic survivor and no valid write is silently lost. |
| reproducer | Tokio barrier fixture selected in Task 7; sleeps are forbidden. |
| root_cause | Unknown. |
| repair | Discovery only; reproduced defects require a reviewed addendum. |
| lint_coverage | Stable duplicate/version drift only; concurrency property remains test-owned. |
| cleanup_class | Unclassified. |
| verification | Not run. |
| follow_up_direction | Task 7 disposition. |
| status | `candidate` |

## B7: Legacy Provenance and Deleted Link Targets

| Field | Evidence |
|---|---|
| issue_id | `B7` |
| scenario | Legacy Page provenance or a deleted wikilink target leaves stale locators/resolution state. |
| observed_live_exposure | Pending probe and reproducer. |
| code_evidence | Pending Task 7 bounded path read. |
| invariant | Deleted targets become explicit unresolved links; valid provenance locator kinds remain distinguishable. |
| reproducer | Legacy provenance and target-delete fixture selected in Task 7. |
| root_cause | Unknown. |
| repair | Discovery only; reproduced defects require a reviewed addendum. |
| lint_coverage | Existing Pages/provenance and broken-link checks first. |
| cleanup_class | Unclassified. |
| verification | Not run. |
| follow_up_direction | Task 7 disposition. |
| status | `candidate` |
