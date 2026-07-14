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
canonical `serving.route_scope_contracts` violation count must move from 40 to
32, 14, 5, and finally 0. Counts are progress summaries, not proof: each wave
also freezes the exact remaining route-key set and runs the corresponding HTTP
behavior cases. The final zero must be derived from the route catalog and backed
by an executed behavior registry, not hard-coded into lint output.

## Evidence Baseline

The current canonical catalog contains 55 sensitive read routes:

- 19 routes are currently labeled as global reads, but four of them expose
  Memory-derived row content and require Space selection:
  `GET /api/home-stats`, `GET /api/retrievals/recent`,
  `GET /api/activities`, and `GET /api/tags`.
- 36 routes are already labeled Space-bound but currently have at least one missing selector,
  direct-ID gate, or unknown-Space fail-closed behavior.
- The corrected contract therefore has 40 Space-bound routes and 15 deliberate
  Global routes. The 40 consist of 8 retrieval routes, 18 Memory/document and
  derived-activity routes, 9 Page routes, and 5 Entity/KG routes.

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
   only as each route family becomes correctly gated and behavior-tested.
7. Preserve the single `wenlan lint` runner and existing report/check contract.
8. Freeze and justify the truly Global route set instead of treating every
   existing `ScopeOwner::Global` label as authoritative.

## Non-Goals

- No mutating route changes.
- No new API endpoint, CLI command, MCP tool, lint product surface, or response
  envelope.
- No authentication, authorization, tenant, user, agent-trust, or ACL system.
- No separate Page workspace selector.
- No first-class Chunk scope or Chunk catalog family.
- No chunk endpoint removal, source parameter migration, or desktop App
  migration.
- No change to the 15 routes whose corrected canonical contract is deliberately
  Global.
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
axis. Space creation currently permits that exact name, so reads must handle the
collision safely now: before treating the literal as the NULL selector, check
for an exact registered Space named `uncategorized`. If it exists, reject the
selector as ambiguous with HTTP `422`; never expose either population. Reserving
the name on the write path is a separate follow-up.

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
5. For the exact case-sensitive reserved literal `uncategorized`, first check
   whether a registered Space has that exact name. If so, reject the ambiguous
   selector with HTTP `422`; otherwise resolve `ReadScope::Uncategorized`.
6. Resolve an exact registered Space name to `ReadScope::Space(name)`.
7. Reject every other explicit value with `ServerError::ValidationError` and
   HTTP `422`. Do not retry as Global and do not fall back from an invalid
   body/query selector to a valid header.

Selector parsing remains HTTP framing in `wenlan-server`; validation and typed
resolution belong to `wenlan-core`. `X-Wenlan-Space` wins when both it and the
legacy `X-Origin-Space` header are present. Routes without a body/query selector
use the header only. Query routes use query then header. `POST /api/pages/search`
adds an optional `space` field to the existing shared request type, and
`POST /api/memory/entities/search` adds an optional `space` field to its local
request type; both use body then header. An unsupported `space` query/body field
on a header-only route is not a selector and is ignored like any other unknown
request field. This avoids middleware magic and per-handler interpretations of
what a valid Space means.

## Architecture

### 1. Core-Owned Resolver

Add a focused core module for `ReadScope`, its resolution error, and resolution
against registered Spaces. It must not depend on Axum or HTTP types. The
resolver performs at most one registration lookup for any explicit value,
including `uncategorized`, and no lookup for Global.

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

The same catalog field must describe more than single-ID routes. Replace
`DirectIdGate` with a selection-gate vocabulary that distinguishes:

```rust
pub enum SelectionGate {
    NotApplicable,
    SingleIdMissing,
    SingleId404,
    BatchMissing,
    BatchFiltered,
    ParentCollectionMissing,
    ParentCollectionFiltered,
}
```

`GET /api/memory/by-ids` omits both nonexistent and out-of-scope IDs, preserves
input order, and never turns a mixed batch into `404`. Page child routes and
snapshot capture routes gate through their parent collection rather than
pretending the child ID owns a Space. `GET /api/memory/{id}/detail` changes its
current missing response from `200 {"memory":null}` to the same `404` body used
for a selected-scope mismatch. For every true direct-ID read, missing and
mismatched records have identical status and response bytes, and no
mismatch-specific log is emitted above DEBUG.

### 5. Chunk Projection Contract

`GET /api/chunks/{source_id}` remains a Memory/document read:

- scope every candidate row through `memories.space`, including legacy
  `source='file'` rows;
- preserve the current `Vec<MemoryDetail>` JSON shape;
- apply scope before preserving memory-before-file source precedence among
  in-scope candidates;
- query a bounded ordered population instead of looping until a missing index;
- order by `chunk_index` deterministically;
- return `404` for a selected-scope mismatch or absent logical source;
- propagate database failure as `500`, never partial or empty success.

The implementation does not add a `Chunk` binding or a `source` path/query
parameter. Tests cover file-only rows and a Memory/file source-ID collision so
compatibility is proven rather than assumed. The desktop App is not modified,
but its existing `ChunkViewer` call is included in downstream compatibility
verification.

### 6. Entity and Relation Contract

Entity list, search, and detail filter on `entities.space`. Add route-specific
scoped query wrappers rather than changing the shared unscoped
`search_entities_by_vector` helper used by graph augmentation and entity
resolution.

For a selected Space, a relation is visible only when both endpoint Entity rows
match that Space. A relation whose endpoints occupy different Spaces is visible
only in Global for v1. The relation row itself does not create another scope
axis, and a matching subject alone is insufficient.

`GET /api/memory/entity-suggestions` returns refinement proposals rather than
Entity rows. Its selected-scope visibility is therefore derived from all
referenced source Memories: a proposal is visible only when `source_ids` is
non-empty, every referenced Memory exists, and all of them match the selected
scope. Mixed, missing-owner, and empty-source proposals are omitted. Global
behavior remains unchanged. The route remains in the Entity/KG delivery wave
but its catalog binding is `MemorySpace`.

### 7. Canonical Route Catalog

The route catalog remains the single completion ledger. Extend its vocabulary
only as required to describe the repaired behavior:

- `SelectorPrecedence::{BodyThenHeader, QueryThenHeader, HeaderOnly}` for the
  three supported selector shapes;
- `UnknownScopePolicy::Rejected` for all Space-bound routes;
- `SelectionGate` for single-ID, batch, and parent-collection gating;
- `ScopeBinding` in place of `ScopeOwner`.

After each wave, update only the rows repaired in that wave. The violation
predicate must continue deriving its count from row contracts. A Space-bound
row is non-violating only when it has a usable selector, rejects unknown
explicit Spaces, and enforces a direct-ID gate when applicable.

The `serving.route_scope_contracts` check ID, report type, severity, and metric
codes remain unchanged. Its `AffectedRecords` count moves as catalog rows become
truthfully compliant.

Catalog metadata is not behavioral proof. Freeze all 55 exact `(method, path)`
keys, the 40 scoped keys, and the 15 Global keys in tests. A test-only executed
behavior-case registry must have exact set equality with the 40 scoped catalog
keys. Each registered case sends a real HTTP request against a hermetic daemon
router and verifies the route's applicable scope behavior; merely constructing
the router or registering a string does not count.

### 8. Deliberate Global Routes

The following exact 15 routes remain Global and are protected by tests that a
Space header does not accidentally change their response contract:

1. `GET /api/profile`
2. `GET /api/agents`
3. `GET /api/agents/{name}`
4. `GET /api/memory/stats`
5. `GET /api/spaces`
6. `GET /api/sources`
7. `GET /api/profile/narrative`
8. `GET /api/knowledge/count`
9. `GET /api/onboarding/milestones`
10. `GET /api/import/state`
11. `GET /api/memory/rejections`
12. `GET /api/refinery/queue`
13. `GET /api/capture-stats`
14. `GET /api/decisions/domains`
15. `GET /api/snapshots`

The content-bearing exceptions are deliberate. Rejections are pre-admission
review data with no accepted Space binding. Refinement queue entries are global
maintenance proposals that can span domains. Snapshot list rows are global
session objects whose summaries may be mixed-space. Profile narrative is global
identity. Aggregate-only routes retain their existing aggregate contract.

### 9. Orphan Binding Audit

Before route changes, run a read-only preflight over distinct non-NULL
`memories.space`, `entities.space`, and `pages.workspace` values and compare
them with the Space registry. Record orphan values in the implementation
evidence. Selected reads remain fail-closed because only registered Spaces can
resolve. This project does not mutate or migrate orphan bindings; extending
lint coverage for Memory/Page orphan bindings is a follow-up unless an existing
check can be corrected surgically without changing this delivery contract.

## Delivery Waves

### Wave 1: Resolver and Retrieval, 40 to 32

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
and candidate generation that cannot surface cross-Space retrieval rows. Tests
enable Page, graph, episode, fact, and summary channels and give every enabled
stream an in-scope positive canary plus cross-Space and NULL negative canaries.
They include `surface_new` graph rows, `uncategorized` fact rows, and highly
ranked out-of-scope candidates before `LIMIT` so an empty supplemental stream or
post-ranking filter cannot produce a false green. Internal Page/Entity/graph
candidate helpers receive the scope in this wave even though their standalone
HTTP routes are delivered in later waves. Page candidates bind only through
`pages.workspace`; source-Memory overlap cannot override a workspace mismatch.

### Wave 2: Memory, Document, and Derived Activity Reads, 32 to 14

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
- `GET /api/home-stats`
- `GET /api/retrievals/recent`
- `GET /api/activities`
- `GET /api/tags`

Memory-derived records inherit the logical Memory's Space. Snapshot capture
reads must scope through their captured Memory membership rather than inventing
snapshot ownership.

Additional route contracts in this wave are:

- `GET /api/home-stats` filters both row populations and aggregates by selected
  Memory scope; Global remains unchanged.
- `GET /api/retrievals/recent` and `GET /api/activities` expose a selected event
  only when it has a non-empty referenced-ID set and every referenced existing
  owner matches the selected scope. Mixed, missing-owner, and no-ID events are
  omitted, and Page titles/snippets are resolved only from in-scope rows.
- `GET /api/tags` includes Memory/document keys only through matching
  `memories.(source, source_id)` rows and Page keys only through matching
  `pages.(id, workspace)` rows. Orphan keys are omitted in selected reads and
  retained in Global; the distinct tag list is recomputed from the included
  map.
- Global briefing uses the existing singleton cache. Selected Space and
  Uncategorized briefing bypass that cache, compute only from scoped queries,
  and never read or write the Global cache. No per-scope cache schema is added.
- Snapshot capture scope derives from the current Space of referenced Memories
  because capture refs do not persist historical Space. A mixed snapshot
  returns only matching captures. A selected snapshot with zero visible
  captures returns the same `404` as a nonexistent snapshot. The with-content
  route uses the same scoped set and propagates required chunk/index failures.

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

`pages.space` remains the Page category/page-type filter and can be combined
independently with the product scope. Fixtures always assign different values
to `pages.space` and `pages.workspace` so accidentally filtering the category
column cannot pass. A Page in the wrong/NULL workspace remains invisible even
if it has in-scope source-Memory overlap.

### Wave 4: Entities and KG, 5 to 0

Gate:

- `POST /api/memory/entities/list`
- `POST /api/memory/entities/search`
- `GET /api/memory/entities/{entity_id}`
- `GET /api/memory/entity-suggestions`
- `GET /api/knowledge/recent-relations`

This wave applies the two-endpoint relation rule, the conservative
Memory-derived suggestion rule, and completes the canonical route contract.

## Error and Completeness Semantics

- Unknown explicit Space: `422` with the existing `{ "error": string }`
  envelope.
- Direct-ID scope mismatch: `404`, indistinguishable from a missing ID.
- Scope resolution, scope-gating query, and required content-load failure:
  existing non-success error mapping; never an empty success used as failure
  concealment. Chunk and snapshot-with-content reads are explicitly required
  content loads.
- Optional retrieval channels retain their existing degrade behavior. A scope
  test cannot claim success merely because an enabled stream vanished: the
  normal path for every enabled stream includes an in-scope positive canary.
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

- retrieval candidate pools include each enabled in-scope Page, graph, episode,
  fact, and summary canary while excluding cross-Space and NULL additions;
- high-ranked out-of-scope candidates are filtered before ranking limits;
- chunk reads cover file-only rows and source-ID collisions while preserving
  deterministic index order and in-scope memory-before-file precedence;
- batch Memory reads preserve input order while omitting missing and mismatched
  IDs;
- selected briefing does not read or write the Global cache;
- mixed snapshots return only matching captures and hide an empty selected
  capture set with `404`;
- selected activities/retrieval events omit empty, mixed, and missing-owner ID
  sets;
- selected tags omit orphan/cross-scope owners and recompute tag inventory;
- Page child reads cannot bypass the parent Page gate;
- Page source responses cannot leak cross-Space Memory content;
- Page category and workspace remain independent;
- Entity vector/name candidates are scoped before ranking;
- mixed/missing-source Entity suggestions are omitted;
- scoped relations require both endpoints to match;
- Global still exposes an intentionally cross-Space relation.

Catalog tests assert exact wave counts `40`, `32`, `14`, `5`, and `0` only at
the corresponding implementation checkpoints and freeze the exact pending-key
set at each wave. The final lint test obtains zero from
`scope_contract_violations().count()` and asserts a passing
`serving.route_scope_contracts` result. The 40 scoped catalog keys must equal the
executed HTTP behavior-case keys.

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

Before publication, run:

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets`
3. `cargo test --workspace --lib`
4. the server integration tests including `space_header_fallback`,
   `list_pages_by_space_e2e`, and `space_scoping_e2e`
5. committed-HEAD `bash scripts/lint-e2e.sh`, updated to expect a naturally
   clean `serving.route_scope_contracts` result and exit `0`
6. the downstream `wenlan-app` contract/build test that exercises its current
   ChunkViewer response expectation, when that checkout is available

Run Cargo commands serially. No live database is required because the scope
contract is reproducible with hermetic two-Space fixtures, but the read-only
orphan-binding preflight must be recorded separately against available live
data.

## Risks and Controls

### Existing tests encode the defect

Several current tests assert unknown-Space fallback or cross-Space results.
Replace those assertions only after the new RED tests prove the approved
contract. Do not delete coverage without an equivalent positive assertion.

### Retrieval side channels bypass the base filter

Graph, Page, episode, fact, summary, rerank, and other candidate streams may
join after a base Memory filter. Wave 1 must verify the final candidate
population, not only the first SQL query. Feature flags are explicitly enabled
and positive canaries prevent stream disappearance from masquerading as scope
correctness.

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

### Chunk source ambiguity is real and bounded

The App currently drops its `source` argument before HTTP. Solving that requires
a coordinated daemon and desktop App contract migration. This project preserves
the current source precedence after scoping through `memories.space`, proves
file-only and collision behavior, and fails loudly on database errors. A later
project can add an explicit discriminator without blocking this repair.

### Global metadata can conceal row-bearing routes

Four catalog rows currently labeled Global return Memory-derived content. Their
binding is corrected before the first checkpoint. The exact 15-route Global set
is frozen so neither accidental relabeling nor deletion can make lint pass.

### Activity and snapshot ownership are derived

Telemetry events and snapshots have no direct Space column. Selected reads use
conservative referenced-Memory membership and omit ambiguous/mixed ownership.
This is a confidentiality rule, not a historical ownership claim.

### Catalog can claim compliance before code does

Update a route row only in the same task and commit as its passing handler/core
contract tests. The canonical lint count is a ledger, not an implementation
substitute.

## Acceptance Gates

The project is complete only when all of the following are true:

- all 40 corrected Space-bound routes have executed behavioral tests;
- the exact 55-route inventory, 40 scoped keys, and 15 justified Global keys are
  frozen;
- `scope_contract_violations().count()` is zero;
- `serving.route_scope_contracts` passes with zero affected routes;
- every explicit unknown Space returns `422` before candidate generation;
- every scoped direct-ID mismatch returns `404` without existence disclosure;
- selected responses contain no cross-Space or NULL-bound canary rows;
- `uncategorized` responses contain only NULL-bound rows;
- a registered exact `uncategorized` Space makes that selector ambiguous and
  returns `422`;
- Global responses preserve pre-change success shapes and intended populations;
- a Space header does not alter any of the 15 deliberate Global routes;
- chunk response shape and memory-before-file precedence remain compatible;
- chunk DB failures no longer become partial or empty `200` responses;
- scoped Page sources do not embed cross-Space Memory content;
- Page category and workspace filters remain independent;
- scoped relations include only edges whose two endpoints match;
- selected activity, snapshot, tag, briefing, and entity-suggestion projections
  satisfy their conservative derived-ownership rules;
- no mutating route, MCP/CLI surface, lint report schema, or deep-provider
  behavior changed;
- formatting, changed-crate Clippy, workspace library tests, and relevant daemon
  HTTP acceptance tests pass.

## Deferred Follow-Ups

These require separate evidence and approval:

- add an explicit source discriminator to the chunk HTTP contract and migrate
  the desktop App;
- reserve or reject the `uncategorized` literal on Space creation;
- reconsider any of the 15 deliberately Global sensitive reads if product
  requirements later make them Space-selectable;
- add lint findings for orphan `memories.space` and `pages.workspace` bindings
  if existing checks cannot absorb them surgically;
- apply the same read-scope contract to a mutating route only through a separate
  write-authorization and CAS design;
- add user, tenant, ACL, or authenticated capability boundaries.
