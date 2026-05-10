---
name: init
description: >
  Set up Origin in this project / workspace. Pick the cloud or on-device LLM
  backend, register MCP clients, and warm the daemon for first use. Invoked
  as `/init`.
allowed-tools: ["Bash", "mcp__origin__doctor", "mcp__origin__context"]
---

# /init

First-run setup for Origin in this workspace.

## What it does

1. Verify the daemon is running on `127.0.0.1:7878` (guide the user through `origin setup` / `origin install` if missing).
2. Pick a backend mode:
   - Basic Memory (no LLM — fastest, embedding-only retrieval)
   - On-device Qwen (privacy-first, GPU required)
   - Anthropic API (best quality, key required)
3. Register origin-mcp into the active AI tool's config (Claude Code, Cursor, Codex, Claude Desktop, Windsurf, Gemini CLI). Skip if already configured.
4. Run a smoke `/brief` to confirm everything wired.

## How to invoke

Call the `origin` MCP server's `doctor` tool first to read current mode and surface any setup gaps. If unconfigured, walk the user through backend choice. Otherwise report current mode + offer to change.

```
doctor()
```

If the user wants to switch backend or install a model, shell out:

```
origin model install
origin key set anthropic
origin doctor
```

## When to use

- First time using Origin in a project.
- User says "set up Origin", "configure memory", "start fresh".
- After installing the local Origin runtime.

## When NOT to use

- Daemon already configured and working — don't run init again. Use `/brief` for session start.
- Just changing one config field — use `/review` or direct settings.
