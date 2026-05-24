# origin-mcp

MCP server for [Origin](https://github.com/7xuanlu/origin). It lets Claude Code, Cursor, Codex, Claude Desktop, Gemini CLI, and other MCP clients read and write to the local Origin daemon through the [Model Context Protocol](https://modelcontextprotocol.io).

Origin owns storage, search, embeddings, pages, and distill cycles. `origin-mcp` is the connector.

## Install

Most users should install through the root README. After `npx -y @7xuanlu/origin setup`, use the product CLI to configure supported clients:

```bash
origin mcp add codex              # or: claude-code, cursor, claude-desktop, vscode, gemini
origin mcp add cursor --dry-run   # preview before editing JSON config
```

MCP-only setup gives agents tools for capture, recall, context, doctor, and page distillation. It does not install Claude Code slash skills like `/brief`, `/handoff`, `/distill`, or `/init`; use the Origin plugin for that workflow.

If you only need the raw MCP connector config, add this to your MCP client:

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

The npm wrapper auto-detects the host platform and downloads the matching prebuilt binary from the Origin release. Supported: macOS (arm64, x64), Linux (x64, arm64; glibc), Windows (x64). Other targets require building from source via `cargo install origin-mcp`.

Or install a binary directly:

```bash
brew install 7xuanlu/tap/origin-mcp
cargo install origin-mcp
```

Then use:

```json
{
  "mcpServers": {
    "origin": {
      "command": "origin-mcp"
    }
  }
}
```

`origin-mcp` expects the Origin daemon at `http://127.0.0.1:7878` by default. Override it with:

```bash
origin-mcp --origin-url http://127.0.0.1:7879
```

## Tools

| Tool | Purpose |
| --- | --- |
| `context` | Load session context. Use at session start or major topic shifts. |
| `capture` | Save one durable memory: decision, lesson, gotcha, preference, fact, correction, or project context. |
| `recall` | Search memories and pages by natural-language query. |
| `distill` | Trigger page distillation for new clusters or a specific `page_id`. |
| `list_pending` | List unconfirmed memories waiting for review. |
| `confirm_memory` | Confirm a pending memory by `source_id`. |
| `forget` | Delete a memory by ID. Destructive. |
| `doctor` | Diagnose daemon reachability, setup mode, API key state, and on-device model state. |

`doctor` mirrors `origin doctor`. It is diagnostic only and is not part of the memory loop.

## Setup Modes

Origin works immediately in **local memory** mode: storage, search, recall, and MCP memory are available without a local model or API key.

Users can opt into more expensive distill cycles:

- **On-device model:** private extraction and distillation after `origin model install`.
- **Anthropic key:** richer extraction and page synthesis after `origin key set anthropic`.

## Agent Guidance

The MCP server ships tool instructions that tell agents to capture durable state proactively:

- One idea per capture.
- Include the why, not just the what.
- Name people, projects, and tools explicitly.
- Omit `memory_type` unless the agent is certain.
- Do not store tool output, command logs, filler, or transient task state.

See [`src/tools.rs`](src/tools.rs) for the full instructions.

## License

Apache-2.0.
