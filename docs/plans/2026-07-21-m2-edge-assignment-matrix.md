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
(relation_type / locator / label_key / cited_id). Each part is **length-prefixed**
before hashing (its byte length, then its bytes) so no two distinct tuples can
collide by concatenation ambiguity even if a part itself contains the separator
byte (locators/labels are unvalidated UTF-8). A retried write recomputes the
same `edge_id`; `INSERT … ON CONFLICT(edge_id) DO NOTHING` (backfill) /
`DO UPDATE` (live) converges on the one edge — no duplicate voter — exactly the
§6.1 retry-identity guarantee, at the edge grain. Every edge still carries its
`operation_id` for audit/receipt linkage.

### Shared edge_id lineage precedence (evidence > synthesis)

One content-addressed `edge_id` can be produced by more than one legacy store
for the same `(page, memory)` fact: `page_evidence` (and `page_sources`) →
`evidence`, a `pages.citations` entry → `synthesis`. The resolved `lineage` is
**deterministic and order-independent**: `evidence` outranks `synthesis`.

- **Backfill** already yields this: migration 81 runs `page_sources` /
  `page_evidence` (→ `evidence`) *before* `page_citations` (→ `synthesis`), and
  `insert_backfilled_edge` is `ON CONFLICT DO NOTHING`, so the first (evidence)
  writer wins.
- **Live** dual-write's `ON CONFLICT DO UPDATE` upgrades `synthesis`→`evidence`
  (and never downgrades), so a live edge and its backfilled twin agree
  regardless of which store's write fired first.

This is fence-safe because, for a given `edge_id`, every writer resolves the
identical page/memory spaces: `evidence`/`synthesis` occur only same-space (the
fence already passed at insert), `legacy` only cross-space, so the precedence
never crosses the `legacy` boundary and the row stays fence-valid.

## Open design question (M2 PR-1 review): cross-space live typed writes

The "Two *fenced* endpoints in different non-NULL spaces is always rejected"
clause above is in tension with the live dual-write code as shipped, and the
resolution is a **design decision deferred out of PR-1** (documented here, not
silently resolved).

**The tension.** A genuinely *new* live write can present two fenced endpoints
in different non-NULL spaces — e.g. `create_relation` over two entities the
agent created in different spaces (entity identity is global, so resolution
crosses spaces), or the `cross_space_discovery` feature minting a page in one
space that cites memories in another. The live dual-write classifies such a
cross-space pair as `lineage='legacy'` (fence-exempt) rather than as its typed
lineage, so the edge is **written as legacy, not rejected**.

**Why the code does this (Option B, shipped).** PR-1's prime directive is that
the shadow dual-write must **not change legacy write behavior** (never weaken an
existing check; no user-visible change). Because the edge write shares the
legacy write's single transaction, classifying the cross-space pair as a *typed*
edge would make the fence **reject** it and thereby **roll back the legacy
write** — a cross-space relation/citation that succeeds today would start
failing. Downgrading to `legacy` keeps the legacy write intact and the shadow
faithful.

**The fork.**
- **Option A — honor the matrix literally (reject cross-space typed writes).**
  Consequence: the single-transaction dual-write rolls back the legacy write, so
  a currently-successful cross-space `create_relation` / cross-space citation
  begins to fail. This *changes legacy behavior* — outside PR-1's contract and
  the goal-prompt's stop condition ("if the audit surfaced a new-write path
  minting genuine cross-space fenced links the fence would newly reject, that is
  the stop condition — report rather than guess around").
- **Option B — downgrade cross-space typed writes to `legacy` (current code).**
  Consequence: legacy behavior is preserved and atomicity holds, but the
  matrix's "always rejected" clause is inaccurate for these new-write paths; the
  clause should be amended to "downgraded to `legacy`" once the design owner
  confirms.

Both options either change legacy behavior (A) or amend the matrix (B); neither
is a pure implementation fix, so PR-1 ships Option B and flags the matrix clause
for the design owner. `audit_legacy_cross_space_links` reports the pre-fence
cross-space counts per store so the blast radius is measured, not guessed.
