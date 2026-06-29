# Wenlan App Route Gap Classification

- **Date:** 2026-06-29 UTC / 2026-06-28 PDT
- **Worktree:** `/Users/lucian/Repos/wenlan-app/.worktrees/wenlan-app-api-parity-audit`
- **Branch:** `codex/wenlan-app-route-gap-classification`
- **Backend source:** `/Users/lucian/Repos/wenlan`

## Input Evidence

Current route-diff artifact:

```text
backend route paths: 123
app source route paths: 110
backend routes with no direct app source path: 13
app source paths with no backend router path: 0
```

Tool boundary:

| Tool | Result |
|---|---|
| CodeGraph | `sync .` was already up to date; `query WenlanClient --json` confirmed the app client surface before classification. |
| ast-grep | Direct `npx -p @ast-grep/cli` was rejected because it would fetch and execute package code against private repo files. No workaround was used. |
| LSP | No callable LSP tool is available in this Codex session. |
| `rg`/reads | Used as fallback for backend handler evidence and app surface checks. |

## Classification

| Route | Backend behavior | Current app coverage | Classification | Next action |
|---|---|---|---|---|
| `/api/config/skip-apps` | Legacy GET/PUT for skip-app list. | App uses daemon `/api/config` sparse update/read through `WenlanClient.get_config` / `update_config`. | Superseded route. | No app work unless backend removes the legacy route. |
| `/api/context` | Trust-gated LLM context injection for agents; logs retrieval access and returns context payload. | Desktop has copy-as-context UI, search, page/memory views, and activity surfaces but no agent-context invocation. | Agent/MCP route, not a desktop parity blocker. | Keep hidden unless a future diagnostics/debug context panel is designed. |
| `/api/debug/pipeline` | Returns DB pipeline status JSON. | Status/reranker diagnostics exist through `/api/status`; no pipeline panel. | Optional operator diagnostics. | Candidate for Settings diagnostics only if useful; not P0. |
| `/api/distill` | Manual distill route; surfaces pending clusters/stale pages/orphan topics for caller-side synthesis; `force` can clear user-edited page state. | App shows setup/intelligence copy and page state, but no manual distill/rebuild action. | Real capability gap with risky UX. | Design before implementation; likely Page Detail/Intelligence action with clear LLM and overwrite semantics. |
| `/api/distill/{page_id}` | Re-distills one page using daemon LLM if available; returns hint payload when unavailable. | Page Detail shows last distilled and revisions; no rebuild action. | Real capability gap adjacent to page maintenance. | Candidate after UX design; avoid silent rewrite of user prose. |
| `/api/ingest/webpage` | Stores webpage content as source `webpage`, source id URL, metadata domain. | App recognizes `webpage` as a memory source type but has no direct URL ingest flow. | Real product gap. | Strong next candidate: add typed wrapper first, then design URL/webpage ingest UI. |
| `/api/memory/entities/{entity_id}/observations` | Adds an observation directly under an entity id. | App has `addObservation` using `/api/memory/observations` from Identity Detail. | Alternate route, not currently blocking. | No app work unless consolidating graph write APIs. |
| `/api/memory/link-entity` | Links an existing memory source id to an entity id. | Entity suggestions/review surfaces exist; no explicit link-memory-to-entity editor. | Graph-authoring gap. | P2 review/editor candidate, needs UX and response typing. |
| `/api/memory/relations` | Creates an explicit relation between entities. | Identity Detail and Constellation read relations; app does not author relations directly. | Graph-authoring gap. | P2 graph editor candidate; avoid adding hidden write surface without UI. |
| `/api/ping` | Returns `pong`. | App already uses `/api/health` and `/api/status`. | Redundant health route. | No action. |
| `/api/spaces/{from}/move-to/{to}` | Reassigns memories from one space to another and returns affected count. | App supports space CRUD, reorder, star, confirm, and per-document space assignment; delete behavior was made honest against daemon semantics. | Real space-management gap. | Candidate after destructive/cleanup UX design; should expose affected count and confirmation. |
| `/api/steep` | Manually runs periodic steep/refinery backstop and returns phase counts. | Activity feed displays `steep` events; no manual trigger. | Maintenance/agent route, not a desktop parity blocker. | Defer unless adding operator maintenance panel. |
| `/ws/updates` | WebSocket subscribe/ingest channel for index progress and ingest completion. | App uses Tauri events, query invalidation, and polling-style status surfaces. | Live-update architecture gap, not required for current parity. | Defer until replacing polling/invalidation with daemon event stream is an explicit product goal. |

## Next Implementation Candidates

| Candidate | Why | Risk |
|---|---|---|
| Webpage ingest wrapper + flow | Backend route is a user-facing ingest capability and the app already has `webpage` source labels. | Needs UI design for URL/content capture source and duplicate behavior. |
| Space move/reassign | Existing sidebar space management is already user-facing and backend route exists. | Can affect many memories; needs affected-count confirmation and copy clarity. |
| Page re-distill/rebuild | Page Detail already exposes page state and revisions. | LLM availability, stale/user-edited semantics, and `force` behavior need careful UX. |
| Graph relation/link editor | Read-side graph UI exists. | Authoring relations can corrupt knowledge graph if the UX is too casual. |

Recommended next checkpoint: implement the smallest safe **webpage ingest wrapper** first, without inventing the full UI. That gives the app a typed Tauri/TS seam for `/api/ingest/webpage`; the visible URL-ingest flow can then be designed against a real wrapper and tests.
