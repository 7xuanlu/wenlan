---
name: help
description: >
  One-screen quick reference for the Wenlan plugin. Lists the daily
  verbs, the daily flow, where data lives, and how to view it without a
  GUI. Use when the user says "help", "what can I do", "list wenlan
  commands", "how do I use wenlan", or invokes `/help`.
allowed-tools: []
---

# /help

Print the Wenlan plugin reference card. Read-only — never calls a tool.

## How to invoke

When triggered, output the block below verbatim. No editing, no
abbreviating, no embellishing. The user is asking for the menu.

```
Wenlan plugin — daily verbs

  /setup        set up or repair Wenlan (auto-installs local runtime)
  /brief        load identity + topic context (start of session)
  /capture <x>  save one durable memory in flow
  /recall <q>   search local memory
  /lint [deep|repair] [scope]   diagnose, or resolve all findings safely
  /distill [t]  synthesize pages from clusters (scoped to current repo)
  /pages [q]    browse + open distilled pages (wenlan pages)
  /curate <surface>   deep audit (surface = captures|revisions); /brief handles daily
  /forget <id>  delete a memory by ID
  /handoff      end-of-session ritual (session log + captures)
  /help         this card

Daily flow (~1 min overhead per session):

  1. start session  →  hook auto-checks runtime, silent if up
  2. /brief         →  ~5 s, load context
  3. work normally  →  Claude proactively /captures durable facts
  4. /recall X      →  as needed for lookups
  5. /handoff       →  ~30 s, narrative session log + captures

Where your data lives (everything under ~/.wenlan/):

  ~/.wenlan/pages/      wiki pages distilled from your memories (md)
  ~/.wenlan/sessions/   session logs by date (md)
  ~/.wenlan/sessions/_status/  current per-project goals + last-handoff
  ~/.wenlan/db/         memories + knowledge graph (symlink to libSQL)
  ~/.wenlan/bin/        installed binaries

View it without a GUI:

  open ~/.wenlan/                  browse in Finder
  code ~/.wenlan/                  open in VS Code
  git -C ~/.wenlan log --oneline   timeline of every memory + distill pass
  ln -s ~/.wenlan/pages ~/Vault/wenlan   # symlink into Obsidian for graph view

~/.wenlan/ is a git repo. Skills auto-commit per logical batch (one per
session, distill pass, or forget). Use git log / git diff / git revert
as a free audit trail. No remote — purely local history.

Three classes of artifact:
  - memories: granular, queryable, live in DB only (confirmed = stays in DB)
  - pages:    synthesized wikis, DB + ~/.wenlan/pages/*.md projection
  - sessions: chronological narrative, ~/.wenlan/sessions/*.md only

The local runtime must run at 127.0.0.1:7878. Hook prints "/wenlan:setup" if down.

Optional upgrades for richer distill cycles:
  wenlan models install           local Qwen, no API cost
  wenlan keys set anthropic       Anthropic API, higher quality
```

## When to use

- User explicitly types `/help`.
- User asks "what can I do with wenlan", "list wenlan commands", "how
  does this plugin work", "remind me what verbs are available".
- First session after install — print this once on `/setup` success too.

## When NOT to use

- Specific factual lookup → use `/recall`.
- Setup troubleshooting → use `/setup` (it diagnoses + auto-installs).
