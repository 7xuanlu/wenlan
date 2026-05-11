# Origin Claude Code Plugin

Claude Code plugin for Origin. It wires Claude Code to `origin-mcp` and adds short workflow skills for setup, help, briefing, capture, recall, distillation, review, forget, handoff, and debrief.

## 30-Second Setup

```text
0s   /plugin marketplace add 7xuanlu/origin
     /plugin install origin@7xuanlu
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
/plugin install origin@7xuanlu
```

The marketplace is defined in [`../../.claude-plugin/marketplace.json`](../../.claude-plugin/marketplace.json) (at the repo root). The plugin metadata is defined in [`plugin.json`](plugin.json). MCP configuration is in [`../.mcp.json`](../.mcp.json) (this plugin's `.mcp.json`).

## Daily Commands

```text
/init       set up + verify Origin works (run once, or to diagnose)
/help       one-screen reference
/brief      load identity + topic context (start of session)
/capture    save one durable memory in flow
/recall     search local memory
/distill    synthesize pages from clusters (scoped to current repo)
/read       preview a distilled page inline
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

## Basic Memory mode and agent-side LLM phases

By default `/init` configures **Basic Memory**: no model download, no API
key, no prompts. The daemon stores, embeds, dedupes, and serves hybrid
search — it does *not* do LLM-heavy work like classification, entity
extraction, page synthesis, or reranking.

The skills compensate. Where the daemon would normally call its LLM, the
skill asks Claude itself to do the equivalent step and posts the result
back via HTTP:

| Phase | LLM-equipped daemon | Basic Memory + skill |
|---|---|---|
| Pick `memory_type` | daemon classifier | `/capture` picks one of 6 types from content |
| Extract entities/relations | daemon `extract.rs` | `/capture` POSTs to `/api/memory/entities` and `/api/memory/relations` |
| Synthesize a page | daemon refinery + `/distill` | `/distill` reads the cluster, writes the page, POSTs to `/api/pages` |
| Expand a query / rerank hits | daemon expansion + rerank | `/recall` rewrites the query before search and reorders hits after |

The daemon stays the single writer and storage owner. Claude does the
thinking. This is why Basic Memory works without an API key or
on-device model — and why the same skills also work when you later add
an LLM. The skills detect the daemon's capabilities and fall through to
the agent path when needed.

## Skill Files

The actual skill instructions live in [`../skills`](../skills):

- `init`: end-to-end setup verifier (daemon + MCP + round-trip)
- `help`: one-screen quick reference
- `brief`: load session context
- `capture`: save one durable memory
- `recall`: targeted lookup
- `distill`: refresh wiki pages
- `read`: preview a distilled page inline
- `review`: audit pending memories
- `forget`: delete a memory by ID
- `handoff`: capture end-of-session decisions, lessons, gotchas, and open threads
- `debrief`: alias for `handoff` (brief/debrief symmetry)

## License

Apache-2.0.
