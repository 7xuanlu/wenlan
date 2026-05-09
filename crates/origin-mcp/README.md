# origin-mcp

MCP connector for [Origin](https://github.com/7xuanlu/origin).

[Origin](https://useorigin.app) runs quietly behind the AI tools you already use. It gives your AI a place to carry decisions, lessons, gotchas, and project context instead of rediscovering them in every new chat.

`origin-mcp` lets Claude Code, Cursor, Codex, Claude Desktop, Windsurf, Gemini CLI, and other MCP clients read and write to your local Origin runtime through the [Model Context Protocol](https://modelcontextprotocol.io). The daemon owns storage, search, embeddings, and background refinement. This repo is only the MCP connector.

## Install

Add to your MCP config (Claude Code, Cursor, Claude Desktop, Windsurf, Gemini CLI):

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

Or install the binary directly:

```bash
# Via Homebrew
brew install 7xuanlu/tap/origin-mcp

# Via cargo
cargo install origin-mcp
```

Then add the binary path to your MCP config:

```json
{
  "mcpServers": {
    "origin": {
      "command": "origin-mcp"
    }
  }
}
```

## How it works

`origin-mcp` connects to the Origin daemon running on `127.0.0.1:7878`.

```
Claude Code / Cursor / Claude Desktop
    |
    | MCP (stdio)
    v
origin-mcp
    |
    | HTTP
    v
Origin runtime
    |
    v
Local SQLite + embeddings + knowledge graph
```

If the daemon is not running, `origin-mcp` returns an actionable setup message. Install the Origin desktop app, or install the headless runtime:

```bash
curl -fsSL https://raw.githubusercontent.com/7xuanlu/origin/main/install.sh | bash
export PATH="$HOME/.origin/bin:$PATH"
origin setup
origin install
origin status
```

Setup has three paths:

- **Basic Memory:** store, search, and recall immediately. No model download or API key.
- **On-device Model:** private local extraction and background refinement after model download.
- **Anthropic Key:** richer extraction and background refinement using your API key.

## Memory tools

| Tool | What it does | Annotations |
|------|-------------|-------------|
| `capture` | Capture a memory, fact, preference, decision, lesson, or gotcha. The backend auto-classifies type, extracts entities, and links to the knowledge graph. (Renamed from `remember` in v0.4.) | write, non-destructive |
| `recall` | Search memories and knowledge graph by natural language. Returns ranked results with source tracing. | read-only |
| `context` | Load session context: identity, preferences, goals, and topic-relevant memories. Call this at session start. | read-only |
| `forget` | Delete a specific memory and clean up entity links. Requires the memory ID. | destructive, idempotent |
| `doctor` | Diagnose the local Origin runtime. Use when tools fail or onboarding a new MCP client. | read-only |
| `distill` | Trigger Origin's distillation pass. Without `page_id`, runs a full pass; with `page_id`, re-distills that single page. | write, idempotent (added in v0.4) |
| `list_pending` | List unconfirmed memories pending review. Pairs with `confirm_memory` (accept) and `forget` (reject). | read-only (added in v0.4) |
| `confirm_memory` | Confirm a pending memory by `source_id`. Used during review to accept a captured memory. | write, idempotent (added in v0.4) |

## Diagnostic tool

| Tool | What it does | Annotations |
|------|-------------|-------------|
| `doctor` | Check daemon reachability, setup mode, Anthropic key state, and on-device model state. | read-only |

`doctor` matches the `origin doctor` CLI command. It is not part of the memory loop. It exists so an MCP client can explain setup and refinement problems without guessing.

### What agents should know

The server ships with proactive-capture instructions that guide agents to store the right things at the right granularity. Key ideas:

- **Two mental models**: `profile` (about the user) vs `knowledge` (about the world). Agents should think in these terms when deciding what to store.
- **One idea per memory.** "Prefers TDD" and "uses pytest" are two memories, not one. Specific memories retrieve better than broad summaries.
- **Include the why.** "Switched to dark mode because of migraines" is more useful than "uses dark mode."
- **Omit `memory_type`.** Let the backend auto-classify. Agents get it wrong more often than the classifier.
- **Anti-noise rules.** Don't store conversation filler, tool output, or things trivially re-derivable from code.

See [`src/tools.rs`](src/tools.rs) for the full `with_instructions` text that agents receive.

### Options

```
--origin-url <URL>    Override Origin server URL (default: http://127.0.0.1:7878)
```

## What Origin does with your memories

Origin works in Basic Memory mode without an on-device model or API key: storage, search, recall, and MCP memory are available immediately.

When the user opts into an on-device model or Anthropic key, Origin can refine memories over time:

- **Deduplication.** Overlapping memories are merged automatically.
- **Page distillation.** Related memories are clustered into pages: compact, wiki-style summaries that save tokens on retrieval.
- **Knowledge graph.** Entities and relations are extracted and linked, so "Alice leads the deploy refactor" connects Alice, the project, and the decision.
- **Contradiction detection.** When new information conflicts with existing memories, Origin surfaces it for your review.

The longer you use it, the better the retrieval gets.

## Requirements

- **Origin runtime** running locally (via the desktop app or `origin setup` / `origin install`)
- **macOS Apple Silicon** (M1+) at v0.1.0. Linux x64 binaries are built but not yet tested in production.

## License

MIT

## Links

- [Origin](https://github.com/7xuanlu/origin): the desktop app, daemon, and core engine
- [Model Context Protocol](https://modelcontextprotocol.io): the protocol spec
