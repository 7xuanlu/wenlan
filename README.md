# origin-plugin

Claude Code plugin for [Origin](https://github.com/7xuanlu/origin), the personal AI memory layer.

Bundles the `origin-mcp` MCP server (auto-installed via `npx -y origin-mcp`) and adds short-typed slash commands for the most common verbs:

| Skill | What it does |
| --- | --- |
| `/origin-context [topic]` | Load identity + preferences + topic-relevant memories. Call this FIRST at session start. |
| `/origin-recall <query>` | Search memory. Targeted lookup. |
| `/origin-store <content>` | Save a durable memory. Auto-classifies type and entities. |
| `/origin-status` | Diagnose the local daemon. |
| `/origin-forget <source_id>` | Delete a memory by ID. Destructive. |

All skills route through the `origin` MCP server, which talks to your local Origin daemon on `127.0.0.1:7878`.

## Install

In Claude Code:

```
/plugin install 7xuanlu/origin-plugin
```

Or add to your plugin config manually. The plugin pulls `origin-mcp` from npm on first use, so make sure `node` and `npx` are on your `PATH`.

You also need the Origin daemon running locally. The simplest path is the desktop app at [7xuanlu/origin-app](https://github.com/7xuanlu/origin-app), which boots the daemon on launch. For headless use:

```bash
curl -fsSL https://raw.githubusercontent.com/7xuanlu/origin/main/install.sh | bash
export PATH="$HOME/.origin/bin:$PATH"
origin install
origin status
```

## Why a plugin

`origin-mcp` exposes 5 verbs (`remember`, `recall`, `context`, `doctor`, `forget`). Triggering them through MCP normally takes a sentence ("call recall with query 'Alice database preferences'"). With this plugin the same call becomes:

```
/origin-recall Alice database preferences
```

Same effect, ~70% fewer keystrokes. Skill descriptions also tell Claude when to use each verb proactively, so memory hygiene improves without the user prompting every time.

## Layout

```
origin-plugin/
├── .claude-plugin/plugin.json   # plugin manifest
├── .mcp.json                    # MCP server config (origin-mcp via npx)
├── skills/
│   ├── origin-context/SKILL.md
│   ├── origin-recall/SKILL.md
│   ├── origin-store/SKILL.md
│   ├── origin-status/SKILL.md
│   └── origin-forget/SKILL.md
└── README.md
```

## License

Apache-2.0. See [LICENSE](LICENSE).

The plugin metadata + skill prompts are Apache-2.0 to match the daemon side. The Tauri desktop app at [7xuanlu/origin-app](https://github.com/7xuanlu/origin-app) is AGPL-3.0; the `origin-mcp` server at [7xuanlu/origin-mcp](https://github.com/7xuanlu/origin-mcp) is MIT.

## Roadmap

- v0.2: refinery transparency. Add `/origin-steep`, `/origin-distill`, `/origin-reclassify` once the matching MCP tools land in `origin-mcp`. Pair with an `auto_refinery: false` daemon config so users / agents drive background refinement explicitly.
- v0.3: per-skill auth gates. Today every tool is open on loopback. As Origin grows remote-access surface, skills should respect a per-tool allowlist.
