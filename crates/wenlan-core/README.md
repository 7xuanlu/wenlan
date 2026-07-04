# wenlan-core

Business logic for Wenlan. This crate has no Axum or Tauri dependency. The daemon, MCP server, CLI, and future clients go through this layer rather than owning storage or memory behavior themselves.

## What Lives Here

- libSQL storage, FTS5, vector search, Reciprocal Rank Fusion
- embedding generation through FastEmbed
- optional on-device model provider through `llama-cpp-2`
- memory classification, extraction, quality gates, deduplication, and contradiction checks
- knowledge graph entities, relations, and observations
- page distillation and Markdown/Obsidian export
- distill cycles that maintain memory quality over time
- LoCoMo and LongMemEval evaluation harnesses

## Core Flow

1. A client stores text through the daemon.
2. `wenlan-core` validates and stores the raw memory.
3. Post-ingest enrichment links entities, deduplicates, and queues review proposals.
4. Distill cycles compile related memories into pages and refresh existing pages.
5. Recall combines vector search, full-text search, graph context, and relevant pages.

Local memory mode does storage and retrieval without a local model or API key. Extraction, richer distill cycles, and page synthesis require an on-device model or configured provider.

## Important Modules

| Module | Purpose |
| --- | --- |
| `db.rs` | `MemoryDB`, migrations, storage, search, graph, pages. |
| `quality_gate.rs` | Pre-store acceptance and warnings. |
| `post_ingest.rs` | Dedup, entity linking, title enrichment, recap, page growth. |
| `refinery.rs` and `refinery/` | Distill-cycle implementation and background maintenance phases. |
| `pages.rs` | Page type and page relevance helpers. |
| `export/` | Markdown, Obsidian, JSON, zip, and PDF export surfaces. |
| `engine.rs` | On-device model wrapper. |
| `llm_provider.rs` | Provider abstraction for API and local model paths. |
| `eval/` | Benchmark runners and analysis helpers. |

See [CLAUDE.md](../../CLAUDE.md) for lower-level invariants and locking rules.

## Eval

Evaluation docs live in [docs/eval](../../docs/eval/README.md). Slow GPU/API benchmarks are manual and should not run in normal CI.

## Links

- [wenlan.app](https://wenlan.app) — project home
- [wenlan.app/learn/local-first-ai-memory](https://wenlan.app/learn/local-first-ai-memory) — storage model explained
- [wenlan.app/learn/markdown-local-index-ai-memory](https://wenlan.app/learn/markdown-local-index-ai-memory) — why Markdown + libSQL together
- [github.com/7xuanlu/wenlan](https://github.com/7xuanlu/wenlan) — source

## License

Apache-2.0.
