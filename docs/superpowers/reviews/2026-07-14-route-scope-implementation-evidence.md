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

Tasks 4-8 pending.

## Downstream App

Pending companion compatibility branch and verifier results.

## Final Reviews

Pending Codex Sol xhigh and Claude Opus xhigh implementation verdicts.
