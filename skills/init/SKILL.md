---
name: init
description: >
  Set up Origin in this project / workspace. Pick the cloud or on-device LLM
  backend, register MCP clients, and warm the daemon for first use. Invoked
  as `/origin:init`.
---

# /origin:init

First-run setup for Origin in this workspace.

## What it does

1. Verify the daemon is running on `127.0.0.1:7878` (boots Origin desktop app or `origin install` if missing).
2. Pick a backend mode:
   - Basic Memory (no LLM — fastest, embedding-only retrieval)
   - On-device Qwen (privacy-first, GPU required)
   - Anthropic API (best quality, key required)
3. Register origin-mcp into the active AI tool's config (Claude Code, Cursor, Codex, Claude Desktop, Windsurf, Gemini CLI). Skip if already configured.
4. Run a smoke `/origin:brief` to confirm everything wired.

## How to invoke

Call the daemon's `/api/setup/status` first to read current mode. If `setup_completed` is false, walk the user through backend choice. Otherwise report current mode + offer to change.

```
GET  /api/setup/status
PUT  /api/config           # to switch backend
```

The MCP `doctor` tool is the agent-side equivalent for diagnosing problems mid-flow; this skill is the user-driven setup verb.

## When to use

- First time using Origin in a project.
- User says "set up Origin", "configure memory", "start fresh".
- After installing Origin desktop app or running the headless installer.

## When NOT to use

- Daemon already configured and working — don't run init again. Use `/origin:brief` for session start.
- Just changing one config field — use `/origin:review` or direct settings.
