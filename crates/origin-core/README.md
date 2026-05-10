# origin-core

Business logic for Origin. This crate has no Axum or Tauri dependency. The daemon, MCP server, CLI, and future clients go through this layer rather than owning storage or memory behavior themselves.

## What Lives Here

- libSQL storage, FTS5, vector search, Reciprocal Rank Fusion
- embedding generation through FastEmbed
- optional on-device LLM provider through `llama-cpp-2`
- memory classification, extraction, quality gates, deduplication, and contradiction checks
- knowledge graph entities, relations, and observations
- page distillation and Markdown/Obsidian export
- refinery phases that maintain memory quality over time
- LoCoMo and LongMemEval evaluation harnesses

## Core Flow

1. A client stores text through the daemon.
2. `origin-core` validates and stores the raw memory.
3. Post-ingest enrichment links entities, deduplicates, and queues refinement.
4. The refinery compiles related memories into pages and refreshes existing pages.
5. Recall combines vector search, full-text search, graph context, and relevant pages.

Basic Memory mode does storage and retrieval without a local LLM or API key. Extraction, richer refinement, and page synthesis require an on-device model or configured provider.

## Important Modules

| Module | Purpose |
| --- | --- |
| `db.rs` | `MemoryDB`, migrations, storage, search, graph, pages. |
| `quality_gate.rs` | Pre-store acceptance and warnings. |
| `post_ingest.rs` | Dedup, entity linking, title enrichment, recap, page growth. |
| `refinery.rs` and `refinery/` | Background maintenance phases. |
| `pages.rs` | Page type and page relevance helpers. |
| `export/` | Markdown, Obsidian, JSON, zip, and PDF export surfaces. |
| `engine.rs` | On-device LLM wrapper. |
| `llm_provider.rs` | Provider abstraction for API and local model paths. |
| `eval/` | Benchmark runners and analysis helpers. |

See [CLAUDE.md](../../CLAUDE.md) for lower-level invariants and locking rules.

## Eval

Evaluation docs live in [docs/eval](../../docs/eval/README.md). Slow GPU/API benchmarks are manual and should not run in normal CI.

## License

Apache-2.0.
