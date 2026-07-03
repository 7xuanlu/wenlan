<p align="center">
  <img src="./docs/assets/social-preview.png" alt="Wenlan: a living personal knowledge library for the AI-native age." width="100%">
</p>

<p align="center">
  <a href="https://github.com/7xuanlu/wenlan/actions/workflows/ci.yml?query=branch%3Amain"><img alt="CI" src="https://github.com/7xuanlu/wenlan/actions/workflows/ci.yml/badge.svg?branch=main&event=push"></a>
  <a href="https://github.com/7xuanlu/wenlan/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/7xuanlu/wenlan?sort=semver&label=release"></a>
  <a href="#license"><img alt="License: Apache-2.0" src="https://img.shields.io/badge/license-Apache--2.0-blue.svg"></a>
</p>

<p align="center">
  <a href="#claude-code-in-30-seconds"><img alt="Claude Code" src="https://img.shields.io/badge/Claude%20Code-plugin-5D4E75"></a>
  <a href="#codex-plugin"><img alt="Codex" src="https://img.shields.io/badge/Codex-plugin-111827"></a>
  <a href="#mcp-setup"><img alt="MCP clients" src="https://img.shields.io/badge/MCP-clients-2563EB"></a>
  <a href="#start-with-the-app"><img alt="Desktop app" src="https://img.shields.io/badge/Desktop-app-24C8DB"></a>
  <a href="#what-you-get"><img alt="Markdown pages" src="https://img.shields.io/badge/Markdown-pages-7C3AED"></a>
</p>

<p align="center">
  English | <a href="./README.zh-Hans.md">简体中文</a> | <a href="./README.zh-Hant.md">繁體中文</a>
</p>

**A living personal knowledge library for the AI-native age, built by your agents and grounded in its sources.**

Unlike a normal llm-wiki that generates pages from a fixed document set, Wenlan keeps a source-cited wiki current with live agent work and trusted sources. It is built for long-running work with AI agents, from software development and research to writing, consulting, product decisions, and client work.

Your agents capture what they learn during sessions, you add pages and sources you already trust, and Wenlan distills both into Markdown pages that refresh between sessions. Each new thread starts from that updated wiki, with a brief to bring context forward and a handoff to record where the work should continue.

Wenlan (文瀾) takes its name from 文瀾閣, an imperial library that held one of China's largest book collections, including a copy of the 四庫全書.

<p align="center">
  <img src="./docs/assets/desktop-wiki-preview.png" alt="Wenlan desktop app showing a source-cited wiki page with a source memory hover card." width="100%">
</p>

---

<a id="start-with-the-app"></a>

## Start with the app

The desktop app is the fastest way to read and curate your source-cited wiki. Agents keep capturing and recalling context in Claude Code, Codex, Cursor, VS Code, Claude Desktop, or any MCP client, and every path talks to the same local daemon and Markdown store.

Set up Wenlan once:

```bash
npx -y wenlan setup
```

Then download the current macOS Apple Silicon build: [wenlan-app-darwin-arm64.dmg](https://github.com/7xuanlu/wenlan/releases/latest/download/wenlan-app-darwin-arm64.dmg).

App source: [wenlan-app](https://github.com/7xuanlu/wenlan-app). Product details: [wenlan.app](https://wenlan.app).

---

## What makes Wenlan distinct

1. **Trustworthy sources.** Every page cites the memories behind it, and Wenlan refuses unsourced pages rather than letting hallucinated summaries in. It dedupes facts and supersedes old versions when facts change, so the wiki stays clean without turning daily capture into an approval queue.
2. **Current between sessions.** Wenlan clusters new captures into source-cited pages between sessions, and feeds retrieval with both the pages and the atomic notes behind them. The wiki reflects your latest work instead of a stale snapshot.
3. **One home, locked to none.** Every MCP client queries the same local daemon, so context built in one tool shows up in the next. Obsidian is one optional view you can symlink in, not where your work lives.
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
/plugin install wenlan@7xuanlu
/setup
```

If Claude Code asks for a restart after installing, restart once, then run `/setup`. The plugin handles local runtime setup, MCP wiring, local memory setup, and the first round-trip check.

Plugin details and daily commands: [plugin/](plugin/.claude-plugin/README.md).

### Codex plugin

```bash
npx -y wenlan setup
codex plugin marketplace add .
codex plugin add wenlan@wenlan-local
```

Start a new Codex thread after installing so the plugin and MCP server load.

Plugin details and development notes: [plugin-codex/](plugin-codex/README.md).

<a id="mcp-setup"></a>

### MCP setup

Both plugins call the same local MCP server under the hood. The core tools are `context`, `capture`, `recall`, `pages`, and `doctor`.

Use this if you want Wenlan tools in Claude Code without the plugin, or in Codex, Cursor, Claude Desktop, VS Code, or Gemini CLI:

```bash
npx -y wenlan setup
wenlan connect claude-code      # or: codex, cursor, claude-desktop, vscode, gemini
```

MCP-only clients use the same core tools for context, capture, recall, doctor checks, and page distillation.

### CLI

Set up Wenlan once:

```bash
npx -y wenlan setup
```

Then use the CLI directly:

```bash
wenlan status
wenlan recall <query>
wenlan capture <text>
```

CLI details: [crates/wenlan-cli](crates/wenlan-cli/README.md).

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
5. **Next session.** `/brief` brings it back in the Claude Code and Codex plugins; MCP-only clients call the `context` tool for the same memory. Recall pulls the relevant slice, not your whole history, so the context window goes to the work.

Full skill reference: [plugin/skills](plugin/skills/README.md).

Works fully local with no API key, cloud account, or signup. Capture, recall, hybrid search, and graph context need nothing external; add a local model or API key only for automatic page distillation. No telemetry.

---

## What you get

- **Typed captures**: every capture is stored with source agent, confidence, stability, and supersession metadata.
- **Source-backed pages**: pages keep source memory IDs, stale reasons, and revision state so distillation can refresh them without losing provenance.
- **Hybrid retrieval on libSQL**: memories, pages, FTS5 text, vector embeddings, and graph context in one local store your MCP clients can query, fused with reciprocal-rank fusion. An optional local cross-encoder reranker sharpens the top results.
- **Connected recall**: people, projects, tools, and decisions come back linked, so a memory arrives with the context around it instead of alone.
- **Distill cycles**: run `/distill` manually today, or add a local model/API key for background extraction, page refreshes, recaps, and richer graph links.
- **Refreshes between sessions**: background passes link entities, grow matching pages, and update each memory's effective confidence from type, access, and age, so recent and load-bearing memories surface while stale ones fade.
- **Review before trust**: low-confidence captures, pending revisions, contradictions, and supersessions can surface instead of silently entering context.
- **Explicit spaces**: tag memories, pages, and recalls with `space=work | personal | client-X` so a day-job capture never bleeds into a side-project brief. Auto-detected from the current repo or workspace when no space is set; overridable always.
- **You own the data**: everything is plain Markdown under `~/.wenlan/`, versioned in git. Grep it, symlink it into Obsidian, or walk away with the files anytime. No lock-in.

---

## Evaluation

Retrieval-only snapshot, not end-to-end answer quality. Method and update workflow live in [docs/eval](docs/eval/README.md).


<!-- EVAL_SNAPSHOT_START -->
| Benchmark | Recall@5 | MRR | NDCG@10 |
|---|---:|---:|---:|
| LME_Oracle (500 Q) | 93.6% | 0.857 | 0.883 |
| LME_S (deep, 90 Q) | 87.7% | 0.815 | 0.822 |
<!-- EVAL_SNAPSHOT_END -->

---

## Build from source

Wenlan builds natively on macOS (Apple Silicon + Intel), Linux (x86_64 + ARM64; glibc), and Windows (x86_64). The npm wrapper (`wenlan`, `wenlan-mcp`) and `install.sh` auto-detect your platform and pull the matching prebuilt release. Most users should install through the Claude Code plugin or `npx`. For local development:

```bash
git clone https://github.com/7xuanlu/wenlan.git
cd wenlan
cargo build --workspace
cargo run -p wenlan-server
```

Build details for the daemon, MCP server, CLI, and core crates live in the crate READMEs linked above. Cross-platform specifics (service registration, paths, Windows install limitation) live in [AGENTS.md](AGENTS.md#cross-platform).

---

## Learn more

Longer-form writing on AI work memory and how Wenlan compares lives at [wenlan.app/learn](https://wenlan.app/learn):

**Concepts**
- [What is AI work memory?](https://wenlan.app/learn/ai-work-memory): the shape of the problem Wenlan solves
- [MCP memory server](https://wenlan.app/learn/mcp-memory-server): how Wenlan exposes memory through the Model Context Protocol
- [Local-first AI memory](https://wenlan.app/learn/local-first-ai-memory): data, privacy, and control
- [Markdown + local index](https://wenlan.app/learn/markdown-local-index-ai-memory): the storage model
- [AI agent handoff loop](https://wenlan.app/learn/ai-agent-handoff-loop): session-end discipline that prevents context loss

**Comparisons**
- [Wenlan vs Basic Memory](https://wenlan.app/learn/origin-vs-basic-memory): Markdown knowledge base vs AI work-session memory
- [Wenlan vs claude-mem](https://wenlan.app/learn/origin-vs-claude-mem): observer-style Claude Code memory vs MCP-first cross-tool memory
- [Wenlan vs Superlocal Memory](https://wenlan.app/learn/origin-vs-superlocal-memory): tradeoffs against another local memory shape

**Docs**
- [Get started](https://wenlan.app/docs/get-started): install + verify the first local memory loop
- [Daily workflow](https://wenlan.app/docs/daily-workflow): capture, handoff, distill
- [MCP clients](https://wenlan.app/docs/mcp-clients): connect Claude Code, Cursor, Codex, Claude Desktop, Gemini CLI

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

Wenlan is licensed under **Apache-2.0**. This includes the local runtime, CLI, MCP server, shared types, and Claude Code/Codex plugin files in this repo.

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
