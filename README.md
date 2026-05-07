<p align="center">
  <img src="./docs/assets/social-preview.png" alt="Origin: Where Personal AI Memory Compounds." width="100%">
</p>

[![CI](https://github.com/7xuanlu/origin/actions/workflows/ci.yml/badge.svg)](https://github.com/7xuanlu/origin/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/7xuanlu/origin)](https://github.com/7xuanlu/origin/releases/latest)
[![Backend License](https://img.shields.io/badge/backend-Apache--2.0-blue)](#license)
![Platform](https://img.shields.io/badge/platform-macOS%20Apple%20Silicon-black)

Local-first memory for decisions, lessons, gotchas, and context across sessions, projects, and time. On-demand wiki pages distilled from your sources.

Plain Markdown for you, hybrid index for your AI. Across Claude Code, Codex, Cursor, and other MCP clients.

The daemon does the memory chores in the background: storing, searching, deduplicating, linking related ideas, distilling pages, and keeping provenance attached. This repo ships that local runtime: the `origin-server` daemon, the `origin` terminal CLI, the `origin-core` memory engine, and shared `origin-types`.

**Status:** Early preview for macOS Apple Silicon. Expect fast iteration and some sharp edges.

---

## Quickstart

**Platform:** macOS Apple Silicon (M1+). Linux, Intel Mac, and Windows are not supported yet.

### 1. Recommended: Claude Code plugin

For the daily experience in Claude Code, install [origin-plugin](https://github.com/7xuanlu/origin-plugin):

```text
/plugin install 7xuanlu/origin-plugin
```

The plugin is how you use Origin day to day. It bundles `origin-mcp` and adds slash commands for setup, briefing, capture, recall, distillation, review, forget, and handoff.

After the local daemon is running in step 2, use short commands instead of asking Claude to call MCP tools manually:

```text
/origin:init
/origin:brief
/origin:capture remember this decision...
/origin:recall database preferences
/origin:handoff
```

### 2. Start Origin locally

Origin still needs the local daemon running:

```bash
curl -fsSL https://raw.githubusercontent.com/7xuanlu/origin/main/install.sh | bash
export PATH="$HOME/.origin/bin:$PATH"
origin setup
origin install
origin status
```

Origin works without a local LLM or API key for storage, search, recall, and MCP memory. To unlock richer extraction, background refinement, and page synthesis, choose a local model or Anthropic key:

```bash
origin model install
origin key set anthropic
origin doctor
```

### 3. Manual MCP config

Use this path for Cursor, Codex, Claude Desktop, Windsurf, Gemini CLI, or any client that accepts a JSON `mcpServers` entry:

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

`origin-mcp` connects to the local Origin daemon on `127.0.0.1:7878`. On first run, `npx origin-mcp` downloads the MCP connector from the [origin-mcp repo](https://github.com/7xuanlu/origin-mcp).

---

## Why Origin?

AI work has a continuity problem. Most memory tools ask you to trust a cloud API, maintain a notes/wiki workflow, or move into another app. Origin takes a different shape: a local daemon and MCP access from the AI tools you already use.

**Your AI starts from scratch too often.** Origin carries decisions, preferences, gotchas, and project context across chats, projects, and time.

**Memory gets worse when nobody maintains it.** Origin runs a background refinery that deduplicates captures, links related ideas, distills pages, and keeps provenance attached.

**You need to see and correct what it learned.** Memories stay local and traceable instead of disappearing into a black box.

Developers already have `origin` for code. Origin gives AI work a source of truth too: local, traceable, and readable by agents through MCP.

For people whose work spans projects, clients, and jobs. Your context should not disappear when a chat ends or when you switch gears.

Origin keeps the useful parts together:

- **Capture:** decisions, lessons, observations, gotchas, and project context.
- **Refine:** deduplicate, link, and compile memories in the background.
- **Recall:** relevant context through MCP when your AI needs it.
- **Inspect:** every memory stays editable and traceable to where it came from.

**96% fewer tokens per query.** Same cost as basic vector search, but 19% more relevant context. 168 tokens instead of 4,505 for full replay. Measured on LoCoMo (2,531 memories, 1,540 queries). Eval harness at `crates/origin-core/src/eval/`.

---

## How Origin works

Use your AI tools normally. Origin runs in the background, keeps the useful parts, and makes them available when context is needed.

1. **Install Origin.** The local daemon owns the memory database and runs on `127.0.0.1:7878`.
2. **Connect your AI tools.** Claude Code, Cursor, Codex, Claude Desktop, Windsurf, Gemini CLI, and other MCP clients connect through `origin-mcp`.
3. **Capture useful context.** Agents save decisions, preferences, project facts, gotchas, and lessons while you work.
4. **Refine in the background.** Origin deduplicates captures, links related ideas, distills pages, and preserves where each memory came from.
5. **Recall when needed.** Retrieval combines vector search, full-text search, and knowledge graph signals without replaying full chat history.
6. **Inspect and export.** Search, verify, delete, and export what Origin learned through the local runtime surfaces.

---

## Evaluation

Retrieval quality on standard long-memory benchmarks. Numbers come from BGE-Base-EN-v1.5-Q embeddings combined with FTS5 and Reciprocal Rank Fusion. Harness at `crates/origin-core/src/eval/`; update workflow in [docs/eval](docs/eval/README.md).


| Benchmark                   | Recall@5 | MRR   | NDCG@10 |
| --------------------------- | -------- | ----- | ------- |
| LongMemEval (oracle, 500 Q) | 88.0%    | 74.2% | 79.0%   |
| LoCoMo (locomo10)           | 67.3%    | 58.9% | 64.0%   |


---

## Local by default

- Memories are stored locally at `~/Library/Application Support/origin/memorydb/origin_memory.db` by default.
- The daemon listens on `127.0.0.1:7878`; MCP clients and local tools call that local API.
- There is no cloud sync or telemetry by default. Anthropic keys are opt-in settings.
- On-device Qwen models download only when requested with `origin model install`, and use the `hf-hub` cache.
- Security reports: [SECURITY.md](SECURITY.md).

---

## Architecture

Origin is daemon-first. `origin-server` owns the local database, embeddings, refinery, knowledge graph, and HTTP API on `127.0.0.1:7878`. MCP clients and local tools are thin clients over that daemon.

Stack: Rust, libSQL, Tokio, FastEmbed, llama-cpp-2, Axum. Full module map: [CLAUDE.md](CLAUDE.md).

The install script downloads `origin-server` and `origin-mcp` under `~/.origin/bin/`, then creates a small `origin` launcher for setup and service commands.

---

## Build from source

This repo builds the daemon, MCP-facing CLI, and shared types.

```bash
git clone https://github.com/7xuanlu/origin.git
cd origin
cargo build -p origin-server
```

Run the daemon directly:

```bash
cargo run -p origin-server
```

Or install as a launchd service:

```bash
cargo build --release -p origin-server
./target/release/origin-server install
./target/release/origin-server status
```

First build takes several minutes while `llama.cpp` compiles for Metal.

---

## Boundaries

- Not a chat UI. Keep using Claude, ChatGPT, Cursor, or your agent of choice.
- Not a notes app or Notion / Obsidian replacement. Markdown export exists so you can read the artifact anywhere.
- Not a memory infrastructure SDK. Origin is meant for people using AI, not as a backend for other apps.
- macOS Apple Silicon only for now.
- Best for work that spans sessions, projects, and weeks. One-off chats may not need it.

---

## Contributing

Bug fixes, eval cases, docs, and features are welcome. Start with [CONTRIBUTING.md](CONTRIBUTING.md). Please also read the [Code of Conduct](CODE_OF_CONDUCT.md). Architecture is in [CLAUDE.md](CLAUDE.md).


| [Bug reports](https://github.com/7xuanlu/origin/issues/new/choose) | [Feature requests](https://github.com/7xuanlu/origin/issues/new?template=feature_request.yml) | [Good first issues](https://github.com/7xuanlu/origin/labels/good%20first%20issue) |
| ------------------------------------------------------------------ | --------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |


---

## License

- Backend crates in this repo (`origin-types`, `origin-core`, `origin-server`, `origin` CLI): **Apache-2.0**
- [origin-mcp](https://github.com/7xuanlu/origin-mcp) (MCP server, npm package, Homebrew): **MIT**

The backend stays permissively licensed so MCP clients and downstream local tools can build on the same types and runtime boundary.

---

## Acknowledgments

Adjacent work shaping this space:

- Andrej Karpathy's note on the LLM-wiki pattern, parallel work in this space.
- Claude Code's `MEMORY.md`, the simplest version of the idea, and the one Origin aims to cooperate with.
- [PAI](https://github.com/danielmiessler/PAI), [claude-memory-compiler](https://github.com/coleam00/claude-memory-compiler), Palinode: different shapes of the same direction.
