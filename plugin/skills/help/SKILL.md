---
name: help
description: >
  One-screen quick reference for the Origin plugin. Lists the 8 daily verbs
  with one-liners and the 30-second daily flow. Use when the user says
  "help", "what can I do", "list origin commands", "how do I use origin",
  or invokes `/help`.
allowed-tools: []
---

# /help

Print the Origin plugin reference card. Read-only — never calls a tool.

## How to invoke

When triggered, output the block below verbatim. No editing, no
abbreviating, no embellishing. The user is asking for the menu.

```
Origin plugin — daily verbs

  /init         set up Origin in this workspace (run once)
  /brief        load identity + topic context (start of session)
  /capture <x>  save one durable memory in flow
  /recall <q>   search local memory
  /distill      synthesize pages from clusters
  /review       audit pending memories
  /forget <id>  delete a memory by ID
  /handoff      end-of-session debrief

Daily flow (~1 min overhead per session):

  1. start session  →  hook auto-checks daemon, silent if up
  2. /brief         →  ~5 s, load context
  3. work normally  →  Claude proactively /captures durable facts
  4. /recall X      →  as needed for lookups
  5. /handoff       →  ~30 s, end-of-session memory snapshot

Daemon must run at 127.0.0.1:7878. Hook prints exact fix if down.
Memory lives at ~/Library/Application Support/origin/. Local-only by default.
```

## When to use

- User explicitly types `/help` (Claude Code may dispatch to this skill).
- User asks "what can I do with origin", "list origin commands", "how does
  this plugin work", "remind me what verbs are available".
- First session after install — print this once on `/init` success too.

## When NOT to use

- Specific factual lookup → use `/recall`.
- Setup troubleshooting → use `/init` (it diagnoses).
