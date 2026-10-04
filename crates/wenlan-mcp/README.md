# wenlan-mcp

MCP server for [Wenlan](https://github.com/7xuanlu/wenlan). It lets Claude Code, Cursor, Codex, Claude Desktop, Gemini CLI, and other MCP clients read and write to the local Wenlan daemon through the [Model Context Protocol](https://modelcontextprotocol.io).

Wenlan owns storage, search, embeddings, pages, and distill cycles. `wenlan-mcp` is the connector.

## Install

Most users should install the runtime through the root README (`npx -y wenlan setup` on macOS Apple Silicon, the automated shell setup on Linux, or the matching release archive on Windows). Then use the product CLI to configure supported clients:

```bash
wenlan connect codex              # or: claude-code, cursor, claude-desktop, vscode, gemini
wenlan connect cursor --dry-run   # preview before editing JSON config
```

MCP-only setup gives agents tools for capture, recall, a Space-owned Brief, and page distillation. It does not install Claude Code slash skills like `/brief`, `/handoff`, `/distill`, or `/setup`; use the Wenlan plugin for that workflow.

If you only need the raw MCP connector config, add this to your MCP client:

```json
{
  "mcpServers": {
    "wenlan": {
      "command": "npx",
      "args": ["-y", "wenlan-mcp"]
    }
  }
}
```

The npm wrapper auto-detects the host platform and downloads the matching prebuilt binary from the Wenlan release. Supported: macOS (arm64), Linux (x64, arm64; glibc), Windows (x64). Other targets require building the connector from source via `cargo install --locked wenlan-mcp`; macOS Intel does not currently have a supported complete local runtime.

Or install a binary directly:

```bash
brew install 7xuanlu/tap/wenlan-mcp
cargo install --locked wenlan-mcp
```

`--locked` builds with the dependency versions the release was tested with; a plain `cargo install wenlan-mcp` also works, since the crate pins its proc-macro crate to the same minor as `rmcp`.

Then use:

```json
{
  "mcpServers": {
    "wenlan": {
      "command": "wenlan-mcp"
    }
  }
}
```

`wenlan-mcp` expects the Wenlan daemon at `http://127.0.0.1:7878` by default. Override it with:

```bash
wenlan-mcp --origin-url http://127.0.0.1:7879
```

## Tools

The local stdio surface is locked at exactly 29 tools. The table below calls
out the primary memory loop and the unique refinement-review queue.

| Tool | Purpose |
| --- | --- |
| `brief` | Read the current Space Brief; an optional topic appends separately labeled same-Space context. |
| `capture` | Save one durable memory and return its explicit `source_memory_id`. |
| `recall` | Search memories and pages by natural-language query. |
| `distill` | Trigger page distillation for new clusters or a specific `page_id`. |
| `list_pending` | List unconfirmed memories waiting for review. |
| `confirm_memory` | Confirm a pending memory by `source_id`. |
| `forget` | Delete a memory by ID. Destructive. |
| `list_refinements` | Explicitly inspect the daemon's unique proposal queue, including `vocab_promote`; never polled ambiently. |
| `accept_refinement` | Accept one listed proposal after an unambiguous item-level decision. Local stdio only. |
| `reject_refinement` | Reject one listed proposal after an unambiguous item-level decision. Local stdio only. |

The refinement trio remains because this review queue has no CLI or replacement
path. Remote HTTP clients can list it, but `accept_refinement` and
`reject_refinement` are hidden and hard-rejected remotely.

Runtime diagnostics live in the CLI: `wenlan doctor`. They are not part of the
MCP memory loop.

### Query-only HTTP profile

For a connector that should retrieve knowledge without exposing knowledge-write
or maintenance tools, explicitly select the query-only profile:

```bash
WENLAN_SPACE=shared wenlan-mcp serve --tool-profile query-only --token-file /path/to/bearer-token
```

Native launchers can instead provide the token in a child-only environment
variable and pass `--token-env WENLAN_REMOTE_MCP_TOKEN`. This is mutually
exclusive with `--token`, `--token-file`, and `--no-auth`. The explicitly named
variable must contain 32-128 ASCII letters, digits, `_`, or `-`; missing or
invalid values stop startup without falling back to a token file. Do not put
the secret itself in command arguments or log the child's environment. This
keeps it out of command-line listings, not out of the child process memory or
the reach of an account that can inspect that process.

This profile advertises only `brief`, `recall`, and `get_page_sources`. The
server also rejects all other tool names before dispatch, including direct
calls to hidden tools. It requires a nonempty bearer token; `--no-auth` is not
permitted. A strict `WENLAN_SPACE` pin is also required; an overridable
`WENLAN_DEFAULT_SPACE` is not an authorization boundary. The default `standard`
profile keeps its existing response format.

Query-only also exposes `GET /connector-info` behind the same bearer and Origin
checks as `/mcp`. It reports contract version 1, `wenlan-mcp`, the query-only
profile, bearer authentication, and the pinned Space. Relay enrollment must
check that anonymous access fails and that the authenticated response matches
the intended Space. This is configuration verification, not proof of device
ownership. Standard mode has no such endpoint; public `/health` reveals no Space.

Query-only success results include `structuredContent` and matching JSON text;
each advertised tool has an output schema generated from its response type.
Search results retain source IDs, titles, content, and archive/review status.
Brief results retain summary, active/backlog text and gates, plus optional
related context. Page sources retain source IDs and available source content;
unavailable linked memories are omitted, including their IDs, because missing
content may be outside the granted Space. Raw import text, ranking and
access metadata are not forwarded by these projections. Stored content itself
can still contain personal data; projection is not content classification or
authorization. Page IDs must be opaque ASCII letters, digits, `_` or `-`, not
URLs or filesystem paths; this input check also applies to Standard callers.

"Query-only" describes the available knowledge operations, not an absence of
all side effects: `recall` still records the query and accessed memory IDs in
the daemon's private activity history. It therefore declares
`readOnlyHint: false` under the plugin submission rules.

This is a tool-access boundary, not OAuth or multi-user isolation. It does not
make a local daemon ready for public marketplace distribution. Public access
to private libraries still needs authenticated user-to-library routing,
authorization, consent, revocation, privacy disclosures, and end-to-end tests.
Do not publish a personal daemon or its bearer token as a universal service.

The repository's [`chatgpt-app-submission.json`](../../chatgpt-app-submission.json)
is a preparation draft for this profile, not the Standard tool inventory. It
contains listing suggestions, three annotation justifications, and five positive
and three negative test cases. The cases explicitly identify pending synthetic
fixtures and public-host execution. Schema validation is not a test pass, an
authentication implementation, or approval to upload or submit the draft.

## Setup Modes

Wenlan works immediately in **local memory** mode: storage, search, recall, and MCP memory are available without a local model or API key.

Users can opt into more expensive distill cycles:

- **On-device model:** private extraction and distillation after `wenlan models install`.
- **Anthropic key:** richer extraction and page synthesis after `wenlan keys set anthropic`.

## Agent Guidance

The Standard profile ships tool instructions that tell agents to capture durable state proactively:

- One idea per capture.
- Include the why, not just the what.
- Name people, projects, and tools explicitly.
- Omit `memory_type` unless the agent is certain.
- Do not store tool output, command logs, filler, or transient task state.

See [`src/tools.rs`](src/tools.rs) for the full instructions.

## Links

- [wenlan.app](https://wenlan.app) — project home
- [wenlan.app/learn/mcp-memory-server](https://wenlan.app/learn/mcp-memory-server) — concept article on Wenlan as an MCP memory server
- [wenlan.app/docs/mcp-clients](https://wenlan.app/docs/mcp-clients) — connect Claude Code, Cursor, Codex, Claude Desktop, Gemini CLI
- [npm: wenlan-mcp](https://www.npmjs.com/package/wenlan-mcp) — standalone npm package
- [github.com/7xuanlu/wenlan](https://github.com/7xuanlu/wenlan) — source

## License

Apache-2.0.
