# AGENTS.md

This file guides any coding agent working in this repository — Claude Code, Cursor, Codex, GitHub Copilot, Zed, Aider, and similar. It is the canonical agent-instruction file; vendor-specific files (such as `CLAUDE.md`) re-import from here so the rules stay in sync. The format follows the [agents.md](https://agents.md/) spec.

This repo holds the **daemon** (`origin-server`), the **CLI** (`origin`), shared **wire types** (`origin-types`), the **business-logic core** (`origin-core`), and the **MCP server** (`origin-mcp`). All five ship from this monorepo. The Tauri desktop app (`origin-app`) ships from a separate repo: [7xuanlu/origin-app](https://github.com/7xuanlu/origin-app). Public product surface lives at [useorigin.app](https://useorigin.app) (marketing, docs at `/docs`, longer-form writing at `/learn`).

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

## Cross-platform

Origin runs on macOS (arm64, x86_64), Linux (x86_64, aarch64; musl), and Windows (x86_64).

| OS | Data dir | Service registration |
|---|---|---|
| macOS | `~/Library/Application Support/origin/` | launchd via `~/Library/LaunchAgents/com.origin.server.plist` (user-level) |
| Linux | `~/.local/share/origin/` (or `$XDG_DATA_HOME/origin`) | systemd user unit at `~/.config/systemd/user/origin-server.service` (qualifier dropped per `ServiceLabel::to_script_name()`). Enable lingering with `loginctl enable-linger` if you want the service alive after logout. |
| Windows | `%LOCALAPPDATA%\origin\` | Per-user Task Scheduler ONLOGON task registered via `schtasks.exe /create /tn OriginServer /sc ONLOGON /tr <exe> /f`. `origin install` short-circuits before service-manager and drives schtasks directly (origin-server is a plain console app and would otherwise time out at 30s under sc.exe + the Windows Service Control Protocol). `origin uninstall` calls `schtasks /delete /tn OriginServer /f`. |

`origin install` / `origin uninstall` work on macOS, Linux, and Windows. macOS + Linux go through the `service-manager` crate (launchd / systemd-user); Windows takes the schtasks path described above so the daemon does not need a service dispatcher.

### llama-cpp-2 backend

By default, Linux and Windows builds are CPU-only. macOS keeps Metal. CUDA and Vulkan backends are not enabled in v1; they will land behind opt-in cargo features in a follow-up.

### ORT (ONNX Runtime) on Windows

If you see `Failed to load onnxruntime.dll` or version-mismatch errors on Windows, set `ORT_DYLIB_PATH` to the bundled `onnxruntime.dll` inside the Origin install directory before starting the daemon. The bundled DLL ships in the Windows release zip.

### Daemon bind address

The daemon binds to `127.0.0.1:7878` by default. To expose it on a non-loopback address (e.g., inside Docker), set `ORIGIN_BIND_ADDR=0.0.0.0:7878` in the daemon's environment. The Docker image already sets this.

### Manual Windows verification from macOS

The CI matrix runs `windows-2022` on every PR and is the primary signal. For hands-on testing, run a Windows 11 VM via UTM or Parallels; install MSVC 2022 Build Tools and Rust; then `cargo build --release -p origin-server` and `scripts/smoke-windows.ps1`.

### Linux smoke from macOS

```bash
bash scripts/smoke-linux.sh
```

Builds the multi-arch daemon image (linux/arm64 for native Apple Silicon speed via OrbStack / Docker Desktop), starts a container, exercises the HTTP API, asserts responses, tears down. Runtime ~3 minutes after the first build.

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
| **L8 pre-release** | Full eval suite vs saved baseline. Commit a **curated, env-stamped snapshot** of headline numbers to a results doc/README (single-run tagged "scaffold"; headline claims need N≥3 + stddev). Raw per-run baselines + history series stay gitignored. See "Commit policy" under Eval Citation Discipline. | Your laptop | Per release | hours | Soft gate |

### What does NOT run in CI and why

- **GPU evals (LongMemEval / LoCoMo runner functions, Qwen3.5-9B inference)** — GitHub macOS runners have no Metal acceleration. The tests are `#[ignore]`d so they don't accidentally run.
- **Anthropic API batch judge** — costs $0.35/run and requires `ANTHROPIC_API_KEY` which we don't expose to PR runs from forks.
- **Tauri / desktop coverage** — the desktop app lives in [7xuanlu/origin-app](https://github.com/7xuanlu/origin-app) and runs its own CI there. This repo's coverage is scoped to `origin-core + origin-server`.

### Why pre-push doesn't run coverage

Tried 90% `cargo llvm-cov` gate in pre-push, removed because:
- **Slow:** instrumented rebuild 5-15min, memory pressure.
- **Not mirrored in CI:** `ci.yml` has no coverage gate, so local-only friction.
- **Percentage gates rot:** new untestable surface forces busywork.

Pre-push now runs clippy + non-instrumented tests only. Coverage = L5 (PR, informational) or L7 (manual).

### Eval baselines cache

The per-scenario DB cache (Phase 1 enrichment + Phase 3 answer cache + judge JSONLs) lives at `<baselines_dir>/`, where `baselines_dir` defaults to `~/.cache/origin-eval` (override with `EVAL_BASELINES_DIR=<path>`):

```bash
export EVAL_BASELINES_DIR=$HOME/.cache/origin-eval
```

Path must be writable and local (network mounts not recommended). When set, also chains `EVAL_ENRICHMENT_CACHE_DIR` default to the same dir unless explicitly overridden. Migration: `bash scripts/migrate-eval-cache.sh <source-baselines>`.

### Cached scenario DBs (page-channel + retrieval-only evals)

The PR-B page-channel runners reuse the fullpipeline_*.db seeded DBs without re-ingesting. They live at `~/.cache/origin-eval/scenario_seeded/{locomo_v1,lme_v1}/origin_memory.db`. Repopulate via `bash scripts/seed-scenario-dbs.sh` from the repo root. The `cached_scenario_db_check.rs` integration test (L7 manual) verifies migration replay against current schema; it auto-resolves the root from `SCENARIO_DB_ROOT > EVAL_BASELINES_DIR/scenario_seeded > ~/.cache/origin-eval/scenario_seeded/`.

For full eval discipline (fixture management, baseline layout, env vars, seed scripts, pre-flight checklist, runner conventions), see `app/eval/AGENTS.md` and `crates/origin-core/src/eval/AGENTS.md`. The subdir AGENTS.md files apply per the agents.md hierarchical-instruction convention when an agent is working under those subtrees.

### Eval pre-flight subset

Set `EVAL_LOCOMO_LIMIT=N` (or `EVAL_LME_LIMIT=N`) to truncate the fixture and run a small-subset eval (~30min) before committing to full 3h runs. Useful for verifying direction on new retrieval variants. Applies to every `run_locomo_eval*` / `run_longmemeval_eval*` variant. Unset (the default) runs the full fixture unchanged.

### TTL policy

Cache directories accumulate fast (1GB+ per full LoCoMo+LME run) and snapshots stay valid only as long as the fixture revision + embedder + provider stack remain unchanged. Rule of thumb:

- **Active cache** (`~/.cache/origin-eval/`): purge whenever (a) `fixture_revision_hash` changes for the bench you're rerunning, (b) embedder weights swap, (c) LLM provider class swaps. Drop the affected `<task>/<fixture>.db` files; keep the JSONL judge cache if the judge model is unchanged.
- **Archive caches** (`~/.cache/origin-eval.archive-YYYY-MM-DD/`): retain for 30 days after the matching baseline JSON ships to a PR. After that, delete unless the baseline is still actively cited in an open issue or recent release notes. Archives are recreated on demand by re-running the harness.
- **Don't depend on archive contents for reproducibility** — baselines are reproduced by re-running with the same `ReportEnv` fields, not by replaying cached intermediates.

### KG-faithfulness bench

`app/eval/kg_fixtures/*.toml` hold hand-curated entity + relation ground-truth per source_text case. The `eval::kg_faithfulness` module's smoke test (`#[ignore]`d, runs in L6 main canary) extracts KG from each case and scores entity + relation precision/recall/F1 against the expected ground truth. No LLM judge in this bench — string-match faithfulness only. LLM-judge variant is a follow-up plan.

### Page-distillation faithfulness bench

`app/eval/page_fixtures/*.toml` hold hand-curated source memories + a distilled page body per case + an `expected_min_faithfulness` floor. The `eval::page_faithfulness` module's smoke test (`#[ignore]`d, runs in L6 main canary) splits each page body into sentences and scores what fraction of sentences have ≥ 50% content-token overlap with the union of source memories. Negative-control fixtures in `seed_hallucinations.toml` carry a high `expected_min` floor specifically to verify the scorer flags hallucinated pages. LLM-judge variant deferred.

**Scope limits.** Token-overlap is lexical, not semantic. Paraphrased faithful claims may fail the 50% floor and hallucinated claims with high keyword overlap may pass. Sentence-level granularity only (no multi-sentence claim composition). Acceptable for the smoke-test floor; a real faithfulness gate needs the LLM-judge variant tracked under C-D-LLM.

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

## Key Modules

Per-crate module tables live in subtree `AGENTS.md` files (loaded when an agent works under that crate, per the agents.md hierarchical-instruction convention):

- `crates/origin-core/AGENTS.md` — all business logic (db, engine, classify, extract, rerank, refinery, pages, eval, ...).
- `crates/origin-server/AGENTS.md` — HTTP daemon (router, routes, state, ingest_batcher, scheduler, ...).

## Key Modules — origin (CLI, `crates/origin-cli/src/`)

The `origin` binary — a thin reqwest-based CLI for the daemon's HTTP API. Subcommands cover `setup`, `install`, `status`, `search`, `recall`, `store`, `list`, `agents`, `model`, `key`, `doctor`. The CLI does not touch the database directly: every command is an HTTP call.

## Conventions

### Eval Citation Discipline

See `app/eval/AGENTS.md` "eval citation discipline" section for the full rules (single-run, schema-version, receipt-only, per-case visibility, layer attribution, commit policy). External-facing numbers MUST satisfy those rules.

### Crate boundaries
- **origin-core must have NO tauri or axum dependencies.** Verify with `grep -rn "use tauri\|use axum" crates/origin-core/src/` — expect zero hits. Any event emission goes through the `EventEmitter` trait.
- **origin-types must be lightweight.** Only serde + serde_json + anyhow. No chrono, no tokio, no heavy deps. These types are shared with `origin-mcp` (same workspace, Apache-2.0) and `origin-app` (AGPL-3.0 separate repo, consumes via crates.io), so adding heavy deps forces them downstream.
- **Don't add business logic to origin-server.** Route handlers should call `origin-core` functions with state snapshots — the server's job is HTTP framing, not logic.
- **Don't add new HTTP endpoints to the CLI.** Use existing daemon endpoints. If a CLI subcommand needs new data, add a daemon endpoint first.
- **MCP wrappers in `origin-mcp` always typed-deserialize.** Every `_impl` method in `crates/origin-mcp/src/tools.rs` deserializes the daemon response into a typed wire struct from `origin-types` (e.g. `SearchPagesResponse { pages: Vec<Page> }`), never into `serde_json::Value`. Untyped responses silently emit whatever shape the daemon returns; typed deserialization fails loud on envelope-key drift. Mirror commit `4f545869` and PR #77.

### Ingest-path parity (training-serving skew)
- **All post-store enrichment goes through `origin_core::ingest::run_canonical_enrichment`.** It is the ONE shared path for classify + extract + `apply_enrichment` + tags (Phase 1), entity/title/page enrichment (Phase 2), and dual-pool dedup/contradiction resolution (Phase 3). The server `handle_store_memory`, the eval seed pipeline, and the importer all call it. Do NOT re-implement a subset of enrichment in any consumer.
- **Why.** The eval seed used to re-implement a divergent subset (`enrich_db_for_eval` = entity + title + page only), so every new write-time feature (importance/T8, event_date/T11+T20, episode/T2, fact-channel/T15, dual-pool/T14, summary-nodes/T18) silently lagged in the eval path and shipped merged-but-inert, re-discovered as "starved" each eval cycle. Sharing the code makes seed-vs-production fidelity hold by construction. This is the standard fix for **training-serving skew** — Google "Rules of ML", Rule #32: *"Re-use code between your training pipeline and your serving pipeline whenever possible"* → *"eliminates a source of training-serving skew."* See also 12-Factor X (dev/prod parity) and the technical-debt framing (Cunningham, OOPSLA '92): the eval shortcut was debt never repaid.
- **New write-time feature checklist.** Add it inside `run_canonical_enrichment` (not in a consumer), then add a seed-completeness assert — a contract test (Fowler, `ContractTest`) — so the seed cache fails loud when the feature's artifact is missing rather than silently absent. A flag merged without its artifact present in the seed is unmeasurable.

### Eval seed + eval read: ONE route, ONE contract (no drift)

The recurring failure mode was not any single missing artifact — it was that seeding a cached scenario DB was a *scatter of manual STEP tests* (`seed_inject_event_dates`, `seed_backfill_classify`, entity sweep, `seed_backfill_episodes`, `distill_pages`) run by memory. Miss one and a channel ships starved, then a graph/temporal A/B over it returns a null that gets misread as "the channel doesn't help" — a lie about a dead substrate, re-discovered every cycle.

- **Seed side — the ONE route.** Re-seed cached scenario DBs with the orchestrator `seed_scenario_dbs_complete` (`crates/origin-core/tests/eval_harness.rs`). It runs every enrichment step in the correct order (event_date inject → classify → entity/`memory_entities` sweep → episodes → distill) then asserts `SeedExpectations::complete()`. **Never hand-run the individual `seed_*` STEP tests** — they are the orchestrator's internals. Run the one route and the seed is complete *and* contract-verified by construction.
- **Contract side — teeth, not prose.** `crates/origin-core/src/eval/seed_contract.rs` is the single liveness contract. `SeedExpectations::complete()` hard-fails the seed when a channel's substrate is empty: `memory_entities = 0` (graph), `event_date = 0` (temporal), plus dupes + classification from `strict()`. These are *presence* checks (`> 0`), not coverage percentages — a percentage floor rots (see the L3/coverage note), but zero links means the channel is dead, which is the bug. `strict()` stays lenient (report-only) for minimal seeds; only `complete()` has teeth.
- **Eval side — refuse, don't lie.** The SAME contract gates the consumer: every per-query eval collector calls `seed_contract::assert_feature_substrate_live(conn, feature)` at entry. A graph A/B over a DB with zero `memory_entities` (or a temporal A/B with zero `event_date`) **errors loud** ("EVAL REFUSED") instead of emitting a null. Producer and consumer share one contract, so neither can drift onto a dead substrate.
- **Adding a write-time channel.** Add its step to `seed_scenario_dbs_complete`, its presence floor to `SeedExpectations` (+ wire it into `assert_feature_substrate_live` if it has an A/B), and a unit test in `seed_contract.rs`. The contract — not a runbook — is what keeps the seed honest.

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

### Worktree cleanup after squash-merge

GitHub squash-merge bundles all PR commits into one new commit on `main` with a fresh SHA. The original commits on the feature branch keep their old SHAs and remain in the local repo + worktree even though their content has shipped. This creates three traps:

- **`git cherry main feature/<name>` lies.** It compares commit SHAs (not patch content) and will mark all squashed commits as "unmerged" (`+` prefix). The branch may be fully merged content-wise. Verify by reading the squash commit body (`git log -1 --format=%B <squash-sha>`); the body lists each original PR commit message. Alternatively, grep `main`'s log for keywords or file paths the branch added.
- **Stale worktrees accumulate.** `.worktrees/<name>/` is not auto-removed when a PR merges. After confirming all content is in `main`, run from the main repo root: `git worktree remove --force .worktrees/<name>` then `git branch -D <branch>` (force `-D` is needed because `git` thinks it's unmerged for the same SHA reason) then `git worktree prune`.
- **`.gitignored` per-checkout artifacts.** Files under gitignored paths (`app/eval/baselines/`, `.fastembed_cache/`, build outputs) live per-worktree. Removing a worktree removes its private copies. If a worktree happened to be the only host of some large gitignored artifact (eval baseline DBs, downloaded models), back it up to the canonical shared location first (e.g. `~/.cache/origin-eval/` via `scripts/migrate-eval-cache.sh`) before deleting the worktree.

Run this hygiene pass roughly once a week or whenever `git worktree list` exceeds ~5 entries. Stale worktree paths waste disk + confuse "is this work merged?" investigations.

### Misc
- `ORIGIN_BIND_ADDR=<host:port>`: override the daemon's bind address (default `127.0.0.1:7878`). Used inside Docker to listen on `0.0.0.0`.
- Log filter default is `warn` — add modules explicitly for `info` logs (e.g., `origin_core::db=info`, `origin_server=info`)
- All local data stored in the platform data directory (`dirs::data_local_dir()/origin/`; on macOS, `~/Library/Application Support/origin/`) — MemoryDB, config, activities, tags
- Crate names: `origin-types`, `origin-core`, `origin-server`, `origin` (CLI), `origin-mcp` — all in this workspace. The desktop app crate `origin-app` lives in [7xuanlu/origin-app](https://github.com/7xuanlu/origin-app).
- **Licenses**: all five workspace crates (`origin-types`, `origin-core`, `origin-server`, `origin` CLI, `origin-mcp`) are **Apache-2.0** via workspace inheritance. The desktop app in `origin-app` is **AGPL-3.0-only** (separate repo).
- `origin-mcp` is in-tree at `crates/origin-mcp/` (merged from the old `7xuanlu/origin-mcp` repo on 2026-05-09 via `git subtree`). It talks to the daemon via HTTP at runtime and is published to npm as a standalone binary (`npx -y origin-mcp`).

### Retrieval helpers location (PR-A, 2026-05-27)

`crates/origin-core/src/retrieval/` is the canonical home for retrieval helpers (`hard_filters`, `signals`). The old `composite/` namespace was deleted along with the dead `CompositeWeights` scaffolding when PR #200 closed. Future retrieval-channel additions (page-channel in PR-B, etc.) live in `retrieval/`.

### Retrieval env flags

- `ORIGIN_ENABLE_TEMPORAL_SOFT_BOOST` — opt-in (default OFF). Multiplicatively BOOSTS in-window dated memories by `(1 + ORIGIN_TEMPORAL_BONUS)` while leaving outside-window / undated / no-cue rows neutral (×1.0, never dropped). Gentler successor to the `ORIGIN_ENABLE_TEMPORAL_FILTER` hard filter; the two are mutually exclusive (soft takes precedence when both are set).
- `ORIGIN_TEMPORAL_BONUS` — additive bonus for the soft boost (default `0.5`). Clamped non-negative: a negative or non-finite value falls back to neutral (`0.0`) / the default so the boost can only lift a row, never demote it.
- `ORIGIN_ENABLE_INTENT_LLM` — opt-in (default OFF). On the deep/expanded path (`search_memory_expanded`), the existing query-expansion LLM call emits a structured intent object `{expansions, use_graph, entities, temporal_window, subqueries}` instead of a plain array of rephrasings (one call, `temperature=0`, per-field-tolerant parse via `engine::extract_json`, fallback to the keyword `classify_query` gate on timeout / error / unparseable). Slice-1 wires only `use_graph` into the deep-path graph gate (through the `graph_override` arg on `search_memory_with_cue`); `entities`, `temporal_window`, and `subqueries` are emitted and logged only (parked: `entities`→#10 graph traversal, `temporal_window`→#13, `subqueries`→#11). DISTINCT from the shipped zero-LLM T19 `ORIGIN_ENABLE_QUERY_INTENT` (channel-weight classifier, `__query_intent` baseline suffix); never reuse that flag for the LLM emitter or eval baselines confound. Wiring + scope: `search_memory_expanded` is an expansion path exercised by the eval harness and the dormant `search_memory_routed` strategy (`ORIGIN_LLM_ROUTE`, default OFF); the live daemon calls `search_memory` (quick) + `search_memory_cross_rerank` (deep), so slice-1 wires and PROVES the `use_graph` signal on the expanded path, while deploying it into the live deep path is a downstream step. Reconciliation with existing prototypes (all default-OFF, all off the live path): the T7 strategy router (`retrieval/route.rs` `classify_strategy` / `parse_strategy`, `ORIGIN_LLM_ROUTE`, dispatched by `search_memory_routed`) and the query-decomposition parser (`retrieval/decompose.rs` `parse_subqueries`, used by the `search_memory_decomposed` prototype); the `subqueries` field deliberately reuses `decompose.rs`'s JSON-array shape so #11 can consume it without re-contracting. Measurement caveat: the paired probe's ON arm feeds the intent object's expansions into RRF while the OFF arm feeds the legacy array-rephrasing expansions, so the A/B contrasts the whole intent pipeline against the legacy pipeline, not `use_graph` in isolation; read a positive result as enable-the-intent-pipeline and add a third arm (intent-expansions + keyword gate) to attribute the delta to routing alone. Never surfaced on the MCP `recall` tool; the daemon owns retrieval routing.
- `ORIGIN_RERANK_SKIP_PREFERENCE` — opt-in (default OFF). On the cross-encoder path (`search_memory_cross_rerank_cued`), preference/recommendation-seeking queries (per `router::classify::is_preference_query` — request-form keywords like "recommend"/"any tips", vetoed by past-recall markers like "you recommended"/"remind me") bypass the CE entirely and return the base `search_memory_with_cue` ranking, byte-identical to the non-rerank baseline. History: built against an older "CE hurts single-session-preference −0.155 NDCG@10" measurement that did NOT reproduce on either current seeded substrate — paired A/Bs at n=479 measured CE *helping* SSP (+0.027 on both the canonical and the re-seeded DB) and the bypass net-negative (−0.0117 agg, BH-sig). Ships as a tested, default-OFF escape hatch, NOT a recommended setting; the CE base-vs-on case itself re-verified at +0.178 NDCG@10 agg (BH-sig, positive every category, N=2 substrates) with P50 1165ms vs 111ms base (CPU BGE-reranker-v2-m3) — the default-ON decision stays gated on the smaller-CE-model benchmark. The keyword lists were validated against the full LME-S fixture: 30/30 SSP detected, 0 false positives across the other 470 questions; generalization beyond the fixture is heuristic, same trust level as the temporal/relational keyword gates. Paired A/B arm: `rerank_skip_pref` in `paired_ab_emit` (CE path, flag toggled). The stacked arm `rerank_graph_stack` (same test) toggles `ORIGIN_GRAPH_MEMORY_STREAM` with the CE active on both arms to measure graph×rerank composition — measured NOT significant (+0.0126, p=0.17): graph_stream's +0.0545 quick-path gain is subsumed under the CE (except temporal-reasoning, +0.037), so the two levers do not stack; graph_stream's value is the non-rerank quick path.
