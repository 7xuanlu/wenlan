# Sensitive Read Route Scope Implementation Evidence

## Baseline

- Captured at: `2026-07-14T08:41:37Z`
- Git HEAD: `f553751f`
- Worktree: `/Users/lucian/.codex/worktrees/7d9f/wenlan`
- Branch: `codex/route-scope-contracts`
- Product code modified before receipt: no

The plan originally named this focused verifier:

```bash
cargo test -p wenlan-core --lib lint::serving_review_test -- --nocapture
```

It executed zero tests (`0 passed; 2267 filtered out`) and was therefore a
false-green command. The plan was corrected to:

```bash
cargo test -p wenlan-core --lib lint::serving::tests -- --nocapture
```

Baseline result: `15 passed; 0 failed; 2252 filtered out`.

## Read-Only Store Preflight

The audit resolved the configured live database and Page projection paths
without constructing `MemoryDB`. It used `sqlite3 -readonly`,
`PRAGMA query_only=ON`, and one explicit read transaction. Only binding columns,
Space registry membership, and grouped counts were queried; no Memory, Entity,
or Page content was read.

Private raw receipt:

```text
/Users/lucian/.wenlan/sessions/route-scope-contracts/preflight-2026-07-14.tsv
SHA-256 cb01295ff0b68d04193227e21eecf7c59f873ef9c01ee440195632a41caab271
```

The raw receipt is mode `0600` outside every git worktree. Raw Space names are
not committed.

Non-mutation fingerprints:

| Surface | Before | After | Verdict |
|---|---|---|---|
| SQLite durable bundle (`.db`, `-wal`, `-journal`) | `979c2e2cedf83c71430bf2d8bdec39ebaaf850420385352a66fdc2e314ce71bf` | `979c2e2cedf83c71430bf2d8bdec39ebaaf850420385352a66fdc2e314ce71bf` | unchanged |
| Complete Page tree | `1b0ad445c8e5cc4ddea12f77849fb9bcc5adff3badd1877ca4fbeb4580d92f67` | `1b0ad445c8e5cc4ddea12f77849fb9bcc5adff3badd1877ca4fbeb4580d92f67` | unchanged |

Binding inventory:

| Domain | Distinct bindings | Rows | Unregistered bindings | Literal `uncategorized` |
|---|---:|---:|---:|---:|
| Memory | 24 | 4,549 | 0 | 0 |
| Entity | 8 | 52 | 0 | 0 |
| Page workspace | 10 | 132 | 0 | 0 |
| Space registry | 24 | 24 | n/a | 0 |

Result: the current live store contains no orphan Memory Space, Entity Space,
or Page workspace binding. Hermetic tests must still cover orphan and literal
`uncategorized` rows because absence in this snapshot is not a system
invariant.

## Implementation Checkpoints

### Task 1: Typed resolver and truthful catalog

RED evidence:

- Core integration test failed to resolve `wenlan_core::read_scope` before the
  module existed.
- Server integration test failed to resolve `wenlan_server::read_scope` before
  the HTTP precedence/error mapper existed.
- Catalog review tests failed to compile before `ScopeBinding`,
  `SelectionGate`, and the new selector states existed.
- The first catalog implementation compile exposed an ambiguous
  `NotApplicable` glob import; route-table imports were made explicit before
  rerunning the same tests.

GREEN evidence:

| Contract | Command | Result |
|---|---|---|
| Core resolver | `cargo test -p wenlan-core --test read_scope -- --nocapture` | 6 passed, 0 failed |
| Server precedence/error mapping | `cargo test -p wenlan-server --test read_scope -- --nocapture` | 5 passed, 0 failed |
| Header aliases and preferred-name precedence | `cargo test -p wenlan-server --lib space_header -- --nocapture` | 8 passed, 0 failed |
| Server catalog mirror | `cargo test -p wenlan-server --lib sensitive_read_routes -- --nocapture` | 7 passed, 0 failed |
| Core catalog and diagnostics | `cargo test -p wenlan-core --lib lint::serving::tests -- --nocapture` | 16 passed, 0 failed |

The catalog freezes 55 unique sensitive route keys: exactly 15 Global and 40
Space-bound. All 40 Space-bound rows remain violations at this checkpoint, so
the foundation does not mark any product read as repaired before its handler
and query path are migrated.

### Task 2: Retrieval and candidate-pool isolation

RED evidence:

- An unknown selector on `POST /api/search` returned `200` instead of `422`.
- The Wave 1 catalog registry expected `Rejected` but observed
  `FallsBackUnscoped`.
- `GET /api/memory/recent?limit=1` with the `work` header returned a newer
  `personal` canary, proving the old path limited before applying scope.
- The focused core test initially failed on the old `Option<&str>` retrieval
  boundary and missing Page/Entity scoped entry points.

GREEN evidence:

| Contract | Command | Result |
|---|---|---|
| Scoped Page candidate pools and visibility | `cargo test -p wenlan-core --lib scoped_pages -- --nocapture` | 5 passed, 0 failed |
| Scoped Entity, graph, episode, fact, and summary channels | `cargo test -p wenlan-core --lib scoped_entities -- --nocapture` | 6 passed, 0 failed |
| Existing core Space acceptance | `cargo test -p wenlan-core --test space_scoping_e2e -- --nocapture` | 2 passed, 0 failed |
| Canonical retrieval stack | `cargo test -p wenlan-core --lib search_memory -- --nocapture` | 18 passed, 0 failed |
| Wave 1 exact HTTP registry | `cargo test -p wenlan-server --test space_scoping_e2e wave_1 -- --nocapture` | 6 passed, 0 failed |
| Header fallback and precedence | `cargo test -p wenlan-server --test space_header_fallback -- --nocapture` | 30 passed, 0 failed |
| Context Space behavior | `cargo test -p wenlan-server --test context_space_filter_e2e -- --nocapture` | 5 passed, 0 failed |
| Core catalog | `cargo test -p wenlan-core --lib lint::serving::tests -- --nocapture` | 16 passed, 0 failed |
| Server catalog mirror | `cargo test -p wenlan-server --lib sensitive_read_routes -- --nocapture` | 7 passed, 0 failed |
| Workspace compile | `cargo check --workspace --all-targets` | passed |

The canonical retrieval APIs now require `&ReadScope`; Global callers state
that choice explicitly. Selected Memory, Page, Entity, graph, episode, fact,
and summary populations are filtered before ranking and `LIMIT`. The exact 8
Wave 1 keys are executed, and the truthful catalog checkpoint is 55 total, 15
Global, and 32 remaining violations.

### Task 3: Memory, document, and history reads

RED evidence:

- The exact 11-route registry initially observed `FallsBackUnscoped` and
  missing selection gates; unknown Space selectors returned `200` instead of
  `422`.
- A direct Memory ID owned by `personal` returned `200` under the `work`
  header instead of the same static `404` as a missing ID.
- Indexed-file and batch reads materialized cross-Space rows.
- After the first scoped implementation, the history gate still failed because
  an in-scope Memory exposed an out-of-scope predecessor ID through its
  `supersedes` field even though traversal had stopped.

GREEN evidence:

| Contract | Command | Result |
|---|---|---|
| Exact 11-route HTTP registry, non-disclosure, ordering, Global/Uncategorized, collision, and history isolation | `cargo test -p wenlan-server --test space_scoping_e2e wave_2_records -- --nocapture` | 5 passed, 0 failed |
| Injected chunk-query failure remains an error | `cargo test -p wenlan-core --lib scoped_chunks_propagate_database_failures -- --nocapture` | 1 passed, 0 failed |
| Direct Memory detail compatibility | `cargo test -p wenlan-core --lib get_memory_detail -- --nocapture` | 3 passed, 0 failed |
| Version-chain behavior | `cargo test -p wenlan-core --lib version_chain -- --nocapture` | 1 passed, 0 failed |
| Pending-revision behavior | `cargo test -p wenlan-core --lib pending_revision -- --nocapture` | 24 passed, 0 failed |
| Chunk behavior | `cargo test -p wenlan-core --lib chunks -- --nocapture` | 12 passed, 0 failed |
| Existing route envelopes | `cargo test -p wenlan-server --test route_convergence -- --nocapture` | 13 passed, 0 failed |
| Existing curation reads | `cargo test -p wenlan-server --test curation_read_routes -- --nocapture` | 7 passed, 0 failed |
| Core catalog and diagnostics | `cargo test -p wenlan-core --lib lint::serving::tests -- --nocapture` | 16 passed, 0 failed |
| Server catalog mirror and handler contracts | `cargo test -p wenlan-server --lib sensitive -- --nocapture` | 8 passed, 0 failed |

Direct and batch Memory routes now select only `source='memory'`; a same-ID
file cannot satisfy Memory ownership. Chunks apply scope before choosing the
Memory/file family, use one ordered query capped at 10,000 rows, and propagate
query and row failures. History anchors, recursive joins, final materialization,
and returned predecessor identifiers are all scope-checked. The truthful
catalog checkpoint is 55 total, 15 Global, and 21 remaining violations.

### Task 4: Derived home, activity, retrieval, and tag projections

RED evidence:

- All four focused tests failed: the catalog observed `Missing`, an unknown
  Space returned `200`, home stats counted a personal Memory, and
  `limit=1` returned a newer personal retrieval before applying work scope.
- The plan's original `cargo test -p wenlan-core --lib agent_activity` command
  executed zero tests. It was removed rather than recorded as a false-green
  verifier; the real activity path is exercised through the HTTP registry.

GREEN evidence:

| Contract | Command | Result |
|---|---|---|
| Home-stat Global compatibility | `cargo test -p wenlan-core --lib home_stats -- --nocapture` | 5 passed, 0 failed |
| Retrieval Global compatibility | `cargo test -p wenlan-core --lib recent_retrievals -- --nocapture` | 7 passed, 0 failed |
| Tag Global compatibility | `cargo test -p wenlan-core --lib document_tags -- --nocapture` | 6 passed, 0 failed |
| Four derived HTTP routes | `cargo test -p wenlan-server --test space_scoping_e2e wave_2_derived -- --nocapture` | 4 passed, 0 failed |
| Cumulative Space HTTP registry | `cargo test -p wenlan-server --test space_scoping_e2e -- --nocapture` | 15 passed, 0 failed |
| Core catalog and diagnostics | `cargo test -p wenlan-core --lib lint::serving::tests -- --nocapture` | 16 passed, 0 failed |
| Server catalog mirror and handler contracts | `cargo test -p wenlan-server --lib sensitive -- --nocapture` | 8 passed, 0 failed |
| Changed-crate lint | `cargo clippy -p wenlan-core -p wenlan-server --all-targets -- -D warnings` | passed |
| Workspace compile | `cargo check --workspace --all-targets` | passed |

The HTTP gate covers unknown `422`, aggregate isolation, filtering before
response limit, mixed/personal/missing/empty event omission, Memory and file
retrieval owners, Memory/Page/orphan tag owners, and unchanged Global
visibility. Selected event scans widen for recall but are hard-capped at 10,000
candidates; extreme invisible prefixes can underfill a response but cannot
leak those events. The truthful catalog checkpoint is 55 total, 15 Global, and
17 remaining violations.

### Task 5: Briefing and snapshot parent collections

RED evidence:

- All four focused parent tests failed before implementation: unknown
  selectors returned success, selected briefing used the wrong population and
  cache path, snapshot membership was unscoped, and catalog rows still
  described missing parent gates.
- The first GREEN run exposed a fixture defect: `capture_refs.source_id` is a
  primary key, so reusing one personal capture across two snapshots replaced
  the mixed-snapshot row. The fixture now uses distinct captures and the
  mixed/global compatibility assertion is effective.
- The original `--lib snapshot` filter also selected unrelated lint tests
  containing the word `snapshot`, including process-environment tests. It was
  replaced with the exact DB snapshot test. The canonical source-snapshot
  identity test passes when run exactly and in isolation.

GREEN evidence:

| Contract | Command | Result |
|---|---|---|
| Canonical source-snapshot identity | `cargo test -p wenlan-core --lib lint::operations::tests::config_queue::source_snapshot_identity_is_canonical_and_semantically_complete -- --exact --nocapture` | 1 passed, 0 failed |
| Briefing assembly and cache behavior | `cargo test -p wenlan-core --lib briefing -- --nocapture` | 8 passed, 0 failed |
| Existing snapshot DB behavior | `cargo test -p wenlan-core --lib db::tests::test_get_captures_for_snapshot -- --exact --nocapture` | 1 passed, 0 failed |
| Three parent routes and registry | `cargo test -p wenlan-server --test space_scoping_e2e wave_2_parent -- --nocapture` | 4 passed, 0 failed |
| Cumulative Wave 2 routes | `cargo test -p wenlan-server --test space_scoping_e2e wave_2 -- --nocapture` | 13 passed, 0 failed |
| Complete Space HTTP registry | `cargo test -p wenlan-server --test space_scoping_e2e -- --nocapture` | 19 passed, 0 failed |
| Core catalog and diagnostics | `cargo test -p wenlan-core --lib lint::serving::tests -- --nocapture` | 16 passed, 0 failed |
| Server catalog mirror | `cargo test -p wenlan-server --lib sensitive_read_routes -- --nocapture` | 7 passed, 0 failed |
| Changed-crate lint | `cargo clippy -p wenlan-core -p wenlan-server --all-targets -- -D warnings` | passed |
| Workspace compile | `cargo check --workspace --all-targets` | passed |

Global briefing retains its existing generation/cache behavior. Selected
briefings use scoped stats and recent memories without reading or writing the
singleton cache. Selected snapshot collections require current Memory
ownership before loading content; mixed-owner collisions are omitted,
required-content failures propagate, and mismatch/missing parents share the
same `404`. Global metadata remains backward compatible, including orphan
capture references. The truthful catalog checkpoint is 55 total, 15 Global,
and 14 remaining violations.

The Task 5 Sol xhigh review found and closed three integration gaps before the
wave was accepted: Global missing-snapshot compatibility again returns the
legacy empty collection, scoped briefing row-decode errors propagate, and
Uncategorized briefing/snapshot behavior has explicit canaries. A deliberately
corrupt SQLite storage-type test could not be retained because pinned libSQL
0.9.30 panics while constructing the row, before `Row::get`; the injected
query-failure test is the stable fail-closed verifier at this dependency
version. The second review verdict was APPROVED with no Critical or Important
findings.

### Task 6: Page workspace and child-route gates

RED evidence:

- Page search ignored both body and header selectors; selected list/search and
  child routes could return cross-workspace rows.
- Page category (`pages.space`) and workspace (`pages.workspace`) were
  conflated by old list behavior.
- Source overlap could materialize cross-Space Memory content through a Page
  whose own workspace did not match.
- The complete server library gate found three stale tests that still froze
  Page search as unscoped and expected 14 catalog violations. Updating these
  tests to the canonical scoped contract was required before acceptance.

GREEN evidence:

| Contract | Command | Result |
|---|---|---|
| Request backward compatibility | `cargo test -p wenlan-types search_pages_request -- --nocapture` | 3 passed, 0 failed |
| MCP constructors and schema compatibility | `cargo test -p wenlan-mcp search_pages -- --nocapture` | 6 passed, 0 failed |
| Page search candidate isolation | `cargo test -p wenlan-core --lib search_pages -- --nocapture` | 5 passed, 0 failed |
| Page link and child behavior | `cargo test -p wenlan-core --lib page_links -- --nocapture` | 8 passed, 0 failed |
| Scoped briefing failure propagation regression | `cargo test -p wenlan-core --lib db::scoped_records_test::scoped_briefing_stats_propagate_database_failures -- --exact --nocapture` | 1 passed, 0 failed |
| Page list compatibility | `cargo test -p wenlan-server --test list_pages_by_space_e2e -- --nocapture` | 3 passed, 0 failed |
| Header/query/body precedence | `cargo test -p wenlan-server --test space_header_fallback -- --nocapture` | 30 passed, 0 failed |
| Complete cumulative Space HTTP registry | `cargo test -p wenlan-server --test space_scoping_e2e -- --nocapture` | 24 passed, 0 failed |
| Core catalog and diagnostics | `cargo test -p wenlan-core --lib lint::serving::tests -- --nocapture` | 16 passed, 0 failed |
| Server catalog and handler contracts | `cargo test -p wenlan-server --lib sensitive_read_routes -- --nocapture` | 7 passed, 0 failed |
| Changed-crate lint | `cargo clippy -p wenlan-types -p wenlan-core -p wenlan-server -p wenlan-mcp --all-targets -- -D warnings` | passed |
| Workspace compile | `cargo check --workspace --all-targets` | passed |
| Formatting and patch integrity | `cargo fmt --all -- --check`; `git diff --check` | passed |

All nine Page keys now bind to `pages.workspace`; category remains an
independent filter. Selected search filters before ranking and limit, direct
and child routes share a parent gate and static `404`, and Memory-backed source
materialization enforces Memory scope. Global behavior and legacy request JSON
remain compatible. The truthful catalog checkpoint is 55 total, 15 Global,
and 5 remaining KG violations. Task 5 review corrections and Task 6 are jointly
recorded in commit `46f22972` because the repository pre-commit hook restaged
all modified Rust files after formatting.

The Task 6 Sol xhigh review found one Important Global compatibility regression:
the selected Page-source loader had also replaced the legacy Global loader, so
external-file provenance retained metadata but lost its materialized content.
A new HTTP canary reproduced this as RED. The handler now delegates to the
legacy non-episode loader only for `ReadScope::Global`; selected scopes keep the
Memory-only scoped loader. The focused canary passed `1/1`, the cumulative
Space suite passed `24/24`, the server catalog passed `7/7`, and Clippy,
workspace check, fmt, and diff gates all passed. Re-review verdict: APPROVED
with no remaining Critical or Important findings.

Tasks 7-8 pending.

## Downstream App

Pending companion compatibility branch and verifier results.

## Final Reviews

Pending Codex Sol xhigh and Claude Opus xhigh implementation verdicts.
