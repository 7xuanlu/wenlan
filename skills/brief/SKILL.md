---
name: brief
description: >
  Session-start briefing from Origin. Loads identity, preferences, and
  topic-relevant memories so the agent walks in with context. Invoked as
  `/origin:brief [topic]`. Call FIRST at session start, before any other
  Origin verb.
---

# /origin:brief

Pull a curated session brief from Origin: who the user is, what they prefer,
and what's relevant to the current topic.

## How to invoke

Call the `origin` MCP server's `context` tool. If the user passed a topic
argument, pass it through; otherwise call with no topic for general
orientation.

```
context(topic="<args or null>")
```

## When to use

- Session start — call BEFORE any other Origin tool.
- Major topic shift mid-session.
- User says "catch me up", "what's the background on X", "remind me about Y".
- Mid-session check-in to confirm assumptions.

## When NOT to use

- Specific factual lookup → use `/origin:recall` (more targeted).
- Storing a new memory → use `/origin:capture`.
- End of session → use `/origin:handoff`.

## How to use the result

Model how the user thinks. Their preferences, corrections, and past decisions
tell you how they want to be helped, not just what they already know. Don't
just look things up: adjust your behavior.
