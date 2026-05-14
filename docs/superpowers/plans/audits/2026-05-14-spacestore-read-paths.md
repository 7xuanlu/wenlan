# SpaceStore Read-Path Audit (2026-05-14)

**Context:** PR-A renames `domain` → `space` workspace-wide. PR-B may drop the `SpaceStore` subsystem. This audit informs PR-B scope by enumerating who actually reads spaces data today.

**Important disambiguation:** There are two separate "spaces" subsystems in this codebase that must be kept distinct:

1. **`MemoryDB.spaces` table** (in `origin_memory.db`, owned by `origin-core/src/db.rs`) — the newer, SQL-native spaces implementation with CRUD methods on `MemoryDB`. Used by the active HTTP API.
2. **`SpaceStore`** (in `origin-core/src/spaces.rs`, backed by a separate `spaces.db` + rusqlite) — the legacy in-memory store loaded at daemon startup from `~/Library/Application Support/origin/spaces.db`. Still live for the **tags subsystem** only; the `SpaceStore.spaces` field itself is a dead read path in production.

---

## Read paths confirmed LIVE

### 1. `MemoryDB::list_spaces` — HTTP GET /api/spaces

- **Location:** `crates/origin-core/src/db.rs:4779`
- **Query:** `SELECT s.id, s.name, ... FROM spaces s ORDER BY ...` with correlated subqueries for `mem_count` and `ent_count` from the `memories` and `entities` tables.
- **Caller chain:** `crates/origin-server/src/memory_routes.rs:1732` `handle_list_spaces` → `crates/origin-server/src/router.rs:181` `GET /api/spaces`.
- **Status:** Fully live. The route is registered and returns `Vec<origin_core::db::Space>` to callers. No MCP or CLI consumers found, but the HTTP endpoint is active and accessible to the desktop app.

### 2. `MemoryDB::get_space` — called internally by `update_space`

- **Location:** `crates/origin-core/src/db.rs:4816`
- **Query:** Same SELECT shape as `list_spaces` but filtered by `name`.
- **Caller chain:** `crates/origin-core/src/db.rs:4954` (inside `update_space`, which returns the updated space) → `crates/origin-server/src/memory_routes.rs:1757` `handle_update_space` → `PUT /api/spaces/{name}`.
- **Status:** Live — called as the return value path for the update handler.

### 3. `SpaceStore.tags` field — HTTP GET /api/tags

- **Location:** `crates/origin-server/src/memory_routes.rs:2417`
- **Access:** `s.space_store.tags.iter().cloned().collect()`
- **Caller chain:** `handle_list_tags` → `crates/origin-server/src/router.rs:366` `GET /api/tags`.
- **Status:** Live. `SpaceStore` is loaded at daemon startup via `origin_core::spaces::load_spaces()` (`main.rs:536`), which reads `spaces.db`. The `tags` field accumulates from `set_document_tags` calls (both at store time and via `PUT /api/documents/{source_id}/tags`). This is the only `SpaceStore` field actively read in a live HTTP response path.

### 4. `SpaceStore::get_document_tags` — HTTP GET /api/suggest-tags

- **Location:** `crates/origin-server/src/memory_routes.rs:2471`
- **Access:** `s.space_store.get_document_tags(&query.source, &query.source_id)`
- **Caller chain:** `handle_suggest_tags` → `crates/origin-server/src/router.rs:368` `GET /api/suggest-tags`.
- **Status:** Live. Reads `SpaceStore.document_tags` to filter already-assigned tags from the suggestion list before returning to caller.

### 5. `MemoryDB::auto_create_space_if_needed` — side effect of memory store

- **Location:** `crates/origin-core/src/db.rs:5148`
- **SQL:** `INSERT OR IGNORE INTO spaces (...)` — technically a write, but the function reads `domain` and touches the `spaces` table in `MemoryDB`.
- **Caller chain (two sites):**
  - `crates/origin-server/src/memory_routes.rs:641` — sync path in `handle_store_memory`
  - `crates/origin-server/src/memory_routes.rs:873` — async classify phase in `handle_store_memory`
- **Status:** Live write path (not a read), but it proves the `MemoryDB.spaces` table is actively maintained during memory ingestion.

---

## Read paths confirmed DEAD

### 6. `SpaceStore.spaces` field (the legacy `Space` list in SpaceStore)

- **Location:** `crates/origin-core/src/spaces.rs:207` — field definition; populated by `load_spaces()` / `SpaceStore::default()`.
- **Evidence:** No server handler reads `s.space_store.spaces` to return data to a caller. The three live `space_store.` accesses in `memory_routes.rs` (lines 882, 2417, 2427) touch only `.tags`, `.set_document_tags()`, and `.delete_tag()` — never `.spaces`.
- **HTTP space CRUD** (`GET /api/spaces`, `POST /api/spaces`, `PUT /api/spaces/{name}`, `DELETE /api/spaces/{name}`) all go through `MemoryDB` methods, not `SpaceStore`.
- **Status:** Dead read path. `SpaceStore.spaces` is loaded at startup and mutated by `SpaceStore::default()`, but no HTTP response or internal consumer reads from it at runtime.

### 7. `SpaceStore.document_spaces` field

- **Location:** `crates/origin-core/src/spaces.rs:209` — field definition; persisted in `spaces.db`.
- **Evidence:** `SpaceStore::set_document_space` and `get_document_space` are defined in `spaces.rs`, but neither is called from any server handler. The `POST /api/documents/{source_id}/space` route (`memory_routes.rs:2368`) calls `db.update_domain(...)` — a `MemoryDB` SQL update — not `space_store.set_document_space(...)`. The comment at `memory_routes.rs:259` even says document space assignment goes through `db.apply_enrichment(...)`.
- **Status:** Dead read path. The whole `SpaceStore.document_spaces` HashMap is loaded/saved but never queried through any live call chain.

### 8. `SpaceStore.activity_streams` field

- **Location:** `crates/origin-core/src/spaces.rs:208` — field definition; persisted in `spaces.db`.
- **Evidence:** No server handler reads `space_store.activity_streams`. Activity data for the HTTP API comes from `db.list_agent_activity(...)` in `MemoryDB`.
- **Status:** Dead read path.

### 9. `origin_core::spaces::load_spaces()` for the `SpaceStore.spaces` seeding purpose

- **Location:** `crates/origin-server/src/main.rs:536`
- **Evidence:** `load_spaces()` is called at daemon startup and the result is stored in `server_state.space_store`. But as noted in item 6, only `tags` and `document_tags` from that store are ever read by live handlers. The seed spaces written to `SpaceStore.spaces` by `Default` are never read back out to any caller.
- **Status:** The call is live (daemon startup), but its primary purpose (loading `SpaceStore.spaces`) produces a dead result. The useful side effect (populating `tags` / `document_tags` from `spaces.db`) is live.

---

## Mixed / unsure

### 10. `SpaceStore.tags` persistence round-trip via `save_spaces`

- **Location:** `crates/origin-server/src/memory_routes.rs:887`
- **Access:** `let _ = origin_core::spaces::save_spaces(&s.space_store);`
- **Concern:** `save_spaces` writes the entire `SpaceStore` (including the dead `spaces` and `document_spaces` fields) back to `spaces.db`. If `load_spaces()` on a future daemon restart reads that data back, the "dead" fields technically survive across restarts. They are dead in the sense that no running handler queries them — but they are silently round-tripped through persistence.
- **Classification:** Mixed. The `tags` subset is live; the `spaces` / `document_spaces` / `activity_streams` subsets are persisted but dead from a query perspective.

---

## Implication for PR-B

There are two independent read systems that PR-B must treat separately. The `MemoryDB.spaces` table (in `origin_memory.db`) is fully live: `GET /api/spaces` calls `list_spaces()`, `PUT /api/spaces/{name}` calls `get_space()` internally via `update_space`, and `auto_create_space_if_needed` keeps the table populated during ingestion. Dropping or renaming this subsystem requires migrating those HTTP endpoints first. The `SpaceStore` subsystem in `spaces.rs` (backed by `spaces.db`) is almost entirely dead — its `spaces`, `document_spaces`, and `activity_streams` fields have no live read consumers — but it cannot be dropped entirely yet because the `tags` field and `document_tags` field have two live HTTP endpoints reading from them (`GET /api/tags` and `GET /api/suggest-tags`). PR-B should scope to: (a) drop `SpaceStore.spaces`, `SpaceStore.document_spaces`, and `SpaceStore.activity_streams` and their persistence machinery, and (b) migrate the tags subsystem (`SpaceStore.tags` + `document_tags`) to a dedicated lightweight store or to the `MemoryDB` SQL layer before removing `SpaceStore` entirely.
