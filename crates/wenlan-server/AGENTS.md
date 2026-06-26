# crates/wenlan-server

Applies to agents working under `crates/wenlan-server/`. Read alongside root `AGENTS.md`, which takes precedence on any topic not covered here.

HTTP daemon — owns the Axum router + all routes. All handlers operate on `Arc<RwLock<ServerState>>` where `ServerState.db: Option<Arc<MemoryDB>>`.

## Key Modules (`crates/wenlan-server/src/`)

| Module | Purpose |
|---|---|
| `main.rs` | Binary entry — daemon startup plus internal maintenance commands, tracing init, port binding with existing-daemon fallback, `MemoryDB::new`, LLM provider init, background tasks, `axum::serve` |
| `state.rs` | `ServerState` struct with `db: Option<Arc<MemoryDB>>`, `llm`, `prompts`, `tuning`, `quality_gate`, `space_store`, `access_tracker`, `llm_processing_ids`, `watch_paths`. `SharedState = Arc<RwLock<ServerState>>` |
| `router.rs` | `build_router(state) -> axum::Router` — all route registrations |
| `routes.rs` | General endpoints: health, search, context, status, profile/agents |
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
| `resources/com.wenlan.server.plist` | launchd plist template (embedded via `include_str!`) |
