# Total Lint Repair Resolution Design

**Status:** Approved for implementation by Lucian on 2026-07-15.

## Problem

The merged lint product already owns a complete diagnostic taxonomy: General
runs 55 deterministic checks, while Deep runs 73 checks and adds nine semantic
families. The repair implementation nevertheless accepts only
`memories.semantic.classification`, prepares at most one manifest, and exposes
only `reclassify_memory` as a writer. This makes valid lint findings disappear
from `/lint repair` merely because no adapter is registered.

That is the wrong user contract. A missing writer may block application, but it
must not block explanation or visibility.

This design supersedes these narrow rules in
`2026-07-14-staged-lint-repair-review-escalation-design.md`:

- the optional single `repair_proposal?` result;
- at most one prepared manifest per invocation;
- semantic-only total resolution; and
- no writers beyond `reclassify_memory`.

The existing read-only lint boundary, explicit approval, immutable manifest,
CAS, rollback artifact, effect proof, and post-repair verification remain
authoritative.

## Considered Approaches

### Chosen: a typed repair-resolution registry after lint

Keep lint diagnostic and read-only. `/lint repair` passes fresh reports to a
core-owned registry that resolves every finding into one visible disposition:
an exact executable repair, a Review Item, a system action, or a blocked item
with a specific reason.

This preserves the existing lint contract while giving repair one canonical
place to bind durable owners, expected receipts, writers, and rollback data.

### Rejected: put manifests directly in `/api/lint`

This would make plain lint allocate repair artifacts and couple diagnostics to
writer availability. It would violate the established read-only boundary and
make an otherwise valid check fail because a repair adapter is unavailable.

### Rejected: infer mutations in the skill from check IDs

Skill-side inference cannot safely resolve opaque evidence, prove current
versions, or share canonical writers with the daemon. It would recreate the
drift that the typed daemon contract was built to prevent.

## User Outcome

`/lint repair [scope]` shows every current deterministic and semantic finding
in one result. No finding is omitted because its writer is unsupported.

Each finding has exactly one disposition:

```text
ready          exact target, before/after, expected receipt, writer,
               rollback artifact, manifest id, and manifest digest
review         exact issue and durable Review Item requiring judgment
system_action  exact non-content action such as migrate, rebuild, update,
               or restart; no fake data manifest
blocked        exact reason such as stale, ambiguous, incomplete, or
               unsupported, plus the next lawful action
```

The skill renders every family and count in chat. Every resolved target is
also written to one durable, line-oriented repair-plan artifact so large
populations do not need to remain in memory or be truncated. For small plans,
the skill renders every target inline. A plan is never called complete when an
upstream report or resolver is truncated.

Displaying or preparing all proposals does not authorize canonical mutation.
Each manifest retains its exact approval tuple and is applied separately. A
future batch-apply contract is outside this design.

## Total Resolution Contract

Add a typed plan owned by `wenlan-types`:

```text
RepairPlan {
    plan_id,
    scope,
    general_report_receipt,
    deep_report_receipt?,
    deterministic_complete,
    semantic_complete,
    entries[],
    plan_digest,
}

RepairPlanEntry {
    check_id,
    occurrence_digest,
    affected_records,
    resolution,
}

RepairResolution =
    Ready { manifest }
  | Review { review_item }
  | SystemAction { action, evidence }
  | Blocked { reason, next_action }
```

The plan constructor enforces totality: every actionable deterministic finding
from General or Deep and every final Deep semantic finding contributes one
entry. It rejects duplicate occurrences, missing findings, added findings, and
a complete phase whose source population or resolver is incomplete. Deep
semantic failure never suppresses complete deterministic entries; it produces
visible blocked semantic families instead.

Entries from overlapping checks are not silently dropped. If they describe the
same target and exact mutation, the plan keeps both source check IDs on one
deduplicated manifest. If they imply different mutations, both remain visible
and are routed to Review rather than selecting one by order.

## Resolver Registry

The registry is keyed by canonical lint check ID. Each adapter owns:

- durable target resolution under the report's exact scope;
- current version/content receipt;
- one typed writer and complete before/after mutation;
- rollback capture and allowed-effect closure;
- post-repair assertions; and
- the fallback disposition when no unique mutation exists.

An unregistered deterministic check still produces a visible `blocked`
entry with `unsupported_deterministic_writer`; it never disappears.

### Current live deterministic findings

The fresh `space:wenlan` General report found these seven families. Their
required resolutions are fixed here rather than rediscovered during
implementation.

| Check | Resolution |
|---|---|
| `identity.memory_state_integrity` | Re-resolve each durable memory and split the mixed predicate. Blank `source_agent` normalizes to `NULL`; a self-supersedes edge may be cleared. Invalid booleans, pinned-vs-confirmed conflicts, missing Space owners, and pending revisions with missing predecessors become Review Items because more than one lawful state exists. |
| `identity.tag_integrity` | Blank tags, unsupported source kinds, and rows whose memory/Page owner is absent become exact row-removal manifests with row-level rollback. |
| `memories.supersession_integrity` | Self-edges may become exact edge-clear manifests. Dangling targets and pending revisions without a target become Review Items preserving the temporal decision. Duplicate proposals shared with memory-state integrity are deduplicated. |
| `pages.links.orphan_labels` | A label with exactly one active same-scope target becomes an exact `target_page_id` binding. Zero or multiple same-scope targets become Review Items. Existing explicit cross-scope targets are never rewritten. |
| `pages.projection.version_alignment` | The DB Page is canonical. Regenerate that Page's Markdown, state edge, and protected projection artifacts through `KnowledgeProjectionWrite::write_page`; rollback captures the prior files and state entry. |
| `runtime.schema_contract` | A known older schema with an available migration becomes a system action using the canonical migration runner. Missing/invalid search objects use the canonical rebuild path. A future schema or unknown shape is blocked rather than rewritten. |
| `serving.route_scope_contracts` | This is a code/deployment contract, not stored content. Show the exact violating route entries and the required daemon update/restart or code correction as a system action. Never fabricate a data manifest. |

### Other deterministic findings

All other General or non-semantic Deep findings are still enumerated. They enter
the same registry and initially resolve to `blocked` until their typed adapter
is implemented. The first implementation wave adds the current live adapters
above; subsequent adapters are selected from actual findings, not speculative
writers.

### Semantic findings

The nine existing semantic families remain canonical:

- memory classification, contradiction, and staleness;
- memory-entity links and entity relations;
- Page faithfulness, provenance adequacy, and evidence links; and
- retrieval quality.

Their existing proposed actions remain visible: reclassify, review or
supersede memory, add/remove memory-entity links, add/remove entity relations,
review a Page claim, add/remove Page evidence, and review retrieval.

A complete semantic resolution with one exact typed mutation may prepare a
manifest. Every other final semantic finding creates or refreshes a durable
Review Item with its evidence, blocked reason, possible choices, and suggested
research queries. Research never grants mutation authority.

## Data Flow

1. `/lint repair` runs fresh General, then Deep for one scope. Deep uses the
   calling-agent protocol when semantic candidates exist.
2. `prepare_repair_plan` authenticates the current reports and opens one stable
   read snapshot. A stale source is rerun; an incomplete check becomes a
   visible blocked entry and does not erase complete checks from the other
   phase.
3. Registry adapters stream findings in canonical check/target order, resolve
   durable owners, and write plan entries incrementally to a pending JSONL
   artifact.
4. Ready entries prepare immutable single-target manifests and rollback
   artifacts. Review entries are deduplicated in the existing refinement
   queue. System actions and blocked entries create no canonical data writes.
5. The completed plan is fsynced and published no-clobber with its digest.
6. The skill renders every family, all small-plan rows, and a clickable path to
   the complete plan artifact. It explicitly states that canonical data is
   unchanged.
7. A later exact `apply repair <manifest-id> <manifest-digest>` applies one
   selected manifest through its CAS writer, followed by the existing General,
   applicable Deep, and effect-verification lifecycle.

## Memory and Concurrency Bounds

- Never clone the full DB, Page tree, lint population, or proposal list into
  memory. Enumerate with ordered queries and fixed-size pages, and stream the
  plan artifact.
- Prepare one rollback artifact at a time and release temporary buffers before
  the next target.
- Run only one General/Deep pair and one agent-assisted Deep call at a time.
- Serialize Cargo verification with `CARGO_BUILD_JOBS=1` and
  `CMAKE_BUILD_PARALLEL_LEVEL=1`; stop heavy gates below the repository's
  memory floor.
- Hold the existing repair writer fence while applying, not while explaining
  or building the read-only plan.
- Apply remains single-manifest. A stale target fails that manifest closed and
  does not invalidate unrelated prepared proposals.

## Error Semantics

- `repair_plan_source_stale`: rerun the same scope; publish no plan.
- `repair_plan_incomplete`: show every observed family/count and the incomplete
  reason; publish no applyable manifest for that incomplete family while
  preserving complete deterministic proposals from other checks.
- `unsupported_deterministic_writer`: show the check and target/count as
  blocked; never omit it.
- `ambiguous_repair_target`: create a Review Item when the ambiguity is a user
  decision; otherwise block with the exact missing prerequisite.
- `conflicting_repair_proposals`: show both source checks and route the target
  to Review.
- `repair_system_action_required`: show the typed operational action and never
  encode it as a content mutation.
- `repair_target_stale`: preserve the existing apply failure and require a new
  plan before another approval.

No failure falls back to arbitrary SQL, JSON Patch, skill-side inference,
automatic network research, or hidden mutation.

## Verification

Implementation must prove:

- all 55 General check IDs and all final semantic findings have a total
  resolution path;
- the seven current live finding families map exactly as specified above;
- an unsupported deterministic writer is visible rather than filtered out;
- unique same-scope orphan links prepare exact bindings while ambiguous labels
  become Review Items;
- Page projection repair changes only the named Page projection closure;
- invalid tag cleanup changes only the exact invalid row;
- mixed memory-state and supersession checks never guess an ambiguous state;
- schema and route-scope findings return system actions, not data manifests;
- overlapping findings deduplicate only identical target/mutation pairs;
- plain `/lint`, `/lint deep`, `GET /api/lint`, and `POST /api/lint` remain
  byte-for-byte non-mutating;
- plan construction is stable, receipt-bound, no-clobber, and memory-bounded;
- prepare writes only repair/control-plane artifacts and Review Items;
- no canonical data changes before exact manifest approval;
- each applied manifest retains CAS, rollback, target-only effect proof, and
  fresh post-repair lint verification; and
- Claude and Codex expose only `/lint`, `/lint deep`, and `/lint repair`.

## Non-Goals

- No `--fix`, provider-slot CLI, separate lint runner, or separate public
  repair skill.
- No unattended or scheduled live-data mutation.
- No arbitrary patch language or generic SQL writer.
- No automatic web search or conversion of research evidence into approval.
- No atomic batch apply in this design; showing and preparing all proposals is
  not permission to apply them.
