# AGENTS.md

This file guides any coding agent working in this repository — Claude Code, Cursor, Codex, GitHub Copilot, Zed, Aider, and similar. It is the canonical agent-instruction file; vendor-specific files (such as `CLAUDE.md`) re-import from here so the rules stay in sync. The format follows the [agents.md](https://agents.md/) spec.

This repo holds the **daemon** (`wenlan-server`), the **CLI** (`wenlan`), shared **wire types** (`wenlan-types`), the **business-logic core** (`wenlan-core`), and the **MCP server** (`wenlan-mcp`). All five ship from this monorepo. The Tauri desktop app (`wenlan-app`) ships from a separate repo: [7xuanlu/wenlan-app](https://github.com/7xuanlu/wenlan-app). Public product surface lives at [wenlan.app](https://wenlan.app) (marketing, docs at `/docs`, longer-form writing at `/learn`).

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

Wenlan is a Cargo workspace with 5 crates: `wenlan-types`, `wenlan-core`, `wenlan-server`, `wenlan` (CLI in `crates/wenlan-cli`), and `wenlan-mcp`.

```bash
# Run the daemon directly:
cargo run -p wenlan-server                # listens on 127.0.0.1:7878

# Or start the daemon as a managed launchd service:
cargo build -p wenlan -p wenlan-server
./target/debug/wenlan setup --basic       # configure local memory
./target/debug/wenlan background on       # writes plist, launchctl load
./target/debug/wenlan status
./target/debug/wenlan background off      # when done

# Workspace-level builds
cargo check --workspace
cargo build --workspace
cargo test --workspace

# Per-crate builds (faster for iteration)
cargo check -p wenlan-types
cargo check -p wenlan-core
cargo check -p wenlan-server
cargo check -p wenlan                     # the CLI binary
cargo build -p wenlan --release
./target/release/wenlan --help

# Run tests for a single crate
cargo test -p wenlan-core
cargo test -p wenlan-core --lib <module>::tests
cargo test -p wenlan-core <test_name>

# Generate coverage reports (opens in browser)
bash scripts/coverage.sh

# Set up git hooks (one-time; .githooks/pre-commit + pre-push)
bash scripts/setup-hooks.sh

# Eval benchmarks (require GPU + model files, run manually)
# Unit tests for eval modules (fast, no GPU):
cargo test -p wenlan-core --lib locomo::tests
cargo test -p wenlan-core --lib longmemeval::tests

# Generate eval baselines (slow, needs Qwen 3.5-9B on Metal GPU):
cargo test -p wenlan-core --test eval_harness save_locomo_baseline -- --ignored --nocapture
cargo test -p wenlan-core --test eval_harness save_longmemeval_baseline -- --ignored --nocapture
# Baselines saved to <EVAL_BASELINES_DIR>/*.json (gitignored, default ~/.cache/origin-eval).
```

Pre-commit auto-formats Rust and runs Clippy on changed crates. Pre-push runs workspace clippy + library tests.

## Cross-platform

Wenlan runs on macOS (arm64, x86_64), Linux (x86_64, aarch64; musl), and Windows (x86_64).

| OS | Data dir | Service registration |
|---|---|---|
| macOS | `~/Library/Application Support/wenlan/` | launchd via `~/Library/LaunchAgents/com.wenlan.server.plist` (user-level) |
| Linux | `~/.local/share/wenlan/` (or `$XDG_DATA_HOME/origin`) | systemd user unit at `~/.config/systemd/user/wenlan-server.service` (qualifier dropped per `ServiceLabel::to_script_name()`). Enable lingering with `loginctl enable-linger` if you want the service alive after logout. |
| Windows | `%LOCALAPPDATA%\origin\` | Per-user Task Scheduler ONLOGON task registered via `schtasks.exe /create /tn OriginServer /sc ONLOGON /tr <exe> /f`. `wenlan background on` short-circuits before service-manager and drives schtasks directly (wenlan-server is a plain console app and would otherwise time out at 30s under sc.exe + the Windows Service Control Protocol). `wenlan background off` calls `schtasks /delete /tn OriginServer /f`. |

`wenlan background on` / `wenlan background off` work on macOS, Linux, and Windows. macOS + Linux go through the `service-manager` crate (launchd / systemd-user); Windows takes the schtasks path described above so the daemon does not need a service dispatcher.

### llama-cpp-2 backend

By default, Linux and Windows builds are CPU-only. macOS keeps Metal. CUDA and Vulkan backends are not enabled in v1; they will land behind opt-in cargo features in a follow-up.

### ORT (ONNX Runtime) on Windows

If you see `Failed to load onnxruntime.dll` or version-mismatch errors on Windows, set `ORT_DYLIB_PATH` to the bundled `onnxruntime.dll` inside the Wenlan install directory before starting the daemon. The bundled DLL ships in the Windows release zip.

### Daemon bind address

The daemon binds to `127.0.0.1:7878` by default. To expose it on a non-loopback address (e.g., inside Docker), set `WENLAN_BIND_ADDR=0.0.0.0:7878` in the daemon's environment. The Docker image already sets this.

### Manual Windows verification from macOS

The CI matrix runs `windows-2022` on every PR and is the primary signal. For hands-on testing, run a Windows 11 VM via UTM or Parallels; install MSVC 2022 Build Tools and Rust; then `cargo build --release -p wenlan-server` and `scripts/smoke-windows.ps1`.

### Linux smoke from macOS

```bash
bash scripts/smoke-linux.sh
```

Builds the multi-arch daemon image (linux/arm64 for native Apple Silicon speed via OrbStack / Docker Desktop), starts a container, exercises the HTTP API, asserts responses, tears down. Runtime ~3 minutes after the first build.

## Local vs CI test responsibilities

Wenlan runs across several layers. The split is driven by three questions: **(1) Can a hosted runner do this?** (no GPU, no API keys, no cost). **(2) Is it under 60s on cold cache?** **(3) Does it gate correctness or measure quality?** Quality measures never gate.

### Terminology: e2e / smoke / live

- **`_e2e.rs`** (`chat_import_e2e`, `doc_reconcile_e2e`, `page_citations_e2e`, ...) = hermetic: full internal pipeline, in-process, external deps (the LLM) faked/stubbed. Fast, deterministic, CI-safe (L4).
- **`scripts/smoke-*.sh`** = HTTP black-box check against a running daemon. Depth varies — check whether the script actually invokes the real on-device model:
  - No real model touched (`smoke-folder-ingest.sh`, `smoke-linux.sh`, `smoke-windows.ps1`) → plain **smoke test**, CI-safe (L4).
  - Real on-device model touched → **live smoke test**, filename folds the qualifier in (`live-smoke-doc-reconcile.sh`, `live-smoke-page-citations.sh`), L7 manual-only (needs the qwen3-4b GGUF cached; GitHub runners have no Metal/GPU).
- Never write bare "smoke test" for a GPU-gated script — the word alone doesn't signal depth. Always pair it with "live" so the non-hermetic tier is legible at a glance, in code comments and docs alike.

| Layer | What runs | Where | When | Time | Blocks? |
|---|---|---|---|---|---|
| **L1 dev loop** | rust-analyzer / IDE | Local | Every save | <1s | No |
| **L2 pre-commit** | `cargo fmt --all`, clippy on staged crates | Local | `git commit` | ~5s | Yes |
| **L3 pre-push** | `cargo clippy --workspace --all-targets`, `cargo test --workspace --lib` | Local | `git push` | ~60-90s | Yes |
| **L4 CI on PR** | Same checks workspace-wide; tests for types + server + CLI; core lib tests + chat_import_e2e + distillation_quality + folder_ingest_e2e; live-daemon HTTP acceptance suite — black-box tests against the running daemon; first member `scripts/smoke-folder-ingest.sh` (ingest → search sentinel → delete → reap), new user-facing flows add a script here as they land | GitHub (`ci.yml`) | Every PR | ~10min | Yes (required) |
| **L5 coverage on PR** | `cargo llvm-cov` on wenlan-core + wenlan-server only | GitHub (`coverage.yml`) | Every PR | ~10min | **No (informational)** |
| **L6 main canary** | Embedding-only eval (`cargo test -p wenlan-core --lib eval::retrieval -- --ignored`) | GitHub (`ci.yml`) | Push to `main` | ~10min | No (post-merge) |
| **L7 manual local** | `bash scripts/coverage.sh` (HTML coverage), GPU eval suite (`cargo test -- --ignored`), Anthropic batch judge (`ANTHROPIC_API_KEY=... cargo test ...`), live smokes with a real on-device judge (`bash scripts/live-smoke-doc-reconcile.sh`, `bash scripts/live-smoke-page-citations.sh`) — run the matching live smoke before merging a feature whose e2e stubs the LLM or never boots the daemon | Your laptop | On demand | minutes-hours | No |
| **L8 pre-release** | Full eval suite vs saved baseline. Commit a **curated, env-stamped snapshot** of headline numbers to a results doc/README (single-run tagged "scaffold"; headline claims need N≥3 + stddev). Raw per-run baselines + history series stay gitignored. See "Commit policy" under Eval Citation Discipline. | Your laptop | Per release | hours | Soft gate |

### What does NOT run in CI and why

- **GPU evals (LongMemEval / LoCoMo runner functions, Qwen3.5-9B inference)** — GitHub macOS runners have no Metal acceleration. The tests are `#[ignore]`d so they don't accidentally run.
- **Anthropic API batch judge** — costs $0.35/run and requires `ANTHROPIC_API_KEY` which we don't expose to PR runs from forks.
- **Tauri / desktop coverage** — the desktop app lives in [7xuanlu/wenlan-app](https://github.com/7xuanlu/wenlan-app) and runs its own CI there. This repo's coverage is scoped to `wenlan-core + wenlan-server`.

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

For full eval discipline (fixture management, baseline layout, env vars, seed scripts, pre-flight checklist, runner conventions), see `app/eval/AGENTS.md` and `crates/wenlan-core/src/eval/AGENTS.md`. The subdir AGENTS.md files apply per the agents.md hierarchical-instruction convention when an agent is working under those subtrees.

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
3. The `v*` tag push triggers `.github/workflows/release.yml`, which builds `wenlan`, `wenlan-server`, and `wenlan-mcp`, uploads standalone binaries to the release, and publishes it.
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

**Version files that must stay in sync:** `version.txt`, `.release-please-manifest.json`, and the root workspace `Cargo.toml` (`# x-release-please-version` marker on the `[workspace.package]` version line; the 4 crates inherit it via `version.workspace = true`). The release-please workflow syncs these automatically on the release branch; manual version changes must update all of them. The desktop app version lives in [7xuanlu/wenlan-app](https://github.com/7xuanlu/wenlan-app) and bumps independently.

**release-please determines "last version" from merged PR commit messages**, not tags or manifest. It scans for commits matching `chore(main): release X.Y.Z`. Deleting a tag or GitHub Release is NOT enough to reset the version. You must also ensure no commit message in the history matches that pattern, or use `release-as` to force-override.

**Never delete a release tag without also cleaning up the commit history.** If you need to undo a release version, you must rewrite the commit message that release-please created (`git filter-branch --msg-filter`), delete the tag, delete the GitHub Release, and rename the merged PR title via API. Otherwise release-please will keep bumping from the old version.

**The `release.yml` workflow ships the local runtime.** It handles: origin CLI, wenlan-server, wenlan-mcp, standalone binary uploads, crates.io publishing for `wenlan-types` + `wenlan-mcp`, and npm publishing for `wenlan-mcp` + `wenlan`. It does NOT build a desktop bundle — wenlan-app builds its own DMG in its own repo.

### Branch protection

Main branch has: required CI (`conclusion` — aggregate gate over `fmt` + `lint` + `test`, rust-lang convention from cargo / rustup / rust-analyzer) before merge, no force pushes, no deletion. `enforce_admins: false` so the repo owner can push directly for hotfixes. Force push requires temporarily enabling it via API (remember to re-disable after).

### Git hooks (auto-activated)

Manual setup: `bash scripts/setup-hooks.sh`. Hooks live under `.githooks/`.

- **Pre-commit:** auto-formats Rust (`cargo fmt --all`, re-stages changed files) + Clippy on changed crates. Formatting issues can never reach CI.
- **Pre-push:** workspace clippy + library tests. No coverage gate (see above).

### Drift-defense (doc/flag/config drift)

Three fail-loud CI teeth live as `#[cfg(test)]` lib tests in `crates/wenlan-core/src/drift_guard.rs` (picked up by the same `cargo test --workspace --lib` that CI + pre-push already run — no extra wiring):

- **Teeth #1 — path resolver:** tracked markdown may not reference an in-repo path that doesn't exist on the branch. Skips `docs/plans/**`, `docs/superpowers/**`, and `*AUDIT.md` (historical/aspirational), and only checks file-like refs. Suppress an intentional ref with `<!-- drift-ok -->`.
- **Teeth #2 — flag doc contract (fail-closed):** every behavioral `WENLAN_*` flag read in `crates/*/src` must be documented in an `AGENTS.md`, else allowlisted (`FLAG_ALLOWLIST`, infra/test) or grandfathered (`BASELINE_UNDOCUMENTED`, the burn-down list of flags undocumented at introduction). A NEW undocumented flag fails the build.
- **Teeth #3 — version sync:** `version.txt`, `.release-please-manifest.json`, and the root workspace `Cargo.toml` must carry an identical version string.

The fuzzy surfaces (eval numbers stale vs the current env-hash, design-doc/decision rot, memory→repo dangling pointers, stale worktrees) are covered by the read-only `doc-drift-auditor` subagent. Run weekly, locally:

- One-off: `bash scripts/drift-audit.sh`
- Recurring: `/loop 7d "bash scripts/drift-audit.sh"`, or a cron/launchd entry. Reports land in `docs/superpowers/drift-reports/` (gitignored working-doc space).

## Architecture

Wenlan is a **Personal Agent Memory Layer** — a local-first memory server on macOS where AI agents write what they learn and humans curate. Daemon-centric: a headless HTTP server owns all business logic and data; the desktop app, the CLI, and external MCP clients are all thin clients over its HTTP API.

### Workspace Layout

The repo is a Cargo workspace with 5 crates:

| Crate | Role | Key dependencies |
|---|---|---|
| `crates/wenlan-types` | Shared API boundary types (request/response, memory, entities). Lightweight: serde + serde_json + anyhow only. Consumed by `wenlan-mcp`, `wenlan-app` (separate repo, via crates.io), and any other downstream tool. | serde |
| `crates/wenlan-core` | All business logic: DB, embeddings, LLM engine, search, classification, knowledge graph, distill cycles, pages, export, eval. **Must have NO axum or tauri dependencies.** | libSQL, FastEmbed, llama-cpp-2, hf-hub |
| `crates/wenlan-server` | Headless HTTP daemon on `127.0.0.1:7878`. Depends on `wenlan-core`. Provides the runtime process used by CLI background management. | axum, tower, clap |
| `crates/wenlan-cli` | CLI binary `wenlan`. Talks to daemon HTTP via `wenlan-types` and owns setup/service commands. Subcommands include `status`, `setup`, `background`, `restart`, `doctor`, `models`, `keys`, `connect`, `sources`, `capture`, `memories`, and `spaces`. | reqwest, clap |
| `crates/wenlan-mcp` | MCP server binary that bridges MCP clients (Claude Code, Cursor, Codex, Claude Desktop, etc.) to the daemon HTTP API. Stdio + streamable-HTTP transports via the `rmcp` crate. Ships as a standalone binary + npm package (`npx -y wenlan-mcp`). | rmcp, reqwest, schemars |

The daemon (`wenlan-server`) is the single source of truth. External tools (the desktop app, MCP clients via `wenlan-mcp`, `wenlan` CLI, curl) all talk HTTP to the same daemon. `wenlan-mcp` source lives in this monorepo; at runtime it's a separate process the MCP client spawns.

### Stack

- **Daemon**: Rust, Axum 0.8 (HTTP), libSQL (Turso's SQLite fork — vectors, knowledge graph, documents), Tokio, FastEmbed (BGE-Base-EN-v1.5-Q, 768-dim, 512-token max), llama-cpp-2 (Qwen3-4B-Instruct-2507 via Metal GPU; Qwen3.5-9B optional), launchd for process management
- **CLI** (`wenlan`): Rust, reqwest, clap

### Database: libSQL (owned by wenlan-core)

One libSQL database at the platform data directory (`dirs::data_local_dir()/origin/memorydb/origin_memory.db`; on macOS, `~/Library/Application Support/wenlan/memorydb/origin_memory.db`), owned by `MemoryDB` in `crates/wenlan-core/src/db.rs`:
- **Document chunks**: `chunks` table with `F32_BLOB(768)` vector column, DiskANN indexing (768-dim, BGE-Base-EN-v1.5-Q)
- **Knowledge graph**: `entities`, `relations`, `observations` tables with FK cascades
- **Full-text search**: FTS5 virtual table (`chunks_fts`) auto-synced via triggers
- **Hybrid search**: Vector similarity + FTS combined with Reciprocal Rank Fusion (RRF)

**Connection pattern**: `tokio::sync::Mutex<libsql::Connection>` — `libsql::Connection` is `Send` but not `Sync`, so it's wrapped in an async `Mutex` inside `MemoryDB`.

**Sharing pattern**: `MemoryDB` is wrapped in `Arc<MemoryDB>` at the state layer (`ServerState.db: Option<Arc<MemoryDB>>`). This lets handlers clone the `Arc` out of the `RwLock<ServerState>` guard and drop the guard before performing long-running operations.

### Events: EventEmitter trait (no tauri::Emitter in core)

Instead of passing `tauri::AppHandle` into business logic, `wenlan-core` defines an `EventEmitter` trait:

```rust
// crates/wenlan-core/src/events.rs
pub trait EventEmitter: Send + Sync {
    fn emit(&self, event: &str, payload: &str) -> Result<()>;
}
pub struct NoopEmitter;
```

- The daemon uses `NoopEmitter` (no UI to notify directly)
- The desktop app (separate `wenlan-app` repo) provides a `TauriEmitter` adapter that wraps `AppHandle::emit`
- `MemoryDB::new(db_path, emitter: Arc<dyn EventEmitter>)` takes the trait object

This keeps `wenlan-core` framework-agnostic and testable with `NoopEmitter` in unit tests.

### IPC Surface

All data flows through the daemon's HTTP API. The desktop app, CLI, and MCP clients all hit it.

- **HTTP API**: Axum on `127.0.0.1:7878`, served by `wenlan-server`. Used by the desktop app, the `wenlan-mcp` MCP server (same workspace, separate binary process), the `wenlan` CLI, and any external tool.
  - General: `/api/health`, `/api/status`, `/api/search`, `/api/context`, `/api/ping`
  - Ingest: `/api/ingest/text`, `/api/ingest/webpage`, `/api/ingest/memory`
  - Memory CRUD: `/api/memory/store`, `/api/memory/search`, `/api/memory/confirm/{id}`, `/api/memory/list`, `/api/memory/delete/{id}`
  - Knowledge graph: `/api/memory/entities`, `/api/memory/relations`, `/api/memory/observations`
  - Profile & Agents: `/api/profile`, `/api/agents`, `/api/agents/{name}`
  - WebSocket: `/ws/updates`

## Key Modules

Per-crate module tables live in subtree `AGENTS.md` files (loaded when an agent works under that crate, per the agents.md hierarchical-instruction convention):

- `crates/wenlan-core/AGENTS.md` — all business logic (db, engine, classify, extract, rerank, refinery, pages, eval, ...).
- `crates/wenlan-server/AGENTS.md` — HTTP daemon (router, routes, state, ingest_batcher, scheduler, ...).

## Key Modules — wenlan (CLI, `crates/wenlan-cli/src/`)

The `wenlan` binary — a thin reqwest-based CLI for the daemon's HTTP API. Subcommands cover `setup`, `background`, `restart`, `status`, `doctor`, `models`, `keys`, `connect`, `sources`, `capture`, `memories`, and `spaces`. The CLI does not touch the database directly: every command is an HTTP call.

## Conventions

### Eval Citation Discipline

See `app/eval/AGENTS.md` "eval citation discipline" section for the full rules (single-run, schema-version, receipt-only, per-case visibility, layer attribution, commit policy). External-facing numbers MUST satisfy those rules.

### Crate boundaries
- **wenlan-core must have NO tauri or axum dependencies.** Verify with `grep -rn "use tauri\|use axum" crates/wenlan-core/src/` — expect zero hits. Any event emission goes through the `EventEmitter` trait.
- **wenlan-types must be lightweight.** Only serde + serde_json + anyhow. No chrono, no tokio, no heavy deps. These types are shared with `wenlan-mcp` (same workspace, Apache-2.0) and `wenlan-app` (AGPL-3.0 separate repo, consumes via crates.io), so adding heavy deps forces them downstream.
- **Don't add business logic to wenlan-server.** Route handlers should call `wenlan-core` functions with state snapshots — the server's job is HTTP framing, not logic.
- **Don't add new HTTP endpoints to the CLI.** Use existing daemon endpoints. If a CLI subcommand needs new data, add a daemon endpoint first.
- **MCP wrappers in `wenlan-mcp` always typed-deserialize.** Every `_impl` method in `crates/wenlan-mcp/src/tools.rs` deserializes the daemon response into a typed wire struct from `wenlan-types` (e.g. `SearchPagesResponse { pages: Vec<Page> }`), never into `serde_json::Value`. Untyped responses silently emit whatever shape the daemon returns; typed deserialization fails loud on envelope-key drift. Mirror commit `4f545869` and PR #77.

### Ingest-path parity (training-serving skew)
- **All post-store enrichment goes through `wenlan_core::ingest::run_canonical_enrichment`.** It is the ONE shared path for classify + extract + `apply_enrichment` + tags (Phase 1), entity/title/page enrichment (Phase 2), and dual-pool dedup/contradiction resolution (Phase 3). The server `handle_store_memory`, the eval seed pipeline, and the importer all call it. Do NOT re-implement a subset of enrichment in any consumer.
- **Why.** The eval seed used to re-implement a divergent subset (`enrich_db_for_eval` = entity + title + page only), so every new write-time feature (importance/T8, event_date/T11+T20, episode/T2, fact-channel/T15, dual-pool/T14, summary-nodes/T18) silently lagged in the eval path and shipped merged-but-inert, re-discovered as "starved" each eval cycle. Sharing the code makes seed-vs-production fidelity hold by construction. This is the standard fix for **training-serving skew** — Google "Rules of ML", Rule #32: *"Re-use code between your training pipeline and your serving pipeline whenever possible"* → *"eliminates a source of training-serving skew."* See also 12-Factor X (dev/prod parity) and the technical-debt framing (Cunningham, OOPSLA '92): the eval shortcut was debt never repaid.
- **New write-time feature checklist.** Add it inside `run_canonical_enrichment` (not in a consumer), then add a seed-completeness assert — a contract test (Fowler, `ContractTest`) — so the seed cache fails loud when the feature's artifact is missing rather than silently absent. A flag merged without its artifact present in the seed is unmeasurable.

### Eval seed + eval read: ONE route, ONE contract (no drift)

The recurring failure mode was not any single missing artifact — it was that seeding a cached scenario DB was a *scatter of manual STEP tests* (`seed_inject_event_dates`, `seed_backfill_classify`, entity sweep, `seed_backfill_episodes`, `distill_pages`) run by memory. Miss one and a channel ships starved, then a graph/temporal A/B over it returns a null that gets misread as "the channel doesn't help" — a lie about a dead substrate, re-discovered every cycle.

- **Seed side — the ONE route.** Re-seed cached scenario DBs with the orchestrator `seed_scenario_dbs_complete` (`crates/wenlan-core/tests/eval_harness.rs`). It runs every enrichment step in the correct order (event_date inject → classify → entity/`memory_entities` sweep → episodes → distill) then asserts `SeedExpectations::complete()`. **Never hand-run the individual `seed_*` STEP tests** — they are the orchestrator's internals. Run the one route and the seed is complete *and* contract-verified by construction.
- **Contract side — teeth, not prose.** `crates/wenlan-core/src/eval/seed_contract.rs` is the single liveness contract. `SeedExpectations::complete()` hard-fails the seed when a channel's substrate is empty: `memory_entities = 0` (graph), `event_date = 0` (temporal), `pages = 0` active (page channel), plus dupes + classification from `strict()`. These are *presence* checks (`> 0`), not coverage percentages — a percentage floor rots (see the L3/coverage note), but zero links means the channel is dead, which is the bug. `strict()` stays lenient (report-only) for minimal seeds; only `complete()` has teeth.
- **Eval side — refuse, don't lie.** The SAME contract gates the consumer: every per-query eval collector calls `seed_contract::assert_feature_substrate_live(conn, feature)` at entry. A graph A/B over a DB with zero `memory_entities` (or a temporal A/B with zero `event_date`, or a page-channel A/B with zero active `pages`) **errors loud** ("EVAL REFUSED") instead of emitting a null. Producer and consumer share one contract, so neither can drift onto a dead substrate.
- **Adding a write-time channel.** Add its step to `seed_scenario_dbs_complete`, its presence floor to `SeedExpectations` (+ wire it into `assert_feature_substrate_live` if it has an A/B), and a unit test in `seed_contract.rs`. The contract — not a runbook — is what keeps the seed honest.

### Async and locking
- **Never hold a `tokio::sync::RwLock` read or write guard across `.await`.** Holding a read guard during an LLM call (which can take seconds) blocks all writers. Pattern: snapshot what you need from the guard into a scoped block that ends before the await, then call the async function with the cloned values. See `crates/wenlan-server/src/memory_routes.rs` `handle_store_memory` for an example of the post-ingest enrichment pattern.
- **`Arc<MemoryDB>` is the sharing primitive.** `ServerState.db` is `Option<Arc<MemoryDB>>`. Clone the Arc out of the guard rather than borrowing through the guard.
- **Daemon is the single writer.** Only `wenlan-server` opens the libSQL database. The desktop app and CLI never touch the DB directly — they talk HTTP.
- **libSQL connection pattern**: `MemoryDB` holds `tokio::sync::Mutex<libsql::Connection>` internally. Never try to share a `libsql::Connection` across tasks directly (`Send` but not `Sync`).

### SQL, strings, data
- **SQL safety**: Always use parameterized queries — never interpolate user input into SQL strings
- **NULL semantics**: Store `Option<T>` as SQL NULL, not empty string — so IS NULL filters work correctly
- **UTF-8 safety**: Never byte-index Rust strings (`&s[..n]`) — use `chars().take(n)` or `strip_prefix`/`strip_suffix`. Exception: byte-slicing after a verified ASCII prefix check is safe (the boundary is guaranteed valid), but prefer the char-safe version anyway for consistency.
- **Batch SQL**: Wrap multi-row insert/delete loops in BEGIN/COMMIT transactions
- **LIKE patterns against JSON**: Quote the match target to avoid substring false positives — `%"{id}"%` not `%{id}%` (e.g., `mem_1` would otherwise match `mem_10`). See the fix in `crates/wenlan-core/src/db.rs` (the `%"{id}"%` quoting shown above) and the regression test.

### Dev environment gotchas

**Daemon lifecycle:**
- **Worktree daemon mismatch**: The daemon on port 7878 can be from launchd, main branch, a stale worktree, or a previous session. Always verify which binary is running: `lsof -i :7878` to get the PID, then `lsof -p <PID> | grep "txt.*wenlan-server"` to see the binary path and size. Kill and restart from the current working tree.
- **Stale binary after merge/pull**: `cargo build -p wenlan-server` may report "0.64s Finished" without recompiling if the source timestamps haven't changed (e.g., after `git pull` fast-forward). Touch a source file to force recompilation: `touch crates/wenlan-server/src/router.rs && cargo build -p wenlan-server`. Verify the binary timestamp matches: `ls -la target/debug/wenlan-server`.
- **kill vs kill -9**: `kill <PID>` may not terminate the daemon cleanly. Always use `kill -9 <PID>` and verify with `lsof -ti :7878` afterward. If the port is still in use, another process took over.
- **Worktree target directories are per-worktree**: Each `.worktrees/<name>` checkout has its own `target/`. Building inside a worktree writes to that worktree's `target/`, not the main repo's. Verify a binary's source with `lsof -p <PID> | grep wenlan-server` so you don't run a stale binary from a different worktree.
- **Upgrading the daemon requires a restart**: installing a new binary does NOT replace an already-running daemon -- the new process detects the healthy incumbent on port 7878 and exits (`wenlan-server/src/main.rs`). `wenlan background on` stops the running service before reinstalling, and `wenlan restart` (stop then start) reloads it explicitly. The MCP version handshake surfaces a stale daemon (`VersionStatus::DaemonOutdated`) and points users at `wenlan restart`. Enabling the cross-encoder (`WENLAN_RERANKER_ENABLED=1`) blocks startup on a one-time ~1.1GB model download and, on failure, serves with no rerank -- `/api/status` now reports `reranker` as `disabled` / `active` / `failed` so the degraded state is visible.

**Other:**
- **Metal/ggml on macOS Tahoe 26.x**: `ggml_metal_init` may fail even though native Metal works. The daemon auto-degrades and continues without LLM. Not a code bug. Check for competing GPU processes: `pgrep -la wenlan`.
- **Dev and prod share data by default**: Both use port 7878 and the platform data directory (on macOS, `~/Library/Application Support/wenlan/`). For isolated testing, override explicitly: `WENLAN_PORT=7879 WENLAN_DATA_DIR=/tmp/origin-test cargo run -p wenlan-server`.

### Worktree cleanup after squash-merge

GitHub squash-merge bundles all PR commits into one new commit on `main` with a fresh SHA. The original commits on the feature branch keep their old SHAs and remain in the local repo + worktree even though their content has shipped. This creates three traps:

- **`git cherry main feature/<name>` lies.** It compares commit SHAs (not patch content) and will mark all squashed commits as "unmerged" (`+` prefix). The branch may be fully merged content-wise. Verify by reading the squash commit body (`git log -1 --format=%B <squash-sha>`); the body lists each original PR commit message. Alternatively, grep `main`'s log for keywords or file paths the branch added.
- **Stale worktrees accumulate.** `.worktrees/<name>/` is not auto-removed when a PR merges. After confirming all content is in `main`, run from the main repo root: `git worktree remove --force .worktrees/<name>` then `git branch -D <branch>` (force `-D` is needed because `git` thinks it's unmerged for the same SHA reason) then `git worktree prune`.
- **`.gitignored` per-checkout artifacts.** Files under gitignored paths (`app/eval/baselines/`, `.fastembed_cache/`, build outputs) live per-worktree. Removing a worktree removes its private copies. If a worktree happened to be the only host of some large gitignored artifact (eval baseline DBs, downloaded models), back it up to the canonical shared location first (e.g. `~/.cache/origin-eval/` via `scripts/migrate-eval-cache.sh`) before deleting the worktree.

Run this hygiene pass roughly once a week or whenever `git worktree list` exceeds ~5 entries. Stale worktree paths waste disk + confuse "is this work merged?" investigations.

### Misc
- `WENLAN_BIND_ADDR=<host:port>`: override the daemon's bind address (default `127.0.0.1:7878`). Used inside Docker to listen on `0.0.0.0`.
- Log filter default is `warn` — add modules explicitly for `info` logs (e.g., `wenlan_core::db=info`, `wenlan_server=info`)
- All local data stored in the platform data directory (`dirs::data_local_dir()/origin/`; on macOS, `~/Library/Application Support/wenlan/`) — MemoryDB, config, activities, tags
- Crate names: `wenlan-types`, `wenlan-core`, `wenlan-server`, `wenlan` (CLI), `wenlan-mcp` — all in this workspace. The desktop app crate `wenlan-app` lives in [7xuanlu/wenlan-app](https://github.com/7xuanlu/wenlan-app).
- **Licenses**: all five workspace crates (`wenlan-types`, `wenlan-core`, `wenlan-server`, `wenlan` CLI, `wenlan-mcp`) are **Apache-2.0** via workspace inheritance. The desktop app in `wenlan-app` is **AGPL-3.0-only** (separate repo).
- `wenlan-mcp` is in-tree at `crates/wenlan-mcp/` (merged from the old `7xuanlu/wenlan-mcp` repo on 2026-05-09 via `git subtree`). It talks to the daemon via HTTP at runtime and is published to npm as a standalone binary (`npx -y wenlan-mcp`).

### Retrieval helpers location (PR-A, 2026-05-27)

`crates/wenlan-core/src/retrieval/` is the canonical home for retrieval helpers (`hard_filters`, `signals`). The old `composite/` namespace was deleted along with the dead `CompositeWeights` scaffolding when PR #200 closed. Future retrieval-channel additions (page-channel in PR-B, etc.) live in `retrieval/`.

### Retrieval / LLM / consolidation tuning

The deep per-flag reference — retrieval-channel flags (`WENLAN_RERANKER_MODEL`, `WENLAN_RERANKER_MODE`, `WENLAN_GRAPH_MEMORY_STREAM`, `WENLAN_ENABLE_TEMPORAL_SOFT_BOOST`, `WENLAN_TEMPORAL_BONUS`, `WENLAN_ENABLE_INTENT_LLM`, `WENLAN_RERANK_SKIP_PREFERENCE`, `WENLAN_ENABLE_ENTITY_SWEEP`), the on-device LLM throughput flags (`WENLAN_LLM_SLOT_BACKFILL`, `WENLAN_LLM_PREFIX_KV_CACHE`), and the always-on consolidation demotion (P3) — lives in **`crates/wenlan-core/AGENTS.md`**, which loads automatically when an agent works under that crate (agents.md hierarchical convention). `drift_guard` teeth #2 scans every tracked `*AGENTS.md`, so the flag-doc contract still holds.
