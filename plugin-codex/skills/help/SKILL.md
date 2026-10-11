---
name: help
description: >
  Show a one-screen Codex reference for the Wenlan plugin. Use when the user
  asks for Wenlan help, command list, invokes /help, or explicitly asks about
  import progress.
allowed-tools: ["mcp__wenlan__list_pending_imports"]
user-invocable: true
---

# /help

Print the Wenlan Codex command card. The default help path is read-only and
calls no tools. Skip the `<!-- ... -->` comment lines inside the card; they mark
the generated verb list and are not part of it.

```text
Wenlan for Codex

<!-- verbs:start -->
<!-- generated from plugin/skills/verbs.json by scripts/gen-plugin-verbs.mjs; do not edit -->
  /setup                       set up or repair the local runtime and MCP bridge
  /brief [topic]               load the Brief for the current Space (optional; not a boot step)
  /capture <x>                 save one durable memory in flow
  /recall <q>                  search local memory
  /lint [deep|repair] [scope]  diagnose, or resolve all findings safely
  /distill [t]                 synthesize or refresh source-backed pages for the active Space
  /pages [q]                   list or open distilled pages in the OS editor
  /curate <surface>            deep audit of captures, revisions or refinements
  /forget <id>                 delete one memory by ID, after confirmation
  /handoff                     end-of-session debrief: captures, session log, Brief update
  /help                        one-screen quick reference
<!-- verbs:end -->

Import progress: ask explicitly; Wenlan checks `list_pending_imports` on demand.

Daily flow:

  1. /setup once after install, or when Wenlan looks broken
  2. /brief when resuming a project or asking to catch up
  3. /capture durable decisions, corrections, lessons, or preferences
  4. /recall when you need a specific memory
  5. /handoff before ending the session

Data lives under ~/.wenlan/:

  pages/              source-backed wiki pages
  sessions/           narrative session logs
  sessions/_status/   human receipts projected from Space Briefs
  bin/                installed wenlan and wenlan-mcp binaries

Open pages with /pages. Memories and the graph live in a local database;
location in docs/setup-and-data.md.

Models and keys do not enable background inference by themselves:

  wenlan steep status             show Everyday + Synthesis as off/ready/paused
  wenlan steep configure --everyday <source> --synthesis <source>
                                  review the exact mapping, disclosure, and confirm
  wenlan steep disable            turn model-backed background work off
```

Only when the user explicitly asks whether an import/export is still running,
call `mcp__wenlan__list_pending_imports`. Never call it during ordinary
`/help`, `/brief`, setup, or session-start flows.

If the local runtime or MCP bridge is down, tell the user to run `/setup`.
