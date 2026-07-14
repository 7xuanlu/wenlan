# Sensitive Read Route Scope Contracts Design

**Status:** Approved for implementation planning on 2026-07-14.

## Decision

Repair every currently known Space-scoping bypass on Wenlan's sensitive read
surface through one core-owned typed scope resolver plus domain-owned core query
gates. Keep the daemon handlers thin, keep the existing routes and response
shapes, and use the canonical lint route catalog as the executable completion
ledger.

Space is the only selectable product scope axis. The `memories.space`,
`entities.space`, and `pages.workspace` columns bind different domain records to
the same Space namespace. A Page workspace is not a second scope axis. A chunk
is not a first-class scoped object; it inherits the Space of its logical
Memory/Document parent.

The implementation proceeds in four independently verifiable waves. The
canonical `serving.route_scope_contracts` violation count must move from 36 to
28, 14, 5, and finally 0. The final zero must be derived from the route catalog,
not hard-coded into lint output.

## Evidence Baseline

The current canonical catalog contains 55 sensitive read routes:

- 19 routes are explicitly global reads.
- 36 routes are Space-bound but currently have at least one missing selector,
  direct-ID gate, or unknown-Space fail-closed behavior.
- The 36 violations consist of 8 retrieval routes, 14 Memory/document routes,
  9 Page routes, and 5 Entity/KG routes.

Focused daemon contract tests currently prove three representative bypasses:

- an unknown requested Space falls back to an unscoped Memory list;
- a direct Memory detail read ignores a conflicting Space header;
- Page search ignores the Space header and returns cross-Space rows.

The desktop App currently calls `GET /api/chunks/{source_id}` from its
ChunkViewer. The App accepts a `source` argument, but its daemon request omits
that argument. The daemon then tries `source='memory'` before
`source='file'`, performs one query per chunk, and converts a database error into
a partial or empty `200` response. This endpoint is active and content-bearing,
so it must be scoped, but its route shape and source ambiguity are not silently
redesigned in this project.

## Goals

1. Give every Space-bound sensitive read one deterministic selector contract.
2. Reject an explicit unknown Space with HTTP `422` rather than falling back to
   Global.
3. Make direct-ID reads return `404` when the record exists outside the selected
   Space, without disclosing cross-Space existence.
4. Enforce Space filtering inside core-owned queries before sensitive rows are
   materialized.
5. Preserve unselected Global behavior and every existing success response
   shape.
6. Make `serving.route_scope_contracts` an executable ledger that reaches zero
   only as each route family becomes correctly gated.
7. Preserve the single `wenlan lint` runner and existing report/check contract.

## Non-Goals

- No mutating route changes.
- No new API endpoint, CLI command, MCP tool, lint product surface, or response
  envelope.
- No authentication, authorization, tenant, user, agent-trust, or ACL system.
- No separate Page workspace selector.
- No first-class Chunk scope or Chunk catalog family.
- No chunk endpoint removal, source parameter migration, or desktop App
  migration.
- No change to the 19 routes whose canonical contract is Global.
- No changes to lint report outcomes, severities, exit codes, profile semantics,
  snapshot machinery, or deep-provider routing.
- No generic SQL scope-builder abstraction shared across unrelated domains.

## Scope Vocabulary

### Product Selector

The HTTP product selector remains `space`:

- body `space`, when a route has a request body;
- query `space` or an existing compatibility alias, when a route has a query
  selector;
- `X-Wenlan-Space`, with `X-Origin-Space` retained as its legacy alias.

`workspace` remains a Page storage/projection field. It is not exposed as a
second read-scope selector.

### Typed Effective Scope

Core owns one resolved type:

```rust
pub enum ReadScope {
    Global,
    Space(String),
    Uncategorized,
}
```

- `Global` means no non-empty selector was supplied.
- `Space(name)` means `name` exactly identifies a registered Space after
  trimming surrounding whitespace.
- `Uncategorized` means the exact reserved selector `uncategorized` and matches
  records whose binding column is `NULL`.

`uncategorized` is a reserved selector for NULL-bound records, not another
axis. Resolving or preventing a registered Space whose literal name collides
with this reserved selector is outside this read-route repair and must be
handled by a separate Space-write contract if live evidence proves the
collision exists.

### Scope Binding

The route catalog's current name `ScopeOwner` is misleading because it does not
describe authorization ownership. Rename it to `ScopeBinding` while touching
the catalog:

```rust
pub enum ScopeBinding {
    Global,
    MemorySpace,
    PageWorkspace,
    EntitySpace,
}
```

The three non-global variants select the storage column used by the owning
domain query. They do not introduce distinct user-visible selectors.

## Selector Semantics

1. Normalize each candidate by trimming whitespace and treating an empty value
   as absent.
2. A non-empty body or query selector wins over a header selector.
3. A missing or empty body/query selector may fall back to the header.
4. If no non-empty selector remains, resolve `ReadScope::Global` without a Space
   lookup.
5. Resolve the exact reserved literal `uncategorized` to
   `ReadScope::Uncategorized`.
6. Resolve an exact registered Space name to `ReadScope::Space(name)`.
7. Reject every other explicit value with `ServerError::ValidationError` and
   HTTP `422`. Do not retry as Global and do not fall back from an invalid
   body/query selector to a valid header.

Selector parsing remains HTTP framing in `wenlan-server`; validation and typed
resolution belong to `wenlan-core`. Routes without body or query selectors use
the preferred or legacy Space header. This avoids both middleware magic and
per-handler interpretations of what a valid Space means.

## Architecture

### 1. Core-Owned Resolver

Add a focused core module for `ReadScope`, its resolution error, and resolution
against registered Spaces. It must not depend on Axum or HTTP types. The
resolver performs at most one registration lookup for an explicit ordinary
Space and no lookup for Global or `uncategorized`.

The daemon maps a resolution error to the existing JSON error envelope and HTTP
`422`. It does not convert the error to `None`.

### 2. Thin Daemon Extraction

Add one server helper that receives an optional primary selector and extracted
Space header, applies precedence and normalization, then calls the core
resolver. Handlers snapshot `Arc<MemoryDB>` out of `ServerState` before awaiting
resolution or a query.

The helper does not load or filter domain rows. It only returns a typed
`ReadScope` or `ServerError`.

### 3. Domain-Owned Query Gates

Each domain accepts `&ReadScope` at its existing query ownership boundary:

- Memory/document queries bind against `memories.space`.
- Page queries bind against `pages.workspace`.
- Entity queries bind against `entities.space`.
- Relation queries require both endpoint Entities to match a selected Space.

Global omits the binding predicate. Space uses equality against the selected
registered name. Uncategorized uses `IS NULL`. Filtering occurs in SQL or the
domain's equivalent candidate-generation query before rows are returned to the
daemon. A handler-level post-filter is not an acceptable implementation.

Do not introduce one generic dynamic SQL builder. Domain queries differ in
joins, visibility rules, pagination, retrieval candidates, and direct-ID
semantics; they should share the typed scope contract, not query text.

### 4. Direct-ID Non-Disclosure

A direct-ID domain query combines identifier and scope binding in the same
lookup. Its externally visible result is:

| Effective scope | Record binding | Result |
|---|---|---|
| Global | any | existing success response |
| Space A | Space A | existing success response |
| Space A | Space B or NULL | `404` |
| Uncategorized | NULL | existing success response |
| Uncategorized | any named Space | `404` |
| explicit unknown selector | any | `422` before record lookup |

The server must not first fetch an unrestricted record and then disclose that
it belongs elsewhere.

### 5. Chunk Projection Contract

`GET /api/chunks/{source_id}` remains a Memory/document read:

- inherit scope from `memories.space`;
- preserve the current `Vec<MemoryDetail>` JSON shape;
- preserve memory-before-file source precedence for compatibility;
- query a bounded ordered population instead of looping until a missing index;
- order by `chunk_index` deterministically;
- return `404` for a selected-scope mismatch or absent logical source;
- propagate database failure as `500`, never partial or empty success.

The implementation does not add a `Chunk` binding, add a `source` path/query
parameter, or modify the desktop App. A source-ID collision between Memory and
file records remains a separately reproducible API-contract risk. This project
locks compatibility behavior rather than guessing a migration.

### 6. Entity and Relation Contract

Entity list, search, detail, and suggestions filter on `entities.space`.

For a selected Space, a relation is visible only when both endpoint Entity rows
match that Space. A relation whose endpoints occupy different Spaces is visible
only in Global for v1. The relation row itself does not create another scope
axis, and a matching subject alone is insufficient.

### 7. Canonical Route Catalog

The route catalog remains the single completion ledger. Extend its vocabulary
only as required to describe the repaired behavior:

- `SelectorPrecedence::HeaderOnly` for scoped reads without body/query input;
- `UnknownScopePolicy::Rejected` for all Space-bound routes;
- `DirectIdGate::Enforced` for scoped direct-ID reads;
- `ScopeBinding` in place of `ScopeOwner`.

After each wave, update only the rows repaired in that wave. The violation
predicate must continue deriving its count from row contracts. A Space-bound
row is non-violating only when it has a usable selector, rejects unknown
explicit Spaces, and enforces a direct-ID gate when applicable.

The `serving.route_scope_contracts` check ID, report type, severity, and metric
codes remain unchanged. Its `AffectedRecords` count moves as catalog rows become
truthfully compliant.

## Delivery Waves

### Wave 1: Resolver and Retrieval, 36 to 28

Introduce the typed resolver and server extraction helper, then gate:

- `POST /api/search`
- `POST /api/context`
- `GET /api/memory/recent`
- `GET /api/memory/unconfirmed`
- `POST /api/memory/search`
- `POST /api/memory/list`
- `GET /api/memory/nurture`
- `GET /api/memory/pinned`

This wave establishes selector precedence, unknown=`422`, Global compatibility,
and candidate generation that cannot surface cross-Space retrieval rows.

### Wave 2: Memory and Document Reads, 28 to 14

Gate:

- `GET /api/memory/{source_id}/enrichment-status`
- `GET /api/memory/{id}/revisions`
- `GET /api/indexed-files`
- `GET /api/chunks/{source_id}`
- `GET /api/suggest-tags`
- `GET /api/memory/{id}/detail`
- `GET /api/memory/by-ids`
- `GET /api/memory/{id}/versions`
- `GET /api/decisions`
- `GET /api/briefing`
- `GET /api/memory/pending-revisions`
- `GET /api/memory/pending-revision/{source_id}`
- `GET /api/snapshots/{id}/captures`
- `GET /api/snapshots/{id}/captures-with-content`

Memory-derived records inherit the logical Memory's Space. Snapshot capture
reads must scope through their captured Memory membership rather than inventing
snapshot ownership.

### Wave 3: Pages, 14 to 5

Gate:

- `GET /api/pages/recent`
- `GET /api/pages/recent-changes`
- `GET /api/pages`
- `POST /api/pages/search`
- `GET /api/pages/orphan-links`
- `GET /api/pages/{id}`
- `GET /api/pages/{id}/sources`
- `GET /api/pages/{id}/links`
- `GET /api/pages/{id}/revisions`

Page list/search/activity queries bind on `pages.workspace`. Direct Page child
reads first gate the parent Page in the same query path; Page sources must not
return a cross-Space parent or cross-Space Memory content through an in-Space
Page.

### Wave 4: Entities and KG, 5 to 0

Gate:

- `POST /api/memory/entities/list`
- `POST /api/memory/entities/search`
- `GET /api/memory/entities/{entity_id}`
- `GET /api/memory/entity-suggestions`
- `GET /api/knowledge/recent-relations`

This wave applies the two-endpoint relation rule and completes the canonical
route contract.

## Error and Completeness Semantics

- Unknown explicit Space: `422` with the existing `{ "error": string }`
  envelope.
- Direct-ID scope mismatch: `404`, indistinguishable from a missing ID.
- Database or retrieval failure: existing non-success error mapping; never an
  empty success used as failure concealment.
- No selector: Global behavior remains available and unchanged.
- Cross-Space rows observed during a selected read: test failure and unresolved
  catalog violation, never a warning-only response.
- Scope resolution failure occurs before domain candidate generation.

This project does not add an HTTP completeness field. Completeness here is an
implementation and test property: a successful scoped response has been fully
filtered by its domain query, while execution failures remain non-success
responses.

## RED-First Verification Strategy

Every wave begins with focused tests that assert the approved behavior and fail
against the current implementation. The fixtures contain registered Spaces
`work` and `personal`, plus NULL-bound records and cross-Space canaries.

Each route family must cover the applicable cases:

1. no selector preserves the prior Global response;
2. body/query selector wins over a conflicting header;
3. empty primary selector permits header fallback;
4. an unknown primary selector returns `422` and does not fall back to header;
5. `uncategorized` returns only NULL-bound records;
6. selected Space excludes records from every other Space and NULL;
7. direct-ID mismatch returns `404`;
8. database errors remain errors rather than partial success;
9. existing success response JSON shape remains unchanged.

Additional domain acceptance cases:

- retrieval candidate pools contain no cross-Space Page, graph, episode, fact,
  or summary additions;
- chunk reads preserve deterministic index order and memory-before-file
  precedence;
- Page child reads cannot bypass the parent Page gate;
- Page source responses cannot leak cross-Space Memory content;
- Entity vector/name candidates are scoped before ranking;
- scoped relations require both endpoints to match;
- Global still exposes an intentionally cross-Space relation.

Catalog tests assert exact wave counts `36`, `28`, `14`, `5`, and `0` only at
the corresponding implementation checkpoints. The final lint test obtains zero
from `scope_contract_violations().count()` and asserts a passing
`serving.route_scope_contracts` result.

## Verification Gates

For each wave:

1. Observe the focused RED tests failing for the intended missing contract.
2. Implement the minimum resolver/query/handler/catalog change for that wave.
3. Run the focused core and daemon tests for the changed route families.
4. Run all `wenlan-core` and `wenlan-server` library tests serially.
5. Run `cargo fmt --all -- --check`.
6. Run Clippy for every changed crate and target required by the repository
   pre-push gate.
7. Inspect the catalog delta and confirm the exact next violation count.
8. Confirm no mutating route or public success response shape changed.

Before publication, run the workspace library test gate and the existing
daemon HTTP acceptance coverage relevant to search, context, Space fallback,
Pages, and KG reads. No live database is required because the scope contract is
fully reproducible with hermetic two-Space fixtures.

## Risks and Controls

### Existing tests encode the defect

Several current tests assert unknown-Space fallback or cross-Space results.
Replace those assertions only after the new RED tests prove the approved
contract. Do not delete coverage without an equivalent positive assertion.

### Retrieval side channels bypass the base filter

Graph, Page, episode, fact, summary, rerank, and other candidate streams may
join after a base Memory filter. Wave 1 must verify the final candidate
population, not only the first SQL query.

### Direct-ID post-filtering leaks existence

Fetching unrestricted data and returning a special mismatch error can reveal
that an ID exists elsewhere. Combine ID and scope in the core lookup and use the
same `404` for absent and mismatched rows.

### Page sources span two bound domains

An in-Space Page can reference a Memory whose stored Space disagrees. The Page
gate alone is insufficient when the response embeds Memory content. The query
must enforce both the parent Page scope and returned Memory scope; inconsistent
links may be omitted or surfaced by lint, but must not leak content.

### Entity relations have two endpoints

Filtering only one endpoint admits cross-Space graph edges. Require both
endpoint bindings for selected reads and retain Global behavior for graph-wide
inspection.

### Chunk source ambiguity is real but separate

The App currently drops its `source` argument before HTTP. Solving that requires
a coordinated daemon and desktop App contract migration. This project preserves
the current source precedence, scopes the returned content, and fails loudly on
database errors. A later project must begin with a deterministic collision
reproducer before changing the route.

### Catalog can claim compliance before code does

Update a route row only in the same task and commit as its passing handler/core
contract tests. The canonical lint count is a ledger, not an implementation
substitute.

## Acceptance Gates

The project is complete only when all of the following are true:

- all 36 current Space-bound route violations have passing behavioral tests;
- `scope_contract_violations().count()` is zero;
- `serving.route_scope_contracts` passes with zero affected routes;
- every explicit unknown Space returns `422` before candidate generation;
- every scoped direct-ID mismatch returns `404` without existence disclosure;
- selected responses contain no cross-Space or NULL-bound canary rows;
- `uncategorized` responses contain only NULL-bound rows;
- Global responses preserve pre-change success shapes and intended populations;
- chunk response shape and memory-before-file precedence remain compatible;
- chunk DB failures no longer become partial or empty `200` responses;
- scoped Page sources do not embed cross-Space Memory content;
- scoped relations include only edges whose two endpoints match;
- no mutating route, MCP/CLI surface, lint report schema, or deep-provider
  behavior changed;
- formatting, changed-crate Clippy, workspace library tests, and relevant daemon
  HTTP acceptance tests pass.

## Deferred Follow-Ups

These require separate evidence and approval:

- add an explicit source discriminator to the chunk HTTP contract and migrate
  the desktop App;
- decide how Space creation handles the reserved `uncategorized` literal;
- reconsider any of the 19 deliberately Global sensitive reads if product
  requirements later make them Space-selectable;
- apply the same read-scope contract to a mutating route only through a separate
  write-authorization and CAS design;
- add user, tenant, ACL, or authenticated capability boundaries.
