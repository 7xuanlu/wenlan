# Staged Lint Repair and Review Escalation Design

**Status:** Design direction approved on 2026-07-14; written spec awaiting
user review.

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
immutable repair manifest. The manifest, rollback, compare-and-swap writer,
effect proof, and post-repair verification contract from
`2026-07-14-versioned-lint-repair-manifest-design.md` remain authoritative.
This spec amends that design only where it assigned repair inference to a
separate skill and exposed internal lint execution tokens to the user.

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

`/lint repair` returns two bounded result groups:

```text
repair_proposal?  # zero or one exact mutation plus manifest id/digest
review_items[]    # unresolved semantic work requiring a user choice
```

At most one repair manifest is prepared per invocation. Review Item emission
is bounded by the final lint evidence cap and deduplicated across repeated
runs.

## Read-Only Boundary

`GET /api/lint`, `POST /api/lint`, the core lint runner, and plain `/lint` and
`/lint deep` remain completely read-only. They may report an exact proposed
repair but cannot create manifests, queue rows, or canonical writes.

`/lint repair` is a skill orchestration mode, not a `wenlan lint --fix` flag or
a mutating lint endpoint. It may:

- create immutable repair artifacts through `/api/repairs/prepare`; and
- create or reuse Review Items through a separate review-queue write.

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
    suggested_research_queries: Vec<String>
}
```

V1 enables only this existing writer. The proposal must match the finding's
`ReclassifyMemory` action, name a valid `MemoryType`, and differ from the
candidate's current type. A proposal for another action or a no-op value fails
the lint submission contract.

The bounded Deep judge returns its proposed resolution with the verdict. The
daemon validates and copies it into the final `LintSemanticFinding`; callers
cannot attach or replace it afterward. The core may force `Review` with
`unsupported_writer` when no registered typed writer exists, but it cannot
invent an exact repair value.

A finding without exactly one valid resolution is an invalid or incomplete
semantic submission, not a queueable Review Item. This prevents `/lint repair`
from inventing the blocked reason or research queries after lint.

This change bumps the relevant lint report contract version. It does not bump
the repair manifest schema: newly prepared manifests keep the existing v1
shape, and already-persisted v1 manifests remain readable and applicable.

### Prepare consumes; it does not infer

The finding-based prepare request no longer accepts an independently supplied
`after_memory_type`. It accepts only a finding whose resolution is `Repair`,
derives the mutation from that proposal, and revalidates:

- complete General and Deep reports with identical durable scope;
- current report, producer, work, DB, and Page receipts;
- one unambiguous durable target;
- action/proposal/writer agreement; and
- current target state differing from the proposed after-state.

Review resolutions return `repair_resolution_requires_review`. Invalid or
mismatched repair proposals return `repair_proposal_invalid`; the `/lint
repair` mode may not fill the gap itself.

### Registered writers remain narrow

The staged policy does not authorize generic patches. A finding is repairable
only when a typed writer is registered and its exact owner closure and
rollback contract are implemented. Deterministic findings without a registered
writer remain unsupported in this slice. Future writers require their own
reproducer, design amendment, RED tests, and approval.

## Review Item Contract

Reuse the existing daemon-owned `refinement_queue`; do not create a second
review database, command family, or UI ontology. Add a typed
`ProposalAction::LintReview` and a versioned payload:

```text
LintReviewItemV1 {
    schema_version,
    lint_scope,
    check_id,
    proposed_action,
    reason_code,
    confidence_basis_points,
    evidence_ids,
    counterevidence_ids,
    producer_receipt,
    agent_work_digest,
    blocked_reason,
    allowed_actions,
    suggested_research_queries
}
```

`blocked_reason` is a closed enum:

- `exact_repair_unavailable`;
- `conflicting_evidence`;
- `user_choice_required`;
- `research_required`; or
- `unsupported_writer`.

The first-slice allowed actions are typed, not free-form strings:

- `InspectEvidence`;
- `ProvideDecision`;
- `Dismiss`.

`InspectEvidence` uses existing read-only owner tools after the evidence digest
resolves; it never changes queue state. `ProvideDecision` calls the tagged
repair-prepare path described below and cannot apply. `Dismiss` uses the
existing refinement rejection lifecycle.

The payload may contain bounded suggested research queries, but it does not
advertise an executable Research action in this slice. The follow-up research
executor will add that action only when it exists end to end.

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

The Review Item id is a stable digest over schema version, exact durable scope,
check id, proposed action, reason code, sorted evidence ids, and sorted
counterevidence ids. Re-running the same unresolved finding preserves the
existing row and its terminal status. New evidence produces a new id.

The existing generic `INSERT OR REPLACE` helper must not be used for lint
reviews because it can reset review history. Add an insert-or-refresh-open
transaction: insert when absent; for an open identical item, refresh only its
source receipts, confidence, and suggested queries from the newer complete
report while preserving id, status, and `created_at`; for a terminal item,
perform no update and never reopen it implicitly.

`list_refinements` remains the one review-list surface. Generic
`accept_refinement` must reject `LintReview`; a review decision prepares a
manifest through the lint-repair control plane and never applies canonical data
through the legacy refinement dispatcher.

### User-provided decision

When the user supplies the missing value for a Review Item, the skill submits
that explicit typed decision with the Review Item id. Internally,
`POST /api/repairs/prepare` accepts a tagged source: either a final
finding-owned repair or a Review Item plus user decision. The daemon
revalidates the stored lint receipts and current target, then prepares a new
immutable repair manifest. The Review Item remains open until the resulting
manifest is successfully verified or the user dismisses it.

Successful verification resolves the Review Item and binds its terminal state
to the verified manifest digest. If the immutable verification receipt is
published but the queue-status update fails, the item may remain visibly open;
an idempotent verification retry reconciles it without reapplying the repair.

The user still approves the exact manifest in a later turn with:

```text
apply repair <manifest-id> <manifest-digest>
```

Providing a decision is therefore permission to prepare a concrete proposal,
not permission to write canonical data.

## Suggested Research Queries

The Deep judge may attach suggested queries only in a `Review` resolution whose
blocked reason is `research_required`. That reason requires one to three unique
queries, each at most 200 characters; every other reason requires an empty
query list. Queries are proposal data, not network operations.

Before a future executor sends them to the open web, Wenlan must display the
exact query strings and require explicit approval bound to their digest. Query
generation and display must redact credentials and avoid copying full private
memory excerpts, private paths, email addresses, or local-only identifiers.

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
   identical; no cross-task cache or new session token is introduced.
2. Otherwise run General and the bounded calling-agent Deep protocol.
3. Stop on incomplete or stale reports.
4. Prefer one exact supported `Repair` resolution in canonical finding order
   and call prepare once.
5. For remaining complete unresolved findings, insert-or-reuse bounded Review
   Items.
6. Render the exact mutation and manifest digest, then state that canonical
   memory is unchanged and request the exact later-turn approval phrase.

If prepare reports stale receipts, the skill may refresh the same scope once.
If the finding disappears, changes target, or remains incomplete, stop without
creating a manifest. A refreshed proposal receives a new digest and requires
new approval.

Apply and verification continue to use the existing lifecycle. Apply is never
performed in the prepare turn.

## Failure Semantics

- `repair_resolution_requires_review`: the final finding deliberately routes
  to review; create/reuse its typed Review Item.
- `lint_resolution_missing`: the semantic submission omitted a resolution;
  mark the report incomplete and create no queue row.
- `repair_proposal_invalid`: action and proposal disagree; fail the report or
  prepare request closed and do not queue the malformed proposal.
- `review_source_stale`: stored review receipts no longer match; rerun lint
  rather than applying the old decision.
- `review_decision_invalid`: supplied decision is not allowed by the payload.
- `review_already_terminal`: the same Review Item was resolved or dismissed;
  do not reopen it implicitly.

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

- Existing v1 repair manifests and receipts remain immutable and valid.
- Existing refinement proposals retain their current payloads and dispatch.
- The new lint-review payload is additive and versioned; malformed or unknown
  versions fail closed.
- Existing live memories are changed only through separately approved
  manifests, one target at a time.
- The repository removes the public `lint-repair` skill only after both Claude
  and Codex `/lint repair` paths and plugin-contract tests are green.
- Installed daemon and plugin updates happen only after code, contract, and
  review gates pass. Live Review Items are created only by an explicit
  `/lint repair` invocation.

## Verification

Implementation follows RED-GREEN-REFACTOR and must prove:

- lint report and agent-submission contracts require exactly one resolution
  and reject unknown variants, mismatched action/proposal pairs, invalid types,
  no-op proposals, and invalid review/query shapes;
- plain lint endpoints remain byte-for-byte non-mutating while carrying exact
  typed proposal data;
- prepare rejects a separately supplied or altered after-value and derives the
  manifest mutation only from the final finding;
- existing v1 manifests still deserialize, apply, and verify;
- complete unresolved findings create one stable typed Review Item while
  incomplete/stale/failed runs create none;
- repeated identical unresolved findings preserve the same review id and
  terminal state;
- a user decision can prepare but cannot apply a manifest;
- generic refinement acceptance cannot apply a lint review;
- suggested queries are bounded, redacted, persisted, and never executed;
- `/lint`, `/lint deep`, and `/lint repair` parsing has Claude/Codex parity;
- removed `profile:*`, `agent`, and `/lint-repair` surfaces fail plugin
  contract validation;
- prepare, apply, post-lint, and verification retain the existing target-only
  effect and receipt proofs.

Run Cargo work serially with `CARGO_BUILD_JOBS=1` and
`CMAKE_BUILD_PARALLEL_LEVEL=1`. Avoid release builds, check memory pressure
between heavy gates, and run only one Deep adjudication at a time.
