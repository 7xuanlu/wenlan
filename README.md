<p align="center">
  <img src="./docs/assets/social-preview.png" alt="Origin: Where AI work compounds. Decisions, lessons, project context, and wiki pages." width="100%">
</p>

[![CI](https://github.com/7xuanlu/origin/actions/workflows/ci.yml/badge.svg?branch=main&event=push)](https://github.com/7xuanlu/origin/actions/workflows/ci.yml?query=branch%3Amain)
[![Release](https://img.shields.io/github/v/release/7xuanlu/origin?sort=semver)](https://github.com/7xuanlu/origin/releases/latest)
[![npm: @7xuanlu/origin](https://img.shields.io/npm/v/%407xuanlu%2Forigin?label=%407xuanlu%2Forigin)](https://www.npmjs.com/package/@7xuanlu/origin)
[![npm: origin-mcp](https://img.shields.io/npm/v/origin-mcp?label=origin-mcp)](https://www.npmjs.com/package/origin-mcp)
[![MCP Server](https://img.shields.io/badge/MCP-server-blue)](https://modelcontextprotocol.io)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](#license)

<p align="center">
  <img alt="macOS" src="https://img.shields.io/badge/macOS-arm64%20%7C%20x64-A2AAAD?logo=apple&logoColor=white">
  <img alt="Linux" src="https://img.shields.io/badge/Linux-x64%20%7C%20arm64-FCC624?logo=linux&logoColor=black">
  <img alt="Windows" src="https://img.shields.io/badge/Windows-x64-0078D6?logo=windows&logoColor=white">
</p>

<p align="center">
  <a href="#claude-code-in-30-seconds"><img alt="Claude Code" src="https://img.shields.io/badge/Claude%20Code-plugin-5D4E75"></a>
  <a href="#mcp-only-setup"><img alt="OpenAI Codex" src="https://img.shields.io/badge/OpenAI%20Codex-MCP-111827"></a>
  <a href="#mcp-only-setup"><img alt="Cursor" src="https://img.shields.io/badge/Cursor-MCP-111111"></a>
  <a href="#mcp-only-setup"><img alt="VS Code" src="https://img.shields.io/badge/VS%20Code-MCP-007ACC"></a>
  <a href="#mcp-only-setup"><img alt="Claude Desktop" src="https://img.shields.io/badge/Claude%20Desktop-MCP-D97757"></a>
  <a href="#mcp-only-setup"><img alt="Gemini CLI" src="https://img.shields.io/badge/Gemini%20CLI-MCP-4285F4"></a>
  <a href="#what-you-get"><img alt="Obsidian" src="https://img.shields.io/badge/Obsidian-Markdown%20pages-7C3AED"></a>
</p>

**Your next AI session should pick up the context you built, not lose it in chat history.**

Origin gives your daily AI workflow one local home: memory your agents can recall, wiki pages you can read, and source-backed context that captures, distills, and versions decisions, lessons, and project context across chats, projects, and time.

Your agent reads searchable memory, graph context, and hybrid retrieval. You read Markdown artifacts under `~/.origin/`. Same store, two surfaces.

[![Watch the Origin demo](./docs/assets/demo-preview.gif)](https://youtu.be/k37gjWVPHwI)

---

## What makes Origin distinct

1. **Composition, not just storage.** Memories distill into pages. Sessions track workflow. An entity graph links people, projects, tools, and relations. Recall pulls vector hits, FTS hits, and graph neighbors together. Keyword search alone misses the connections. ~30 MCP tools across one daemon, not 100+ skills bolted on.
2. **Real git versioning.** Every memory write is a `git commit` in `~/.origin/.git/`. Inspect with `git log`. Revert with `git checkout`. Branch and blame as needed.
  ```text
   $ cd ~/.origin && git log --oneline -5
   a1b2c3d page: embedding-retrieval refreshed (4 sources)
   9f8e7d6 session: handoff embedding-work
   5a4b3c2 capture: fact mem_def456
   8d7c6b5 capture: decision mem_abc123
   3e2f1a0 init: origin v0.6.1
  ```
3. **Mandatory, refreshable provenance.** Wiki pages cite source memory IDs. The daemon rejects pages with empty `source_memory_ids` (HTTP 422). Pages aren't write-once: each carries `stale_reasons` and `revision_state`. `/distill` re-runs and background cycles refresh them as new memories arrive, without losing the citation chain.
  ```bash
   $ curl -X POST http://127.0.0.1:7878/pages \
       -d '{"title":"X","content":"Y","source_memory_ids":[]}'
   HTTP/1.1 422 Unprocessable Entity
   {"error":"source_memory_ids cannot be empty"}
  ```
4. **Auditable memory.** Low-confidence captures and contradictions surface for review when they happen, instead of silently entering context. Supersession chains and protected-memory conflicts stay visible. Audit when you want, trust the defaults when you don't.

---

## Quickstart

### Claude Code in 30 seconds

```text
/plugin marketplace add 7xuanlu/origin
/plugin install origin@7xuanlu
/init
```

If Claude Code asks for a restart after installing, restart once, then run `/init`. The plugin handles daemon setup, MCP wiring, local memory setup, and the first round-trip check.

Then try `/brief`, `/capture <decision>`, or `/handoff` inside Claude Code.

Plugin details and daily commands: [plugin/](plugin/.claude-plugin/README.md).

### MCP-only setup

Use this if you want Origin tools in Claude Code without the plugin, or in Codex, Cursor, Claude Desktop, VS Code, or Gemini CLI.

```bash
npx -y @7xuanlu/origin setup
~/.origin/bin/origin mcp add claude-code      # or: codex, cursor, claude-desktop, vscode, gemini
```

MCP-only gives agents tools for capture, recall, context, doctor, and page distillation. It does not install Claude Code slash skills like `/brief`, `/handoff`, `/distill`, or `/init`.

### Terminal runtime setup

Set up the local Origin runtime:

```bash
npx -y @7xuanlu/origin setup
```

Then start with `~/.origin/bin/origin status`, `~/.origin/bin/origin recall <query>`, or `~/.origin/bin/origin store <text>`. CLI details: [crates/origin-cli](crates/origin-cli/README.md).

---

## How Origin works

Origin follows the rhythm of an AI work session, with five verbs you use directly:

1. **Session starts.** `/brief [topic]` loads project status, identity, preferences, and topic-relevant memories so the agent walks in with context.
2. **During work.** `/capture <thing>` saves a decision, lesson, gotcha, or project fact in flow. `/recall <query>` looks anything up.
3. **Session ends.** `/handoff` writes what changed, what's still open, and where to continue, so the next run picks up cleanly.
4. **Between sessions.** The daemon deduplicates overlapping captures and links related ideas in the background. `/distill` synthesizes wiki pages from clusters of related memories when you want a deliberate pass.
5. **Next session.** `/brief` brings it all back in the Claude Code plugin. MCP-only clients call the `context` tool for the same underlying memory without replaying full chat history.

Full skill reference: [plugin/skills](plugin/skills/README.md).

No cloud sync or telemetry by default. Local models and Anthropic keys are opt-in for automatic distill cycles.

---

## What you get

- **Atomic memory layer**: every capture is stored first as a typed memory with source agent, confidence, stability, and supersession metadata.
- **Source-backed pages**: pages keep source memory IDs, stale reasons, and revision state so distillation can refresh them without losing provenance.
- **Hybrid retrieval on libSQL**: memories, pages, FTS5 text search, vector embeddings, and graph context live in one local store your MCP clients can query.
- **Knowledge graph context**: people, projects, tools, observations, and relations become retrievable context instead of isolated notes.
- **Distill cycles**: run `/distill` manually today, or add a local model/API key for background extraction, page refreshes, recaps, and richer graph links.
- **Background enrichment and decay**: post-ingest passes link entities, enrich titles, grow matching pages, and update effective confidence based on memory type, access, and age.
- **Review before trust**: low-confidence captures, pending revisions, protected-memory conflicts, contradictions, and supersession chains can surface instead of silently entering context.
- **Explicit spaces**: tag memories, pages, and recalls with `space=work | personal | client-X` so a day-job capture never bleeds into a side-project brief. Auto-detected from the current repo or workspace when no space is set; overridable always.
- **Local artifacts**: Markdown pages live in `~/.origin/pages/`, session logs and project status live under `~/.origin/sessions/`, and `~/.origin/` keeps local git history you can inspect, revert, or symlink into Obsidian.

---

## Evaluation

**Hybrid retrieval, transparent eval.** BGE-Base-EN-v1.5-Q + FTS5 + Reciprocal Rank Fusion. Recall@5 = 88% on LongMemEval (oracle, 500 Q), 67% on LoCoMo. ~168 tokens per recall query. Eval harness at [`crates/origin-core/src/eval/`](crates/origin-core/src/eval/). Run it yourself.

Update workflow in [docs/eval](docs/eval/README.md).


| Benchmark                   | Recall@5 | MRR   | NDCG@10 |
| --------------------------- | -------- | ----- | ------- |
| LongMemEval (oracle, 500 Q) | 88.0%    | 74.2% | 79.0%   |
| LoCoMo (locomo10)           | 67.3%    | 58.9% | 64.0%   |


---

## Repo Map

Origin is daemon-first. `origin-server` owns the local database, embeddings, distill cycles, knowledge graph, and HTTP API on `127.0.0.1:7878`. The plugin, MCP server, CLI, and local tools are thin clients over that daemon.


| Path                                                   | What lives there                                                                                                                                                                                           |
| ------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [crates/origin-core](crates/origin-core/README.md)     | Storage, search, embeddings, distill cycles, graph, pages, export, eval.                                                                                                                                   |
| [crates/origin-server](crates/origin-server/README.md) | Local daemon and HTTP API.                                                                                                                                                                                 |
| [crates/origin-mcp](crates/origin-mcp/README.md)       | MCP server, tools, npm package.                                                                                                                                                                            |
| [crates/origin-cli](crates/origin-cli/README.md)       | User CLI for setup, service management, search, recall, store, list, agents, model/key setup, and doctor.                                                                                                  |
| [plugin/](plugin/.claude-plugin/README.md)             | Claude Code plugin (`plugin.json`, skills, hooks, `.mcp.json`).                                                                                                                                           |
| [docs/eval](docs/eval/README.md)                       | Benchmark workflow and methodology.                                                                                                                                                                        |


Full contributor map: [CLAUDE.md](CLAUDE.md).

---

## Build from source

Most users should install through the Claude Code plugin. For local development:

```bash
git clone https://github.com/7xuanlu/origin.git
cd origin
cargo build --workspace
cargo run -p origin-server
```

Build details for the daemon, MCP server, CLI, and core crates live in the crate READMEs linked above.

---

## What Origin is NOT

- **Not a Life OS.** No habits, calendar, journal, or life-management modules. Origin scopes to AI work artifacts only. If you want a full personal OS, look at [PAI](https://github.com/danielmiessler/PAI).
- **Not a workflow suite.** ~30 MCP tools across one daemon. If you want 30+ skills, 8+ agents, and an auto-research loop bundled, look at [pro-workflow](https://github.com/rohitg00/pro-workflow). Origin trades breadth for focus.
- **Not a memory infrastructure SDK.** For people using AI daily, not as a backend for other apps building memory features.
- **Not for one-off chats.** Best when work spans sessions, projects, and weeks.

---

## Contributing

Bug fixes, eval cases, docs, and features are welcome. Start with [CONTRIBUTING.md](CONTRIBUTING.md). Architecture and development rules are in [CLAUDE.md](CLAUDE.md). Security reports: [SECURITY.md](SECURITY.md). Please also read the [Code of Conduct](CODE_OF_CONDUCT.md).

---

## License

Origin is licensed under **Apache-2.0**. This includes the local runtime, CLI, MCP server, shared types, and Claude Code plugin files in this repo.

The permissive license keeps the daemon boundary usable for MCP clients and downstream local tools.

---

## Acknowledgments

Predecessors:

- [Karpathy's LLM-wiki note](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f). Raw-to-wiki distillation pattern.
- Claude Code's `MEMORY.md`. The simplest version of the idea.

Peers:

- [agentmemory](https://github.com/rohitg00/agentmemory). Agent-side memory framework.
- [basic-memory](https://github.com/basicmachines-co/basic-memory). Local-first knowledge management for Claude.
- [pro-workflow](https://github.com/rohitg00/pro-workflow). Claude Code productivity suite.
- [mcp-memory-service](https://github.com/doobidoo/mcp-memory-service). Memory service for MCP.
- [Memoria](https://github.com/matrixorigin/Memoria). "Git for AI Agent Memory" via Copy-on-Write.
- [OpenMemory](https://github.com/CaviraOSS/OpenMemory), [claude-memory-compiler](https://github.com/coleam00/claude-memory-compiler), [PAI](https://github.com/danielmiessler/PAI), Palinode. Adjacent shapes.

Different shapes of the same problem. Try the one that fits.
