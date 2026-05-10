# Origin Claude Code Plugin

Claude Code plugin for Origin. It wires Claude Code to `origin-mcp` and adds short workflow skills for setup, briefing, capture, recall, distillation, review, forget, and handoff.

## Install

```text
/plugin marketplace add 7xuanlu/origin
/plugin install origin@7xuanlu
```

The marketplace is defined in [`marketplace.json`](marketplace.json). The plugin metadata is defined in [`plugin.json`](plugin.json). MCP configuration comes from the repository root [`.mcp.json`](../.mcp.json).

## Daily Commands

```text
/init
/brief
/capture remember this decision...
/recall database preferences
/distill
/review
/forget mem_...
/handoff
```

The local Origin daemon still needs to be running. Use the root README or [`crates/origin-server`](../crates/origin-server/README.md) for daemon setup.

## Skill Files

The actual skill instructions live in [`../skills`](../skills):

- `brief`: load session context
- `capture`: save one durable memory
- `recall`: targeted lookup
- `distill`: refresh wiki pages
- `review`: audit pending memories
- `forget`: delete a memory by ID
- `handoff`: capture end-of-session decisions, lessons, gotchas, and open threads
- `init`: setup flow

## License

Apache-2.0.
