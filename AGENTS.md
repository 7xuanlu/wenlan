# AGENTS.md

This file guides any coding agent working in this repository — Claude Code, Cursor, Codex, GitHub Copilot, Zed, Aider, and similar. It is the canonical agent-instruction file; vendor-specific files (such as `CLAUDE.md`) re-import from here so the rules stay in sync. The format follows the [agents.md](https://agents.md/) spec.

This repo holds the **daemon** (`origin-server`), the **CLI** (`origin`), shared **wire types** (`origin-types`), the **business-logic core** (`origin-core`), and the **MCP server** (`origin-mcp`). All five ship from this monorepo. The Tauri desktop app (`origin-app`) ships from a separate repo: [7xuanlu/origin-app](https://github.com/7xuanlu/origin-app).

## Design Philosophy

- **Simple and elegant over clever** — prefer the straightforward solution; reads-like-easy-to-write usually is right.
- **Use existing packages** — check for a well-maintained library before custom implementation. Don't reinvent the wheel.
- **Minimize moving parts** — fewer abstractions, layers, indirections. Complexity must justify itself.
- **Standard idioms first** — follow ecosystem conventions. Surprising code is usually wrong code.
- **No speculative surface** — no features beyond what's asked, no abstractions for single-use code, no "flexibility" or "configurability" that wasn't requested, no error handling for impossible scenarios. (Karpathy's Simplicity First.)
- **Surgical changes** — touch only what the task requires. No "while I'm here" refactors, no adjacent-code cleanups, no formatting fixes outside the diff. Match existing style. Remove only imports/vars your own change made unused; pre-existing dead code needs an explicit ask. (Karpathy's Surgical Changes.)
- **Challenge assumptions** — don't follow user framing uncritically. If multiple interpretations exist, present them rather than pick silently. Push back when the approach is wrong. (Karpathy's Think Before Coding.)
- **Verify before claiming done** — run tests, check the build, confirm behavior. Evidence before assertions. (Karpathy's Goal-Driven Execution.)

## Build & Dev Commands

Origin is a Cargo workspace with 5 crates: `origin-types`, `origin-core`, `origin-server`, `origin` (CLI in `crates/origin-cli`), and `origin-mcp`.

```bash
# Run the daemon directly:
cargo run -p origin-server                # listens on 127.0.0.1:7878

# Or start the daemon as a managed launchd service:
cargo build -p origin -p origin-server
./target/debug/origin setup --basic       # configure local memory
./target/debug/origin install             # writes plist, launchctl load
./target/debug/origin status
./target/debug/origin uninstall           # when done

# Workspace-level builds
cargo check --workspace
cargo build --workspace
cargo test --workspace

# Per-crate builds (faster for iteration)
cargo check -p origin-types
cargo check -p origin-core
cargo check -p origin-server
cargo check -p origin                     # the CLI binary
cargo build -p origin --release
./target/release/origin --help

# Run tests for a single crate
cargo test -p origin-core
cargo test -p origin-core --lib <module>::tests
cargo test -p origin-core <test_name>

# Generate coverage reports (opens in browser)
bash scripts/coverage.sh

# Set up git hooks (one-time; .githooks/pre-commit + pre-push)
bash scripts/setup-hooks.sh

# Eval benchmarks (require GPU + model files, run manually)
# Unit tests for eval modules (fast, no GPU):
cargo test -p origin-core --lib locomo::tests
cargo test -p origin-core --lib longmemeval::tests

# Generate eval baselines (slow, needs Qwen 3.5-9B on Metal GPU):
cargo test -p origin-core --test eval_harness save_locomo_baseline -- --ignored --nocapture
cargo test -p origin-core --test eval_harness save_locomo_reranked_baseline -- --ignored --nocapture
cargo test -p origin-core --test eval_harness save_locomo_expanded_baseline -- --ignored --nocapture
cargo test -p origin-core --test eval_harness save_longmemeval_baseline -- --ignored --nocapture
cargo test -p origin-core --test eval_harness save_longmemeval_reranked_baseline -- --ignored --nocapture
cargo test -p origin-core --test eval_harness save_longmemeval_expanded_baseline -- --ignored --nocapture
# Baselines saved to <EVAL_BASELINES_DIR>/*.json (gitignored, default ~/.cache/origin-eval).
```

Pre-commit auto-formats Rust and runs Clippy on changed crates. Pre-push runs workspace clippy + library tests.

## Local vs CI test responsibilities

Origin runs across several layers. The split is driven by three questions: **(1) Can a hosted runner do this?** (no GPU, no API keys, no cost). **(2) Is it under 60s on cold cache?** **(3) Does it gate correctness or measure quality?** Quality measures never gate.

| Layer | What runs | Where | When | Time | Blocks? |
|---|---|---|---|---|---|
| **L1 dev loop** | rust-analyzer / IDE | Local | Every save | <1s | No |
| **L2 pre-commit** | `cargo fmt --all`, clippy on staged crates | Local | `git commit` | ~5s | Yes |
| **L3 pre-push** | `cargo clippy --workspace --all-targets`, `cargo test --workspace --lib` | Local | `git push` | ~60-90s | Yes |
| **L4 CI on PR** | Same checks workspace-wide; tests for types + server + CLI; core lib tests + chat_import_e2e + distillation_quality | GitHub (`ci.yml`) | Every PR | ~10min | Yes (required) |
| **L5 coverage on PR** | `cargo llvm-cov` on origin-core + origin-server only | GitHub (`coverage.yml`) | Every PR | ~10min | **No (informational)** |
| **L6 main canary** | Embedding-only eval (`cargo test -p origin-core --lib eval::retrieval -- --ignored`) | GitHub (`ci.yml`) | Push to `main` | ~10min | No (post-merge) |
| **L7 manual local** | `bash scripts/coverage.sh` (HTML coverage), GPU eval suite (`cargo test -- --ignored`), Anthropic batch judge (`ANTHROPIC_API_KEY=... cargo test ...`) | Your laptop | On demand | minutes-hours | No |
| **L8 pre-release** | Full eval suite vs saved baseline. Record deltas in vault/memory **never git** (Apache-2.0 public-repo rule for daemon; treat numbers as private until you have signed-off baselines) | Your laptop | Per release | hours | Soft gate |

### What does NOT run in CI and why

- **GPU evals (LongMemEval / LoCoMo runner functions, Qwen3.5-9B inference)** — GitHub macOS runners have no Metal acceleration. The tests are `#[ignore]`d so they don't accidentally run.
- **Anthropic API batch judge** — costs $0.35/run and requires `ANTHROPIC_API_KEY` which we don't expose to PR runs from forks.
- **Tauri / desktop coverage** — the desktop app lives in [7xuanlu/origin-app](https://github.com/7xuanlu/origin-app) and runs its own CI there. This repo's coverage is scoped to `origin-core + origin-server`.

### Why pre-push doesn't run coverage

Earlier versions of `.githooks/pre-push` enforced a 90% `cargo llvm-cov` gate. That violated the principles above:
- **Slow:** instrumented rebuild took 5-15min and overloaded memory.
- **Not mirrored in CI:** the main `ci.yml` lane doesn't run coverage at all, so the gate added local friction without upstream protection.
- **Percentage gates rot:** any new untestable surface drops the percentage and forces busywork.

The current pre-push runs only clippy + non-instrumented tests. Coverage is L5 (informational on PR) or L7 (manual command on laptop).

### Eval baselines cache

The per-scenario DB cache (Phase 1 enrichment + Phase 3 answer cache + judge JSONLs) lives at `<baselines_dir>/`, where `baselines_dir` defaults to `~/.cache/origin-eval` (override with `EVAL_BASELINES_DIR=<path>`):

```bash
export EVAL_BASELINES_DIR=$HOME/.cache/origin-eval
```

Path must be writable and local (network mounts not recommended). When set, also chains `EVAL_ENRICHMENT_CACHE_DIR` default to the same dir unless explicitly overridden. Migration: `bash scripts/migrate-eval-cache.sh <source-baselines>`.

## Releasing (release-please)

Releases are automated via [release-please](https://github.com/googleapis/release-please). The workflow runs on every push to `main`.

**How it works:**
1. Every push to `main`, release-please scans new commits and maintains an open "release PR" that accumulates changes and updates `CHANGELOG.md`.
2. When you're ready to ship, merge the release PR. That triggers a GitHub release (draft) + git tag.
3. The `v*` tag push triggers `.github/workflows/release.yml`, which builds `origin`, `origin-server`, and `origin-mcp`, uploads standalone binaries to the release, and publishes it.
4. The release-please workflow also syncs daemon `Cargo.toml` versions on the release branch (release-please can't handle Cargo workspaces reliably with `simple` release type).

**Commit messages control version bumps.** Pre-1.0:

| Commit prefix | Version bump | Example |
|---|---|---|
| `fix:` | patch | 0.1.1 -> 0.1.2 |
| `feat:` | **minor** | 0.1.1 -> 0.2.0 |
| `BREAKING CHANGE` | minor (capped) | 0.1.1 -> 0.2.0 |
| `chore:`, `ci:`, `docs:`, `refactor:`, `test:` | no bump | (hidden in changelog) |

After 1.0, standard semver: `feat:` bumps minor, `BREAKING CHANGE` bumps major.

**IMPORTANT: `feat:` bumps minor, not patch.** Use `fix:` for small features, improvements, and bug fixes. Only use `feat:` when you intentionally want 0.x.0 -> 0.(x+1).0. The `bump-patch-for-minor-pre-major` and `release-as` config flags do NOT work with release-please v17 + simple release type. If a `feat:` commit lands on main, the only way to prevent the minor bump is to rewrite the commit message via `git filter-branch` and force-push.

**Squash merge commit messages matter.** When GitHub squash-merges a PR, the commit message defaults to the PR title. A PR titled `feat: ...` creates a `feat:` commit on main, triggering a minor version bump. Review PR titles before merging — rename to `fix:` if a minor bump is not intended.

**Config files:**
- `release-please-config.json` — release type, version bump behavior
- `.release-please-manifest.json` — current version
- `.github/workflows/release-please.yml` — creates/updates release PRs, syncs daemon Cargo.toml versions
- `.github/workflows/release.yml` — builds daemon + uploads artifacts on `v*` tag push

```bash
# To release: just merge the open release-please PR on GitHub.
# To check what version is pending:
cat .release-please-manifest.json
```

### Release pipeline gotchas (learned the hard way)

**Version files that must stay in sync:** `version.txt`, `.release-please-manifest.json`, and the four daemon `Cargo.toml` files (`# x-release-please-version` marker on the `version` line). The release-please workflow syncs these automatically on the release branch; manual version changes must update all of them. The desktop app version lives in [7xuanlu/origin-app](https://github.com/7xuanlu/origin-app) and bumps independently.

**release-please determines "last version" from merged PR commit messages**, not tags or manifest. It scans for commits matching `chore(main): release X.Y.Z`. Deleting a tag or GitHub Release is NOT enough to reset the version. You must also ensure no commit message in the history matches that pattern, or use `release-as` to force-override.

**Never delete a release tag without also cleaning up the commit history.** If you need to undo a release version, you must rewrite the commit message that release-please created (`git filter-branch --msg-filter`), delete the tag, delete the GitHub Release, and rename the merged PR title via API. Otherwise release-please will keep bumping from the old version.

**The `release.yml` workflow ships the local runtime.** It handles: origin CLI, origin-server, origin-mcp, standalone binary uploads, crates.io publishing for `origin-types` + `origin-mcp`, and npm publishing for `origin-mcp` + `@7xuanlu/origin`. It does NOT build a desktop bundle — origin-app builds its own DMG in its own repo.

### Branch protection

Main branch has: required CI (`conclusion` — aggregate gate over `fmt` + `lint` + `test`, rust-lang convention from cargo / rustup / rust-analyzer) before merge, no force pushes, no deletion. `enforce_admins: false` so the repo owner can push directly for hotfixes. Force push requires temporarily enabling it via API (remember to re-disable after).

### Git hooks (auto-activated)

Manual setup: `bash scripts/setup-hooks.sh`. Hooks live under `.githooks/`.

- **Pre-commit:** auto-formats Rust (`cargo fmt --all`, re-stages changed files) + Clippy on changed crates. Formatting issues can never reach CI.
- **Pre-push:** workspace clippy + library tests. No coverage gate (see above).

## Architecture

Origin is a **Personal Agent Memory Layer** — a local-first memory server on macOS where AI agents write what they learn and humans curate. Daemon-centric: a headless HTTP server owns all business logic and data; the desktop app, the CLI, and external MCP clients are all thin clients over its HTTP API.

### Workspace Layout

The repo is a Cargo workspace with 5 crates:

| Crate | Role | Key dependencies |
|---|---|---|
| `crates/origin-types` | Shared API boundary types (request/response, memory, entities). Lightweight: serde + serde_json + anyhow only. Consumed by `origin-mcp`, `origin-app` (separate repo, via crates.io), and any other downstream tool. | serde |
| `crates/origin-core` | All business logic: DB, embeddings, LLM engine, search, classification, knowledge graph, distill cycles, pages, export, eval. **Must have NO axum or tauri dependencies.** | libSQL, FastEmbed, llama-cpp-2, hf-hub |
| `crates/origin-server` | Headless HTTP daemon on `127.0.0.1:7878`. Depends on `origin-core`. Provides `install/uninstall/status` subcommands for launchd management. | axum, tower, clap |
| `crates/origin-cli` | CLI binary `origin`. Talks to daemon HTTP via `origin-types` and owns setup/service commands. Subcommands: status/search/recall/store/list/agents/install/setup/model/key/doctor. | reqwest, clap |
| `crates/origin-mcp` | MCP server binary that bridges MCP clients (Claude Code, Cursor, Codex, Claude Desktop, etc.) to the daemon HTTP API. Stdio + streamable-HTTP transports via the `rmcp` crate. Ships as a standalone binary + npm package (`npx -y origin-mcp`). | rmcp, reqwest, schemars |

The daemon (`origin-server`) is the single source of truth. External tools (the desktop app, MCP clients via `origin-mcp`, `origin` CLI, curl) all talk HTTP to the same daemon. `origin-mcp` source lives in this monorepo; at runtime it's a separate process the MCP client spawns.

### Stack

- **Daemon**: Rust, Axum 0.8 (HTTP), libSQL (Turso's SQLite fork — vectors, knowledge graph, documents), Tokio, FastEmbed (BGE-Base-EN-v1.5-Q, 768-dim, 512-token max), llama-cpp-2 (Qwen3-4B-Instruct-2507 via Metal GPU; Qwen3.5-9B optional), launchd for process management
- **CLI** (`origin`): Rust, reqwest, clap

### Database: libSQL (owned by origin-core)

One libSQL database at the platform data directory (`dirs::data_local_dir()/origin/memorydb/origin_memory.db`; on macOS, `~/Library/Application Support/origin/memorydb/origin_memory.db`), owned by `MemoryDB` in `crates/origin-core/src/db.rs`:
- **Document chunks**: `chunks` table with `F32_BLOB(768)` vector column, DiskANN indexing (768-dim, BGE-Base-EN-v1.5-Q)
- **Knowledge graph**: `entities`, `relations`, `observations` tables with FK cascades
- **Full-text search**: FTS5 virtual table (`chunks_fts`) auto-synced via triggers
- **Hybrid search**: Vector similarity + FTS combined with Reciprocal Rank Fusion (RRF)

**Connection pattern**: `tokio::sync::Mutex<libsql::Connection>` — `libsql::Connection` is `Send` but not `Sync`, so it's wrapped in an async `Mutex` inside `MemoryDB`.

**Sharing pattern**: `MemoryDB` is wrapped in `Arc<MemoryDB>` at the state layer (`ServerState.db: Option<Arc<MemoryDB>>`). This lets handlers clone the `Arc` out of the `RwLock<ServerState>` guard and drop the guard before performing long-running operations.

### Events: EventEmitter trait (no tauri::Emitter in core)

Instead of passing `tauri::AppHandle` into business logic, `origin-core` defines an `EventEmitter` trait:

```rust
// crates/origin-core/src/events.rs
pub trait EventEmitter: Send + Sync {
    fn emit(&self, event: &str, payload: &str) -> Result<()>;
}
pub struct NoopEmitter;
```

- The daemon uses `NoopEmitter` (no UI to notify directly)
- The desktop app (separate `origin-app` repo) provides a `TauriEmitter` adapter that wraps `AppHandle::emit`
- `MemoryDB::new(db_path, emitter: Arc<dyn EventEmitter>)` takes the trait object

This keeps `origin-core` framework-agnostic and testable with `NoopEmitter` in unit tests.

### IPC Surface

All data flows through the daemon's HTTP API. The desktop app, CLI, and MCP clients all hit it.

- **HTTP API**: Axum on `127.0.0.1:7878`, served by `origin-server`. Used by the desktop app, the `origin-mcp` MCP server (same workspace, separate binary process), the `origin` CLI, and any external tool.
  - General: `/api/health`, `/api/status`, `/api/search`, `/api/context`, `/api/chat-context`, `/api/ping`
  - Ingest: `/api/ingest/text`, `/api/ingest/webpage`, `/api/ingest/memory`
  - Memory CRUD: `/api/memory/store`, `/api/memory/search`, `/api/memory/confirm/{id}`, `/api/memory/list`, `/api/memory/delete/{id}`
  - Knowledge graph: `/api/memory/entities`, `/api/memory/relations`, `/api/memory/observations`
  - Profile & Agents: `/api/profile`, `/api/agents`, `/api/agents/{name}`
  - WebSocket: `/ws/updates`

## Key Modules — origin-core (`crates/origin-core/src/`)

All business logic lives here. No tauri, no axum. Framework-agnostic.

| Module | Purpose |
|---|---|
| `db.rs` | `MemoryDB` — libSQL storage, vectors, chunks, hybrid search, embeddings, knowledge graph, migrations. Three search methods: `search_memory` (embedding+FTS+RRF), `search_memory_reranked` (+ LLM reranking after), `search_memory_expanded` (+ LLM query expansion before). Uses `EventEmitter` trait for UI notifications (no tauri). |
| `events.rs` | `EventEmitter` trait and `NoopEmitter` |
| `engine.rs` | `LlmEngine` — llama-cpp-2 wrapper, model download, inference loop, format helpers |
| `classify.rs` | Memory/profile classification via `LlmEngine` |
| `extract.rs` | Knowledge-graph extraction (entities, relations) via `LlmEngine` |
| `rerank.rs` | LLM reranker |
| `merge.rs` | Memory merging, pattern extraction, contradiction detection |
| `llm_provider.rs` | `LlmProvider` trait + `ApiProvider` (Anthropic API) + `OnDeviceProvider` shim |
| `llm_classifier.rs` | Higher-level classification orchestration |
| `refinery.rs` | Distill-cycle orchestration, dedup, auto-linking, consolidation |
| `post_ingest.rs` | Post-ingest enrichment (dedup check, entity linking, title enrich, recap, page growth) |
| `pages.rs` | Type definitions for the `Page` struct (synthesized wiki entries distilled from memory clusters). Actual clustering + distillation live in `db.rs` + `refinery.rs`. SQL tables are `pages`/`page_sources` (renamed from `concepts`/`concept_sources` in migration 46). |
| `spaces.rs` | Spaces / tag store |
| `narrative.rs` | Profile narrative assembly (editorial prose) |
| `briefing.rs` | Daily briefing assembly |
| `working_memory.rs` | Working memory builder |
| `access_tracker.rs` | Memory access counts + time decay |
| `contradiction.rs` | Contradiction detection |
| `context_packager.rs` | Context bundle → prompt packaging |
| `importer.rs` | File importer pipeline |
| `quality_gate.rs` | Pre-store quality gate |
| `tuning.rs` | Tuning config (distill cycles, distillation, weights) |
| `schema.rs` | Memory schema definitions (formerly `memory_schema.rs`) |
| `prompts/` | Prompt registry (defaults + override dir loader) |
| `chunker/` | Code-aware, Markdown-aware, fixed-size chunking |
| `sources/` | `RawDocument`, file watchers, Obsidian importer. `RawDocument` and related types re-exported from `origin-types`. |
| `privacy.rs` | PII redaction |
| `router/classify.rs`, `content_score.rs` | Smart router scoring helpers (non-tauri parts) |
| `config.rs` | Persistent config at `dirs::data_local_dir()/origin/config.json` (on macOS, `~/Library/Application Support/origin/config.json`) |
| `export/` | Markdown/JSON/zip/PDF exporters |
| `eval/` | Benchmark harness: LoCoMo, LongMemEval. Each benchmark has base (embedding-only), reranked (LLM rescores after search), and expanded (LLM query expansion before search) variants. Baselines under `EVAL_BASELINES_DIR` (gitignored). |
| `state.rs` | `CoreState` — shared state struct used by origin-server |

## Key Modules — origin-server (`crates/origin-server/src/`)

HTTP daemon — owns the Axum router + all routes. All handlers operate on `Arc<RwLock<ServerState>>` where `ServerState.db: Option<Arc<MemoryDB>>`.

| Module | Purpose |
|---|---|
| `main.rs` | Binary entry — daemon startup plus internal maintenance commands, tracing init, port binding with existing-daemon fallback, `MemoryDB::new`, LLM provider init, background tasks, `axum::serve` |
| `state.rs` | `ServerState` struct with `db: Option<Arc<MemoryDB>>`, `llm`, `prompts`, `tuning`, `quality_gate`, `space_store`, `access_tracker`, `llm_processing_ids`, `watch_paths`. `SharedState = Arc<RwLock<ServerState>>` |
| `router.rs` | `build_router(state) -> axum::Router` — all route registrations |
| `routes.rs` | General endpoints: health, search, context, chat-context, status, profile/agents |
| `memory_routes.rs` | Memory CRUD, knowledge graph, classification, entities, pages |
| `ingest_routes.rs` | `/api/ingest/*` — text, webpage, memory |
| `ingest_batcher.rs` | Request-level coalescer for concurrent `/api/memory/store` — folds QualityGate in-line; async classify/extract; passes enrichment + hint through in the response |
| `knowledge_routes.rs` | Entity/relation/observation read paths + knowledge-graph queries |
| `source_routes.rs` | Source registry endpoints |
| `import_routes.rs` | Bulk import endpoints |
| `config_routes.rs` | Config read/write endpoints |
| `onboarding_routes.rs` | First-run wizard / milestone state |
| `scheduler.rs` | Background periodic tasks (distill cycles, distillation, etc.) |
| `websocket.rs` | `/ws/updates` |
| `error.rs` | `ServerError` + axum `IntoResponse` impl |
| `resources/com.origin.server.plist` | launchd plist template (embedded via `include_str!`) |

## Key Modules — origin (CLI, `crates/origin-cli/src/`)

The `origin` binary — a thin reqwest-based CLI for the daemon's HTTP API. Subcommands cover `setup`, `install`, `status`, `search`, `recall`, `store`, `list`, `agents`, `model`, `key`, `doctor`. The CLI does not touch the database directly: every command is an HTTP call.

## Conventions

### Crate boundaries
- **origin-core must have NO tauri or axum dependencies.** Verify with `grep -rn "use tauri\|use axum" crates/origin-core/src/` — expect zero hits. Any event emission goes through the `EventEmitter` trait.
- **origin-types must be lightweight.** Only serde + serde_json + anyhow. No chrono, no tokio, no heavy deps. These types are shared with `origin-mcp` (same workspace, Apache-2.0) and `origin-app` (AGPL-3.0 separate repo, consumes via crates.io), so adding heavy deps forces them downstream.
- **Don't add business logic to origin-server.** Route handlers should call `origin-core` functions with state snapshots — the server's job is HTTP framing, not logic.
- **Don't add new HTTP endpoints to the CLI.** Use existing daemon endpoints. If a CLI subcommand needs new data, add a daemon endpoint first.
- **MCP wrappers in `origin-mcp` always typed-deserialize.** Every `_impl` method in `crates/origin-mcp/src/tools.rs` deserializes the daemon response into a typed wire struct from `origin-types` (e.g. `SearchPagesResponse { pages: Vec<Page> }`), never into `serde_json::Value`. Untyped responses silently emit whatever shape the daemon returns; typed deserialization fails loud on envelope-key drift. Mirror commit `4f545869` and PR #77.

### Async and locking
- **Never hold a `tokio::sync::RwLock` read or write guard across `.await`.** Holding a read guard during an LLM call (which can take seconds) blocks all writers. Pattern: snapshot what you need from the guard into a scoped block that ends before the await, then call the async function with the cloned values. See `crates/origin-server/src/memory_routes.rs` `handle_store_memory` for an example of the post-ingest enrichment pattern.
- **`Arc<MemoryDB>` is the sharing primitive.** `ServerState.db` is `Option<Arc<MemoryDB>>`. Clone the Arc out of the guard rather than borrowing through the guard.
- **Daemon is the single writer.** Only `origin-server` opens the libSQL database. The desktop app and CLI never touch the DB directly — they talk HTTP.
- **libSQL connection pattern**: `MemoryDB` holds `tokio::sync::Mutex<libsql::Connection>` internally. Never try to share a `libsql::Connection` across tasks directly (`Send` but not `Sync`).

### SQL, strings, data
- **SQL safety**: Always use parameterized queries — never interpolate user input into SQL strings
- **NULL semantics**: Store `Option<T>` as SQL NULL, not empty string — so IS NULL filters work correctly
- **UTF-8 safety**: Never byte-index Rust strings (`&s[..n]`) — use `chars().take(n)` or `strip_prefix`/`strip_suffix`. Exception: byte-slicing after a verified ASCII prefix check is safe (the boundary is guaranteed valid), but prefer the char-safe version anyway for consistency.
- **Batch SQL**: Wrap multi-row insert/delete loops in BEGIN/COMMIT transactions
- **LIKE patterns against JSON**: Quote the match target to avoid substring false positives — `%"{id}"%` not `%{id}%` (e.g., `mem_1` would otherwise match `mem_10`). See the fix at `crates/origin-core/src/db.rs:~9895` and the regression test.

### Dev environment gotchas

**Daemon lifecycle:**
- **Worktree daemon mismatch**: The daemon on port 7878 can be from launchd, main branch, a stale worktree, or a previous session. Always verify which binary is running: `lsof -i :7878` to get the PID, then `lsof -p <PID> | grep "txt.*origin-server"` to see the binary path and size. Kill and restart from the current working tree.
- **Stale binary after merge/pull**: `cargo build -p origin-server` may report "0.64s Finished" without recompiling if the source timestamps haven't changed (e.g., after `git pull` fast-forward). Touch a source file to force recompilation: `touch crates/origin-server/src/router.rs && cargo build -p origin-server`. Verify the binary timestamp matches: `ls -la target/debug/origin-server`.
- **kill vs kill -9**: `kill <PID>` may not terminate the daemon cleanly. Always use `kill -9 <PID>` and verify with `lsof -ti :7878` afterward. If the port is still in use, another process took over.
- **Worktree target directories are per-worktree**: Each `.worktrees/<name>` checkout has its own `target/`. Building inside a worktree writes to that worktree's `target/`, not the main repo's. Verify a binary's source with `lsof -p <PID> | grep origin-server` so you don't run a stale binary from a different worktree.

**Other:**
- **Metal/ggml on macOS Tahoe 26.x**: `ggml_metal_init` may fail even though native Metal works. The daemon auto-degrades and continues without LLM. Not a code bug. Check for competing GPU processes: `pgrep -la origin`.
- **Dev and prod share data by default**: Both use port 7878 and the platform data directory (on macOS, `~/Library/Application Support/origin/`). For isolated testing, override explicitly: `ORIGIN_PORT=7879 ORIGIN_DATA_DIR=/tmp/origin-test cargo run -p origin-server`.

### Misc
- `ORIGIN_BIND_ADDR=<host:port>`: override the daemon's bind address (default `127.0.0.1:7878`). Used inside Docker to listen on `0.0.0.0`.
- Log filter default is `warn` — add modules explicitly for `info` logs (e.g., `origin_core::db=info`, `origin_server=info`)
- All local data stored in the platform data directory (`dirs::data_local_dir()/origin/`; on macOS, `~/Library/Application Support/origin/`) — MemoryDB, config, activities, tags
- Crate names: `origin-types`, `origin-core`, `origin-server`, `origin` (CLI), `origin-mcp` — all in this workspace. The desktop app crate `origin-app` lives in [7xuanlu/origin-app](https://github.com/7xuanlu/origin-app).
- **Licenses**: all five workspace crates (`origin-types`, `origin-core`, `origin-server`, `origin` CLI, `origin-mcp`) are **Apache-2.0** via workspace inheritance. The desktop app in `origin-app` is **AGPL-3.0-only** (separate repo).
- `origin-mcp` is in-tree at `crates/origin-mcp/` (merged from the old `7xuanlu/origin-mcp` repo on 2026-05-09 via `git subtree`). It talks to the daemon via HTTP at runtime and is published to npm as a standalone binary (`npx -y origin-mcp`).
