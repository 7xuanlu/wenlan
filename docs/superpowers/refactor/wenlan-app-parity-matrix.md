# Wenlan App Parity and Compatibility Matrix

- **Date:** 2026-06-28
- **App checkout:** `/Users/lucian/Repos/wenlan-app`
- **App branch:** `codex/wenlan-app-runtime-doc-refresh`
- **Wenlan backend source:** `/Users/lucian/Repos/wenlan`
- **Purpose:** prerequisite matrix before the full `origin-app` -> `wenlan-app` refactor run.
- **Current status:** refreshed on 2026-06-27 after typed-client, sidecar, MCP bridge, Dock/app-activation, avatar path, neutral theme fallback, Home pending-revision/refinery review work, daemon-backed setup status work, post-merge API parity wrappers, `/api/capture-stats`, daemon-backed global tag inventory, and daemon-backed status/reranker diagnostics.

## Evidence Snapshot

| Evidence | Current value | Source |
|---|---:|---|
| Frontend `invoke(...)` calls | 156 | `docs/superpowers/refactor/wenlan-app-inventory/frontend-invokes.txt` |
| Registered Tauri commands | 176 | `app/src/lib.rs` + `search-rs-outline.txt` |
| Rust `origin_types` references | 0 | `app/src` residual scan |
| Runtime identity references | 148 | `Origin`/`origin-server`/`origin-mcp`/`com.origin`/relay residual scan |
| Stale taxonomy references | 183 | `concept`/`goal`/`domain` residual scan |
| Wenlan typed request/response declarations in `requests.rs` + `responses.rs` | 99 | `wenlan-types` scan |

## Compatibility Gates

| Gate | Required before | Check | Fallback behavior |
|---|---|---|---|
| Daemon reachability | Any app UI data load | `GET /api/health` typed as `HealthResponse` | show daemon-offline state; do not assume local DB access |
| Daemon capability set | P0 feature UI | `GET /api/status` plus route probes for optional surfaces | hide unsupported panels; show "requires newer daemon" diagnostics |
| Typed wire contract | API wrapper migration | `wenlan-types` dependency and serde smoke tests for critical envelopes | app-local DTO allowed only for Tauri-local or missing shared types |
| Sidecar availability | `cargo build`/Tauri bundle | target-specific `wenlan-server`, `wenlan-mcp`, and `cloudflared` sidecars present | fail loud with migration instructions; do not hand-copy ad hoc binaries |
| Origin bridge detection | Runtime identity rename | old LaunchAgents, app paths, MCP config keys, token/relay ids detected | preserve old state for bridge release; do not silently open empty Wenlan state |

## P0 Matrix

These block "caught up enough for full refactor run".

| Surface | Current source | App status | Action | Daemon compatibility |
|---|---|---|---|---|
| `/api/health` | `wenlan-server/src/router.rs`; `HealthResponse` | present in `WenlanClient.health` | keep typed daemon reachability gate | required; app cannot proceed without daemon |
| `/api/status` | `StatusResponse`, `RerankerStatus` | `get_index_status` now merges daemon count/source/reranker fields with app-local indexing activity; StatusBar surfaces reranker failures without treating daemon `is_running=true` as perpetual indexing | broaden settings diagnostics if operator detail needs more than the compact StatusBar signal | required for capability gate; tolerate absent fields by feature-gating |
| `/api/capture-stats` | daemon capture stats route | `get_capture_stats` calls the dedicated daemon route and maps `total_chunks` to the existing frontend `total` key | keep route-level regression test so it does not drift back to `/api/status` | required for capture-count UI |
| `/api/refinery/queue` | `ListRefinementsResponse` | typed Rust/Tauri/TS wrappers present; Home Worth-a-glance surfaces proposals | keep Home review lane; consider a dedicated review screen only if queue volume requires it | require route for review queue; hide panel if absent |
| `/api/refinery/queue/{id}/accept` | `AcceptRefinementResponse` | typed Rust/Tauri/TS wrappers present; Home Accept calls it and invalidates review/recent caches | reuse typed response for any future dedicated review queue | optional until queue route exists |
| `/api/refinery/queue/{id}/reject` | `RejectRefinementResponse` | typed Rust/Tauri/TS wrappers present; Home Dismiss calls it and invalidates the review queue | reuse typed response for any future dedicated review queue | optional until queue route exists |
| `/api/memory/pending-revisions` | daemon route | typed Tauri/TS wrapper present; Home Worth-a-glance surfaces pending revision cards | keep Home review lane; consider a dedicated review screen only if queue volume requires it | require for P0 review surface; hide list if absent |
| `/api/memory/pending-revision/{source_id}` | daemon route | present as per-memory `getPendingRevision` | keep but type through `wenlan-types` if available | optional detail route |
| `/api/memory/revision/{id}/accept` | `RevisionAcceptResponse` | present command accepts `sourceId`; Home review lane calls it and invalidates pending revision/recent memory caches | reuse typed response for any future dedicated review queue | require typed response before review UI |
| `/api/memory/revision/{id}/dismiss` | `RevisionDismissResponse` | present command accepts `sourceId`; Home review lane calls it and invalidates pending revision cache | reuse typed response for any future dedicated review queue | require typed response before review UI |
| `/api/memory/contradiction/{source_id}/dismiss` | `ContradictionDismissResponse` | present command returns typed response | surface in review queue | optional if contradictions route absent |
| `/api/memory/{source_id}/enrichment-status` | `EnrichmentStatusResponse` | typed Rust/Tauri/TS wrapper present; Memory Detail shows non-blocking status when available | keep hidden on old daemons and do not block memory render | optional per-memory route; show unknown state if absent |
| `/api/sources` | daemon source routes | Obsidian registered source list/add/remove/sync route through daemon-backed Tauri commands; local directory sources remain app-config/watcher compatibility entries merged into `list_registered_sources` | keep legacy source-name commands and directory watcher compatibility only where still needed | required for source management |
| `/api/config` | `ConfigResponse`, `UpdateConfigRequest` | switched to `wenlan-types`; wizard completion now updates `setup_completed` through daemon config; app-local builder helper still wraps shared type | converge remaining Settings/local toggles to daemon config or mark them app-local sensors; remove stale comments/helpers when shared defaults exist | required for settings |
| `/api/setup/status` | daemon setup route | explicit app DTO, Tauri command, TS wrapper, and wizard gating are present | keep as setup source of truth; broaden settings diagnostics around mode/model/key state | required for setup correctness |
| MCP config bridge | `app/src/mcp_config.rs`, setup wizard | writes `wenlan`; detects and preserves legacy `origin` entries | keep bridge behavior through at least one compatibility release | bridge release required |
| Remote access bridge | `app/src/remote_access.rs` | token path is `~/.config/wenlan-mcp/token` with legacy `origin-mcp` import fallback; relay URL intentionally remains legacy Origin until endpoint strategy exists | define relay endpoint strategy before URL rename; do not import legacy relay ids blindly | bridge release required |
| Sidecar build contract | `app/tauri.conf.json`, `package.json`, `scripts/prepare-sidecars.sh` | uses `wenlan`, `wenlan-server`, `wenlan-mcp`, and `cloudflared`; `cargo build` passes after repeatable sidecar prep | keep repeatable sidecar prep; no hand-copying ad hoc binaries | required before build verification |

## P1 Matrix

These are required for feature parity but can follow the P0 review/status/setup surfaces.

| Surface | Current source | App status | Action | Daemon compatibility |
|---|---|---|---|---|
| `/api/memory/{id}/revisions` | `ListMemoryRevisionsResponse` | typed Rust/Tauri/TS wrapper present; Memory Detail shows non-blocking revision history | keep old version-chain wrapper only as legacy fallback until removed in a later cleanup | hide revisions panel if route absent |
| `/api/pages/{id}/revisions` | `ListPageRevisionsResponse` | typed Rust/Tauri/TS wrapper present; Page Detail shows non-blocking revision history | reuse for future page diff UI | hide revisions panel if route absent |
| `/api/pages/{id}/links` | `PageLinksResponse` | typed Rust/Tauri/TS wrappers present; PageDetail uses daemon outbound/inbound links and no longer infers links via `listPages` | keep non-blocking link UI; unresolved outbound labels stay inert | hide links section if route absent or errors |
| `/api/pages/orphan-links` | `OrphanLinksResponse` | typed Rust/Tauri/TS wrappers present; no dedicated UI yet | add orphan link review or diagnostics section | optional diagnostics until route exists |
| `/api/pages/{id}/sources` | `PageSourceWithMemory` | present via `getPageSources` | keep and type through `wenlan-types` | optional |
| `/api/pages/export` | export response | present as `exportPagesToObsidian` | keep; rename "concept" UI/file wording where user-facing | optional |
| `/api/pages/{id}/export` | export response | present as `exportPageToObsidian` | keep; rename "concept" UI/file wording where user-facing | optional |
| `/api/on-device-model` | config route | present wrapper | keep; type through `wenlan-types` | optional settings section |
| `/api/llm/test` | config route | present wrapper | keep; type through `wenlan-types` | optional settings section |
| `/api/tags` | `TagsResponse` | `list_all_tags` reads the daemon global tag list and preserves additive `document_tags` maps when available; older daemons default to an empty map | merge daemon `document_tags` response support before relying on per-document tag filters in release notes | global tags available; per-document tag map gated by daemon version |

## Taxonomy Matrix

| Surface | Current app state | Required action | Gate |
|---|---|---|---|
| `goal` memory type | removed from advertised TypeScript facet union; `lesson`/`gotcha` added; legacy `goal` remains only for old rows/maps | keep compatibility path only; do not reintroduce first-class Goal UI | `taxonomyCopy`, StructuredEditor, ProfilePage tests |
| `concept` copy | user-facing copy migrated to Page/wiki language in product-owned surfaces; deprecated aliases remain for API compatibility | keep `Concept = Page`, `listConcepts`, `getConceptSources`, `ActivityKind = "concept"` compatibility wrappers | `taxonomyCopy` + page/onboarding/import tests |
| `domain` copy | product-owned visible labels use Space; daemon wire keys still use `domain`/`space` compatibility mapping | keep daemon back-compat field names and request keys | `taxonomyCopy`, domain wire tests |
| `Origin` app copy | product-owned visible copy and runtime names are Wenlan; legacy Origin references remain for bridge paths, tests, fixtures, and compatibility docs | keep legacy references explicit as bridge artifacts; do not globally replace | runtime identity and taxonomy copy tests |

## Runtime Identity Matrix

| Surface | Current app state | Required action | Gate |
|---|---|---|---|
| Bundle id | `com.wenlan.desktop`; runtime identity test asserts product name, identifier, updater endpoint, package/repo identity | keep legacy Origin state readable through bridge paths | runtime identity tests |
| LaunchAgents | current labels are `com.wenlan.desktop` and `com.wenlan.server`; legacy `com.origin.desktop`/`com.origin.server` are detected and cleaned only when owned | preserve owned legacy cleanup and foreign-file safety | lifecycle tests |
| LaunchAgent template | current `com.wenlan.desktop.plist` uses Wenlan placeholders; legacy `com.origin.desktop.plist` retained as bridge artifact | keep legacy template untouched until bridge cleanup | lifecycle template/install tests |
| Stable app path | accepts `/Applications/Wenlan.app`, `~/Applications/Wenlan.app`, and legacy `Origin.app` bridge paths | keep old app path detection until bridge cleanup | lifecycle tests |
| Lifecycle quit command | `quit_wenlan_full` primary; `quit_origin_full` legacy alias | keep Origin alias through bridge release | Tauri wrapper + Rust command tests |
| Daemon sidecar | `wenlan-server` sidecar, plus `wenlan` CLI for service management | keep legacy process cleanup only where bridge behavior requires it | `cargo build` reaches app code |
| MCP sidecar | `wenlan-mcp` sidecar and setup entry; legacy `origin` MCP config entries are preserved | bridge old config key/package through compatibility release | setup wizard and MCP config tests |
| Remote relay | `origin-relay.originmemory.workers.dev` intentionally retained | provision/decide Wenlan relay endpoint and relay-id migration before changing URL | remote access tests |
| Data/config paths | `WENLAN_DATA_DIR` preferred; `ORIGIN_DATA_DIR` legacy fallback | do not open fresh Wenlan path if legacy data exists | migration smoke |
| UI preference keys | `wenlan-*` / `wenlan:*` primary; Origin-prefixed localStorage keys import-only legacy fallback | preserve user UI state while new writes use Wenlan keys | preference helper + consumer tests |
| Daemon port env | `WENLAN_PORT` preferred; `ORIGIN_PORT` legacy fallback | keep legacy fallback through bridge release; default to `7878` | `api::tests::wenlan_client_*_port` |

## Go/No-Go Before Full Refactor Run

Go when all are true:

- This matrix is committed.
- `bash scripts/refactor/inventory.sh` runs and generated summary is current.
- CodeGraph evaluation is recorded, `.codegraph/` is ignored, and cross-cutting edits use `codegraph sync` plus target-specific `query`/`impact`/`affected` probes before changing behavior.
- `pnpm install --frozen-lockfile --offline` exits 0.
- `pnpm test` exits 0.
- `cargo build` exits 0 with the repeatable `wenlan-server`/`wenlan-mcp` sidecar strategy.
- Remaining public rename work is gated by bridge classification: keep legacy Origin state readable, then migrate visible product/runtime identity in separate commits.

No-go:

- Starting public `Origin` -> `Wenlan` rename before typed client and sidecar bridge.
- Replacing text with `rg` globally before AST inventory surfaces are classified.
- Removing old MCP config or token paths automatically.
- Hand-copying sidecar binaries to make `cargo build` pass without a repeatable build/copy contract.
