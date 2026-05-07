---
name: capture
description: >
  Save a memory to Origin in flow. Active capture verb — use proactively
  when the user states a preference, makes a decision, corrects you, or
  shares a durable fact. Invoked as `/origin:capture <content>`.
---

# /origin:capture

Capture a single memory in the moment. Active verb: agent captures the
moment of insight, like a photograph.

## How to invoke

Call the `origin` MCP server's `remember` tool (will be renamed to `capture`
in origin-mcp v0.4) with the user's content as a complete, self-contained
statement.

```
remember(content="<args, written as a full sentence with WHY>")
```

The daemon auto-classifies type, extracts structured fields, detects
entities, and links the knowledge graph. Don't set `memory_type` or
`structured_fields` unless you're confident — omitting beats guessing wrong.

## What to capture

- Decisions: "Going with approach A because B"
- Preferences: "Prefers TDD because catches regressions early"
- Corrections: "Actually it's C, not D"
- Identity / project facts: "Works on Origin, a local memory daemon for AI tools"

## What NOT to capture

- System prompts, boot logs, heartbeats
- Transient task state ("currently working on...")
- Tool output, command results, architecture dumps
- Single-word acknowledgments
- Things the user can trivially re-derive (file paths, recent git history)

## Atomic ideas

One capture = one idea. "Prefers TDD" and "Uses pytest" are two captures, not
one.

## When to use

- User explicitly says "remember this", "save that", "capture this".
- User states a durable preference / decision / correction proactively (no
  ask required — that's the floor, not the trigger).

## When NOT to use

- End of session bulk store → use `/origin:handoff` (multi-item batch).
- Pulling memories back out → use `/origin:recall`.
