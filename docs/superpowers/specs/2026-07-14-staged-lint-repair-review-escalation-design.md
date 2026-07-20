# Staged Lint Repair and Review Escalation Design

**Status:** Design direction approved on 2026-07-14; `review2` findings
incorporated; implementation has not started.

## Decision

Make lint the sole owner of repair judgment. Every final semantic finding
carries one typed resolution: either an exact repair proposal or a reasoned
Review Item disposition. `/lint repair` consumes that resolution; it does not
ask an agent to infer the mutation or escalation a second time.

Resolution follows a staged policy:

1. prepare an exact repair when a registered deterministic resolver can do so;
2. let Deep semantic adjudication attempt an exact typed repair when judgment
   is required;
3. if a complete final finding still has no safe exact repair, persist one
   typed Review Item in Wenlan's existing refinement queue.

Canonical product data remains unchanged until the user approves the exact
immutable repair manifest. This spec supersedes
`2026-07-14-versioned-lint-repair-manifest-design.md` in four named areas:
repair inference and the public skill surface; manifest schema/source and
frozen-v1 decoding; prepare identity/idempotency; and Review Item verification
reconciliation. All other manifest, rollback, compare-and-swap, effect-proof,
and post-repair assertions remain authoritative. Where the two documents
conflict in those four areas, this document wins.

## User Outcome

The public skill surface becomes:

```text
/lint [scope]          # fast, read-only General report
/lint deep [scope]     # complete, read-only Deep report
/lint repair [scope]   # prepare repairs or Review Items; no canonical write
```

There is no public `profile:general`, `profile:deep`, `agent`, or separate
`/lint-repair` command. The current calling-agent adjudication protocol remains
an internal implementation detail. Invoking `/lint deep` or `/lint repair`
consents to the bounded candidate packet being judged by the current agent;
the skill must not silently switch provider routes after the run starts.

`/lint repair` returns three bounded result groups:

```text
repair_proposal?       # zero or one exact mutation plus manifest id/digest
review_items[]         # open semantic work requiring a user choice
terminal_review_items[] # still-present work terminal in a prior run
```

At most one repair manifest is prepared per invocation. Review Item emission
is bounded by `LINT_AGENT_CANDIDATE_CAP` (currently 48) and deduplicated across
repeated runs.

## Read-Only Boundary

`GET /api/lint`, `POST /api/lint`, the core lint runner, and plain `/lint` and
`/lint deep` remain completely read-only. They may report an exact proposed
repair but cannot create manifests, queue rows, or canonical writes.

`/lint repair` is a skill orchestration mode, not a `wenlan lint --fix` flag or
a mutating lint endpoint. It may:

- create immutable repair artifacts through `/api/repairs/prepare`; and
- create or reuse Review Items through the typed local-only
  `POST /api/refinery/queue/lint-reviews` endpoint and matching
  `enqueue_lint_reviews` MCP tool.

It may not change memories, Pages, graph rows, projections, or any other
canonical knowledge owner before exact manifest approval.

## Repairability Contract

### Total lint-owned resolution

Add one required typed resolution to every final semantic finding:

```text
LintSemanticResolution::Repair {
    proposal: LintProposedRepair::ReclassifyMemory {
        after_memory_type: MemoryType
    }
}

LintSemanticResolution::Review {
    blocked_reason: LintReviewBlockedReason,
    suggested_research_queries: Vec<String>,
    query_rejections: Vec<LintQueryRejectionV1>
}
```

The agent-submission form carries `blocked_reason` and proposed research
queries but has no `query_rejections` field. The daemon validates each proposal
and constructs the final resolution above; callers cannot supply rejection
metadata or reinsert a rejected raw query.

This slice enables only this existing writer. The proposal must match the finding's
`ReclassifyMemory` action, name a valid `MemoryType`, and differ from the
candidate's current type. A proposal for another action or a no-op value fails
the lint submission contract.

The bounded Deep judge returns its proposed resolution with the verdict. The
daemon validates and copies it into the final `LintSemanticFinding`; callers
cannot attach or replace it afterward. The core may force `Review` with
`unsupported_writer` when no registered typed writer exists, but it cannot
invent an exact repair value.

Report schema v5 also adds a daemon-computed `candidate_receipt` to every final
semantic finding. It hashes only the canonical `LintAgentCandidate` plus the
referenced `LintAgentRecord` values, excluding verdict fields, unrelated
candidates, and whole DB/Page snapshot receipts. Calling agents cannot submit
or replace it. It gives every finding a content-sensitive occurrence receipt
without inheriting unrelated database churn.

Every final v5 report also carries a daemon-authenticated
`LintReportReceiptV1 { daemon_instance_id, sequence, report_digest, mac }`.
The daemon assigns a monotonically increasing sequence and authenticates the
canonical final report payload excluding the receipt itself with keyed BLAKE3
and a process-local random 256-bit key that is never returned. The signed
material is the schema tag, daemon instance id, sequence, and report digest.
The receipt is valid only for the current daemon instance. It proves that the
General or Deep report passed the canonical daemon finalization path; it does
not make calling-agent semantic judgment trusted, and it is not approval to
mutate data. The local-agent trust boundary remains localhost/stdio, while the
exact manifest approval is the authorization boundary for canonical writes.

A finding without exactly one valid resolution is an invalid or incomplete
semantic submission, not a queueable Review Item. This prevents `/lint repair`
from inventing the blocked reason or research queries after lint.

This change bumps `LINT_REPORT_SCHEMA_VERSION` from 4 to 5 and
`LINT_AGENT_WORK_SCHEMA_VERSION` from 2 to 3. A same-task report is reusable by
`/lint repair` only when both versions are current; older reports are rerun,
never coerced into the new resolution shape. Missing or unsupported versions
return `lint_contract_upgrade_required` without creating a manifest or queue
row.

The required resolution is also a persisted-repair compatibility boundary.
Newly prepared manifests and newly emitted apply/verification receipts use
schema v2. The implementation must retain a closed frozen copy of the complete
transitive v1 graph for **every persisted v1 repair artifact** in a dedicated
module: manifest, rollback payload, apply receipt, and verification receipt.
At minimum the manifest graph includes the manifest/draft, source, lint
scope/report scope, target/scope, expected state, writer, mutation, allowed
effects, rollback artifact, post assertions, check baselines, snapshots,
producer receipt, `LintEvidenceRef`, and the v4 semantic finding reachable both
from `RepairSource.finding` and from
`post_assertions.*_baseline[].evidence`. Receipt graphs also freeze their writer,
allowed-effects, target-receipt, and lint-snapshot leaves.

No `Frozen*V1` field may use any live application, lint, repair, or domain type;
every serde-reachable non-primitive leaf is a frozen enum/struct owned by the v1
module. Legacy conversion to current domain values happens only after the
stored bytes and digest verify, through an explicit fallible mapping that keeps
removed/renamed variants readable. `RepairSource` does not embed a full
`LintReport`, but its baseline evidence still makes the transitive freeze
necessary.

This boundary has mechanical teeth. Keep all frozen definitions in one
`frozen_v1` module with no glob imports or type aliases to live modules. A
`syn` or ast-grep syntax-aware architecture test parses every `Frozen*V1` field
type and fails CI
unless the leaf is a primitive/container or resolves inside `frozen_v1`; it also
rejects `use crate::...` and fully qualified live-type paths in that module.
Golden round trips then cover values, while the architecture test covers type
reachability. Neither test may be updated merely to make a live-type refactor
green.

Read `manifest_schema_version` through a minimal untyped envelope before typed
deserialization, then select the concrete decoder. A v1 manifest's canonical
bytes and digest are never rebuilt through v2 types. Existing v1 manifests
therefore remain readable, applicable, and verifiable byte-for-byte; only v2
manifests can contain a resolution or a Review Item origin. Golden fixtures
must cover a v1 source finding, a v1 post-assertion baseline whose evidence
contains a semantic finding, rollback bytes, an apply receipt, and a verification
receipt. CI compares their exact stored bytes and digests; fixture regeneration
or removal/rename of any reachable enum variant is forbidden without an
explicit legacy-format migration decision.

Represent loaded artifacts as
`StoredRepairManifest::V1(FrozenRepairManifestV1) | V2(RepairManifestV2)`.
Apply and verification receipts use matching
`StoredRepairApplyReceipt::{V1,V2}` and
`StoredRepairVerificationReceipt::{V1,V2}` envelopes.
Expose the current apply/verify fields through shared read-only accessors, but
dispatch canonical serialization and digest verification to the concrete
version. Do not deserialize v1 and then up-convert it before verifying its
digest.

### Prepare consumes; it does not infer

The prepare request no longer accepts an independently supplied
`after_memory_type` for a finding-owned repair. Its source is a tagged union:

```text
PrepareRepairSourceV2::Finding {
    lint_scope, general_report, deep_report, selected_finding
}

PrepareRepairSourceV2::ReviewDecision {
    review_item_id,
    occurrence_digest,
    decision: LintReviewDecisionV1,
    lint_scope,
    general_report,
    deep_report
}

LintReviewDecisionV1::ReclassifyMemory {
    after_memory_type: MemoryType
}
```

The finding variant accepts only a finding whose resolution is `Repair` and
derives the mutation from that proposal. The Review Item variant accepts only
the explicit typed value missing from that item's resolution. Both variants
revalidate:

- complete General and Deep reports with identical durable scope;
- current authenticated report, producer, work, DB, and Page receipts;
- one unambiguous durable target;
- action/proposal/writer agreement; and
- current target state differing from the proposed after-state.

Review resolutions return `repair_resolution_requires_review`. Invalid or
mismatched repair proposals return `repair_proposal_invalid`; the `/lint
repair` mode may not fill the gap itself.

V2 prepare is idempotent across callers. Derive a preparation key from the
source kind/id, current report and producer receipts, exact target and expected
canonical receipt, writer, and mutation, excluding wall-clock time. Derive the
manifest id with UUIDv5 over the canonical preparation-key bytes and the fixed
namespace UUID `3a26c259-f3c8-5c28-9f67-61f719f6016e`, so it retains the
existing `repair_<uuid>` format while its version nibble cannot collide with
legacy UUIDv4 ids. Persist the full preparation key inside manifest v2 and pin
the fixed key-to-id vector in a cross-implementation fixture. Concurrent
identical prepares first serialize on a daemon-process keyed
`tokio::sync::Mutex`, then on a root-level `fs2` advisory file lock keyed by the
same id for cross-process exclusion. The blocking file-lock and synchronous
artifact critical section run inside `spawn_blocking`; lock order is always
async keyed mutex then file lock. After both locks are held, prepare checks the
final manifest directory before building a draft: if present, load it, verify
its digest and preparation key, and return that persisted manifest including
its original `prepared_at`; never compute or persist the loser's draft. If
absent, remove only validated same-id stale `.tmp-*` directories while holding
both locks, then build and publish once. Release the file lock before the keyed
mutex and remove unused keyed entries after the last waiter.
Different target receipts or mutations produce different ids. CAS remains the
apply-time stale-state gate.

The UUID input is a hand-written binary encoder, never derived `Serialize` or
JSON. `RepairPreparationKeyV1` has this exact logical field order:

```text
source = Finding { candidate_receipt[32] }
       | ReviewDecision { review_item_id, occurrence_digest[32], decision_digest[32] }
lint_scope_digest[32]
general_report_digest[32]
deep_report_digest[32]
general_producer_digest[32]
deep_producer_digest[32]
target_kind
target_id
target_scope_digest[32]
expected_version: u64
expected_canonical_receipt[32]
writer
before_memory_type
after_memory_type
```

Encoding starts with ASCII `wenlan.repair.prepare.v1\0`. Variant tags are one
byte (`Finding=0x01`, `ReviewDecision=0x02`, `Memory target=0x01`,
`ReclassifyMemory writer=0x01`). Digests are their raw 32 bytes, `u64` and string
lengths are unsigned big-endian (`u64` and `u32` respectively), and strings are
length-prefixed UTF-8. Fields are emitted in the order above; there are no maps,
floats, optional fields, serde attributes, or omitted values. Feed those bytes
directly to `Uuid::new_v5(fixed_namespace, bytes)`.

The same canonical bytes are persisted with the manifest. A golden vector pins
the complete input fields, exact base64 bytes, SHA-256 digest, namespace, and
expected UUID; property tests cover both source variants, non-ASCII strings,
empty strings, and every one-byte enum tag. Only the daemon derives this id;
skills never independently reimplement the encoding.

Pinned vector 1 uses `Finding`, zero bytes for every digest, target id `mem_1`,
expected version `7`, before type `note`, and after type `preference`:

```text
canonical_length = 323
canonical_base64 = d2VubGFuLnJlcGFpci5wcmVwYXJlLnYxAAEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAABW1lbV8xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABwAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAQAAAARub3RlAAAACnByZWZlcmVuY2U=
canonical_sha256 = c2d892e0bd13bb2b654b7c88f2c4a0e8df9aa3bcffa40dfd68ffeb49b320e16f
namespace = 3a26c259-f3c8-5c28-9f67-61f719f6016e
uuid_v5 = fa845ad4-bdd4-5dbc-ad05-b9ed16c61d28
```

### Registered writers remain narrow

The staged policy does not authorize generic patches. A finding is repairable
only when a typed writer is registered and its exact owner closure and
rollback contract are implemented. Deterministic findings without a registered
writer remain unsupported in this slice. Future writers require their own
reproducer, design amendment, RED tests, and approval.

## Review Item Contract

Reuse the existing daemon-owned `refinement_queue`; do not create a second
review database, command family, or UI ontology. Add a typed
`ProposalAction::LintReview` plus
`RefinementPayload::LintReview(LintReviewItemV1)` and a versioned payload:

```text
LintReviewItemV1 {
    schema_version,
    review_key,
    occurrence_digest,
    lint_scope,
    check_id,
    proposed_action,
    reason_code,
    confidence_basis_points,
    candidate_receipt,
    evidence_ids,
    counterevidence_ids,
    canonical_owners: Vec<LintReviewOwnerV1>,
    general_producer_receipt,
    deep_producer_receipt,
    general_report_receipt,
    deep_report_receipt,
    agent_work_digest,
    blocked_reason,
    base_capabilities,
    suggested_research_queries,
    query_rejections,
    query_policy_version,
    query_digest
}

LintReviewOwnerV1 {
    kind: Memory | Page | Entity,
    durable_id,
    canonical_receipt
}
```

`blocked_reason` is a closed enum:

- `exact_repair_unavailable`;
- `conflicting_evidence`;
- `user_choice_required`;
- `research_required`; or
- `unsupported_writer`.

The persisted first-slice capabilities are typed, not free-form strings, and
are computed by the daemon rather than accepted from the caller:

- `InspectEvidence`;
- `ProvideDecision`.

`InspectEvidence` is present only when at least one durable owner resolves and
dispatches by the typed owner kind to existing read-only owner tools; it never
changes queue state. An empty legacy `source_ids` list therefore cannot
advertise `InspectEvidence`.
`ProvideDecision` is present only when the proposed action maps to a currently
registered typed writer and the missing value is representable by
`LintReviewDecisionV1`; `unsupported_writer` can advertise only the actions
that are actually executable, normally `InspectEvidence` when available.
`ProvideDecision` calls the tagged repair-prepare path described below and
cannot apply.

Persisted payloads never store status-dependent `Dismiss` or `Reopen` actions.
Every list/detail response derives `effective_actions` from the current row
status and binding columns: `awaiting_review` exposes applicable base
capabilities plus `Dismiss`; `dismissed` exposes `InspectEvidence` when
available plus `Reopen` only while both verification bindings are null;
`resolved` and `superseded` expose only read-only `InspectEvidence` when
available. Thus a status-only dismiss cannot leave a stale action list in the
payload. `Reopen` is local-only, requires the exact dismissed id and fresh
reports, and is never inferred from a fresh lint run.
`Dismiss` uses the existing refinement rejection lifecycle with the lint-review
terminal guards added here.

The payload may contain bounded suggested research queries, but it does not
advertise an executable Research action in this slice. The follow-up research
executor will add that action only when it exists end to end.

### Creation surface and initial state

`POST /api/refinery/queue/lint-reviews` and the local-stdio-only
`enqueue_lint_reviews` MCP tool accept the exact durable lint scope plus the
complete current General and Deep reports. They derive every Review Item from
the final Deep findings whose resolution is `Review`; callers cannot submit a
free-standing blocked reason, action list, query, or queue payload. The daemon
verifies both daemon-authenticated report receipts, report versions, matching
scopes, producer/work/DB/Page receipts, completeness, and resolution shape
before one bounded transaction.

The cap is enforced upstream, not discovered after enqueue: v3 `LintAgentWork`
contains at most `LINT_AGENT_CANDIDATE_CAP` candidates, finalization emits at
most one final finding per candidate, and therefore a complete Deep report can
derive at most that many Review Items. If the candidate population is larger,
Deep marks it truncated/incomplete; `/lint repair` renders the eligible/packet
counts, returns `lint_review_scope_too_broad`, and asks for a narrower durable
scope without calling enqueue. An enqueue-time overrun is therefore an internal
`lint_review_cap_invariant_violation`, creates no rows, and still renders the
already-returned lint findings; it is not a normal wide-scope path and never
silently truncates or drops a subset.

```text
EnqueueLintReviewsRequestV1 {
    lint_scope,
    general_report,
    deep_report
}

EnqueueLintReviewsResponseV1 {
    review_items: Vec<LintReviewItemSummaryV1>,
    terminal_review_items: Vec<LintTerminalReviewItemSummaryV1>
}

LintTerminalReviewItemSummaryV1 {
    item,
    additional_terminal_count
}
```

### Page optimistic-consistency boundary

Page Markdown is an intentionally external canonical surface: Obsidian, VS
Code, or another process may edit it without a daemon lock. This slice therefore
does **not** claim an atomic snapshot across Page files and the DB. Page receipts
are an optimistic freshness guard; only the DB target receipt participates in
the canonical mutation CAS.

Durable prepare/enqueue/reopen/decision paths compare an immediate Page scan to
the authenticated report before their artifact/queue operation, then scan again
before returning success. A pre-scan mismatch creates no artifact or queue
write. Prepare builds in a private temp directory and publishes only after its
second matching scan. Enqueue/reopen writes queue metadata in `BEGIN IMMEDIATE`;
if the post-commit scan changed, a compensating transaction marks only rows
written by that exact request `superseded(page_changed_during_enqueue)`, returns
`repair_context_stale`, and surfaces no open proposal. The advanced sequence
watermark may remain; the required rerun is issued by the same daemon instance
with a higher sequence and replaces it.

Apply rechecks the manifest's Page receipt immediately before the DB target CAS.
A mismatch prevents apply. Because an external editor can still write during or
after the CAS, post-repair General/Deep verification rescans Pages; any observed
change leaves the operation `applied_unverified`, reports the non-target drift,
and preserves the rollback artifact. This is a declared consistency limit, not
permission to ignore Page drift or claim target-only verification. Queue rows
and private repair artifacts are control-plane metadata, not canonical
knowledge, so optimistic compensation cannot itself change user knowledge.

New lint Review Items enter `awaiting_review` directly. They never use the
table's legacy `pending` default, so the background refinement processor cannot
ignore an unknown action while `list_refinements` hides the row. The response
returns open items and matching terminal items separately so `/lint repair`
cannot silently drop a still-present dismissed item. Return only the most
recent terminal occurrence per current `review_key`, ordered by `created_at`
descending, with `additional_terminal_count` for older history. The terminal
vector is therefore capped by the current derived Review Item cap rather than
growing with queue history.

Explicit reopen uses local-only
`POST /api/refinery/queue/lint-reviews/{id}/reopen` and
`reopen_lint_review`. Its request includes fresh same-scope General and Deep
reports; the daemon revalidates that the exact dismissed occurrence is still a
current final finding. In one transaction it refreshes every daemon-derived
mutable field from that finding—evidence, counterevidence, blocked reason,
base capabilities, receipts, confidence, and sanitized queries—then changes
`dismissed -> awaiting_review` with a CAS predicate requiring both verification
binding digests to remain null. If verification won the race, reopen fails and
preserves the terminal row. It never revives the stale stored payload unchanged.

### When a Review Item is allowed

`/lint repair` may enqueue a Review Item only from a complete, current, final
Deep finding. It does not enqueue from provider failure, truncated population,
stale work, failed-to-run checks, or incomplete reports; those states remain
visible lint failures instead of durable review noise.

Examples:

- a classification mismatch with no unique target type becomes
  `user_choice_required`;
- a contradiction with credible evidence on both sides becomes
  `conflicting_evidence` or `research_required`;
- a valid finding whose typed writer is not implemented becomes
  `unsupported_writer`.

### Durable identity and deduplication

The Review Item's blocked reason and suggested queries are copied from the
final finding's `Review` resolution. The skill cannot rewrite them.
The queue row's legacy `source_ids` field contains only uniquely resolved
durable owners and may be empty; ambiguous evidence is never converted into a
guessed owner merely to satisfy the old queue shape.

Use two identities:

- `review_key` is a stable digest over schema version, exact durable scope,
  check id, semantic candidate kind, proposed action, reason code, and every
  evidence/counterevidence stable id grouped by role, plus each resolved owner's
  typed kind and durable id. It never drops unresolved evidence merely because
  one owner resolved. Blocked reason, confidence, and disposition stay outside
  the key so those changes refresh one semantic issue instead of minting
  siblings.
- `occurrence_digest` adds the finding's daemon-computed `candidate_receipt`
  **for every finding**, plus the current typed canonical owner receipts.
  Whole-DB, whole-Page, producer, and agent-work receipts are revalidated for
  freshness but excluded from identity so unrelated writes do not churn Review
  Items. Candidate content or any owned canonical content change does mint a
  new occurrence.

A complete Deep report containing two final Review findings with the same
`review_key` is contract-invalid and enqueues nothing. The daemon never merges
or overwrites duplicates by insertion order.

The queue row id is `lint_review_<occurrence_digest>`. The existing
`refinement_queue.id` primary key deduplicates an occurrence. Add nullable
`scope_digest`, `review_key`, `occurrence_digest`, `report_daemon_instance_id`, and
`report_sequence` columns for lint rows, plus a partial unique index that
permits at most one `awaiting_review` lint row per `review_key`; legacy rows keep
nulls. The existing generic `INSERT OR REPLACE` helper must not be used because
it can reset review history.

Add a small `lint_review_scope_watermarks` metadata table keyed by exact durable
`scope_digest`, holding the current daemon instance id and highest accepted
complete Deep report sequence. This is queue ordering metadata, not a second
review-item store.

After the Page pre-scan matches, DB receipt freshness, report-receipt
authentication, scope-watermark comparison, supersession, and insert/refresh
happen in one `BEGIN IMMEDIATE` transaction; the Page post-scan and compensation
rule above close the observable queue result without claiming cross-resource
atomicity. A sequence at or below the current-instance scope watermark
performs no write and returns the current rows; it may never refresh, insert,
supersede, or replace work accepted from a later complete report. A report from
the current instance may replace rows issued by a prior instance after full
freshness checks; delayed requests from the prior instance fail receipt
authentication.

For an accepted higher-sequence complete report, derive the full set of current
`review_key`s. Before insertion, mark prior open rows in the same scope whose
key is absent as `superseded` with reason `no_longer_present`; a changed
occurrence of a present key uses reason `replaced_by_occurrence` and binds
`superseded_by_id`. Advance the scope watermark only in the same transaction.
For a higher-sequence new occurrence, mark the prior open row superseded before
inserting the new open row so the partial unique index remains satisfied. Then:

- insert a missing occurrence as `awaiting_review`;
- for the same open occurrence, atomically refresh evidence,
  counterevidence, blocked reason, daemon-derived base capabilities, receipts,
  confidence, and sanitized queries while preserving id, status, and
  `created_at`;
- when a higher-sequence occurrence for the same `review_key` is inserted, mark
  the prior open occurrence `superseded` and bind `superseded_by_id`; and
- for the same terminal occurrence, perform no implicit reopen but return it in
  `terminal_review_items` with its status.

`Dismissed` items may be reopened only by an explicit `Reopen` action on the
exact id. `Resolved` and `superseded` items cannot reopen. A verified repair
changes the target receipt, so a genuinely recurring issue becomes a new
occurrence instead of remaining permanently hidden behind the old terminal
row.

Extend the queue lifecycle and every terminal-status guard with `superseded`.
The scheduler continues to process only legacy `pending` actions; lint reviews
are created directly as `awaiting_review` and are never daemon-auto-applied.

`list_refinements` remains the one review-list surface. Generic
`accept_refinement` must reject `LintReview`; a review decision prepares a
manifest through the lint-repair control plane and never applies canonical data
through the legacy refinement dispatcher.

### User-provided decision

When the user supplies the missing value for a Review Item, the skill submits
that explicit `LintReviewDecisionV1` with the Review Item id and occurrence
digest. The skill first reruns fresh same-scope General and Deep reports and
calls `enqueue_lint_reviews` before prepare; the stored Review Item alone is not
sufficient source evidence. If the exact item remains the current occurrence,
the skill includes those reports in `PrepareRepairSourceV2::ReviewDecision`.
The daemon revalidates the report receipts, target, writer, and typed value and
rejects an invalid/no-op decision. If the same `review_key` now has a different
current occurrence, enqueue returns and the skill renders
`review_source_changed { successor_item }`; it never silently carries the old
decision forward. The user may confirm the typed decision against that visible
successor without rediscovering the issue. If no successor exists, return
`review_source_stale`. Only an exact current occurrence prepares a v2 immutable
manifest whose source records the Review Item id, occurrence digest, and typed
decision. The Review Item remains open until that manifest is successfully
verified or the user dismisses it.

Add nullable `bound_manifest_digest`,
`bound_verification_receipt_digest`, `superseded_by_id`, and
`superseded_reason` columns to
`refinement_queue`. These names record an exact verification binding without
implying that the row's terminal status is `resolved`. Successful verification
of a review-origin v2 manifest publishes the immutable verification receipt
first, then writes both digests to the exact Review Item id and occurrence named
by the manifest in one `BEGIN IMMEDIATE` transaction. If the item is still
`awaiting_review`, the transaction also marks it `resolved`. If it became
`superseded` or `dismissed`, the transaction preserves that terminal status but
still records the exact verification binding; it never binds the successor row
or reopens the item. This exact same-occurrence binding is the sole exception to
the generic terminal-write guard. A retry with the same binding is success; a
different manifest or verification digest is a conflict. If receipt publication
succeeds but the queue transaction fails, an idempotent verification retry
loads the existing receipt and performs only the missing queue reconciliation
without reapplying the repair. Reopen and verification share this serialized
transaction rule, and reopen's null-binding CAS prevents a verified terminal row
from returning to `awaiting_review`.

Add an index on `(review_key, status, created_at)` for terminal/current lookup.
The canonical `latest_verified_for_review_key` query returns the newest row
with non-null bound digests, independent of whether its terminal status is
`resolved`, `dismissed`, or `superseded`; UI status and verification history
remain separate facts.

The user still approves the exact manifest in a later turn with:

```text
apply repair <manifest-id> <manifest-digest>
```

Providing a decision is therefore permission to prepare a concrete proposal,
not permission to write canonical data.

## Suggested Research Queries

The Deep judge may attach suggested queries only in a `Review` resolution whose
blocked reason is `research_required`. Its submission requires one to three
unique proposed queries, each at most 200 characters; every other reason
requires an empty query list. Queries are untrusted proposal data, not network
operations. The daemon, not the judge, derives the final accepted-query list and
`query_rejections` metadata.

```text
LintQueryRejectionV1 {
    proposed_index,
    reason_code,
    proposed_query_digest
}
```

The rejection metadata never stores the rejected raw string.

Before a query enters the final finding or local `refinement_queue`, the daemon
runs a deterministic obvious-secret floor against both the string and the
resolved evidence packet. Policy v1 rejects a query when existing
`privacy::redact_pii` would change it; when the quality-gate credential matcher
detects a token; when a punctuation-aware token resolves under the current
user's home, Wenlan data root, or active worktree; or when it contains an exact
durable owner id, opaque evidence id, or local Page path from the packet.
Extract the credential matcher into a shared pure helper rather than duplicating
its regexes. Do not reject generic absolute system paths such as `/etc/...`,
generic long identifiers, or verbatim technical phrases merely because they
appear in evidence.

The accepted exact strings, ordered rejection metadata, and validator-policy
version are covered by the payload's query digest. A rejected query is removed
individually and recorded by digest/reason; it does not invalidate or hide the
Review Item. If all proposals are rejected, the item remains visible with zero
accepted queries and no future Research action until a new validated query is
proposed. The daemon never silently rewrites a query.

This floor catches known obvious leaks before local persistence; it is not a
complete PII classifier and does not authorize egress. The future research
executor must add a separate, broader execution-time DLP check immediately
before network use, then display the exact post-check query and require user
approval bound to its digest. Physical addresses, international identifiers,
IP/coordinate data, and other categories not covered by policy v1 remain
blocked from automatic execution by that absent executor.

Before a future executor sends them to the open web, Wenlan must display the
exact accepted query strings and require explicit approval bound to their
digest. Query generation remains a soft first pass, while the daemon validator
is the local-persistence boundary.

The research executor is a separate follow-up slice. Its required lifecycle is:

```text
approve exact queries -> search -> retain citations/receipt -> rerun Deep
-> prepare exact repair or keep Review Item unresolved
```

Research never writes canonical knowledge directly and never turns a search
result into an implicit repair.

## Skill Flow

### `/lint`

Run one General report in the resolved scope and render it. No repair or queue
tool is callable.

### `/lint deep`

Run the existing bounded calling-agent two-call Deep protocol in the resolved
scope and render only the final canonical report. The public command does not
expose its provider route.

### `/lint repair`

1. Reuse a fresh same-task General/Deep context only when its durable scope is
   identical, its report/work schema versions are current, and both
   daemon-authenticated report receipts verify for the current daemon instance;
   no cross-task cache or reusable bearer/session token is introduced.
2. Otherwise run General and the bounded calling-agent Deep protocol.
3. Stop on incomplete or stale reports.
4. Prefer one exact supported `Repair` resolution in canonical finding order
   and call prepare once.
5. Submit the same current reports once to `enqueue_lint_reviews`; the daemon
   inserts or refreshes all remaining bounded Review resolutions.
6. Render open and still-present terminal Review Items. Never hide a matching
   dismissed item merely because its status is terminal.
7. Render the exact mutation and manifest digest, then state that canonical
   memory is unchanged and request the exact later-turn approval phrase.

If prepare reports stale receipts, the skill may refresh the same scope once.
If the finding disappears, changes target, or remains incomplete, stop without
creating a manifest. A refreshed proposal receives a new digest and requires
new approval.

Apply and verification continue to use the existing lifecycle. Apply is never
performed in the prepare turn.

## Failure Semantics

- `lint_contract_upgrade_required`: report/work schema is not current for
  repair; rerun after updating the daemon/skill and create no durable artifact.
- `lint_contract_unavailable`: the contract handshake remained transiently
  unreachable after one bounded retry; preserve read-only context, create no
  artifact, and do not misreport it as a version mismatch.
- `lint_report_receipt_invalid`: a v5 report was not finalized by the current
  daemon instance; rerun lint and create no durable artifact.
- `lint_review_scope_too_broad`: Deep candidate population was truncated at the
  upstream cap; show eligible/packet counts, request a narrower durable scope,
  and do not enqueue.
- `lint_review_cap_invariant_violation`: a purported complete report derives
  more Review Items than candidates allowed by its contract; render the report,
  create no rows, and treat it as a daemon contract defect.
- `repair_context_stale`: a DB/Page receipt changed before prepare/apply or
  during the optimistic Page pre/post window; do not surface a new proposal,
  compensate request-owned queue rows when necessary, and rerun lint.
- `repair_resolution_requires_review`: the final finding deliberately routes
  to review; create/reuse its typed Review Item.
- `lint_resolution_missing`: the semantic submission omitted a resolution;
  mark the report incomplete and create no queue row.
- `repair_proposal_invalid`: action and proposal disagree; fail the report or
  prepare request closed and do not queue the malformed proposal.
- `review_source_changed`: the same logical issue has a successor occurrence;
  surface that exact successor and require confirmation against it rather than
  carrying the old decision forward.
- `review_source_stale`: no current successor matches the stored logical issue;
  rerun lint rather than applying the old decision.
- `review_decision_invalid`: supplied decision is not allowed by the payload.
- `review_already_terminal`: the same Review Item was resolved, dismissed, or
  superseded; do not apply ordinary actions, while an exact same-occurrence
  verification binding may still be recorded without changing terminal status;
  surface its terminal status and do not reopen it implicitly.
- `review_action_unavailable`: the requested action lacks a resolved owner or
  registered writer and therefore was not advertised.
- `review_query_filtered`: one or more proposed research queries failed the
  local-persistence floor; keep the Review Item, omit the rejected raw strings,
  and surface only rejection reasons/digests.
- `review_resolution_binding_conflict`: verification retry names a different
  manifest or receipt than the Review Item's durable terminal binding.

No failure falls back to agent inference after lint, arbitrary SQL, a generic
JSON patch, direct refinement apply, or an automatic network search.

## Non-Goals

- No canonical write without the exact manifest approval tuple.
- No `wenlan lint --fix`, provider-slot CLI, or live-data auto-repair.
- No separate `/lint-repair` skill or second review queue.
- No arbitrary or free-form Review Item action executor.
- No web-search execution, citation ingestion, or automatic source capture in
  this slice.
- No batch apply or multiple manifests from one invocation.
- No new repair writers beyond `reclassify_memory`.
- No migration or rewrite of existing memories merely because this contract
  ships.

## Compatibility and Rollout

- Existing v1 repair manifests, rollback payloads, and receipts remain immutable
  and valid through their closed frozen v1 decoders; all new prepares and
  receipts emit schema v2.
- Existing refinement proposals retain their current payloads and dispatch.
- The new lint-review payload is additive and versioned; malformed or unknown
  versions fail closed.
- The queue migration adds nullable lint identity, report-sequence,
  verification-binding, and supersession columns plus lint-only indexes; it
  does not rewrite existing rows.
- Existing live memories are changed only through separately approved
  manifests, one target at a time.
- Ship the daemon, `wenlan` CLI, `wenlan-mcp`, shared types, and Claude/Codex
  skills as one staged compatibility set, but do not pretend their running
  processes switch atomically: MCP clients spawn and retain their own
  `wenlan-mcp` process.
- For one compatibility release the daemon serves two **read-only** HTTP lint
  contracts. Requests without `X-Wenlan-Lint-Contract` receive frozen report/work
  v4/v2; updated CLI and MCP send
  `X-Wenlan-Lint-Contract: report=5;work=3` and receive only v5/v3. Select the
  request decoder from that header before typed body deserialization. Each
  client binary therefore still typed-deserializes exactly one report shape; no
  untyped union crosses the client boundary.
- New prepare/review endpoints require authenticated v5/v3 report receipts and
  manifest schema v2. Old CLI/MCP clients remain able to run read-only `/lint`,
  but repair attempts receive HTTP `426 Upgrade Required` with a stable
  string-coded `lint_contract_upgrade_required` body and create no artifact;
  even an old closed-enum client must preserve the HTTP status/message as an
  actionable update prompt.
- Updated CLI and MCP call `/api/lint/contract` on first use and before every
  repair operation. Cache entries are keyed by `daemon_instance_id`; reconnect,
  connection reset, or any receipt with a different instance id invalidates the
  cache and triggers another handshake. Enable repair only when
  report/work/manifest 5/3/2 is advertised. HTTP 404 or an explicitly lower
  contract maps to `lint_contract_upgrade_required`; timeout, connection reset,
  or 5xx receives one bounded retry and then `lint_contract_unavailable`, never
  a false upgrade diagnosis. A running old MCP must be restarted by its owning
  client; the daemon never claims it can restart that process.
- Remove frozen read-only v4/v2 serving only in a later release after the
  compatibility and client-restart tests have remained green. A same-task
  v4/v2 report is always rerun under v5/v3 before repair.
- The compatibility matrix covers old/new CLI and old/new MCP against the new
  daemon, plus new CLI/MCP against the old daemon's fail-closed handshake path.
  It asserts user-visible 426 handling by old clients, 404 versus transient
  branching by new clients, and re-handshake after a daemon-instance change.
- Remove the public `lint-repair` skill only after both Claude and Codex
  `/lint repair` paths, the compatibility handshake, rollback test, and
  plugin-contract tests are green.
- Installed daemon and plugin updates happen only after code, contract, and
  review gates pass. Live Review Items are created only by an explicit
  `/lint repair` invocation.

## Verification

Implementation follows RED-GREEN-REFACTOR and must prove:

- lint report and agent-submission contracts require exactly one resolution
  and reject unknown variants, mismatched action/proposal pairs, invalid types,
  no-op proposals, and invalid review/query shapes;
- report/work schema 5/3 rejects stale repair context; altered, replayed across
  daemon instances, or caller-fabricated final reports fail report-receipt
  authentication;
- frozen HTTP v4/v2 remains read-only for old CLI/MCP clients,
  header-negotiated v5/v3 works through updated CLI/MCP, and the first-call
  handshake fails repair closed on every unsupported old/new matrix entry;
  old clients surface HTTP 426, new clients distinguish 404 from transient
  failure, and daemon-instance changes force re-handshake;
- plain lint endpoints make no persistent writes while carrying exact typed
  proposal data and daemon-authenticated report receipts;
- prepare rejects a separately supplied or altered after-value and derives the
  manifest mutation only from the final finding;
- the closed frozen v1 transitive graphs for manifest, rollback, apply receipt,
  and verification receipt retain identical canonical bytes/digests and still
  deserialize, apply, and verify, including a semantic finding nested in
  post-assertion baseline evidence; no frozen field imports a live domain type,
  the syntax-aware architecture test rejects live-type reachability, and all new
  persisted repair artifacts emit v2;
- concurrent identical prepare requests return one deterministic manifest id,
  the loser loads the winner's persisted manifest/time/digest, and changed
  target/report receipts produce a different id; a fixed preparation key maps
  to the pinned UUIDv5 vector and never a legacy UUIDv4 id; same-process races
  serialize on the keyed async mutex and cross-process races on the blocking
  file lock without stalling the Tokio runtime;
- complete unresolved findings create one stable typed Review Item while
  incomplete/stale/failed runs create none;
- a truncated over-cap candidate population remains visibly incomplete and
  requests narrower scope before enqueue; no complete v3 report can derive more
  Review Items than the upstream candidate cap;
- enqueue creates rows directly as `awaiting_review`; they are visible through
  `list_refinements` and never depend on the background processor recognizing
  the action;
- concurrent identical enqueues converge through the queue primary key;
  repeated identical occurrences preserve id/status, a changed occurrence
  supersedes the prior open item, and a terminal same-occurrence item remains
  visibly reported;
- candidate receipts for every finding remain stable under unrelated DB/Page
  writes and change when the referenced candidate packet changes; distinct
  evidence roles produce distinct review keys, and duplicate review keys in one
  final report fail closed rather than overwrite;
- out-of-order lower report sequences cannot supersede a higher-sequence open
  occurrence, and DB receipt freshness plus sequence comparison are committed
  in the same `BEGIN IMMEDIATE` transaction;
- external Page edits between optimistic pre/post scans prevent an approvable
  artifact or compensate request-owned queue rows; edits racing the DB CAS are
  detected by post-repair lint and leave the operation `applied_unverified`;
- open refresh updates blocked reason, base capabilities, and queries atomically,
  so query shape cannot disagree with the reason;
- explicit reopen performs the same payload refresh before changing status and
  loses safely to an already-written verification binding;
- effective actions are projected from row status/bindings, so dismissed rows
  expose `Reopen` without persisting stale open-state actions;
- empty-owner items do not advertise `InspectEvidence`, and
  `unsupported_writer` items do not advertise `ProvideDecision`;
- an explicit typed user decision plus fresh reports can prepare but cannot
  apply a v2 manifest; a changed occurrence returns its visible successor and
  requires renewed confirmation;
- generic refinement acceptance cannot apply a lint review;
- suggested queries are bounded, checked individually by the daemon's
  documented obvious-secret floor before local persistence, covered by a
  policy-versioned digest, and never executed; rejected raw strings are omitted
  without dropping the Review Item, while legitimate generic system paths and
  technical identifiers remain accepted;
- verification binds a review-origin manifest and verification receipt to the
  exact queue occurrence idempotently even after dismissal/supersession; a
  receipt-before-queue crash reconciles on retry without binding the successor;
- terminal enqueue results return at most one recent item per current review
  key plus an overflow count;
- `/lint`, `/lint deep`, and `/lint repair` parsing has Claude/Codex parity;
- removed `profile:*`, `agent`, and `/lint-repair` surfaces fail plugin
  contract validation;
- prepare, apply, post-lint, and verification retain the existing target-only
  effect and receipt proofs.

Run Cargo work serially with `CARGO_BUILD_JOBS=1` and
`CMAKE_BUILD_PARALLEL_LEVEL=1`. Avoid release builds, check memory pressure
between heavy gates, and run only one Deep adjudication at a time.
