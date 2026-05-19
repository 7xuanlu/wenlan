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
  <a href="#claude-code--30-seconds"><img alt="Claude Code" src="https://img.shields.io/badge/Claude%20Code-plugin-5D4E75"></a>
  <a href="#other-mcp-clients-and-terminal-use"><img alt="OpenAI Codex" src="https://img.shields.io/badge/OpenAI%20Codex-MCP-111827"></a>
  <a href="#other-mcp-clients-and-terminal-use"><img alt="Cursor" src="https://img.shields.io/badge/Cursor-MCP-111111"></a>
  <a href="#other-mcp-clients-and-terminal-use"><img alt="VS Code" src="https://img.shields.io/badge/VS%20Code-MCP-007ACC"></a>
  <a href="#other-mcp-clients-and-terminal-use"><img alt="Claude Desktop" src="https://img.shields.io/badge/Claude%20Desktop-MCP-D97757"></a>
  <a href="#other-mcp-clients-and-terminal-use"><img alt="Gemini CLI" src="https://img.shields.io/badge/Gemini%20CLI-MCP-4285F4"></a>
  <a href="#what-you-get"><img alt="Obsidian" src="https://img.shields.io/badge/Obsidian-Markdown%20pages-7C3AED"></a>
</p>

**A lightweight daemon for daily AI work — memories, pages, sessions, versioned, agent-agnostic.**

Origin captures decisions, lessons, and project context as you work with AI. Distills clusters of memories into wiki pages with citation chains. Hands off sessions cleanly. Every write is a real `git commit` to `~/.origin/.git/` — inspect, revert, branch, blame.

Your agent reads searchable memory, graph context, and hybrid retrieval. You read Markdown artifacts under `~/.origin/`. Same store, two surfaces.

**Status:** Early preview. Expect fast iteration and some sharp edges.

[![Watch the Origin demo](./docs/assets/demo-preview.gif)](https://youtu.be/k37gjWVPHwI)

---

## What makes Origin distinct

1. **Real git versioning.** Every memory write is a `git commit` in `~/.origin/.git/`. Inspect with `git log`, revert with `git checkout`, branch and blame — not a Copy-on-Write metaphor.
2. **Mandatory provenance.** Wiki pages cite source memory IDs. The daemon rejects page writes with empty `source_memory_ids` (HTTP 422). Every distilled claim traces to a captured atom.
3. **Agent-agnostic from day one.** MCP-native. Works with Claude Code, Cursor, Codex, Claude Desktop, VS Code, Gemini CLI. No lock-in.
4. **Composition over storage.** Memories distill into pages. Sessions track workflow. ~30 MCP tools across one daemon — not 100+ skills bolted on.

---

## Quickstart

### Claude Code — 30 seconds

```text
/plugin marketplace add 7xuanlu/origin
/plugin install origin@7xuanlu
/init
```

If Claude Code asks for a restart after installing, restart once, then run `/init`. The plugin handles daemon setup, MCP wiring, local memory setup, and the first round-trip check.

Then try `/brief`, `/capture <decision>`, or `/handoff` inside Claude Code.

Plugin details and daily commands: [plugin/](plugin/.claude-plugin/README.md).

### Other MCP clients and terminal use

Set up the local Origin runtime:

```bash
npx -y @7xuanlu/origin setup
```

That installs the `origin` CLI, `origin-server` daemon, and `origin-mcp` connector into `~/.origin/bin/`, configures local memory, registers the daemon with launchd, and verifies status.
For terminal use, start with `origin status`, `origin recall <query>`, or `origin store <text>`. CLI details: [crates/origin-cli](crates/origin-cli/README.md).

Then add the MCP connector to Cursor, Codex, VS Code, Claude Desktop, Gemini CLI, or any client that accepts a JSON `mcpServers` entry:

```json
{
  "mcpServers": {
    "origin": {
      "command": "npx",
      "args": ["-y", "origin-mcp"]
    }
  }
}
```

The `origin-mcp` connector runs on demand and talks to the local Origin daemon from setup above.

---

## How Origin works

Origin follows the rhythm of an AI work session, with five verbs you use directly:

1. **Session starts** — `/brief [topic]` loads project status, identity, preferences, and topic-relevant memories so the agent walks in with context.
2. **During work** — `/capture <thing>` saves a decision, lesson, gotcha, or project fact in flow. `/recall <query>` looks anything up.
3. **Session ends** — `/handoff` writes what changed, what's still open, and where to continue, so the next run picks up cleanly.
4. **Between sessions** — the daemon deduplicates overlapping captures and links related ideas in the background. `/distill` synthesizes wiki pages from clusters of related memories when you want a deliberate pass.
5. **Next session** — `/brief` brings it all back through the plugin or `origin-mcp`, without replaying full chat history.

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
- **Project-scoped context**: skills infer the current repo or workspace so captures, recalls, and pages stay tied to the work at hand.
- **Local artifacts**: Markdown pages live in `~/.origin/pages/`, session logs and project status live under `~/.origin/sessions/`, and `~/.origin/` keeps local git history you can inspect, revert, or symlink into Obsidian.

---

## Evaluation

Retrieval quality on standard long-memory benchmarks. Numbers come from BGE-Base-EN-v1.5-Q embeddings combined with FTS5 and Reciprocal Rank Fusion. Harness at `crates/origin-core/src/eval/`; update workflow in [docs/eval](docs/eval/README.md).

Token efficiency on LoCoMo: 168 tokens per query instead of 4,505 for full replay, with 19% more relevant context than basic vector search.

| Benchmark                   | Recall@5 | MRR   | NDCG@10 |
| --------------------------- | -------- | ----- | ------- |
| LongMemEval (oracle, 500 Q) | 88.0%    | 74.2% | 79.0%   |
| LoCoMo (locomo10)           | 67.3%    | 58.9% | 64.0%   |

---

## Repo Map

Origin is daemon-first. `origin-server` owns the local database, embeddings, distill cycles, knowledge graph, and HTTP API on `127.0.0.1:7878`. The plugin, MCP server, CLI, and local tools are thin clients over that daemon.

| Path | What lives there |
| --- | --- |
| [crates/origin-core](crates/origin-core/README.md) | Storage, search, embeddings, distill cycles, graph, pages, export, eval. |
| [crates/origin-server](crates/origin-server/README.md) | Local daemon and HTTP API. |
| [crates/origin-mcp](crates/origin-mcp/README.md) | MCP server, tools, npm package. |
| [crates/origin-cli](crates/origin-cli/README.md) | User CLI for setup, service management, search, recall, store, list, agents, model/key setup, and doctor. |
| [plugin/](plugin/.claude-plugin/README.md) | Claude Code plugin (`plugin.json`, skills, hooks, `.mcp.json`). Marketplace entry at root [`.claude-plugin/marketplace.json`](.claude-plugin/marketplace.json) lists this plugin via `source: "./plugin"`. |
| [docs/eval](docs/eval/README.md) | Benchmark workflow and methodology. |

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
- **Not a Copy-on-Write metaphor.** `~/.origin/.git/` is a real git directory. `cd` into it. Run `git log`. Compare to alternatives that simulate versioning without `.git/`.
- **Not a chat UI.** Keep using Claude Code, Cursor, Codex, or your agent of choice. Origin runs alongside.
- **Not a notes app or Notion / Obsidian replacement.** Markdown exists so you can read the artifact anywhere.
- **Not a memory infrastructure SDK.** For people using AI daily, not as a backend for other apps.
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

Patterns and projects that shaped this space:

- **The Karpathy LLM-wiki pattern.** [Andrej Karpathy's LLM-wiki note](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f) made the raw-to-wiki distillation pattern legible to the community. Origin builds on it with mandatory provenance and a versioned daemon.
- **Claude Code's `MEMORY.md`.** The simplest version of the idea, and the one Origin cooperates with.

Honest peers Origin sits next to:

- **[agentmemory](https://github.com/rohitg00/agentmemory)** — large agent-side memory framework. Origin sits closer to the human-curated workflow surface; agentmemory is closer to the agent runtime.
- **[basic-memory](https://github.com/basicmachines-co/basic-memory)** — local-first knowledge management with Claude. Obsidian-style notes; Origin's pages are citation-backed distillations from atoms.
- **[pro-workflow](https://github.com/rohitg00/pro-workflow)** — Claude Code productivity suite with skills, wikis, and an auto-research loop. Broader scope; Origin trades breadth for focus.
- **[mcp-memory-service](https://github.com/doobidoo/mcp-memory-service)** — production-grade memory service for MCP. Origin layers pages and sessions on top of the memory primitive.
- **[Memoria](https://github.com/matrixorigin/Memoria)** — "Git for AI Agent Memory" via Copy-on-Write semantics. Origin uses a real `~/.origin/.git/` directory instead.
- **[OpenMemory](https://github.com/CaviraOSS/OpenMemory)**, **[claude-memory-compiler](https://github.com/coleam00/claude-memory-compiler)**, **[PAI](https://github.com/danielmiessler/PAI)**, Palinode — different shapes in the same direction.

Different shapes of the same problem. Try the one that fits.
