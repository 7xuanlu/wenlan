# crates/origin-core

Applies to agents working under `crates/origin-core/`. Read alongside root `AGENTS.md`, which takes precedence on any topic not covered here.

All business logic lives here. No tauri, no axum. Framework-agnostic.

## Key Modules (`crates/origin-core/src/`)

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
| `eval/` | Benchmark harness: LoCoMo, LongMemEval. Each benchmark has base (embedding-only), reranked (LLM rescores after search), and expanded (LLM query expansion before search) variants. Baselines under `EVAL_BASELINES_DIR` (gitignored). See `crates/origin-core/src/eval/AGENTS.md`. |
| `state.rs` | `CoreState` — shared state struct used by origin-server |
