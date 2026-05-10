---
name: recall
description: >
  Search Origin's local memory by query. Targeted lookup, not orientation.
  Invoked as `/recall <query>`. Use when the user asks "do you
  remember", "what do you know about", "look up".
argument-hint: "<query>"
allowed-tools: ["mcp__plugin_origin_origin__recall"]
---

# /recall

Search Origin's memory by natural-language query. Returns matching memories
ranked by hybrid vector + FTS search.

## How to invoke

Call the `origin` MCP server's `recall` tool with the user's query.

```
recall(query="<args>", domain=<inferred>, memory_type=<inferred>)
```

**Infer scope yourself — don't ask the user.** The user types `/recall <query>`
and nothing else. You attach `domain` and `memory_type` based on:

- `domain`: current working directory (e.g. `~/Repos/origin/...` → `"origin"`),
  the topic being discussed, or whatever space was mentioned in recent turns.
  Omit if no clear signal.
- `memory_type`: only when the query itself names a type ("decision on X",
  "lesson about Y", "preference for Z"). Otherwise omit and let hybrid
  search rank.
- `limit`: default 10. Use 3-5 for quick lookups, 10-20 for exploration.

## When to use

- "What did I say about X?"
- "Do you remember the decision on Y?"
- Need a specific fact before continuing.

## When NOT to use

- Broad session orientation → use `/brief` instead.
- Storing a new memory → use `/capture`.

## Hint: write specific queries

"Alice database preference" finds more than "database stuff". The semantic
matcher rewards specificity. If too many results return, add filters rather
than making the query longer.
