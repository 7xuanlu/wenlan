# Sensitive Read Route Scope Contracts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` to implement this plan task-by-task. Use `superpowers:test-driven-development` for each task and `superpowers:systematic-debugging` when a RED test fails for an unexpected reason. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make all 40 sensitive Space-bound daemon read routes enforce one typed, fail-closed Space selector while preserving the 15 deliberate Global routes and existing success response shapes.

**Architecture:** `wenlan-core` owns `ReadScope` resolution and domain query gates; `wenlan-server` only extracts body/query/header candidates and maps core errors to HTTP. The canonical lint route catalog remains the progress ledger, while executed hermetic HTTP cases prove behavior for the exact 40-route set. Four delivery waves move the catalog violation count `40 -> 32 -> 14 -> 5 -> 0` without adding an endpoint, mutating route, or second scope axis.

**Tech Stack:** Rust 2021, Axum 0.8, Tokio, libSQL/SQLite, Serde, Tower HTTP test router, Cargo, Bash, `jq`.

## Global Constraints

- Make daemon/core/types/MCP changes only in
  `/Users/lucian/.codex/worktrees/7d9f/wenlan` on
  `codex/route-scope-contracts`. Create a separate Wenlan App worktree and
  `codex/route-scope-compat` branch for the two companion client files; never
  edit either repo's `main` checkout.
- Canonical design: `docs/superpowers/specs/2026-07-14-route-scope-contracts-design.md` at or after commit `02104092`.
- Space is the only product selector; bind Memory/document rows on `memories.space`, Pages on `pages.workspace`, and Entities on `entities.space`.
- Keep all 55 existing sensitive read paths and success envelopes. Do not add an endpoint, CLI command, MCP tool, authentication layer, mutating lint, repair action, schema migration, or Page `workspace` selector.
- Preserve Global behavior for the exact 15 routes frozen in the design. A supplied Space header must not alter those routes.
- Exact case-sensitive `uncategorized` means NULL-bound rows only when no registered Space has that exact name; a collision returns `422`.
- Missing and mismatched true direct IDs return identical `404` status/body bytes. Batch reads omit missing/mismatched IDs and preserve input order.
- Filter before ranking, `LIMIT`, materialization, or response assembly. Do not fetch unrestricted sensitive rows and post-filter them in a handler.
- Optional retrieval channels retain existing degradation, but every enabled channel must have an in-scope positive canary so disappearance cannot pass a scope test.
- Before the first product-code edit, run the Task 8 read-only orphan-binding
  queries and record a DB/Page-tree before fingerprint. Orphan rows do not
  block implementation, but their observed populations become explicit test
  fixtures and residual risk. Repeat the same audit and fingerprint after all
  implementation gates.
- Run Cargo commands serially. After each task: inspect `git diff`, run `git diff --check`, run the task's focused tests, and commit only that task.
- Do not mutate the live Wenlan database. The Task 1 baseline and Task 8
  comparison are read-only and record only distinct binding names/counts, never
  content.

## File Ownership Map

- Create `crates/wenlan-core/src/read_scope.rs`: typed scope and registered-Space resolver only; no Axum types and no dynamic SQL builder.
- Create `crates/wenlan-server/src/read_scope.rs`: HTTP candidate precedence and core error mapping only; no domain filtering.
- Modify `crates/wenlan-core/src/db.rs`: Memory, retrieval, activity, tag,
  snapshot, and projection queries at their existing ownership seams.
- Create `crates/wenlan-core/src/db/scoped_pages.rs` and
  `crates/wenlan-core/src/db/scoped_pages_test.rs`: route-only Page workspace
  gates and focused query tests.
- Create `crates/wenlan-core/src/db/scoped_entities.rs` and
  `crates/wenlan-core/src/db/scoped_entities_test.rs`: route-only Entity,
  relation, and suggestion gates and focused query tests.
- Modify `crates/wenlan-core/src/briefing.rs`: Global cached versus selected uncached briefing orchestration.
- Modify `crates/wenlan-server/src/routes.rs`: `/api/search`, `/api/context`, recent retrieval/Page feeds.
- Modify `crates/wenlan-server/src/memory_routes.rs`: the remaining Memory, Page, Entity, derived, and snapshot handlers.
- Modify `crates/wenlan-server/src/knowledge_routes.rs`: recent relation extraction and scope handoff.
- Modify `crates/wenlan-types/src/requests.rs`: additive optional `space` only on `SearchPagesRequest`; do not alter response types.
- Modify `crates/wenlan-mcp/src/tools.rs`: keep both in-workspace
  `SearchPagesRequest` struct literals compiling with `space: None`.
- Modify `crates/wenlan-core/src/lint/serving/routes*.rs`: typed catalog vocabulary, exact route bindings, and wave ledger.
- Create `crates/wenlan-server/tests/space_scoping_e2e.rs` plus
  `tests/space_scoping/{fixture,case_runner,retrieval_cases,record_cases,page_cases,knowledge_cases,global_cases}.rs`:
  route-bound fixtures and executed registries for all 40 scoped and 15 Global routes.
- Modify `crates/wenlan-server/tests/space_header_fallback.rs` and `crates/wenlan-server/tests/list_pages_by_space_e2e.rs`: replace defect-preserving expectations.
- Modify `scripts/lint-e2e.sh` and `scripts/lint-e2e.py`: final real-daemon lint
  expectation changes from actionable route finding to clean route check while
  an independent synthetic report still proves actionable exit `1`.
- Create `docs/superpowers/reviews/2026-07-14-route-scope-implementation-evidence.md`: RED/GREEN checkpoints, read-only orphan preflight, downstream App check, final review verdicts.

---

### Task 1: Typed Resolver and Truthful Catalog Baseline

**Files:**
- Create: `crates/wenlan-core/src/read_scope.rs`
- Create: `crates/wenlan-core/tests/read_scope.rs`
- Modify: `crates/wenlan-core/src/lib.rs`
- Create: `crates/wenlan-server/src/read_scope.rs`
- Create: `crates/wenlan-server/tests/read_scope.rs`
- Modify: `crates/wenlan-server/src/lib.rs`
- Modify: `crates/wenlan-server/src/space_header.rs`
- Modify: `crates/wenlan-server/src/memory_routes.rs`
- Modify: `crates/wenlan-core/src/lint/serving/routes.rs`
- Modify: `crates/wenlan-core/src/lint/serving/routes/{retrieval,records,pages,knowledge}.rs`
- Modify: `crates/wenlan-core/src/lint/serving_review_test.rs`
- Modify: `crates/wenlan-server/src/sensitive_read_routes/tests.rs`
- Test: `crates/wenlan-server/src/sensitive_read_routes/handler_contract_tests.rs`
- Create: `docs/superpowers/reviews/2026-07-14-route-scope-implementation-evidence.md`

**Interfaces:**
- Produces: `wenlan_core::read_scope::ReadScope`.
- Produces: `wenlan_core::read_scope::resolve_read_scope(&MemoryDB, Option<&str>) -> Result<ReadScope, ReadScopeResolveError>`.
- Produces: `wenlan_server::read_scope::effective_read_scope(&MemoryDB, Option<&str>, Option<&str>) -> Result<ReadScope, ServerError>`.
- Produces: `ScopeBinding`, `SelectorPrecedence`, `SelectionGate`, and `UnknownScopePolicy` catalog vocabulary used by every later task.

- [x] **Step 0: Capture the read-only live baseline before product edits**

Resolve the configured DB and Page projection paths without constructing
`MemoryDB`; `MemoryDB::new` runs migrations/bootstrap and is forbidden for this
audit. Fingerprint the durable DB bundle (`.db`, `-wal`, and `-journal`, with
explicit absent markers) and complete Page tree immediately before the audit.
Use `sqlite3 -readonly` with `PRAGMA query_only=ON` and one explicit read
transaction for the four grouped binding queries from Task 8, then repeat the
same fingerprints immediately afterward. Store only domain, distinct binding,
registered/unregistered status, count, and a receipt-local opaque binding hash
in the committed implementation evidence file; never commit raw personal Space
names. Keep the raw read-only output in the private local receipt directory and
link only its path plus SHA-256. The reserved contract literal
`"uncategorized"` may be named. Record current git HEAD and exact commands. If the durable bundle changes due
to concurrent daemon activity, retry from a quiescent store; do not claim a
non-mutation receipt from mismatched hashes. Do not hash `-shm`, whose reader
coordination bytes are not durable data. If the store cannot be opened
read-only, record the blocker before proceeding rather than silently treating
the population as empty.

- [x] **Step 1: Add focused RED resolver and precedence tests**

Add core tests for absent, whitespace, registered exact name, unknown name,
NULL selector, and the registered-`uncategorized` collision. Add server tests
for primary-over-header, empty-primary header fallback, invalid-primary no
fallback, preferred-header-over-legacy, and unknown=`422`:

```rust
#[tokio::test]
async fn exact_uncategorized_space_makes_null_selector_ambiguous() {
    let (db, _tmp) = crate::db::tests::test_db().await;
    db.create_space("uncategorized", None, false).await.unwrap();
    let error = resolve_read_scope(&db, Some("uncategorized"))
        .await
        .expect_err("collision must fail closed");
    assert!(matches!(
        error,
        ReadScopeResolveError::AmbiguousUncategorized
    ));
}

#[tokio::test]
async fn invalid_primary_does_not_fall_back_to_valid_header() {
    let response = effective_read_scope(&db, Some("missing"), Some("work")).await;
    assert!(matches!(response, Err(ServerError::ValidationError(_))));
}
```

Run:

```bash
cargo test -p wenlan-core --lib read_scope -- --nocapture
cargo test -p wenlan-server --lib space_header -- --nocapture
cargo test -p wenlan-server --lib sensitive_read_routes::handler_contract_tests -- --nocapture
```

Expected RED: `read_scope` modules and fail-closed resolver do not exist; the
existing helper falls back to Global for an unknown selector.

- [x] **Step 2: Implement the core and server resolver**

Use these exact public shapes:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadScope {
    Global,
    Space(String),
    Uncategorized,
}

impl ReadScope {
    pub fn matches(&self, binding: Option<&str>) -> bool;
}

#[derive(Debug)]
pub enum ReadScopeResolveError {
    Unknown(String),
    AmbiguousUncategorized,
    Store(WenlanError),
}

pub async fn resolve_read_scope(
    db: &MemoryDB,
    raw: Option<&str>,
) -> Result<ReadScope, ReadScopeResolveError>;

pub async fn effective_read_scope(
    db: &MemoryDB,
    primary: Option<&str>,
    header: Option<&str>,
) -> Result<ReadScope, ServerError>;
```

`effective_read_scope` trims both candidates, treats empty as absent, chooses
primary then header, and delegates validation. `resolve_read_scope` performs no
lookup for Global and exactly one `db.get_space(name)` lookup for a non-empty
selector. For `uncategorized`, an existing exact Space returns
`ReadScopeResolveError::AmbiguousUncategorized`; no row returns
`ReadScope::Uncategorized`. An absent ordinary Space returns
`ReadScopeResolveError::Unknown(name)`. The server maps those two variants to
`ValidationError`/`422` and `Store` to the ordinary internal DB error path.

Keep `registered_read_space` only as a private compatibility helper while
unmigrated read handlers still call it, then delete it in Task 7 after the last
read handler moves to `effective_read_scope`. Leave `registered_request_space`
for mutating routes unchanged. Read handlers migrate to
`effective_read_scope` only in the wave that fixes their core query.

- [x] **Step 3: Make the route catalog distinguish contract state**

Replace ambiguous catalog enums with:

```rust
pub enum SelectorPrecedence {
    NotApplicable,
    Missing,
    BodyThenHeader,
    QueryThenHeader,
    HeaderOnly,
}

pub enum ScopeBinding { Global, MemorySpace, PageWorkspace, EntitySpace }

pub enum SelectionGate {
    NotApplicable,
    SingleIdMissing,
    SingleId404,
    BatchMissing,
    BatchFiltered,
    ParentCollectionMissing,
    ParentCollectionFiltered,
}

pub enum UnknownScopePolicy { NotApplicable, FallsBackUnscoped, Rejected }
```

Rename `scope_owner` to `scope_binding`. Reclassify the four false-Global rows
as `MemorySpace` with pending selector state, so
`scope_contract_violations().count()` is exactly `40`. Freeze the exact 55 keys
and exact 15 Global keys in both core and server catalog tests. The violation
predicate remains derived from row metadata and treats every `*Missing`,
`SelectorPrecedence::Missing`, or non-`Rejected` scoped row as violating.

- [x] **Step 4: Verify the foundation and commit**

```bash
cargo test -p wenlan-core --test read_scope -- --nocapture
cargo test -p wenlan-core --lib lint::serving::tests -- --nocapture
cargo test -p wenlan-server --lib space_header -- --nocapture
cargo test -p wenlan-server --lib sensitive_read_routes -- --nocapture
cargo test -p wenlan-server --test read_scope -- --nocapture
cargo fmt --all -- --check
git diff --check
git add crates/wenlan-core/src/read_scope.rs crates/wenlan-core/src/lib.rs \
  crates/wenlan-core/tests/read_scope.rs \
  crates/wenlan-server/src/read_scope.rs crates/wenlan-server/src/lib.rs \
  crates/wenlan-server/tests/read_scope.rs \
  crates/wenlan-server/src/space_header.rs crates/wenlan-server/src/memory_routes.rs \
  crates/wenlan-core/src/lint/serving crates/wenlan-core/src/lint/serving_review_test.rs \
  crates/wenlan-server/src/sensitive_read_routes
git add -f docs/superpowers/reviews/2026-07-14-route-scope-implementation-evidence.md
git commit -m "fix: add fail-closed read scope contracts"
```

Acceptance gate: the pre-edit live baseline is recorded without mutation,
typed resolver tests pass, the catalog has 55 unique rows,
exactly 15 Global rows, exactly 40 truthful violations, and no product read row
is marked repaired yet.

---

### Task 2: Wave 1 Retrieval and Candidate-Pool Isolation

**Files:**
- Create: `crates/wenlan-server/tests/space_scoping_e2e.rs`
- Create: `crates/wenlan-server/tests/space_scoping/fixture.rs`
- Create: `crates/wenlan-server/tests/space_scoping/case_runner.rs`
- Create: `crates/wenlan-server/tests/space_scoping/retrieval_cases.rs`
- Create: `crates/wenlan-server/tests/space_scoping/record_cases.rs`
- Create: `crates/wenlan-server/tests/space_scoping/page_cases.rs`
- Create: `crates/wenlan-server/tests/space_scoping/knowledge_cases.rs`
- Create: `crates/wenlan-server/tests/space_scoping/global_cases.rs`
- Modify: `crates/wenlan-server/tests/common/mod.rs`
- Modify: `crates/wenlan-server/src/routes.rs`
- Modify: `crates/wenlan-server/src/memory_routes.rs`
- Modify: `crates/wenlan-core/src/db.rs`
- Create: `crates/wenlan-core/src/db/scoped_pages.rs`
- Create: `crates/wenlan-core/src/db/scoped_pages_test.rs`
- Create: `crates/wenlan-core/src/db/scoped_entities.rs`
- Create: `crates/wenlan-core/src/db/scoped_entities_test.rs`
- Modify: `crates/wenlan-core/src/{document_enrichment,kg_quality}.rs`
- Modify: `crates/wenlan-core/src/synthesis/{decision_logs,recaps}.rs`
- Modify: `crates/wenlan-core/src/eval/{answer_quality,context_path,layer,lifecycle,locomo,longmemeval,pipeline,retrieval,retrieval_drift,runner}.rs`
- Modify: `crates/wenlan-server/src/scheduler.rs`
- Modify: `crates/wenlan-server/tests/context_space_filter_e2e.rs`
- Modify: `crates/wenlan-server/tests/space_header_fallback.rs`
- Modify: `crates/wenlan-core/src/lint/serving/routes/retrieval.rs`

**Interfaces:**
- Consumes: `ReadScope` and `effective_read_scope` from Task 1.
- Produces: typed-scope migrations for the existing `search_memory*` stack and
  scoped wrappers `search_pages_scoped`, `select_visible_pages_scoped`,
  `load_memories_by_type_scoped`, `list_recent_memories_scoped`,
  `list_unconfirmed_memories_scoped`, and `list_pinned_memories_scoped`.
- Produces: executed registry entries for the 8 Wave 1 routes.
- Produces: one route-bound case runner shared by every wave; test cases can
  provide only path parameters, query fields, body, and assertions, never an
  independent method/path key.
- Establishes the minimal Page/Entity scoped query modules now; Tasks 6 and 7
  extend these modules instead of moving Wave 1 logic out of `db.rs` later.

- [x] **Step 1: Add Wave 1 RED HTTP cases and channel canaries**

Create one hermetic fixture with registered `work` and `personal` Spaces plus
NULL rows and unregistered literal `space='uncategorized'` rows. Global includes
the literal orphan; `ReadScope::Uncategorized` includes only NULL rows. Build
the runner around the canonical catalog row:

```rust
type RouteKey = (Method, &'static str);
type RunCase = for<'a> fn(
    BoundRoute<'a>,
) -> Pin<Box<dyn Future<Output = CaseEvidence> + Send + 'a>>;

struct BehaviorCase {
    key: RouteKey,
    expected: ExpectedContract,
    run: RunCase,
}

struct ExpectedContract {
    precedence: ExpectedPrecedence,
    binding: ExpectedBinding,
    gate: ExpectedGate,
    unknown_rejected: bool,
}

struct RequestSpec {
    path_params: Vec<(&'static str, String)>,
    query: Vec<(&'static str, String)>,
    json: Option<Value>,
}
```

`CaseRunner::bind(case.key)` must find exactly one catalog row and return a
`BoundRoute` that owns its method and path template. A case cannot override
either value. `ExpectedContract` is a separately declared test fixture and must
not be derived from or generated by catalog metadata. `BoundRoute::send`
renders only declared path placeholders; the case injects body/query/header
selectors according to its independent expected precedence.
Record `CaseEvidence` only after the response and all assertions complete.

`CaseRunner::finish()` rejects duplicate/missing keys, checks catalog metadata
against the independently authored `ExpectedContract`, and requires the probe
matrix implied by that expected fixture: Global/work/Uncategorized/unknown for
every scoped collection; primary precedence plus empty-primary fallback for
body/query routes; missing and mismatch equivalence for direct and parent-gated
routes; batch omission/order for batch routes. Cumulative executed-set equality
is 8 after Wave 1, 26 after Wave 2, 35 after Wave 3, and 40 after Wave 4.
Maintain a separate independently enumerated 15-case Global registry that
compares canonical response bytes with and without both Space headers; it must
equal `EXPECTED_GLOBAL_15` before Task 7 closes.

Exercise exactly the 8 Wave 1 keys. Add precedence cases, unknown `422`, and
Uncategorized. Seed Page, graph (`surface_new`), episode, fact, and summary
positive/cross-Space/NULL canaries. Pin:

```text
WENLAN_ENABLE_PAGE_CHANNEL=1
WENLAN_GRAPH_MEMORY_STREAM=1
WENLAN_GRAPH_SURFACE_NEW=1
WENLAN_ENABLE_EPISODE_CHANNEL=1
WENLAN_ENABLE_FACT_CHANNEL=1
WENLAN_ENABLE_GLOBAL_PRELUDE=1
WENLAN_RERANK_SKIP_PREFERENCE=0
```

For every ranked/limited retrieval stream, insert at least eight higher-scoring
cross-Space candidates before one selected result and request `limit=1`. This
exceeds the current `limit * 3` global ANN window, so the old filter-after-ANN
implementation cannot pass. Use a non-preference query so the normal rerank
path is exercised. Assert every enabled in-scope stream appears and no
cross-Space/NULL canary appears. Use one shared process-wide retrieval-feature
mutex for this composite test; per-feature environment locks do not serialize
against each other.

Run:

```bash
cargo test -p wenlan-server --test space_scoping_e2e wave_1 -- --nocapture
```

Expected RED: header-only GET routes ignore the selector; unknown selectors
fall back; Page/graph supplemental candidates can cross Space or vanish.

- [x] **Step 2: Add typed scoped retrieval entry points**

Replace the existing `space: Option<&str>` argument, in place, with
`scope: &ReadScope` at every canonical retrieval boundary:

```text
search_memory
search_memory_with_cue
search_memory_temporal
search_memory_cross_rerank
search_memory_cross_rerank_cued
search_memory_expanded
search_memory_prf
search_memory_decomposed
```

Preserve every other current parameter and behavior, including
`confirmation_boost`, `recap_penalty`, `SearchScoringConfig`, temporal cues,
graph overrides, and rerankers. Internal and eval callers that intentionally
need unscoped behavior pass `&ReadScope::Global`; update all compiler-identified
callers in the files owned by this task. Do not retain an Option-based
compatibility path that can recreate sentinel ambiguity. Representative shape:

```rust
pub async fn search_memory(
    &self,
    query: &str,
    limit: usize,
    memory_type: Option<&str>,
    scope: &ReadScope,
    source_agent: Option<&str>,
    confirmation_boost: Option<f32>,
    recap_penalty: Option<f32>,
    scoring: Option<&SearchScoringConfig>,
) -> Result<Vec<SearchResult>, WenlanError>;

pub async fn search_pages_scoped(
    &self,
    query: &str,
    limit: usize,
    page_type: Option<&str>,
    scope: &ReadScope,
) -> Result<Vec<Page>, WenlanError>;
```

Do not add a second Option-compatible or `_scoped` search stack. Every
vector/FTS branch adds the domain predicate
before candidate fetch and before `LIMIT`. A scoped `WHERE` after
`vector_top_k(..., N)` is insufficient because global top-N can starve the
selected population. For selected `Space`/`Uncategorized`, use the existing
brute-force `vector_distance_cos` fallback shape with a scope predicate in the
same SQL query before `ORDER BY ... LIMIT`; keep indexed `vector_top_k` only for
`Global`. This intentionally trades selected-scope latency for complete,
truthful results until libSQL exposes a filtered ANN primitive. Page predicates
use `pages.workspace`; `page_type` continues to filter `pages.space`.
Graph-only Memory additions, graph anchors/k-hop endpoints, episodes, facts,
summaries, and Page visibility all consume the same `ReadScope`. Selected
summary nodes require a non-empty source set whose every owner exists and
matches. Keep the general unscoped Entity-vector helper only for non-route
write/entity-resolution paths; add a scoped retrieval wrapper.

- [x] **Step 3: Migrate the 8 handlers and preserve Global wrappers**

For body routes call:

```rust
let scope = effective_read_scope(
    &db,
    req.space.as_deref(),
    header_space.as_deref(),
).await?;
```

For query routes use query then header; for routes with no primary selector use
header only. Keep the state `RwLock` guard dropped before resolver/query awaits.
Replace `.unwrap_or_default()` only where it could hide a required scope-gating
failure; keep optional-channel degrade semantics.

- [x] **Step 4: Update Wave 1 catalog rows and old assertions**

Set the 8 rows to their real selector precedence,
`UnknownScopePolicy::Rejected`, and completed selection gates. Assert exact
pending-key set and count `32`. Replace unknown-falls-back expectations in
`space_header_fallback.rs` and context tests with `422`; retain positive Global
and registered/Uncategorized assertions.

- [x] **Step 5: Verify and commit**

```bash
cargo test -p wenlan-core --lib search_memory -- --nocapture
cargo test -p wenlan-server --test context_space_filter_e2e -- --nocapture
cargo test -p wenlan-server --test space_header_fallback -- --nocapture
cargo test -p wenlan-server --test space_scoping_e2e wave_1 -- --nocapture
cargo test -p wenlan-core --lib lint::serving::tests -- --nocapture
cargo check --workspace --all-targets
cargo fmt --all -- --check
git diff --check
git add crates/wenlan-core/src/db.rs crates/wenlan-server/src/routes.rs \
  crates/wenlan-core/src/db/scoped_pages.rs crates/wenlan-core/src/db/scoped_pages_test.rs \
  crates/wenlan-core/src/db/scoped_entities.rs crates/wenlan-core/src/db/scoped_entities_test.rs \
  crates/wenlan-core/src/document_enrichment.rs crates/wenlan-core/src/kg_quality.rs \
  crates/wenlan-core/src/synthesis/decision_logs.rs crates/wenlan-core/src/synthesis/recaps.rs \
  crates/wenlan-core/src/eval/{answer_quality,context_path,layer,lifecycle,locomo,longmemeval,pipeline,retrieval,retrieval_drift,runner}.rs \
  crates/wenlan-server/src/memory_routes.rs crates/wenlan-server/tests \
  crates/wenlan-server/src/scheduler.rs \
  crates/wenlan-core/src/lint/serving/routes/retrieval.rs
git commit -m "fix: isolate scoped retrieval candidate pools"
```

Acceptance gate: 8 executed keys equal the exact Wave 1 set; all enabled
positive streams survive; all cross-Space/NULL candidates are excluded before
ranking; catalog violations are exactly `32`.

---

### Task 3: Wave 2 Memory, Document, and History Reads

**Files:**
- Modify: `crates/wenlan-core/src/db.rs`
- Modify: `crates/wenlan-server/src/memory_routes.rs`
- Modify: `crates/wenlan-server/tests/space_scoping_e2e.rs`
- Modify: `crates/wenlan-server/tests/space_scoping/record_cases.rs`
- Modify: `crates/wenlan-server/tests/route_convergence.rs`
- Modify: `crates/wenlan-server/tests/curation_read_routes.rs`
- Modify: `crates/wenlan-core/src/lint/serving/routes/records.rs`

**Interfaces:**
- Produces: `get_enrichment_status_scoped`, `walk_supersede_chain_scoped`,
  `list_indexed_files_scoped`, `get_chunks_scoped`,
  `get_tag_suggestion_inputs_scoped`, `get_memory_detail_scoped`,
  `get_memories_by_source_ids_scoped`, `get_version_chain_scoped`,
  `list_pending_revisions_scoped`, and `get_pending_revision_for_scoped`.
- Reuses: Wave 1 `list_memories_scoped` for `GET /api/decisions`.
- Produces: executed registry entries for 11 Wave 2 routes; Wave 2 closes in Task 5.

- [x] **Step 1: Add RED cases for the 11 routes**

Exercise enrichment status, revisions, indexed files, chunks, suggest-tags,
detail, by-IDs, versions, decisions, pending-revisions list, and pending-revision
detail. Cover Global, work, Uncategorized, unknown `422`, direct
missing/mismatch identical `404`, batch omission/order, file-only chunks,
Memory/file source-ID collision, and injected DB failure. Direct response bytes
must match exactly:

```rust
assert_eq!(missing.status(), StatusCode::NOT_FOUND);
assert_eq!(mismatch.status(), StatusCode::NOT_FOUND);
assert_eq!(body_bytes(missing).await, body_bytes(mismatch).await);
```

For history routes, seed a predecessor or staged revision in another Space and
assert traversal never materializes it. For pending revisions, require both the
target and staged revision Memory owner to match.

Run:

```bash
cargo test -p wenlan-server --test space_scoping_e2e wave_2_records -- --nocapture
```

Expected RED: direct routes ignore Space, unknown reads fall back, batches leak,
and chunk errors can become partial/empty `200`.

- [x] **Step 2: Implement scoped direct, batch, and history queries**

Direct queries combine ID and binding in one SQL lookup. History anchors and
every returned predecessor/successor row must match; stop at a mismatch. Batch
query filters inside `IN (...)`, then reorders according to the input vector and
omits missing/mismatched rows. Core direct lookup returns `Ok(None)` for both
absent and mismatched owners; the HTTP handler alone maps either case to the
same static `404` body, `"memory not found"`, without logging the requested ID
above DEBUG.

Implement exact route-facing shapes:

```rust
pub async fn get_memory_detail_scoped(
    &self, source_id: &str, scope: &ReadScope,
) -> Result<Option<MemoryItem>, WenlanError>;

pub async fn get_memories_by_source_ids_scoped(
    &self, source_ids: &[String], scope: &ReadScope,
) -> Result<Vec<MemoryItem>, WenlanError>;

pub async fn get_chunks_scoped(
    &self, source_id: &str, scope: &ReadScope,
) -> Result<Option<Vec<MemoryDetail>>, WenlanError>;
```

- [x] **Step 3: Replace the chunk loop with one bounded ordered query**

Build concrete parameterized SQL branches per `ReadScope`. Apply
`memories.space` before choosing source priority, then select only the first
in-scope family (`memory` before `file`), order by `chunk_index`, and cap at
`10_000`. Return `Ok(None)` for an empty selected population so the HTTP layer
can apply the same static `404` non-disclosure response, and propagate query/row
failures.

- [x] **Step 4: Migrate handlers and catalog entries**

`GET /api/decisions` is `QueryThenHeader`; the other 10 use `HeaderOnly` even
when they already carry unrelated query fields. `/api/memory/by-ids` uses
`SelectionGate::BatchFiltered`; true direct routes use `SingleId404`; list
routes use `NotApplicable`. Replace the old revisions missing-ID `200` assertion
with `404` and preserve all success envelope tests.

- [x] **Step 5: Verify and commit**

```bash
cargo test -p wenlan-core --lib get_memory_detail -- --nocapture
cargo test -p wenlan-core --lib version_chain -- --nocapture
cargo test -p wenlan-core --lib pending_revision -- --nocapture
cargo test -p wenlan-core --lib chunks -- --nocapture
cargo test -p wenlan-server --test route_convergence -- --nocapture
cargo test -p wenlan-server --test curation_read_routes -- --nocapture
cargo test -p wenlan-server --test space_scoping_e2e wave_2_records -- --nocapture
cargo fmt --all -- --check
git diff --check
git add crates/wenlan-core/src/db.rs crates/wenlan-server/src/memory_routes.rs \
  crates/wenlan-server/tests/space_scoping_e2e.rs \
  crates/wenlan-server/tests/route_convergence.rs \
  crates/wenlan-server/tests/curation_read_routes.rs \
  crates/wenlan-core/src/lint/serving/routes/records.rs
git commit -m "fix: scope memory and document reads"
```

Acceptance gate: 11 real HTTP keys execute; direct non-disclosure, history
isolation, batch ordering/omission, chunk compatibility, and error propagation
pass without changing a success envelope.

---

### Task 4: Wave 2 Derived Home, Activity, Retrieval, and Tag Projections

**Files:**
- Modify: `crates/wenlan-core/src/db.rs`
- Modify: `crates/wenlan-server/src/routes.rs`
- Modify: `crates/wenlan-server/src/memory_routes.rs`
- Modify: `crates/wenlan-server/tests/space_scoping_e2e.rs`
- Modify: `crates/wenlan-server/tests/space_scoping/record_cases.rs`
- Modify: `crates/wenlan-core/src/lint/serving/routes/{retrieval,records,knowledge}.rs`

**Interfaces:**
- Produces: `get_home_stats_scoped`, `list_recent_retrievals_scoped`,
  `list_agent_activity_scoped`, and one atomic `list_tags_scoped` result that
  contains both the filtered document map and recomputed distinct tag list.

- [x] **Step 1: Add RED fixtures for conservative derived ownership**

For `/api/home-stats`, assert selected row lists and every aggregate exclude
other/NULL rows. For retrieval/activity events seed ID sets that are empty,
work-only, personal-only, mixed, and missing-owner; selected work returns only
work-only. Put query/detail canaries on mixed events to prove the whole event is
omitted. Insert more recent invisible events than the requested limit to prove
filtering occurs before the response limit. For tags seed Memory, Page, orphan,
and cross-Space owner keys; selected work returns only work owners and
recomputes the distinct tag list.

Run:

```bash
cargo test -p wenlan-server --test space_scoping_e2e wave_2_derived -- --nocapture
```

Expected RED: all four routes are cataloged Global and expose mixed rows.

- [x] **Step 2: Implement scoped aggregate and event queries**

Each scoped event query obtains a bounded candidate population wider than the
response limit, parses/deduplicates referenced IDs, and resolves all owners in
one bounded batch. Filter events, then truncate to the requested limit. Use:

```rust
fn all_ids_match_scope(
    ids: &[String],
    owners: &HashMap<String, Option<String>>,
    scope: &ReadScope,
) -> bool {
    !ids.is_empty()
        && ids.iter().all(|id| {
            owners.get(id).is_some_and(|space| scope.matches(space.as_deref()))
        })
}
```

Keep this helper private beside activity queries. Page titles use the same
`pages.workspace` scope, and Memory snippets/titles use `memories.space`.
Global delegates to the existing population unchanged.

For home stats, apply scope to every `memories` alias, both sides of
supersession joins, access-log owner joins, and top-memory candidates before
`LIMIT`. For tags, join Memory/document keys through exact
`memories.(source, source_id)` and Page keys through `(pages.id,
pages.workspace)`. Selected reads drop orphan keys; Global retains them.

- [x] **Step 3: Migrate handlers and catalog rows**

All four routes are `HeaderOnly` even when they carry unrelated query controls.
Move their catalog bindings from Global to `MemorySpace`, set
`UnknownScopePolicy::Rejected`, and register their executed cases. Do not add
Space to their response envelopes.

- [x] **Step 4: Verify and commit**

```bash
cargo test -p wenlan-core --lib home_stats -- --nocapture
cargo test -p wenlan-core --lib recent_retrievals -- --nocapture
cargo test -p wenlan-core --lib document_tags -- --nocapture
cargo test -p wenlan-server --test space_scoping_e2e wave_2_derived -- --nocapture
cargo fmt --all -- --check
git diff --check
git add crates/wenlan-core/src/db.rs crates/wenlan-server/src/routes.rs \
  crates/wenlan-server/src/memory_routes.rs crates/wenlan-server/tests/space_scoping_e2e.rs \
  crates/wenlan-core/src/lint/serving/routes
git commit -m "fix: scope derived memory projections"
```

Acceptance gate: four additional HTTP keys execute; mixed/missing/empty events,
pre-limit invisible events, orphan tags, and cross-Space aggregates cannot leak;
Global payloads remain unchanged.

---

### Task 5: Wave 2 Briefing and Snapshot Parent Collections

**Files:**
- Modify: `crates/wenlan-core/src/briefing.rs`
- Modify: `crates/wenlan-core/src/db.rs`
- Modify: `crates/wenlan-server/src/memory_routes.rs`
- Modify: `crates/wenlan-server/tests/space_scoping_e2e.rs`
- Modify: `crates/wenlan-server/tests/space_scoping/record_cases.rs`
- Modify: `crates/wenlan-core/src/lint/serving/routes/records.rs`
- Modify: `crates/wenlan-core/src/lint/serving_review_test.rs`

**Interfaces:**
- Produces: `generate_briefing_scoped(..., &ReadScope)` with existing Global
  generation/cache behavior and selected uncached branches.
- Produces: `get_captures_for_snapshot_scoped(snapshot_id, &ReadScope)` and `get_snapshot_captures_with_content_scoped`.

- [x] **Step 1: Add RED cache-bleed and mixed-snapshot cases**

Seed the Global briefing cache with a personal canary and work Memory input.
Selected work must not return the cache canary and must not change the cache
fingerprint. Seed a mixed snapshot plus a snapshot whose captures are all
personal. Work returns only work captures; all-personal and nonexistent
snapshot IDs produce identical `404` bytes. With-content injects a required
chunk/index failure and must return non-success.

Run:

```bash
cargo test -p wenlan-server --test space_scoping_e2e wave_2_parent_collections -- --nocapture
```

Expected RED: briefing can read/write the singleton cache and snapshot captures
resolve content without a Space gate.

- [x] **Step 2: Split briefing orchestration by effective scope**

Use this exact entry point:

```rust
pub async fn generate_briefing_scoped(
    db: &MemoryDB,
    llm: Option<&dyn LlmProvider>,
    prompts: &PromptRegistry,
    tuning: &BriefingConfig,
    scope: &ReadScope,
) -> Result<BriefingResponse, WenlanError>;
```

`Global` delegates to the existing generation path and its cache write. `Space` and
`Uncategorized` call `get_briefing_stats_scoped` and
`get_recent_memories_for_briefing_scoped`, assemble the same
`BriefingResponse`, and never invoke `get_cached_briefing` or
`upsert_briefing_cache`. Do not add per-scope cache rows.

- [x] **Step 3: Scope snapshot membership before content loading**

Map capture source names to their current Memory source names inside core, join
current Memory ownership, and filter by `ReadScope`. A selected empty set
returns `WenlanError::NotFound("snapshot not found")`. Both handlers consume the
same scoped capture set. Move content projection into the scoped core method and
replace `list_indexed_files().await.unwrap_or_default()` plus per-chunk error
swallowing with propagated required-load errors.

- [x] **Step 4: Close the exact Wave 2 checkpoint**

Add the three executed keys: briefing and two snapshot routes. Mark snapshot
rows `SelectionGate::ParentCollectionFiltered`. Assert all 18 Wave 2 keys have
executed and the catalog pending set is exactly the 9 Page + 5 KG keys, count
`14`.

- [x] **Step 5: Verify and commit**

```bash
cargo test -p wenlan-core --lib briefing -- --nocapture
cargo test -p wenlan-core --lib db::tests::test_get_captures_for_snapshot -- --exact --nocapture
cargo test -p wenlan-server --test space_scoping_e2e wave_2 -- --nocapture
cargo test -p wenlan-core --lib lint::serving::tests -- --nocapture
cargo fmt --all -- --check
git diff --check
git add crates/wenlan-core/src/briefing.rs crates/wenlan-core/src/db.rs \
  crates/wenlan-server/src/memory_routes.rs crates/wenlan-server/tests/space_scoping_e2e.rs \
  crates/wenlan-core/src/lint/serving/routes/records.rs \
  crates/wenlan-core/src/lint/serving_review_test.rs
git commit -m "fix: isolate briefing and snapshot reads"
```

Acceptance gate: scoped briefing has no Global cache read/write; snapshot
parents enforce current Memory scope; all 18 Wave 2 HTTP keys execute; catalog
violations are exactly `14`.

---

### Task 6: Wave 3 Page Workspace and Child-Route Gates

**Files:**
- Modify: `crates/wenlan-types/src/requests.rs`
- Modify: `crates/wenlan-mcp/src/tools.rs`
- Modify: `crates/wenlan-core/src/db.rs`
- Modify: `crates/wenlan-core/src/db/scoped_pages.rs`
- Modify: `crates/wenlan-core/src/db/scoped_pages_test.rs`
- Modify: `crates/wenlan-core/src/pages.rs`
- Modify: `crates/wenlan-server/src/routes.rs`
- Modify: `crates/wenlan-server/src/memory_routes.rs`
- Modify: `crates/wenlan-server/tests/list_pages_by_space_e2e.rs`
- Modify: `crates/wenlan-server/tests/space_header_fallback.rs`
- Modify: `crates/wenlan-server/tests/space_scoping_e2e.rs`
- Modify: `crates/wenlan-server/tests/space_scoping/page_cases.rs`
- Modify: `crates/wenlan-core/src/lint/serving/routes/pages.rs`

**Interfaces:**
- Extends: `SearchPagesRequest { query, limit, page_type, #[serde(default)] space: Option<String> }`.
- Preserves: both MCP `SearchPagesRequest` constructors with explicit
  `space: None`; no MCP tool schema or response type changes.
- Consumes: the Task 2 `search_pages_scoped` candidate-pool method.
- Produces in `db/scoped_pages.rs`: `list_recent_pages_with_badges_scoped`,
  `list_recent_changes_scoped`, `list_pages_scoped`,
  `list_orphan_link_labels_scoped`, `get_page_scoped`,
  `get_page_sources_scoped`, `get_page_outbound_links_scoped`,
  `get_page_inbound_links_scoped`, and `get_page_changelog_scoped`. Each keeps
  the unscoped counterpart's return type and adds `scope: &ReadScope` as its
  final argument.

- [ ] **Step 1: Add RED cases with category/workspace divergence**

Create work Pages whose `pages.space='decision'`, personal Pages whose
`pages.space='recap'`, and NULL-workspace Pages. Add an in-work source Memory to
a personal Page to prove overlap cannot override workspace. Exercise all 9 Page
keys, body/query/header precedence, unknown `422`, selected direct/child `404`,
and mixed `page_sources` where a work Page references personal Memory content.
Insert at least eight higher-ranked out-of-scope search candidates before a
low-ranked in-scope Page and use `limit=1`, exceeding the current global ANN
fetch window. Also prove a workspace with fewer Pages than the requested limit
returns a truthful short list. Add a Page whose literal workspace is
`"uncategorized"`; Global includes it and the NULL-only selector excludes it.

Update defect-preserving tests so an unknown query expects `422` and a selected
list filters `workspace`, not category.

Run:

```bash
cargo test -p wenlan-server --test list_pages_by_space_e2e -- --nocapture
cargo test -p wenlan-server --test space_scoping_e2e wave_3 -- --nocapture
```

Expected RED: list filters `pages.space`, search ignores the header, direct
children bypass parent scope, and source overlap can mask a workspace mismatch.

- [ ] **Step 2: Add Page-scoped core methods**

Implement the route-only wrappers in `db/scoped_pages.rs`; register the module
from `db.rs` and keep focused query tests in `db/scoped_pages_test.rs`. Existing
unscoped methods remain for write/refinery/internal callers. Every Page
selection branch matches the following semantics, using
parameterized concrete SQL per branch:

```rust
match scope {
    ReadScope::Global => "",
    ReadScope::Space(_) => " AND c.workspace = ?scope",
    ReadScope::Uncategorized => " AND c.workspace IS NULL",
}
```

Keep `page_type` as an independent `c.space = ?page_type` predicate. Apply the
workspace clause to vector and FTS candidates before ranking/fetch limits. For
selected scope, use the existing brute-force `vector_distance_cos` query shape
with workspace and page-type predicates before `ORDER BY ... LIMIT`; keep ANN
only for Global. A legitimately small workspace returns a short list, never an
error.

Direct and child methods gate the parent Page in the same query path. Page
sources return only rows whose parent matches; when response assembly resolves
a Memory-backed locator to content, that Memory must also match. Typed
non-Memory evidence metadata can remain without embedding cross-Space Memory
text. Every missing or mismatched direct/child parent maps to the same static
`404` body bytes, `"page not found"`; do not include the requested ID.

- [ ] **Step 3: Migrate handlers and close Wave 3**

`POST /api/pages/search` uses body then header. `GET /api/pages` uses query then
header. Other Page routes use header only; existing query controls remain
independent. Add all 9 executed entries and update catalog rows. Assert exact
pending KG set and violation count `5`.

- [ ] **Step 4: Verify and commit**

```bash
cargo test -p wenlan-types search_pages_request -- --nocapture
cargo test -p wenlan-mcp search_pages -- --nocapture
cargo test -p wenlan-core --lib search_pages -- --nocapture
cargo test -p wenlan-core --lib page_links -- --nocapture
cargo test -p wenlan-server --test list_pages_by_space_e2e -- --nocapture
cargo test -p wenlan-server --test space_header_fallback -- --nocapture
cargo test -p wenlan-server --test space_scoping_e2e wave_3 -- --nocapture
cargo test -p wenlan-core --lib lint::serving::tests -- --nocapture
cargo check --workspace --all-targets
cargo fmt --all -- --check
git diff --check
git add crates/wenlan-types/src/requests.rs crates/wenlan-mcp/src/tools.rs \
  crates/wenlan-core/src/db.rs \
  crates/wenlan-core/src/db/scoped_pages.rs crates/wenlan-core/src/db/scoped_pages_test.rs \
  crates/wenlan-core/src/pages.rs crates/wenlan-server/src/routes.rs \
  crates/wenlan-server/src/memory_routes.rs crates/wenlan-server/tests \
  crates/wenlan-core/src/lint/serving/routes/pages.rs
git commit -m "fix: bind page reads to workspaces"
```

Acceptance gate: all 9 Page HTTP keys execute; category and workspace stay
independent; child routes gate parents; sources cannot leak Memory content; and
catalog violations are exactly `5`.

---

### Task 7: Wave 4 Entity, Relation, and Suggestion Gates

**Files:**
- Modify: `crates/wenlan-core/src/db.rs`
- Modify: `crates/wenlan-core/src/db/scoped_entities.rs`
- Modify: `crates/wenlan-core/src/db/scoped_entities_test.rs`
- Modify: `crates/wenlan-server/src/memory_routes.rs`
- Modify: `crates/wenlan-server/src/knowledge_routes.rs`
- Modify: `crates/wenlan-server/tests/space_scoping_e2e.rs`
- Modify: `crates/wenlan-server/tests/space_scoping/knowledge_cases.rs`
- Modify: `crates/wenlan-core/src/lint/serving/routes/knowledge.rs`
- Modify: `crates/wenlan-core/src/lint/serving_review_test.rs`
- Modify: `crates/wenlan-server/src/sensitive_read_routes/tests.rs`

**Interfaces:**
- Extends local server `SearchEntitiesRequest` with `space: Option<String>`.
- Produces in `db/scoped_entities.rs`: `list_entities_scoped`,
  `search_entities_by_vector_scoped`, `get_entity_detail_scoped`,
  `list_recent_relations_scoped`, and `list_entity_suggestions_scoped`. Each
  keeps the unscoped counterpart's return type and adds `scope: &ReadScope` as
  its final argument.
- Preserves: shared unscoped `search_entities_by_vector` for internal graph/entity resolution.

- [ ] **Step 1: Add RED same-name, relation-endpoint, and proposal cases**

Seed same-name Entities in work/personal, a NULL Entity, work-work,
work-personal, and personal-personal relations, plus suggestion proposals with
work-only, mixed, missing, and empty source IDs. Exercise all 5 routes. Assert
selected entity detail cannot embed relations whose other endpoint differs.
Insert a higher-ranked personal Entity before a work Entity and request a small
limit. Assert Global still includes the cross-Space relation. Add a literal
`space='uncategorized'` Entity and prove it is Global-only orphan inventory, not
part of the NULL selector.

Run:

```bash
cargo test -p wenlan-server --test space_scoping_e2e wave_4 -- --nocapture
```

Expected RED: entity search/detail and relation projections are unscoped, and
suggestions are filtered only by action/status.

- [ ] **Step 2: Implement route-specific scoped wrappers**

Implement and test the route-only wrappers in `db/scoped_entities.rs` and
`db/scoped_entities_test.rs`. Use them only in HTTP and retrieval-route call
sites that require scope. Do not change the shared unscoped
`search_entities_by_vector` used by entity resolution and write-time graph
augmentation. Entity vector/name candidates add `entities.space` before
ranking/limit. Selected Entity vector search uses `vector_distance_cos` with
the binding predicate before `ORDER BY ... LIMIT`; only Global keeps the
existing ANN path. Entity detail joins both
relation endpoints and applies both endpoint predicates. Recent relation SQL
joins subject and object Entities and requires both to match for selected reads.
Missing and mismatched detail both return the same static `404` body bytes,
`"entity not found"`; do not include the requested ID.

Suggestion visibility is computed before materialization/limit in one SQL query
over `refinement_queue` using `json_valid(source_ids)`, `json_each(source_ids)`,
and paired `EXISTS`/`NOT EXISTS` predicates. Empty, malformed, missing-owner, or
mismatched owner sets are excluded before proposal payloads are materialized.
The repo already uses `json_each(refinement_queue.source_ids)` in core; do not
add a bounded application-side candidate window or a partial-result fallback.
The SQL visibility predicate must be equivalent to:

```rust
!proposal.source_ids.is_empty()
    && proposal.source_ids.iter().all(|id| owner_exists_and_matches(id, scope))
```

Global returns the existing proposal population. The suggestion catalog binding
is `MemorySpace`, although delivery remains in this KG wave.

- [ ] **Step 3: Close the catalog and behavior registry**

Set the 5 final catalog rows to completed contracts. Assert:

```rust
assert_eq!(sensitive_read_routes().count(), 55);
assert_eq!(global_keys(), EXPECTED_GLOBAL_15);
assert_eq!(scoped_keys(), EXPECTED_SCOPED_40);
assert_eq!(executed_case_keys(), EXPECTED_SCOPED_40);
assert_eq!(executed_global_case_keys(), EXPECTED_GLOBAL_15);
assert_eq!(scope_contract_violations().count(), 0);
```

Run the core lint runner and assert `serving.route_scope_contracts` is clean
with affected records `0`.

- [ ] **Step 4: Verify and commit**

```bash
cargo test -p wenlan-core --lib search_entities -- --nocapture
cargo test -p wenlan-core --lib recent_relations -- --nocapture
cargo test -p wenlan-core --lib entity_suggestions -- --nocapture
cargo test -p wenlan-server --test space_scoping_e2e wave_4 -- --nocapture
cargo test -p wenlan-server --lib sensitive_read_routes -- --nocapture
cargo test -p wenlan-core --lib lint::serving::tests -- --nocapture
cargo fmt --all -- --check
git diff --check
git add crates/wenlan-core/src/db.rs crates/wenlan-core/src/db/scoped_entities.rs \
  crates/wenlan-core/src/db/scoped_entities_test.rs crates/wenlan-server/src/memory_routes.rs \
  crates/wenlan-server/src/knowledge_routes.rs crates/wenlan-server/tests/space_scoping_e2e.rs \
  crates/wenlan-core/src/lint/serving crates/wenlan-core/src/lint/serving_review_test.rs \
  crates/wenlan-server/src/sensitive_read_routes
git commit -m "fix: scope entity and relation reads"
```

Acceptance gate: all 5 KG HTTP keys execute; exact 40-route scoped registry and
15-route Global invariance registry equality, two-endpoint relations,
conservative suggestions, and zero canonical route-scope violations pass.

---

### Task 8: Live Read-Only Preflight, Compatibility, and Publication Gates

**Files:**
- Modify: `scripts/lint-e2e.sh`
- Modify: `scripts/lint-e2e.py`
- Modify: `docs/superpowers/reviews/2026-07-14-route-scope-implementation-evidence.md`
- Companion modify: `<wenlan-app-worktree>/app/src/api.rs`
- Companion modify: `<wenlan-app-worktree>/app/src/search.rs`
- Reference only: `/Users/lucian/Repos/wenlan-app/src/lib/tauri.ts`
- Reference only: `/Users/lucian/Repos/wenlan-app/src/components/ChunkViewer.tsx`

**Interfaces:**
- Consumes the final zero-violation catalog and all executed HTTP cases.
- Produces a redacted evidence record and PR-ready branch; no product API.

- [ ] **Step 1: Update the exact-checkout lint E2E expectation**

In `scripts/lint-e2e.sh` and `scripts/lint-e2e.py`, change real-daemon
baseline/global/registered/Uncategorized runs from expected CLI exit `1` with a
`serving.route_scope_contracts` finding to exit `0` with that check clean.
`clean_fixture` and `precedence_fixture` must no longer require a finding from
the real baseline. Add an independent synthetic valid report containing one
typed `serving.route_scope_contracts` actionable finding and use it for CLI exit
`1`, precedence, and tarball assertions. Keep the incomplete fixture for exit
`2`, HTTP/CLI parity, no `/api/wiki/check`, producer SHA, and non-mutation
fingerprints.

Run from committed HEAD after the script change is committed; the script builds
`git archive HEAD`, so a dirty-tree run is not valid evidence.

- [ ] **Step 2: Repeat and compare the read-only orphan-binding preflight**

Resolve the configured live DB path without modifying it. Execute read-only
queries equivalent to:

```sql
SELECT 'memory', space FROM memories WHERE space IS NOT NULL GROUP BY space;
SELECT 'entity', space FROM entities WHERE space IS NOT NULL GROUP BY space;
SELECT 'page', workspace FROM pages WHERE workspace IS NOT NULL GROUP BY workspace;
SELECT name FROM spaces GROUP BY name;
```

Record only domain, opaque binding hash, whether registered, and count in the
committed evidence file. Use the same `sqlite3 -readonly`/`query_only` transaction and
immediate before/after durable-bundle plus Page-tree fingerprints as Task 1.
Each audit's own before/after pair must match; Task 1 and Task 8 populations may
legitimately differ because the user can keep using Wenlan during
implementation, so report that difference as store drift rather than lint
mutation. Record literal non-NULL `"uncategorized"` bindings explicitly as
Global-only orphan inventory and keep a hermetic test proving the
`Uncategorized` selector still means SQL NULL only. The approved design says
orphan inventory does not block this project. Do not repair orphan values here.

- [ ] **Step 3: Preserve downstream App behavior and wire compatibility**

Read the current App `AGENTS.md` before editing its separate repo. Re-read the
three ChunkViewer call sites and confirm the request remains
`GET /api/chunks/{source_id}` and response remains `Vec<MemoryDetail>`.

The App currently maps `GET /api/memory/{id}/detail` to
`Result<Option<MemoryItem>, String>` but its generic `get_json` turns the new
static `404` into `Err`. In a companion App branch, add a focused
`get_optional_json` helper that returns `Ok(None)` only for HTTP 404, propagates
all other non-success statuses, and otherwise deserializes normally. Route
`get_memory_detail` through it and add RED/GREEN tests for 404, 500, malformed
success, and present Memory. Do not weaken the daemon's selected-scope 404
contract.

The App is currently pinned to an older `wenlan-types`, so its
`SearchPagesRequest` literal still serializes without `space`; the daemon must
accept that payload as Global through `#[serde(default)]`. Add a daemon
compatibility test using the old JSON shape. Do not bump the App dependency in
this project. Record that its Rust constructor needs `space: None` when its
`wenlan-types` pin is upgraded.

Run the App's focused Rust tests plus its existing build/contract command and
record exact results. Failure to run or repair the App compatibility gate blocks
publication; do not downgrade it to residual risk. Commit and open the companion
App PR before the daemon PR is marked ready.

- [ ] **Step 4: Run final serial repository gates**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets
cargo test --workspace --lib
cargo test -p wenlan-server --test space_header_fallback -- --nocapture
cargo test -p wenlan-server --test list_pages_by_space_e2e -- --nocapture
cargo test -p wenlan-server --test space_scoping_e2e -- --nocapture
git diff --check
```

Commit the script/evidence change, then run:

```bash
bash scripts/lint-e2e.sh
```

Expected: all commands pass; lint E2E ends with its PASS line and proves CLI
exit `0/1/2`, HTTP parity, producer SHA, privacy, and zero mutation.

- [ ] **Step 5: Run final independent reviews and address findings**

Run one Codex Sol xhigh implementation review and one Claude Opus xhigh
adversarial review against `origin/main`. Review data isolation, query-before-
limit behavior, 404 non-disclosure, response compatibility, and test false
greens. Apply only evidence-backed findings through RED tests, rerun affected
gates, and record both verdicts in the evidence file.

- [ ] **Step 6: Commit, publish, and open both PRs**

```bash
git add scripts/lint-e2e.sh scripts/lint-e2e.py
git add -f docs/superpowers/reviews/2026-07-14-route-scope-implementation-evidence.md
git commit -m "test: verify sensitive read scope contracts"
git status --short
git log --oneline origin/main..HEAD
```

Use `github:yeet` first in the companion App branch, then push
`codex/route-scope-contracts` and open the daemon PR. The App PR contains only
the optional-404 client compatibility change and its tests. The daemon PR body
links that companion PR and lists the four catalog checkpoints, exact final
verifier commands, live preflight non-mutation receipt, downstream App result,
and final two-model review verdicts.

Acceptance gate: worktree clean, all serial gates green, committed-HEAD lint E2E
green, each read-only live audit has matching before/after durable fingerprints,
the companion App compatibility PR is green, final reviews are resolved, and
both PR URLs are available.
