---
name: origin-context
description: >
  Load session context from Origin — identity, preferences, goals, and
  topic-relevant memories. Wraps the origin MCP `context` tool. Invoked as
  `/origin-context [topic]`. Call FIRST at the start of every session before
  doing anything else.
---

# /origin-context

Load curated session context: who the user is, what they prefer, and any
memories relevant to the current topic.

## How to invoke

Call the `origin` MCP server's `context` tool. If the user passed args, use
them as the topic hint; otherwise call with no topic for general orientation.

```
context(topic="<the args string or null>")
```

## When to use

- Session start — call this BEFORE any other Origin tool.
- Major topic shift mid-session.
- User says "catch me up", "what's the background on X", "remind me about Y".

## When NOT to use

- Specific factual lookup → use `/origin-recall` (more targeted).
- Storing → use `/origin-store`.

## How to use the result

Model how the user thinks — their preferences, corrections, and past decisions
tell you how they want to be helped, not just what they already know. Don't
just look things up: adjust your behavior.
