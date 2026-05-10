# Origin Claude Code Plugin

Claude Code plugin for Origin. It wires Claude Code to `origin-mcp` and adds short workflow skills for setup, help, briefing, capture, recall, distillation, review, forget, handoff, and debrief.

## 30-Second Setup

```text
0s   /plugin marketplace add 7xuanlu/origin
     /plugin install origin@origin-plugins
5s   restart Claude Code
10s  /init   auto-installs daemon if missing, configures Basic Memory,
            verifies daemon + MCP + round-trip, prints "Origin ready"
30s  /brief  (or /capture <something to remember>)
```

`/init` is self-healing — if the daemon isn't running and the `origin`
CLI isn't on PATH, it runs the install one-liner for you. No copy/paste,
no restart loop. The `SessionStart` hook only nudges you toward `/init`
if the daemon ever stops.

## Install

```text
/plugin marketplace add 7xuanlu/origin
/plugin install origin@origin-plugins
```

The marketplace is defined in [`../../.claude-plugin/marketplace.json`](../../.claude-plugin/marketplace.json) (at the repo root). The plugin metadata is defined in [`plugin.json`](plugin.json). MCP configuration is in [`../.mcp.json`](../.mcp.json) (this plugin's `.mcp.json`).

## Daily Commands

```text
/init       set up + verify Origin works (run once, or to diagnose)
/help       one-screen reference
/brief      load identity + topic context (start of session)
/capture    save one durable memory in flow
/recall     search local memory
/distill    synthesize pages from clusters
/review     audit pending memories
/forget     delete a memory by ID
/handoff    end-of-session debrief
/debrief    alias for /handoff (brief/debrief symmetry)
```

A `SessionStart` hook (`hooks/check-daemon.sh`) probes the local daemon at `127.0.0.1:7878`. If down, it prints a single line: `daemon not running. Run /origin:init to set up.` The skill owns the install logic — the hook is just a nudge. Hook never blocks the session.

## Where your data lives

```text
~/.origin/pages/               wiki pages distilled from memories (md)
~/.origin/sessions/            session logs by date (md)
~/.origin/sessions/_status/    current per-project goals + last-handoff timestamp
~/.origin/db/                  symlink to the libSQL store
~/.origin/bin/                 installed binaries
```

Browse with `open ~/.origin/` (Finder), `code ~/.origin/` (VS Code), or symlink `~/.origin/pages/` into an Obsidian vault for the graph view. No Tauri app required.

## Skill Files

The actual skill instructions live in [`../skills`](../skills):

- `init`: end-to-end setup verifier (daemon + MCP + round-trip)
- `help`: one-screen quick reference
- `brief`: load session context
- `capture`: save one durable memory
- `recall`: targeted lookup
- `distill`: refresh wiki pages
- `review`: audit pending memories
- `forget`: delete a memory by ID
- `handoff`: capture end-of-session decisions, lessons, gotchas, and open threads
- `debrief`: alias for `handoff` (brief/debrief symmetry)

## License

Apache-2.0.
