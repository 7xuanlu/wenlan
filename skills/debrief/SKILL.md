---
name: debrief
description: >
  Alias for `/origin:handoff` — symmetric brief/debrief naming. Same
  behavior: end-of-session capture of decisions, lessons, gotchas, and
  open threads. Invoked as `/debrief`. Use when the user prefers the
  brief/debrief pair over brief/handoff.
allowed-tools: ["mcp__origin__capture"]
---

# /debrief

Alias for `/handoff`. Identical behavior — end-of-session memory capture.

## How to invoke

Run `/handoff` exactly. Same four steps:

1. Summarize the session: list decisions, lessons, gotchas, blockers, and
   pending threads.
2. For each item, call the `origin` MCP server's `capture` tool with one
   atomic statement per memory. Store directly — the daemon dedups against
   existing knowledge, so re-storing known facts is a no-op.

```
capture(content="<one decision / lesson / gotcha as a complete sentence>")
```

3. Only surface items to the user BEFORE storing if they meet one of these
   bars:
   - Contradicts an existing memory.
   - Marks a critical incident, irreversible action, or production change.
   - Durability is uncertain.

   Otherwise just store and report a one-line summary at the end:
   "Debrief: stored N captures, daemon dedup'd M as already-known."

## Why both /handoff and /debrief

Some users prefer the symmetric `/brief` (start) ↔ `/debrief` (end) pair.
Others prefer `/handoff` for the end-of-session connotation. Both work.
Identical underlying capture flow — pick whichever feels natural.

## When to use

- User says "wrapping up", "let's call it", "we're done", "let's debrief".
- Session about to close and useful state would otherwise be lost.

## When NOT to use

- Mid-flow capture during work → use `/capture` (single memory).
- Search / lookup → use `/recall`.
