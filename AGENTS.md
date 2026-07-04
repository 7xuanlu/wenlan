# AGENTS.md — wenlan-app

Portable instructions for any coding agent (Claude Code, Codex, etc.) working
in this repo. Tool-specific files (`CLAUDE.md`) import this rather than
duplicating it.

## Project shape

Tauri 2 desktop app: single Rust crate `app/` (crate-type lib/cdylib/staticlib)
+ React 19 / Vite 6 / Tailwind v4 frontend. `pnpm-workspace.yaml` is not a
multi-package workspace — one frontend package.

Daemon and shared types (`wenlan-server`, `wenlan-mcp`, `wenlan-cli`,
`wenlan-types`) live in the separate `7xuanlu/wenlan` repo (Apache-2.0). This
repo is AGPL-3.0, extracted 2026-05-07 from the `origin` monorepo
(see `HISTORY.md`).

## Build & run

- `pnpm dev:all` — `clean:dev` → `dev:daemon` → `tauri dev`.
- `pnpm dev:daemon` builds `wenlan-server` from a sibling backend checkout and
  runs it on `:7878`. Requires `../wenlan` next to this repo (or
  `$WENLAN_BACKEND_DIR`) with `crates/wenlan-server`, `crates/wenlan-mcp`,
  `crates/wenlan-cli`. Resolved by `scripts/resolve-backend-dir.sh`; sidecars
  built by `scripts/prepare-sidecars.sh`.
- `tauri dev` runs Vite on `:1420` per `app/tauri.conf.json`.
- `pnpm release` — `clean:release` → `release:daemon` (release-profile
  sidecar build) → `tauri build`.

## Test

- Frontend: `pnpm test` / `test:watch` / `test:coverage` (Vitest 4 + jsdom +
  Testing Library). Coverage gate (90/90/85/90) applies only to 4 whitelisted
  modules, not the whole tree.
- Backend: `cd app && cargo test` (or `pnpm test:all` for both).
- No e2e/Playwright config exists yet — UI changes aren't browser-tested.

## CI / release

- `.github/workflows/ci.yml`: fmt + clippy (`-D warnings`) + `cargo test` +
  `tsc` + vitest, on push/PR to `main`. Sidecar binaries are **empty
  placeholders** in CI (`touch`'d) — the daemon is never actually built or
  exercised there.
- `.github/workflows/release.yml`: tag-triggered (`v*`) or manual dispatch.
  Resolves the backend at its **latest** `7xuanlu/wenlan` GitHub release tag,
  checks `Cargo.toml`/`package.json`/`tauri.conf.json` versions match, builds
  and signs via `tauri-action`. The workflow's own inline comments flag the
  `tauri-action` wiring as unverified — dry-run via `workflow_dispatch`
  (draft) before trusting a real tag push. No changelog automation.

## Known gotcha: daemon/crate version drift

`app/Cargo.toml` pins `wenlan-types` by crates.io semver. The daemon
**binary** comes from a separate, unrelated process — a local sibling
checkout at whatever commit during dev, or the backend's latest GitHub
release tag in CI. Nothing in `app/src` does a runtime version handshake
between the two. If you see a "daemon vX, plugin/app expects vY" mismatch,
this split pinning is why — it's a known gap, not a regression.

## Environment

- `WENLAN_BACKEND_DIR` — override for the sibling backend checkout path
  (default: `../wenlan`). Consumed by `scripts/resolve-backend-dir.sh`. Its
  fallbacks don't reach worktrees under `.claude/worktrees/<name>` — set it
  explicitly there. Agent launch/drive recipe: `.claude/skills/run-wenlan-app/`.
- `TAURI_SIGNING_PRIVATE_KEY` (+ `_PASSWORD`) — required by `tauri build`
  because `app/tauri.conf.json` sets `bundle.createUpdaterArtifacts: true` with
  an updater `pubkey`. Unset it and the build fails at the updater-signing step
  (`release.yml` guards this with a "latest.json was not uploaded" check).

## Code style

No separate style doc — enforcement _is_ `ci.yml`, which is strict, not
advisory. Run these locally before pushing:

- `cargo fmt --check --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `pnpm exec tsc -b`
- `pnpm test`

## Where things live

- Rust app logic: `app/src/*.rs` (e.g. `api.rs`, `config.rs`, `lifecycle.rs`,
  `search.rs`, `state.rs`, `privacy.rs`).
- Frontend: `src/` — React + Tailwind v4 (CSS-first config in
  `src/index.css`, no `tailwind.config.*`). Notable libs: `react-force-graph-2d`
  / `d3-force` (entity graph), `react-markdown`, `sonner`, `@tanstack/react-query`.
  No Storybook / formal design-system package.
- Daemon/backend source: **not in this repo** — see `7xuanlu/wenlan`.
- Historical plans/inventories: `docs/superpowers/` (dated plan docs under
  `plans/`, structural dumps under `refactor/`).
