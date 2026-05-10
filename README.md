<p align="center">
  <img src="./docs/assets/social-preview.png" alt="Origin: Memory that compounds with your AI work." width="100%">
</p>

[![CI](https://github.com/7xuanlu/origin/actions/workflows/ci.yml/badge.svg)](https://github.com/7xuanlu/origin/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/7xuanlu/origin?sort=semver)](https://github.com/7xuanlu/origin/releases/latest)
[![origin-mcp](https://img.shields.io/badge/dynamic/json?label=origin-mcp&query=%24.version&url=https%3A%2F%2Fraw.githubusercontent.com%2F7xuanlu%2Forigin%2Fmain%2Fcrates%2Forigin-mcp%2Fnpm%2Fpackage.json)](crates/origin-mcp)
[![License](https://img.shields.io/badge/dynamic/json?label=license&query=%24.license&url=https%3A%2F%2Fraw.githubusercontent.com%2F7xuanlu%2Forigin%2Fmain%2Fcrates%2Forigin-mcp%2Fnpm%2Fpackage.json)](#license)

Developers have `origin` for code. Origin gives AI work its own source of truth: decisions, lessons, gotchas, project context, and wiki pages that carry across chats, projects, and time.

Markdown you can read, plus a local database with hybrid retrieval for your AI. Use it through the Claude Code plugin or any MCP client.

The daemon does the memory chores in the background: storing, searching, deduplicating, linking related ideas, distilling pages, and keeping provenance attached. This repo ships the whole local runtime: the `origin-server` daemon, setup commands, `origin-mcp`, the Claude Code plugin, the `origin-core` memory engine, and shared `origin-types`.

**Status:** Early preview. Expect fast iteration and some sharp edges.

---

## Quickstart

### 1. Install the Claude Code plugin

For the daily experience in Claude Code, install the Origin plugin from this repo:

```text
/plugin marketplace add 7xuanlu/origin
/plugin install origin@7xuanlu
```

The first command registers this repo as a Claude Code plugin marketplace. The second installs the `origin` plugin and its skills.

After Origin is installed in step 2, use short commands instead of asking Claude to call MCP tools manually:

```text
/init
/brief
/capture remember this decision...
/recall database preferences
/handoff
```

Plugin details: [.claude-plugin](.claude-plugin/README.md).

### 2. Install Origin on your machine

Origin runs locally in the background. The current prebuilt runtime supports macOS Apple Silicon:

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

Daemon details: [crates/origin-server](crates/origin-server/README.md).

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

`origin-mcp` connects to the local Origin daemon on `127.0.0.1:7878`. On first run, `npx origin-mcp` downloads the MCP connector published from `crates/origin-mcp/` in this repo.

MCP tools and options: [crates/origin-mcp](crates/origin-mcp/README.md).

---

## How Origin works

Use your AI tools normally. Origin runs in the background and makes durable context available when it is needed.

1. The local daemon owns the memory database and listens on `127.0.0.1:7878`.
2. Claude Code, Cursor, Codex, Claude Desktop, Windsurf, Gemini CLI, and other MCP clients connect through `origin-mcp`.
3. Agents capture decisions, preferences, project facts, gotchas, and lessons while you work.
4. Origin deduplicates, links related ideas, distills pages, and preserves where each memory came from.
5. Recall combines vector search, full-text search, and knowledge graph signals without replaying full chat history.

No cloud sync or telemetry by default. Local models and Anthropic keys are opt-in.

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

Origin is daemon-first. `origin-server` owns the local database, embeddings, refinery, knowledge graph, and HTTP API on `127.0.0.1:7878`. The plugin, MCP server, CLI, and local tools are thin clients over that daemon.

| Path | What lives there |
| --- | --- |
| [crates/origin-core](crates/origin-core/README.md) | Storage, search, embeddings, refinery, graph, pages, export, eval. |
| [crates/origin-server](crates/origin-server/README.md) | Local daemon, setup, launchd service, HTTP API. |
| [crates/origin-mcp](crates/origin-mcp/README.md) | MCP server, tools, npm package. |
| [crates/origin-cli](crates/origin-cli/README.md) | Source-built developer CLI for daemon search, recall, store, list, and agents. |
| [.claude-plugin](.claude-plugin/README.md) and [skills](skills/README.md) | Claude Code plugin metadata and workflow skills. |
| [docs/eval](docs/eval/README.md) | Benchmark workflow and methodology. |

Full contributor map: [CLAUDE.md](CLAUDE.md).

---

## Build from source

For local development:

```bash
git clone https://github.com/7xuanlu/origin.git
cd origin
cargo build -p origin-server
cargo run -p origin-server
```

Component build details live in the crate READMEs linked above.

---

## Boundaries

- Not a chat UI. Keep using Claude, ChatGPT, Cursor, or your agent of choice.
- Not a notes app or Notion / Obsidian replacement. Markdown exists so you can read the artifact anywhere.
- Not a memory infrastructure SDK. Origin is for people using AI, not as a backend for other apps.
- Best for work that spans sessions, projects, and weeks. One-off chats may not need it.

---

## Contributing

Bug fixes, eval cases, docs, and features are welcome. Start with [CONTRIBUTING.md](CONTRIBUTING.md). Architecture and development rules are in [CLAUDE.md](CLAUDE.md). Security reports: [SECURITY.md](SECURITY.md). Please also read the [Code of Conduct](CODE_OF_CONDUCT.md).

---

## License

- Rust workspace crates (`origin-types`, `origin-core`, `origin-server`, `origin` CLI, `origin-mcp`): **Apache-2.0**
- Claude Code plugin files (`.claude-plugin/`, `skills/`) ship from this repo under the same project license metadata.

The runtime stays permissively licensed so MCP clients and downstream local tools can build on the same types and daemon boundary.

---

## Acknowledgments

Adjacent work shaping this space:

- Andrej Karpathy's note on the LLM-wiki pattern, parallel work in this space.
- Claude Code's `MEMORY.md`, the simplest version of the idea, and the one Origin aims to cooperate with.
- [PAI](https://github.com/danielmiessler/PAI), [claude-memory-compiler](https://github.com/coleam00/claude-memory-compiler), Palinode: different shapes of the same direction.
