# Origin Claude Code Plugin

Claude Code plugin for Origin. It wires Claude Code to `origin-mcp` and adds short workflow skills for setup, briefing, capture, recall, distillation, review, forget, and handoff.

## 30-Second Setup

```text
0s   /plugin marketplace add 7xuanlu/origin
     /plugin install origin@7xuanlu
5s   restart Claude Code
10s  hook auto-checks daemon — silent if up, exact fix if down
20s  /init   (verifies daemon + MCP + round-trip, prints "Origin ready")
30s  /brief  (or /capture <something to remember>)
```

If the hook prints a daemon warning, follow the printed command. The
daemon is the only out-of-band dependency. Everything else is wired by
the plugin.

## Install

```text
/plugin marketplace add 7xuanlu/origin
/plugin install origin@7xuanlu
```

The marketplace is defined in [`marketplace.json`](marketplace.json). The plugin metadata is defined in [`plugin.json`](plugin.json). MCP configuration comes from the repository root [`.mcp.json`](../.mcp.json).

## Daily Commands

```text
/init       set up + verify Origin works (run once, or to diagnose)
/brief      load identity + topic context (start of session)
/capture    save one durable memory in flow
/recall     search local memory
/distill    synthesize pages from clusters
/review     audit pending memories
/forget     delete a memory by ID
/handoff    end-of-session debrief
/help       one-screen reference
```

A `SessionStart` hook (`hooks/check-daemon.sh`) probes the local daemon at `127.0.0.1:7878`. Three states:

| State | Hook output |
|---|---|
| Daemon up | silent |
| Daemon down, `origin` CLI installed | print `origin install && origin status` |
| `origin` CLI missing entirely | print install one-liner |

Hook never blocks the session.

## Skill Files

The actual skill instructions live in [`../skills`](../skills):

- `init`: end-to-end setup verifier (daemon + MCP + round-trip)
- `brief`: load session context
- `capture`: save one durable memory
- `recall`: targeted lookup
- `distill`: refresh wiki pages
- `review`: audit pending memories
- `forget`: delete a memory by ID
- `handoff`: capture end-of-session decisions, lessons, gotchas, and open threads
- `help`: one-screen quick reference

## License

Apache-2.0.
