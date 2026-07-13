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

The read-only real-store probe ran at commit
`5dad852e621b1cb39ee7074fdd8ded27142caf5e`. Its manifest is stored outside the
worktree at
`$REPO_DATA_ROOT/wenlan/lint-maintenance/5dad852e621b1cb39ee7074fdd8ded27142caf5e/20260713T205217Z-29318-23317/manifest.json`.
The tree was clean. Database before/after SHA-256 was
`fd764fbbed662b8dec9fffe218de7dd4a90acf4cc932968024f6252a3078cf4e`;
Page before/snapshot/after SHA-256 was
`84ac4b66441d964dde0792ea404f794c70f185ce3191ed87fd84176c732f1e4a`.
General exited 1 and Deep exited 2. The run is not accepted for aggregate
conclusions: `complete=false`, `reason=count_oracle_mismatch`. External SQLite
reported `COUNT(*)=0` for the vector-indexed `pages` table while row
enumeration and `COUNT(id)` reported 191. The raw aggregate counters remain
artifacts only and are not evidence for cleanup.

The branch binaries were then run against the frozen DB/Page copies under an
isolated HOME, data directory, config, and ephemeral daemon port. General was
complete with 55 checks, 48 passes, and 7 actionable finding rows; Deep without
a judge was incomplete with 73 checks, 51 passes, 13 finding rows, and 9
incomplete rows. API and CLI normalized contracts were byte-equal after
excluding durations. Six valid SQLite backups taken around CLI General, CLI
Deep, and agent submission all had SHA-256
`b3efb38f26f128e1f95d461e1881fbc75f9b07596ba357a6671b48b45ce0ca2a`.
The copied Page tree retained SHA-256
`84ac4b66441d964dde0792ea404f794c70f185ce3191ed87fd84176c732f1e4a`.

Agent-assisted Deep prepared 67 bounded records and 40 candidates in 84.63
seconds. This Codex session submitted exactly one typed verdict per candidate;
the daemon regenerated the work, accepted the digest, and returned in 83.83
seconds with 14 finding rows and 8 incomplete rows. The one untruncated
semantic family surfaced four Pages with inadequate provenance. Truncated
families remained incomplete, as required, but currently discard known judged
findings from their evidence and `affected_records`; see `D1` below.

## A1: Page Source Locator Ownership

| Field | Evidence |
|---|---|
| issue_id | `A1` |
| scenario | Cleanup evaluates Page provenance that names a memory by logical `source_id` or internal row `id`. |
| observed_live_exposure | Canonical branch lint on the frozen copy reported zero affected source-to-evidence coverage rows. The external aggregate run was invalidated by its count oracle, so no historical orphan owner is claimed. |
| code_evidence | Pre-fix `cleanup_orphaned_page_sources` validated only `memories.source_id`; the canonical writer accepts both logical and internal memory identities. |
| invariant | Either authorized memory identity preserves `page_sources` and memory `page_evidence`; only a missing owner is removable. |
| reproducer | Focused cleanup fixture for logical id, row id, and missing owner. |
| root_cause | Confirmed: both cleanup predicates recognized only logical `memories.source_id`, so a valid internal row-id locator was deleted. |
| repair | Both dual-write cleanup predicates now use correlated `NOT EXISTS` and preserve a non-episode memory matching either `source_id` or `id` in the existing transaction. |
| lint_coverage | Existing `pages.provenance.source_evidence_coverage` now computes non-episode owner presence using both logical `memories.source_id` and internal `memories.id`; a matching evidence kind cannot mask a missing owner. No new check id or runner surface was added. |
| cleanup_class | `do_not_touch`; no accepted finding identifies a removable locator. |
| verification | RED removed 2 rows instead of 1. GREEN: focused cleanup locator fixture 1/1, orphan-cleanup group 3/3, missing-owner lint RED/GREEN 1/1, Page provenance 6/6, whole lint 191/191 with bounded test concurrency, `wenlan-types` 102/102, core/types all-target Clippy clean. |
| follow_up_direction | Task 8 missing-owner lint coverage; Task 9 measures live residue without mutation. |
| status | `fixed` |

## A2: Entity Merge and Delete References

| Field | Evidence |
|---|---|
| issue_id | `A2` |
| scenario | Merge or delete an entity referenced by `memory_entities`, legacy memory ownership, aliases, relations, observations, or Pages. |
| observed_live_exposure | No accepted canonical finding exposed a dangling Page/entity owner. The external aggregate value is unusable because the probe was incomplete. |
| code_evidence | Pre-fix `merge_entities` and `delete_entity` did not handle Page and canonical junction ownership as one transaction. |
| invariant | Merge transfers every surviving reference without duplicates; delete nulls nullable owners and rolls back all statements on failure. |
| reproducer | Junction collision, Page owner, and abort-trigger rollback fixtures. |
| root_cause | Confirmed: merge omitted `memory_entities` and `pages.entity_id`; delete omitted Page ownership and ran memory/alias/entity statements without one transaction. |
| repair | Merge now transfers canonical junction links with `INSERT OR IGNORE`, removes loser links, and re-points Pages inside its existing transaction. Delete now nulls memory/Page owners and deletes aliases/entity in one rollback-safe transaction; declared FK cascades remove junction/graph children. |
| lint_coverage | `memory_entities.integrity` covers junction owners. Task 8 did not add a Page-entity catalog check because no accepted live probe has reproduced `pages_dangling_entity > 0`; the read-only aggregate remains and Task 9 will revisit if exposure is nonzero. |
| cleanup_class | `do_not_touch` until a canonical owner finding exists. |
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
| observed_live_exposure | The external multi-chunk aggregate is unusable because the probe was incomplete. Multi-chunk storage is an expected shape and is not cleanup evidence. |
| code_evidence | RED observed a content edit leave 7 chunks under one logical memory, and an HTTP request combining content/confirm with an invalid taxonomy returned 200 after sequential mutation. |
| invariant | One validated request updates primary content/metadata, removes stale secondary chunks, replaces derived children, and synchronizes episodes in one transaction while preserving untouched metadata. |
| reproducer | Invalid multi-field HTTP request; 7-chunk edit; exact head-metadata snapshot; FTS sentinel; content-backed and source-text-backed episodes; feature-off/word-gate deletion; child-delete abort trigger followed by same-connection retry. |
| root_cause | Confirmed: the server owned a sequence of independent mutations, while core content editing updated only chunk zero and rebuilt child vectors in a later transaction. Taxonomy was not validated at the update boundary. |
| repair | The route now validates `MemoryType`, resolves registered-space fallback, and calls one `post_write::update_memory` capability. One DB primitive prepares embeddings before `BEGIN`, updates the head in place, deletes stale chunks, replaces/deletes children, synchronizes episodes, applies requested metadata/confirmation, and rolls back every mutation on failure. |
| lint_coverage | Lifecycle integrity may detect stable stale-row shapes; transaction atomicity remains test-only. |
| cleanup_class | `do_not_touch` without a durable stale-child owner finding. |
| verification | RED: stale chunk population was 7 instead of 1; invalid multi-field request returned 200. GREEN: core update group 6/6, source-text episode 1/1, derived deletion 1/1, server update group 2/2, unknown-space fallback 1/1, core/server all-target Clippy clean. |
| follow_up_direction | Use the read-only real-store probe to classify any historical stale secondary chunks; do not infer cleanup ownership from the transaction test alone. |
| status | `fixed`; live historical exposure remains unclassified. |

## A5: Scope-Safe Page Growth and Wikilinks

| Field | Evidence |
|---|---|
| issue_id | `A5` |
| scenario | Automatic enrichment grows a Page or resolves a wikilink when equivalent titles/entities exist across spaces. |
| observed_live_exposure | Frozen-copy General reported 3 duplicate active-title rows and 73 orphan labels. Agent review rejected all six sampled existing Page-evidence removals because each excerpt directly supported the Page claim; truncation prevents a population-wide clean conclusion. |
| code_evidence | RED fixtures proved that Page growth could select an equivalent Page from another scope, ignored an entity linked during the current enrichment run, resolved duplicate titles by arbitrary global row order, and repaired every orphan sharing a label. |
| invariant | Automatic matching is deterministic within the source scope; same-scope ambiguity remains unresolved; intentional cross-space links are preserved. |
| reproducer | Duplicate cross-space titles/entities, same-scope ambiguity, and source-specific orphan-link fixtures. |
| root_cause | Confirmed: Page growth passed the pre-enrichment entity to a global matcher; entity-first matching fetched one global Page before checking scope; wikilink resolution omitted the source Page scope; orphan repair grouped updates by `label_key` alone. |
| repair | Re-read the final memory entity and scope before growth; query entity and embedding candidates within that scope; resolve titles only when exactly one active same-scope Page exists; repair orphan rows by `(source_page_id, label_key)` while leaving explicit targets untouched. |
| lint_coverage | Add only a deterministic wrong-scope detector with complete population; semantic relatedness remains Deep review. |
| cleanup_class | Duplicate titles and orphan labels are `needs_semantic_review`; sampled valid Page evidence is `do_not_touch`. |
| verification | RED: `cargo test -p wenlan-core --lib page_growth_ -- --nocapture` failed 2/2 and `cargo test -p wenlan-core --lib wikilink_ -- --nocapture` failed the three automatic-resolution cases. GREEN: Page growth 2/2, direct growth 3/3, wikilink 21/21, scoped matcher 3/3, and all post-ingest tests 17/17. |
| follow_up_direction | Use the read-only real-store probe to inventory wrong-scope existing links as bounded semantic-review candidates; do not infer that a same-title cross-space link is wrong without evidence. |
| status | `fixed`; live historical exposure remains unclassified. |

## B1: KG and Dual-Pool Partial Commit

| Field | Evidence |
|---|---|
| issue_id | `B1` |
| scenario | Relation/observation or dual-pool resolution fails between owned writes. |
| observed_live_exposure | Frozen-copy General reported 11 supersession-integrity rows. They are candidates only; shape does not determine which memory, if any, should supersede another. |
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
| observed_live_exposure | Frozen-copy lint reported 3 duplicate active-title rows and 4 untruncated semantic Page-provenance findings. No Page revision/card mismatch was independently proven. |
| code_evidence | Page version-CAS update commits before a separate pending-card consumption UPDATE; projection follows both. |
| invariant | CAS/version state and projection receipts converge or report incomplete; consumed work is not lost. |
| reproducer | `accept_page_revision_consume_failure_keeps_page_retryable` aborts card consumption after the Page CAS; `accept_page_revision_source_failure_keeps_page_retryable` aborts a required source attachment inside the same write. |
| root_cause | Page mutation and work-item consumption had separate transaction boundaries, while required `page_sources` INSERT errors were swallowed. |
| repair | Page content/version/changelog, exact source/evidence reconcile, and pending-card consumption now share the existing Page update transaction. Required source INSERT, card consume, and CAS failures roll back the whole acceptance; legacy unversioned cards use the same path. |
| lint_coverage | Pages/projections state and non-atomic snapshot contracts first. |
| cleanup_class | Existing Page/card mismatches need version/content comparison before any proposal; default `needs_semantic_review`. |
| verification | RED: consume failure committed proposed Page content while leaving the card pending; source attachment failure was swallowed and consumed the card. GREEN: both transaction-fault tests 2/2; accept/retry/conflict/legacy/not-found group 9/9; core all-target Clippy clean. |
| follow_up_direction | Probe historical Page/card/projection mismatches read-only; archive and watcher boundaries remain evidence-gated rather than assumed fixed. |
| status | `fixed` for Page revision acceptance; remaining B2 boundary exposure is unclassified. |

## B3: Delete, Episode, and Source-ID Rebinding

| Field | Evidence |
|---|---|
| issue_id | `B3` |
| scenario | Ordinary/time-range delete or logical source-id rebinding leaves owned children, episodes, or provenance. |
| observed_live_exposure | No accepted canonical finding identified an old/new source-id split owner or missing episode parent. |
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
| observed_live_exposure | Frozen-copy Deep reported 2 source lifecycle residue rows. The scratch source-configuration finding is environmental because the isolated config deliberately omitted live sources. |
| code_evidence | Document enrichment calls `mark_done` before best-effort `record_sync_state`; same-hash enqueue does not reopen a done item. |
| invariant | Terminal receipt implies required writes exist; retry is idempotent and resumes from a valid checkpoint. |
| reproducer | `sync_receipt_failure_does_not_leave_same_hash_queue_terminal` aborts the receipt INSERT, removes the trigger, and re-enqueues the same hash. |
| root_cause | Terminal queue state is committed before its required receipt, and receipt failure is swallowed. |
| repair | Source-sync receipt upsert and queue completion now commit in one DB transaction. Any receipt, queue update, or commit failure rolls back and explicitly pauses the row for retry; outcome reports `paused=true`. |
| lint_coverage | Queue/runtime completeness checks first. |
| cleanup_class | Missing receipt with done queue is `environment_or_config` or retryable state; data cleanup waits for live source ownership. |
| verification | RED: `cargo test -p wenlan-core --lib sync_receipt_failure_does_not_leave_same_hash_queue_terminal -- --nocapture` observed `done` with no receipt and no same-hash recovery. GREEN: receipt fault 1/1; no-provider and provider completion 2/2; folder-ingest terminal/rescan/delete lifecycle 1/1; core all-target Clippy clean. |
| follow_up_direction | Inspect live queue/receipt mismatches read-only; historical rows remain cleanup candidates, not automatic repairs. |
| status | `fixed`; live historical exposure remains unclassified. |

## B5: Source Page Replacement Failure

| Field | Evidence |
|---|---|
| issue_id | `B5` |
| scenario | Source Page replacement deletes the old projection before the new projection is durable. |
| observed_live_exposure | No accepted canonical finding proves a lost source Page or provenance row from this failure mode. |
| code_evidence | `write_source_page` deletes the deterministic Page before a separate PageWrite create; Page source/evidence rows cascade on delete. |
| invariant | A failed replacement preserves the last valid Page and provenance or reports an incomplete projection. |
| reproducer | `source_page_replacement_failure_preserves_last_valid_page_and_provenance` aborts replacement Page insertion after an existing source Page is valid. |
| root_cause | Delete-then-create spans two commits; failed create has no last-known-good row to restore. |
| repair | Existing machine-owned source Pages now use a canonical PageWrite replacement variant backed by one in-place DB transaction for Page metadata, source ids, and typed evidence; creation remains the existing create path. |
| lint_coverage | Page DB/filesystem/state receipt agreement first. |
| cleanup_class | Lost historical Page prose/provenance is not reconstructable from shape alone; default `needs_semantic_review` or external-source replay. |
| verification | RED: `cargo test -p wenlan-core --lib source_page_replacement_failure_preserves_last_valid_page_and_provenance -- --nocapture` left the Page absent after insertion abort. GREEN: failure-preservation 1/1; source-Page creation and multi-chunk enrichment 2/2; folder-ingest replacement lifecycle 1/1; core all-target Clippy clean. |
| follow_up_direction | Use external-source replay only for live Pages already lost before this fix; do not fabricate provenance. |
| status | `fixed`; live historical exposure remains unclassified. |

## B6: Concurrent Projection and Capture Writes

| Field | Evidence |
|---|---|
| issue_id | `B6` |
| scenario | Concurrent Page projection writes or identical captures race around version/dedup ownership. |
| observed_live_exposure | No barrier-backed live or synthetic race was executed; ordinary duplicate inventory is not proof of a concurrent-write defect. |
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
| observed_live_exposure | Frozen-copy General reported 73 explicit orphan labels. Source-evidence coverage passed; the four semantic provenance findings are Pages with no typed evidence at all, not wrong-kind source locators. |
| code_evidence | `page_links.target_page_id` intentionally has no FK; `delete_page` deletes only the target row and orphan repair scans only NULL targets. |
| invariant | Deleted targets become explicit unresolved links; valid provenance locator kinds remain distinguishable. |
| reproducer | `page_links_target_delete_becomes_orphan_and_reresolves` deletes a resolved target, recreates its title under a new ID, and runs orphan repair. |
| root_cause | Target deletion does not null inbound link targets, so stale non-NULL IDs never re-enter orphan resolution. |
| repair | `delete_page` now nulls inbound targets and deletes the Page in one transaction, with rollback on either write or COMMIT failure. |
| lint_coverage | Existing Pages/provenance and broken-link checks first. |
| cleanup_class | A target ID absent from Pages can be proposed as `deterministic_safe` orphaning; semantic rebinding remains separate. |
| verification | RED: `cargo test -p wenlan-core --lib page_links_target_delete_becomes_orphan_and_reresolves -- --nocapture` retained the deleted target ID. GREEN: Page-link group 7/7, wikilink group 21/21, and core all-target Clippy. |
| follow_up_direction | Stable real-store rows targeting absent Page IDs can now be proposed for deterministic orphaning; semantic rebinding remains separate. |
| status | `fixed`; live historical exposure remains unclassified. |

## B8: Revision Chain Duplicates Derived and Secondary Rows

| Field | Evidence |
|---|---|
| issue_id | `B8` |
| scenario | Revision history is read for a superseding memory that also has a content-backed episode or secondary chunks. |
| observed_live_exposure | Not claimed. The defect was exposed by the exact workspace library gate after episode-channel tests exercised the legal shared-`source_id` shape; isolated chain tests without that shape had passed. |
| code_evidence | `walk_supersede_chain` excluded episodes and secondary chunks only at the recursive CTE anchor. Its recursive member joined every chunk, and its final projection joined both canonical memory and episode rows sharing a `source_id`. |
| invariant | Each revision depth contains exactly one canonical non-episode `chunk_index=0` row, independent of derived retrieval channels or document chunk count. |
| reproducer | `walk_chain_ignores_derived_and_secondary_rows` seeds a two-entry chain plus one secondary chunk and two same-owner episode rows. |
| root_cause | Missing canonical-row predicates in the recursive member and final join multiplied one two-entry chain into six result rows. |
| repair | Both joins now require `source != 'episode'` and `chunk_index = 0`; the existing anchor and public response contract are unchanged. |
| lint_coverage | None. This is a read-path correctness invariant with a deterministic product test, not stored-data cleanup evidence. |
| cleanup_class | `do_not_touch`; episode rows and secondary chunks are valid storage shapes. |
| verification | RED: expected 2 canonical IDs, received 6. GREEN: all `walk_chain_` tests 6/6. Exact `cargo test --workspace --lib`: 2,699 passed, 0 failed, 26 ignored (CLI 22, core 2,238, MCP 184, server 156, types 99). One preceding full run hit the unchanged sparse-scale 2-second wall-clock assertion at 2.69 seconds; four isolated reruns passed in 0.86-1.02 seconds and the exact full rerun passed without changing the threshold. |
| follow_up_direction | Keep legal derived/channel rows out of canonical lifecycle traversals by query predicate, never by deleting them. |
| status | `fixed`; live historical exposure remains unproven. |

## B9: Missing Memory Update Reports Success

| Field | Evidence |
|---|---|
| issue_id | `B9` |
| scenario | A client submits a non-empty update for a logical memory that does not exist. |
| observed_live_exposure | Not claimed. The final branch review found the silent-success path by comparing the atomic writer with peer typed write capabilities. |
| code_evidence | `apply_memory_update` returned `Ok(())` when its canonical-head pre-read found no row, so the daemon converted a missing target into HTTP 200. |
| invariant | A requested mutation must either identify one canonical head or fail with a typed not-found response; an empty update remains a no-op. |
| reproducer | Core missing-head update plus `PUT /api/memory/missing-update-head/update` with a content change. |
| root_cause | The pre-read missing branch retained legacy no-op semantics after the route was consolidated behind the atomic writer. |
| repair | The missing branch now returns `WenlanError::NotFound`, which the existing daemon error mapping renders as HTTP 404. The transaction-time affected-row conflict remains unchanged. |
| lint_coverage | None. This is synchronous command feedback, not stored-data residue. |
| cleanup_class | `do_not_touch`; no row exists to repair. |
| verification | RED: core returned `Ok(())` and HTTP returned 200. GREEN: core 1/1 and HTTP 1/1 return the typed not-found behavior. |
| follow_up_direction | Keep missing-target semantics consistent across typed write capabilities; no new endpoint or response type is needed. |
| status | `fixed`. |

## D1: Truncated Agent Findings Are Hidden

| Field | Evidence |
|---|---|
| issue_id | `D1` |
| scenario | Calling-agent Deep judges every bounded packet candidate, including known findings, while the candidate population is larger than the packet. |
| observed_live_exposure | The isolated dogfood submission judged 40/40 packet candidates. Six classification findings and one relation finding occurred in truncated families. |
| code_evidence | `semantic_result` checks `population.truncated()` before converting adjudicated verdicts into findings. The resulting rows are `failed_to_run`, report `affected_records=0`, and retain only `semantic_population_incomplete`. |
| invariant | Truncation must keep the check and report incomplete, but known judged findings must remain visible as partial evidence and must not be rendered clean. |
| reproducer | The frozen-copy prepare/submit flow: work digest `05264be432fe41b6`; final report records six judged candidates in each truncated family but zero affected rows. Existing unit test `candidate_truncation_is_incomplete_not_clean` codifies only incompleteness, not finding preservation. |
| root_cause | The current report contract derives top-level completeness solely from outcome and offers no dual state for `finding` plus incomplete coverage; the semantic runner resolves the conflict by discarding findings. |
| repair | Deferred contract task: preserve typed judged findings and affected counts on an incomplete row, define totals/rendering semantics, and prioritize missing/disagreeing second-judge reasons without weakening truncation. |
| lint_coverage | RED acceptance must prove known findings survive, top-level `complete=false`, CLI exit remains 2, and no high-risk finding bypasses second-judge policy. |
| cleanup_class | `environment_or_config`; this is diagnostic-report loss, not stored-data cleanup. |
| verification | Reproduced through the branch CLI prepare/submission path with unchanged DB/Page fingerprints. |
| follow_up_direction | Reconcile `agent-assisted-lint-design.md` Outcomes with the versioned report contract before implementation; do not add a second report type. |
| status | `deferred` |

## D2: Deep Has No Progress Surface

| Field | Evidence |
|---|---|
| issue_id | `D2` |
| scenario | A user runs Deep or agent-assisted Deep against a medium real store. |
| observed_live_exposure | Plain Deep took 84.37 seconds, prepare took 84.63 seconds, and submit took 83.83 seconds on the isolated 5,736-row snapshot. CLI emitted no progress before the final report. |
| code_evidence | Deep performs deterministic full enumeration and local candidate generation before returning one canonical response. |
| invariant | Deep may be expensive, but users need bounded progress/cancellation and must understand that Deep without a selected judge is diagnosis preparation, not completed semantic review. |
| reproducer | Isolated API/CLI dogfood timings recorded above. |
| root_cause | The canonical endpoint is request/response only; candidate-generation stages have no typed progress stream. |
| repair | Deferred UX task: expose stage/progress through the existing daemon event surface or UI polling without introducing a second runner. Preserve one final canonical report. |
| lint_coverage | Add cancellation/deadline and stage-order tests; latency itself remains a measurement, not a hard CI threshold. |
| cleanup_class | `environment_or_config`. |
| verification | Three independent Deep-family calls took 83.83-84.63 seconds and returned valid reports. |
| follow_up_direction | Design after this campaign; provider-slot CLI remains a separate already-deferred concern. |
| status | `deferred` |
