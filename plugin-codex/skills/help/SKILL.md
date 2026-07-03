---
name: help
description: >
  Show a one-screen Codex reference for the Wenlan plugin. Use when the user
  asks for Wenlan help, command list, or invokes /help.
allowed-tools: []
user-invocable: true
---

# /help

Print the Wenlan Codex command card. Read-only; call no tools.

```text
Wenlan for Codex

  /setup          set up or repair the local runtime and MCP bridge
  /brief [topic]  load session status, identity, preferences, and memories
  /capture <x>    save one durable memory
  /recall <q>     search local memory
  /distill [t]    synthesize or refresh source-backed pages
  /pages [q]      list or open distilled pages in the OS editor
  /curate <s>     review pending captures or revisions (s = captures|revisions)
  /forget <id>    delete one memory by exact id after confirmation
  /handoff        close a session with captures, session log, and status
  /help           show this card

Daily flow:

  1. /setup once after install, or when Wenlan looks broken
  2. /brief at session start
  3. /capture durable decisions, corrections, lessons, or preferences
  4. /recall when you need a specific memory
  5. /handoff before ending the session

Data lives under ~/.wenlan/:

  pages/              source-backed wiki pages
  sessions/           narrative session logs
  sessions/_status/   current per-project status
  bin/                installed wenlan and wenlan-mcp binaries

Open pages with /pages. Inspect history with:

  git -C ~/.wenlan log --oneline
```

If the local runtime or MCP bridge is down, tell the user to run `/setup`.
