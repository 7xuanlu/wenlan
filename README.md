<p align="center">
  <img src="./docs/assets/social-preview.png" alt="Wenlan: a living personal knowledge library for the AI-native age." width="100%">
</p>

[![CI](https://github.com/7xuanlu/wenlan/actions/workflows/ci.yml/badge.svg?branch=main&event=push)](https://github.com/7xuanlu/wenlan/actions/workflows/ci.yml?query=branch%3Amain)
[![Release](https://img.shields.io/github/v/release/7xuanlu/wenlan?sort=semver)](https://github.com/7xuanlu/wenlan/releases/latest)
[![npm: @7xuanlu/origin](https://img.shields.io/npm/v/%407xuanlu%2Forigin?label=%407xuanlu%2Forigin)](https://www.npmjs.com/package/@7xuanlu/origin)
[![npm: wenlan-mcp](https://img.shields.io/npm/v/wenlan-mcp?label=wenlan-mcp)](https://www.npmjs.com/package/wenlan-mcp)
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

**A living personal knowledge library for the AI-native age, built by your agents and grounded in its sources.**

Wenlan (文瀾) takes its name from an imperial library that held one of China's largest book collections. Your AI agents capture what they learn as they work, and Wenlan keeps it current on its own, distilling scattered notes into source-cited wiki pages.

A brief opens each session, a handoff closes it, so the thread carries forward instead of restarting.

Unlike a static llm-wiki, it keeps evolving between sessions. Unlike a black-box memory, every page shows its sources, so you can read, trust, or correct it.

[![Watch the Wenlan demo](./docs/assets/demo-preview.gif)](https://youtu.be/k37gjWVPHwI)

---

## What makes Wenlan distinct

1. **Evolves on its own.** Most memory tools just hand back what you put in. Wenlan keeps working between sessions: it dedupes, links, and clusters your captures into source-cited wiki pages that feed retrieval alongside the atomic notes they came from. Unlike a static llm-wiki, the library stays current without you maintaining it.
2. **One home, locked to none.** Every MCP client queries the same local daemon, so context built in one tool shows up in the next. Obsidian is one optional view you can symlink in, not where your work lives.
3. **Sourced, so you can trust it.** Every page cites the memories it came from, and the daemon refuses unsourced pages rather than letting hallucinated summaries in. Low-confidence captures and contradictions surface for you to confirm or correct, but the everyday flow never stops for approval. Fix something once and Wenlan supersedes the old fact instead of serving both.
4. **Real git versioning.** Memory, page, and session writes commit into `~/.wenlan/.git/`, so you can inspect, diff, revert, or branch the Markdown artifacts.
   ```text
   a1b2c3d page: embedding-retrieval refreshed (4 sources)
   9f8e7d6 session: handoff embedding-work
   5a4b3c2 capture: decision mem_abc123
   ```

---

## Quickstart

### Claude Code in 30 seconds

```text
/plugin marketplace add 7xuanlu/claude-plugins
/plugin install origin@7xuanlu
/init
```

If Claude Code asks for a restart after installing, restart once, then run `/init`. The plugin handles daemon setup, MCP wiring, local memory setup, and the first round-trip check.

Then try `/brief`, `/capture <decision>`, or `/handoff` inside Claude Code.

Plugin details and daily commands: [plugin/](plugin/.claude-plugin/README.md).

### MCP-only setup

Use this if you want Wenlan tools in Claude Code without the plugin, or in Codex, Cursor, Claude Desktop, VS Code, or Gemini CLI.

```bash
npx -y @7xuanlu/wenlan setup
~/.wenlan/bin/origin mcp add claude-code      # or: codex, cursor, claude-desktop, vscode, gemini
```

MCP-only gives agents tools for capture, recall, context, doctor, and page distillation. It does not install Claude Code slash skills like `/brief`, `/handoff`, `/distill`, or `/init`.

### Terminal runtime setup

Set up the local Wenlan runtime:

```bash
npx -y @7xuanlu/wenlan setup
```

Then start with `~/.wenlan/bin/wenlan status`, `~/.wenlan/bin/wenlan recall <query>`, or `~/.wenlan/bin/wenlan store <text>`. CLI details: [crates/wenlan-cli](crates/wenlan-cli/README.md).

Service management:

```bash
wenlan install            # register + start the daemon (stops a running one first)
wenlan restart            # stop + start the daemon -- run this after upgrading
wenlan status
wenlan uninstall
```

After upgrading Wenlan (`npx -y @7xuanlu/wenlan setup` or `install.sh`), the new binary is on disk but the already-running daemon keeps serving the old code until you restart it. `wenlan install` now restarts automatically; if you upgraded another way, run `wenlan restart`.

---

## How Wenlan works

The same loop runs every session: capture while you work, let the daemon refine between sessions, and return with the knowledge already in context.

```text
      ┌──────── loops back · /handoff closes each pass ─────────┐
      ▼                                                         │
┌─────┴─────┐    ┌─────────────┐    ┌────────────────┐    ┌─────┴─────┐
│ CAPTURE   │    │ DAEMON      │    │ ONE STORE      │    │ RECALL +  │
│  in flow  │ ─▶ │  refines    │ ─▶ │  (local)       │ ─▶ │  BRIEF    │
│  /capture │    │  between    │    │  · memories    │    │  next     │
│           │    │  sessions   │    │  · wiki pages  │    │  session  │
│           │    │  dedup·link │    │  · graph       │    │  /recall  │
│           │    │  /distill   │    │                │    │  /brief   │
└───────────┘    └─────────────┘    └────────────────┘    └───────────┘
   one local daemon · one store · every MCP client reads it
   Claude Code · Cursor · Codex · Claude Desktop · VS Code · Gemini
```

Each pass leaves the store sharper. Captures that would sit as loose snippets elsewhere get deduped, linked to the people and projects they touch, and distilled into source-citing pages, so the next session brings back knowledge, not raw history. That is the compounding the loop is named for.

These five verbs drive it:

1. **Session starts.** `/brief [topic]` loads project status, identity, preferences, and topic-relevant memories so the agent walks in with context.
2. **During work.** `/capture <thing>` saves a decision, lesson, gotcha, or project fact in flow. `/recall <query>` looks anything up.
3. **Session ends.** `/handoff` writes what changed, what's still open, and where to continue, so the next run picks up cleanly.
4. **Between sessions.** The daemon deduplicates overlapping captures and links related ideas in the background. `/distill` synthesizes wiki pages from clusters of related memories when you want a deliberate pass.
5. **Next session.** `/brief` brings it back in the Claude Code plugin; MCP-only clients call the `context` tool for the same memory. Recall pulls the relevant slice, not your whole history, so the context window goes to the work.

Full skill reference: [plugin/skills](plugin/skills/README.md).

Works fully local with no API key, cloud account, or signup. Capture, recall, hybrid search, and graph context need nothing external; add a local model or API key only for automatic page distillation. No telemetry.

---

## What you get

- **Atomic memory layer**: every capture is stored first as a typed memory with source agent, confidence, stability, and supersession metadata.
- **Source-backed pages**: pages keep source memory IDs, stale reasons, and revision state so distillation can refresh them without losing provenance.
- **Hybrid retrieval on libSQL**: memories, pages, FTS5 text, vector embeddings, and graph context in one local store your MCP clients can query, fused with reciprocal-rank fusion. An optional local cross-encoder reranker sharpens the top results.
- **Connected recall**: people, projects, tools, and decisions come back linked, so a memory arrives with the context around it instead of alone.
- **Distill cycles**: run `/distill` manually today, or add a local model/API key for background extraction, page refreshes, recaps, and richer graph links.
- **Stays fresh on its own**: background passes link entities, grow matching pages, and update each memory's effective confidence from type, access, and age, so recent and load-bearing memories surface while stale ones fade.
- **Review before trust**: low-confidence captures, pending revisions, contradictions, and supersessions can surface instead of silently entering context.
- **Explicit spaces**: tag memories, pages, and recalls with `space=work | personal | client-X` so a day-job capture never bleeds into a side-project brief. Auto-detected from the current repo or workspace when no space is set; overridable always.
- **You own the data**: everything is plain Markdown under `~/.wenlan/`, versioned in git. Grep it, symlink it into Obsidian, or walk away with the files anytime. No lock-in.

### Spaces

Memories belong to a **space** like `origin`, `career`, or
`ideas`. Set the active space per shell:

    ORIGIN_SPACE=career claude

Or declaratively via `~/.wenlan/spaces.toml` (see
`plugin/examples/spaces.toml`). To manage spaces from the CLI:

    wenlan space list
    wenlan space add ideas --default
    wenlan space show ideas
    wenlan space move scratch career

`wenlan doctor` prints the current resolver state so you can see exactly
which layer chose the active space.

---

## Evaluation

**Hybrid retrieval, transparent eval.** BGE-Base-EN-v1.5-Q + FTS5 + Reciprocal Rank Fusion; local BGE-Reranker-Base cross-encoder rerank is the default path when enabled, with BGE-Reranker-V2-M3 available as a higher-quality option. The table below is retrieval-only, not end-to-end answer quality. ~168 tokens per recall query. Eval harness at [`crates/wenlan-core/src/eval/`](crates/wenlan-core/src/eval/). Run it yourself.

Update workflow in [docs/eval](docs/eval/README.md).


<!-- EVAL_SNAPSHOT_START -->
| Benchmark | Recall@5 | MRR | NDCG@10 |
|---|---:|---:|---:|
| LongMemEval (oracle, 500 Q) | 93.6% | 0.857 | 0.883 |
| LoCoMo (locomo10) | 70.0% | 0.647 | 0.684 |
<!-- EVAL_SNAPSHOT_END -->


---

## Repo Map

Wenlan is daemon-first. `wenlan-server` owns the local database, embeddings, distill cycles, knowledge graph, and HTTP API on `127.0.0.1:7878`. The plugin, MCP server, CLI, and local tools are thin clients over that daemon.


| Path                                                   | What lives there                                                                                                                                                                                           |
| ------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [crates/wenlan-core](crates/wenlan-core/README.md)     | Storage, search, embeddings, distill cycles, graph, pages, export, eval.                                                                                                                                   |
| [crates/wenlan-server](crates/wenlan-server/README.md) | Local daemon and HTTP API.                                                                                                                                                                                 |
| [crates/wenlan-mcp](crates/wenlan-mcp/README.md)       | MCP server, tools, npm package.                                                                                                                                                                            |
| [crates/wenlan-cli](crates/wenlan-cli/README.md)       | User CLI for setup, service management, search, recall, store, list, agents, model/key setup, and doctor.                                                                                                  |
| [plugin/](plugin/.claude-plugin/README.md)             | Claude Code plugin (`plugin.json`, skills, hooks, `.mcp.json`).                                                                                                                                           |
| [docs/eval](docs/eval/README.md)                       | Benchmark workflow and methodology.                                                                                                                                                                        |


Full contributor map: [CLAUDE.md](CLAUDE.md).

---

## Build from source

Wenlan builds natively on macOS (Apple Silicon + Intel), Linux (x86_64 + ARM64; glibc), and Windows (x86_64). The npm wrapper (`@7xuanlu/origin`, `wenlan-mcp`) and `install.sh` auto-detect your platform and pull the matching prebuilt release. Most users should install through the Claude Code plugin or `npx`. For local development:

```bash
git clone https://github.com/7xuanlu/wenlan.git
cd origin
cargo build --workspace
cargo run -p wenlan-server
```

Build details for the daemon, MCP server, CLI, and core crates live in the crate READMEs linked above. Cross-platform specifics (service registration, paths, Windows install limitation) live in [AGENTS.md](AGENTS.md#cross-platform).

---

## Learn more

Longer-form writing on AI work memory and how Wenlan compares lives at [useorigin.app/learn](https://useorigin.app/learn):

**Concepts**
- [What is AI work memory?](https://useorigin.app/learn/ai-work-memory): the shape of the problem Wenlan solves
- [MCP memory server](https://useorigin.app/learn/mcp-memory-server): how Wenlan exposes memory through the Model Context Protocol
- [Local-first AI memory](https://useorigin.app/learn/local-first-ai-memory): data, privacy, and control
- [Markdown + local index](https://useorigin.app/learn/markdown-local-index-ai-memory): the storage model
- [AI agent handoff loop](https://useorigin.app/learn/ai-agent-handoff-loop): session-end discipline that prevents context loss

**Comparisons**
- [Wenlan vs Basic Memory](https://useorigin.app/learn/origin-vs-basic-memory): Markdown knowledge base vs AI work-session memory
- [Wenlan vs claude-mem](https://useorigin.app/learn/origin-vs-claude-mem): observer-style Claude Code memory vs MCP-first cross-tool memory
- [Wenlan vs Superlocal Memory](https://useorigin.app/learn/origin-vs-superlocal-memory): includes the honest LoCoMo benchmark concession

**Docs**
- [Get started](https://useorigin.app/docs/get-started): install + verify the first local memory loop
- [Daily workflow](https://useorigin.app/docs/daily-workflow): capture, handoff, distill
- [MCP clients](https://useorigin.app/docs/mcp-clients): connect Claude Code, Cursor, Codex, Claude Desktop, Gemini CLI

---

## What Wenlan is NOT

- **Not a Life OS.** No habits, calendar, journal, or life-management modules. Wenlan scopes to AI work artifacts only. If you want a full personal OS, look at [PAI](https://github.com/danielmiessler/PAI).
- **Not a workflow suite.** ~30 MCP tools across one daemon. If you want 30+ skills, 8+ agents, and an auto-research loop bundled, look at [pro-workflow](https://github.com/rohitg00/pro-workflow). Wenlan trades breadth for focus.
- **Not a memory infrastructure SDK.** For people using AI daily, not as a backend for other apps building memory features.
- **Not for one-off chats.** Best when work spans sessions, projects, and weeks.

---

## Contributing

Bug fixes, eval cases, docs, and features are welcome. Start with [CONTRIBUTING.md](CONTRIBUTING.md). Architecture and development rules are in [CLAUDE.md](CLAUDE.md). Security reports: [SECURITY.md](SECURITY.md). Please also read the [Code of Conduct](CODE_OF_CONDUCT.md).

---

## License

Wenlan is licensed under **Apache-2.0**. This includes the local runtime, CLI, MCP server, shared types, and Claude Code plugin files in this repo.

The permissive license keeps the daemon boundary usable for MCP clients and downstream local tools.

---

## Acknowledgments

Predecessors:

- [Karpathy's LLM-wiki note](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f). Raw-to-wiki distillation pattern.
- Claude Code's `MEMORY.md`. The simplest version of the idea.

Peers:

- [agentmemory](https://github.com/rohitg00/agentmemory). Agent-side memory framework.
- [basic-memory](https://github.com/basicmachines-co/basic-memory). Local-first knowledge management for Claude.
- [obsidian-mind](https://github.com/breferrari/obsidian-mind). Obsidian-native memory and review loop for coding agents.
- [pro-workflow](https://github.com/rohitg00/pro-workflow). Claude Code productivity suite.
- [mcp-memory-service](https://github.com/doobidoo/mcp-memory-service). Memory service for MCP.
- [Memoria](https://github.com/matrixorigin/Memoria). "Git for AI Agent Memory" via Copy-on-Write.
- [OpenMemory](https://github.com/CaviraOSS/OpenMemory), [claude-memory-compiler](https://github.com/coleam00/claude-memory-compiler), [PAI](https://github.com/danielmiessler/PAI), Palinode. Adjacent shapes.

Different shapes of the same problem. Try the one that fits.
