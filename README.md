# origin-plugin

Claude Code plugin for [Origin](https://github.com/7xuanlu/origin). Where Personal AI Memory Compounds.

Bundles the `origin-mcp` MCP server (auto-installed via `npx -y origin-mcp`) and adds 8 short-typed slash commands for the user-facing memory verbs.

## Verbs

Two-syllable max. Active where it matters.

| Skill | What it does |
| --- | --- |
| `/origin:init` | First-run setup: pick backend (Basic / On-device / Anthropic), register MCP clients, smoke check. |
| `/origin:brief [topic]` | Session-start briefing — identity + preferences + topic context. Call FIRST. |
| `/origin:capture <content>` | Save a memory in flow. Auto-classifies type + entities. |
| `/origin:recall <query>` | Targeted memory search. |
| `/origin:distill [page_id]` | Trigger synthesis pass — clusters memories into pages. |
| `/origin:review` | Walk unconfirmed memories: accept / edit / reject. |
| `/origin:forget <id>` | Delete a memory. Destructive. |
| `/origin:handoff` | Session-end debrief — capture decisions, lessons, gotchas. Alias: `/origin:debrief`. |

All skills route through MCP where the tool exists; refinery verbs (`/origin:distill`, parts of `/origin:review`) talk to the daemon's HTTP API directly until matching MCP tools land in `origin-mcp` v0.4.

## Install

In Claude Code:

```
/plugin install 7xuanlu/origin-plugin
```

The plugin pulls `origin-mcp` from npm on first use, so make sure `node` and `npx` are on your `PATH`.

You also need the Origin daemon running locally. The simplest path is the desktop app at [7xuanlu/origin-app](https://github.com/7xuanlu/origin-app), which boots the daemon on launch. For headless use:

```bash
curl -fsSL https://raw.githubusercontent.com/7xuanlu/origin/main/install.sh | bash
export PATH="$HOME/.origin/bin:$PATH"
origin install
origin status
```

## Why a plugin

`origin-mcp` exposes MCP tools (`remember` / `recall` / `context` / `forget` / `doctor`). Triggering them through MCP normally takes a sentence ("call recall with query 'Alice database preferences'"). With this plugin the same call becomes:

```
/origin:recall Alice database preferences
```

Same effect, ~70% fewer keystrokes. Skill descriptions also tell Claude when to invoke each verb proactively, so memory hygiene improves without the user prompting every time.

## Layout

```
origin-plugin/
├── .claude-plugin/plugin.json   # plugin manifest
├── .mcp.json                    # MCP server config (origin-mcp via npx)
├── skills/
│   ├── init/SKILL.md
│   ├── brief/SKILL.md
│   ├── capture/SKILL.md
│   ├── recall/SKILL.md
│   ├── distill/SKILL.md
│   ├── review/SKILL.md
│   ├── forget/SKILL.md
│   └── handoff/SKILL.md
├── LICENSE
└── README.md
```

## Companion repos

- [7xuanlu/origin](https://github.com/7xuanlu/origin) — daemon (`origin-server`), CLI (`origin`), shared types. Apache-2.0.
- [7xuanlu/origin-app](https://github.com/7xuanlu/origin-app) — Tauri desktop app + React UI. AGPL-3.0.
- [7xuanlu/origin-mcp](https://github.com/7xuanlu/origin-mcp) — MCP server. MIT.

## Status

v0.1 preview. Skill names locked. Pending before tagging:
- Backing MCP tool rename `remember → capture` in `origin-mcp` v0.4.
- New MCP tools `distill`, `review` (or HTTP-direct path documented).

## License

Apache-2.0. See [LICENSE](LICENSE).
