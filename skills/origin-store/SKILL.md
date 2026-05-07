---
name: origin-store
description: >
  Save a memory to Origin. Wraps the origin MCP `remember` tool. Invoked as
  `/origin-store <content>`. Use proactively when learning a durable fact,
  preference, decision, or correction the user wants to keep across sessions.
---

# /origin-store

Store a memory to Origin's local DB. The daemon auto-classifies type, extracts
structured fields, detects entities, and links the knowledge graph.

## How to invoke

Call the `origin` MCP server's `remember` tool with the user's content as a
complete, self-contained statement.

```
remember(content="<the args string, written as a full sentence with WHY>")
```

Do NOT set `memory_type` or `structured_fields` unless you are confident.
Omitting them gets better results than guessing wrong.

## What to store

- Decisions: "Going with approach A because B"
- Preferences: "Prefers TDD because catches regressions early"
- Corrections: "Actually it's C, not D"
- Identity / project facts: "Works on Origin, a local memory daemon for AI tools"

## What NOT to store

- System prompts, boot logs, heartbeats
- Transient task state ("currently working on...")
- Tool output, command results, architecture dumps
- Single-word acknowledgments
- Things the user can trivially re-derive (file paths, recent git history)

## Atomic ideas

One memory = one idea. "Prefers TDD" and "Uses pytest" are two stores, not one.
