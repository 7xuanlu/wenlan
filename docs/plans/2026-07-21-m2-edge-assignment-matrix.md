# M2 edge assignment matrix (PR-1, stage a)

Status: committed **before** the `edges` schema code, per the M2 goal prompt
("The assignment matrix is the FIRST artifact, committed before schema code").
It pins, for every one of the five legacy stores and every new typed write,
which `edge_type` the row becomes and what `lineage`, `root_id`, and `grounded`
it gets. Source of truth: spec v3 §2 (edges) and the Q6 decision draft.

## The one grounding rule (spec §2)

> Only edges whose derivation bottoms out in captured external reality vote.

`grounded` is computed at write time from the derivation chain and is then
immutable. **M2 has no span-validation pipeline and does not root the existing
corpus** (both are M3+ / deferred, per the goal prompt's out-of-scope list and
its "confidently rooting the full memory corpus" stop-and-confirm clause).
Therefore, honestly, **every edge M2 writes is `grounded = false`** — a typo-safe
extraction edge cannot claim grounding without the span validator, and a `cites`
edge cannot claim it without a rooted, external memory. `grounded` is a real
computed function (`grounded_at_write`) that returns `false` for all M2 inputs
because no M2 input can satisfy the rule yet; M3+ extends it. This is "legacy
honesty" applied uniformly, not a stub.

## root_id in M2

`provenance_roots` (the table, the Q6 digest, `ON CONFLICT … RETURNING`
convergence, independence grouping) is **built and tested** in M2. But
**attaching roots to the existing memory corpus is deferred** (the goal prompt's
explicit stop-and-confirm: rooting the full corpus balloons the rung; building
`genesis_candidate_roots` / the floor's consumption is M6). So in M2 every edge
carries `root_id = NULL`. The root machinery is proven by a direct two-writer
convergence test, not by corpus attachment. When M3+ roots memories, edge
`root_id` back-references land then.

## Node kinds and the space of an edge

`src_kind` / `dst_kind` ∈ `{page, memory, entity, external}`.
Endpoint space lookups (`pages.space`, `memories.space`, `entities.space`):
`pages.space` is NOT NULL since M1 (migration 80); `memories.space` and
`entities.space` are **nullable** (migration 50 renamed `domain→space`; only
pages were normalized in M1). An edge's `space` is derived from a fenced
endpoint (the page for page-incident edges; the source entity for `relates`).

## Assignment matrix

| Legacy store | → edge_type | src_kind → dst_kind | New-write `lineage` | `grounded` | `root_id` | Natural key (→ content-addressed `edge_id`) |
|---|---|---|---|---|---|---|
| `relations` | `relates` | entity → entity | `assertion` | false | NULL | (`relates`, from_entity, to_entity, relation_type) |
| `page_sources` | `cites` | page → memory | `evidence` | false | NULL | (`cites`, page_id, memory_source_id) |
| `page_evidence` (source_kind=`memory`) | `cites` | page → memory | `evidence` | false | NULL | (`cites`, page_id, locator) |
| `page_evidence` (source_kind=`external_url`/`external_file`) | `cites` | page → external | `evidence` | false | NULL (external) | (`cites`, page_id, locator) |
| `page_links` (target resolved) | `links` | page → page | `synthesis` | false | NULL | (`links`, source_page_id, label_key) |
| `pages.citations` blob (per entry) | `cites` | page → memory | `synthesis` | false | NULL | (`cites`, page_id, cited_id) |

`lineage` records the *immediate author* and is display/audit only in M2
(grounded is the load-bearing bit; all M2 edges are non-voting). Choices:
`relates` from extraction = `assertion` (the extractor asserts the relation);
`cites` from page provenance = `evidence` (the page's evidentiary basis);
`links` = `synthesis` (distill-generated cross-reference). These are refined
when M3+ adds span validation and real grounding.

## Backfill classification (legacy honesty) → the classifiable-vs-unknown report

Existing rows are mirrored into `edges` once, by migration 81's backfill. Per the
spec's legacy-honesty rule and the goal prompt's "report of classifiable vs
unknown counts", each backfilled row is classified deterministically:

- **classifiable** — the store + row fields unambiguously resolve to a real
  edge (both endpoints resolvable, semantics unambiguous): stamped with the
  matrix `lineage` above, `grounded=false`, `root_id=NULL`, fence **exempt**
  (backfill shadows predate the fence — see below).
- **unknown** → `lineage = 'legacy'` — the row is ambiguous or dangling and its
  authorship can't be confidently classified:
  - `relations` with a dangling endpoint (from/to entity absent),
  - `page_links` with `target_page_id IS NULL` (orphan wikilink — no page→page
    edge exists yet),
  - `pages.citations` entries that don't parse / don't resolve to a memory.

Counts land in the durable `edges_migration_state` row (`report_json`) and are
cited in the PR body.

## The fence, NULL-safety, and the lineage='legacy' exemption

Spec §2: a trigger with **NULL-safe (`IS NOT`) comparisons** enforces both
endpoints in the edge's `space`; sole spec exemption is `cites` to an external
URI. `IS NOT` is SQLite's null-safe distinct operator, so a NULL-space endpoint
against a non-NULL `edge.space` is **caught (rejected)**, never silently passed —
which is the whole point (a `!=` trigger silently passes NULL rows).

Because `memories.space` / `entities.space` are still nullable (not normalized
until a later rung), a strict fence would reject a *new* internal edge whose
memory/entity endpoint is a legacy NULL-space row — which would either break the
authoritative legacy write (unacceptable: never weaken an existing check, no
user-visible change) or the single-transaction atomicity proof. Resolution
(assume-and-announce; not a wire-contract or user-visible-data change):

- The fence trigger is exempt for `lineage='legacy'` edges **and** external-URI
  `cites`, and binds (`IS NOT`) on every other (typed) edge.
- Dual-write derives `edge.space` from the page endpoint (always fenced) and
  checks the other endpoint's space in Rust *before* the insert. If both
  endpoints are fenceable and co-spaced → the edge is typed (real `lineage`) and
  the fence passes. If the other endpoint is NULL-spaced / unfenceable (a legacy
  memory/entity not yet normalized) → the edge is written as `lineage='legacy'`
  (fence-exempt), grounded=false. **The edge is always written in the same
  transaction** → atomicity holds, the legacy write is never blocked, and the
  shadow stays faithful. Two *fenced* endpoints in different non-NULL spaces is
  always rejected.

A pre-migration audit (`audit_legacy_cross_space_links`) reports, per store, how
many legacy links are same-space / cross-space / NULL-space **before** the fence
binds; those counts are in the PR body. If the audit surfaced a *new-write* path
minting genuine cross-space fenced links the fence would newly reject, that is
the goal prompt's stop condition — the audit result is reported rather than
guessed around.

## Retry identity / idempotency

`edge_id` is content-addressed: `sha256(edge_type, src_kind, src_id, dst_kind,
dst_id, discriminator)` (sha2 is already the crate's hasher, used for
`memories.content_hash`) where the discriminator is the store's natural-key tail
(relation_type / locator / label_key / cited_id). A retried write recomputes the
same `edge_id`; `INSERT … ON CONFLICT(edge_id) DO NOTHING` converges on the one
edge — no duplicate voter — exactly the §6.1 retry-identity guarantee, at the
edge grain. Every edge still carries its `operation_id` for audit/receipt
linkage.
