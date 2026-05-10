---
name: handoff
description: >
  Session-end handoff. Capture decisions, lessons, gotchas, and open
  threads so the next session walks in primed. Invoked as
  `/handoff`.
allowed-tools: ["mcp__origin__capture"]
---

# /handoff

End-of-session debrief. Stores what was decided, what was learned, and what
remains open as a single coherent handoff so the next session boots with
context.

## How to invoke

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
   - Contradicts an existing memory (recall returned a conflicting fact).
   - Marks a critical incident, irreversible action, or production change.
   - You are uncertain whether the item is durable vs transient.

   Otherwise just store and report a one-line summary at the end:
   "Handoff: stored N captures, daemon dedup'd M as already-known."

## What to store at handoff

- Decisions made (with the WHY)
- Lessons learned (gotchas, things that broke and how)
- Open threads (what was started, what remains)
- Corrections (things the user pushed back on)
- Preferences observed (tools, patterns, vocabulary)

## What NOT to store

- Tool output, file paths, command results (re-derivable)
- Single-word acknowledgments
- Transient task state still in flight (use `/capture` mid-flow
  instead)

## When to use

- User says "wrapping up", "let's call it", "we're done".
- Session about to close and useful state would otherwise be lost.

## When NOT to use

- Mid-flow capture during work → use `/capture` (single memory).
- Search / lookup → use `/recall`.
