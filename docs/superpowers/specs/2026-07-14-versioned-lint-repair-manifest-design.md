# Versioned Lint Repair Manifest Design

**Status:** Approved for implementation on 2026-07-14.

## Decision

Build a separate, approval-gated repair control plane around the existing
read-only lint contract. `/lint`, `GET /api/lint`, and `POST /api/lint` remain
diagnostic-only and must retain their non-mutation fingerprint tests. The new
`lint-repair` skill prepares and applies one typed repair manifest at a time;
it never infers or batches live-data mutations.

The first vertical writer is `reclassify_memory`. The manifest contract is
versioned and target-kind aware, but every other writer returns
`unsupported_writer` until a reproduced repair class justifies its own typed
adapter and tests. In particular, v1 does not delete or merge records, edit
Pages, supersede memories, or add/remove graph links.

## User Outcome

A user can take one actionable agent-assisted Deep finding and receive:

1. an explanation tied to the originating lint receipt;
2. the exact memory-type mutation and its durable owner/scope;
3. an immutable manifest digest and durable rollback artifact;
4. an explicit approval prompt bound to that digest;
5. a compare-and-swap canonical write;
6. fresh General and agent-assisted Deep results; and
7. a durable verification receipt proving the target finding disappeared and
   the repair did not write outside its declared owner closure.

No canonical data changes before the explicit approval in step 4.
The exact phrase is an intent-binding workflow gate for cooperating agents,
not authentication against malicious software already running as the user.

## Non-Goals

- No `wenlan lint --fix`, repair CLI, or hidden lint mutation mode.
- No provider-slot CLI or second lint runner/endpoint.
- No automatic, scheduled, or bulk live-data repair.
- No arbitrary SQL, arbitrary JSON Patch, or caller-selected writer name.
- No deletion, merge, Page refresh, supersession, observation, relation, or
  memory-entity-link adapter in this slice.
- No application of a manifest whose target cannot be resolved from durable
  lint evidence without ambiguity.
- No Windows repair execution in v1; core returns
  `lint_repair_unsupported_platform` until Windows has equivalent durable
  no-clobber publication and user-only artifact ACLs.

## Architecture

The control plane has four boundaries:

1. `wenlan-types` owns the versioned wire contract and rejects malformed or
   unsupported manifests during deserialization.
2. `wenlan-core::repair` resolves opaque semantic evidence to one durable
   owner, computes canonical receipts, writes rollback artifacts, performs the
   CAS writer, and verifies allowed effects.
3. `wenlan-server::repair_routes` exposes prepare, apply, and verification
   receipt endpoints. It contains HTTP framing only.
4. `wenlan-mcp` exposes typed local tools consumed by matching Claude Code and
   Codex `lint-repair` skills.

The repair endpoints are separate from `/api/lint`. The skill continues to use
the one canonical lint MCP tool for both the before and after reports.

## Versioned Contract

### Manifest identity

`REPAIR_MANIFEST_SCHEMA_VERSION` starts at `1`. A manifest is immutable after
prepare. Its `manifest_digest` is lowercase SHA-256 over canonical JSON of all
immutable fields except `manifest_digest` itself. Struct field order and
`BTreeMap` ordering define canonical serialization; unknown fields and unknown
schema versions fail closed.

Each manifest contains:

- `manifest_id`: random UUID prefixed `repair_`;
- `manifest_schema_version`;
- `prepared_at`;
- `source`: lint report schema/catalog versions, profile, exact scope, check
  id, semantic action/reason, evidence digests, DB/Page snapshot receipts,
  producer receipt, and agent work digest when applicable;
- `target`: durable owner kind/id and exact registered/uncategorized scope;
- `expected_state`: optional monotonic version plus required canonical receipt;
- `writer`: one typed canonical writer variant;
- `mutation`: complete typed before/after values with no omitted wildcard;
- `allowed_effects`: exact owner closure the writer may change;
- `rollback`: root-relative artifact locator, SHA-256 digest, and format
  version;
- `post_assertions`: target finding absence, complete General/Deep reports,
  no new actionable or incomplete checks, and allowed non-target check deltas;
- `manifest_digest`.

V1 target and writer variants are deliberately narrow:

```text
RepairTarget::Memory { source_id, scope }
RepairWriter::ReclassifyMemory
RepairMutation::ReclassifyMemory { before_memory_type, after_memory_type }
```

`after_memory_type` must parse as the existing `MemoryType`; it must differ
from `before_memory_type`. The source finding must be
`memories.semantic.classification` with proposed action
`reclassify_memory`. The target evidence must resolve to exactly one memory.

### Durable evidence resolution

Deep semantic findings already carry content-free record digests derived from
stable record keys such as `memory:<source_id>`. Prepare recomputes the scoped
candidate population, requires the submitted work/snapshot receipts to match,
and resolves the finding's memory digest to exactly one current source ID.
Zero or multiple matches reject preparation. Durable IDs are returned only in
the repair manifest; `/api/lint` keeps its opaque evidence contract unchanged.

General findings or Deep actions without a registered resolver return
`unsupported_finding`. Run-local ordinals alone never authorize repair.

### Expected state and owner closure

For a memory target, the canonical receipt hashes the ordered canonical row set
for `source='memory' AND source_id=<target>` including row IDs, chunk indexes,
content, memory type, Space, lifecycle fields, and last-modified values. The
receipt is computed again immediately before apply.

The v1 `reclassify_memory` owner closure is every memory chunk with the target
source ID. No Page, graph, observation, relation, tag, or other memory owner is
allowed to change. Prepare stores the full before-image for that closure in the
rollback artifact. Apply computes a non-target lint-relevant fingerprint before
and after the canonical transaction; a mismatch rolls the transaction back.

## Artifact Layout

Repair artifacts are small, sensitive, and operational rather than source
code. Store them outside every worktree:

```text
<wenlan-data-dir>/repairs/<manifest-id>/
  manifest.json
  rollback-v1.json
  apply-receipt.json          # absent before apply
  verification-receipt.json   # absent before successful verification
```

On Unix, create the directory and files with owner-only access. Hold an
advisory per-manifest operation lock across apply/recovery or verification so a
live writer cannot be mistaken for crash residue. Write each receipt to a
sibling pending path, `fsync`, then publish it with an atomic no-clobber hard
link and sync the parent directory. Non-Unix repair operations fail closed in
v1. Never overwrite an existing immutable manifest or receipt. Tool output may show
IDs, field names, enum values, and digests; it must not print rollback content
or unrelated memory excerpts.

## Lifecycle

### 1. Diagnose

The skill runs General, then agent-assisted Deep through the existing lint MCP
tool. It stops if either report is incomplete, stale, truncated, or rejected.
Only a final canonical Deep semantic finding can be selected.

### 2. Prepare

`prepare_repair` accepts the two typed lint reports, selected semantic finding,
and proposed `after_memory_type`. The daemon revalidates receipts, resolves the
owner, captures expected state and rollback bytes, writes the immutable
artifacts, and returns the full manifest.

Prepare may create only repair artifacts. It cannot modify the database, Page
projection, lint report, or any canonical product record. A regression test
fingerprints canonical lint-relevant state around prepare.

### 3. Explain and approve

The skill renders the durable owner/scope, source check and reason, exact
`before_memory_type -> after_memory_type` mutation, allowed effects, rollback
artifact digest, post-assertions, and full manifest digest. It then stops.

Only the exact reply `apply repair <manifest-id> <manifest-digest>` authorizes
apply. Ambiguous approval, approval of another digest, or a changed manifest
does not mutate.

This contract prevents a cooperating skill from treating vague conversation as
authorization. It is not a daemon authentication mechanism: another local
process can derive the phrase from the manifest and remains inside Wenlan's
existing trusted-local-process threat model.

### 4. Apply

`apply_repair` accepts only `manifest_id` and `approved_manifest_digest`; the
daemon loads its own immutable manifest bytes. It validates the digest and
schema, confirms no prior apply receipt exists, recomputes the target receipt,
and returns `409 repair_target_stale` on any CAS mismatch.

The core holds the per-manifest operation lock, then executes
`post_write::reclassify_memory_cas` in one transaction.
The writer validates target scope, updates every chunk consistently, verifies
the declared owner closure/non-target fingerprint, and commits. Any writer,
fingerprint, receipt, or artifact failure rolls back. The pending apply receipt
is fsynced before database commit, published no-clobber after commit, and
recovered after a crash by matching the current target to its recorded
after-receipt. A successful apply receipt contains the before/after target
receipts, actual allowed effects, writer result, and manifest digest.

### 5. Rerun and verify

The skill reruns fresh General and agent-assisted Deep with the identical
scope. It submits those typed final reports plus the apply receipt to
`record_repair_verification`.

This endpoint does not run lint. It validates report contracts, requires their
post-run snapshot receipts to describe the current state, verifies the target
evidence digest no longer appears in the classification finding, checks both
reports are complete, rejects any new actionable/incomplete check outside the
declared assertion set, binds both reports to current DB and Page receipts, and
confirms the apply receipt contains the in-transaction non-target fingerprint
proof. It does not require unrelated daemon state to remain frozen after the
apply transaction. Success writes the immutable verification receipt through
the same per-manifest lock and no-clobber publication rule.

If post-repair lint fails, the canonical mutation remains applied but the run
is visibly `applied_unverified`; the skill surfaces the failure and stops. It
does not auto-rollback or retry because a second mutation needs fresh approval.

## HTTP and MCP Surface

The daemon adds:

```text
POST /api/repairs/prepare
POST /api/repairs/apply
POST /api/repairs/verify
```

The MCP server adds typed local tools:

```text
prepare_repair
apply_repair
record_repair_verification
```

Every MCP implementation typed-deserializes daemon responses through
`wenlan-types`. No `serde_json::Value` response plumbing is permitted.

The shared plugin contract gains `lint-repair` on both Claude and Codex
surfaces. The skill is explicit-only and may call only the canonical lint tool
plus the three repair tools. It has no Bash or HTTP fallback for canonical data
mutation.

## Error Contract

- `422 unsupported_manifest_version`: schema is unknown.
- `422 unsupported_finding`: evidence/action has no registered resolver.
- `422 unsupported_writer`: writer is not enabled in this slice.
- `422 ambiguous_repair_target`: durable evidence resolves to zero/many owners.
- `422 approval_digest_mismatch`: approval is not for the immutable manifest.
- `409 repair_target_stale`: expected version/receipt or scope changed.
- `409 repair_already_applied`: immutable apply receipt already exists.
- `409 repair_verification_stale`: final report receipts are not current.
- `500 repair_artifact_failure`: durable artifact could not be written/read.
- `500 repair_effect_escape`: actual write escaped the declared owner closure;
  the database transaction is rolled back.

Failures never fall back to direct SQL, a broader writer, or a fresh implicit
manifest.

## Verification

Implementation follows RED-GREEN-REFACTOR and must prove:

- contract rejects unknown versions, fields, writers, multi-target manifests,
  no-op mutations, invalid memory types, and digest mismatches;
- prepare resolves one hashed semantic memory target and rejects stale,
  ordinal-only, cross-scope, ambiguous, or incomplete evidence;
- prepare changes only the external repair artifact directory;
- apply without the exact approval tuple performs zero canonical writes;
- CAS mismatch and duplicate apply perform zero writes;
- writer failure and effect-escape injection roll back every target chunk;
- successful apply changes only the declared memory owner closure;
- rollback and manifest artifact hashes verify after restart;
- concurrent apply/verify is serialized per manifest and cannot replace an
  immutable receipt;
- crash recovery promotes a committed pending receipt even after an unrelated
  background write;
- verification rejects stale/incomplete/new-finding reports and accepts the
  exact clean target delta, while fresh reports remain verifiable after
  unrelated post-apply daemon writes;
- verification rejects DB or Page receipts that no longer describe current
  state;
- existing `/api/lint` and both `/lint` skills remain read-only and contain no
  repair inference or mutation path;
- Claude/Codex plugin contract parity and typed MCP response checks pass.

Run Cargo work serialized with `CARGO_BUILD_JOBS=1` and
`CMAKE_BUILD_PARALLEL_LEVEL=1`. Do not run release builds for this slice. Check
memory pressure between heavy gates and stop below 20% free memory or when swap
is nearly exhausted. Run only one agent-assisted Deep call at a time.

## Existing-Data Rollout

Code and fixture verification complete before any live manifest is prepared.
Then:

1. run one live General and one agent-assisted Deep sequentially;
2. select at most one supported memory-classification finding;
3. prepare and display its exact manifest;
4. run `review3`: contract/correctness, adversarial safety/concurrency, and
   runtime/live-data reviewers submit independent verdicts;
5. stop for the user's exact approval tuple;
6. apply once, rerun lint, and record verification.

If no supported candidate survives Deep review, the slice remains complete and
no live data changes. A new writer adapter requires a separate reproduced
repair class, design amendment, RED tests, and approval.
