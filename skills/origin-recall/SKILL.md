---
name: origin-recall
description: >
  Search Origin's local memory. Use when the user asks "do you remember", "what
  do you know about", "look up", or when you need a specific fact before
  acting. Wraps the origin MCP `recall` tool. Invoked as `/origin-recall <query>`.
---

# /origin-recall

Search Origin's memory by query. Returns matching memories ranked by hybrid
vector + FTS search.

## How to invoke

Call the `origin` MCP server's `recall` tool with the user's query. If the
user passed extra args after the slash, treat the whole args string as the
search query.

```
recall(query="<the args string>")
```

## When to use

- "What did I say about X?"
- "Do you remember the decision on Y?"
- Need a specific fact before continuing.

## When NOT to use

- Broad session orientation → use `/origin-context` instead.
- Storing a new memory → use `/origin-store`.
- Daemon health check → use `/origin-status`.
