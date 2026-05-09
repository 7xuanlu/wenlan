---
name: recall
description: >
  Search Origin's local memory by query. Targeted lookup, not orientation.
  Invoked as `/origin:recall <query>`. Use when the user asks "do you
  remember", "what do you know about", "look up".
---

# /origin:recall

Search Origin's memory by natural-language query. Returns matching memories
ranked by hybrid vector + FTS search.

## How to invoke

Call the `origin` MCP server's `recall` tool with the user's query.

```
recall(query="<args>")
```

Optional filters:
- `memory_type` — narrow to "profile", "knowledge", or precise types
  (identity, preference, fact, decision, lesson, gotcha)
- `domain` — narrow to a topic scope (e.g. "rust", "work", "origin")
- `limit` — default 10. Use 3-5 for quick lookups, 10-20 for exploration.

## When to use

- "What did I say about X?"
- "Do you remember the decision on Y?"
- Need a specific fact before continuing.

## When NOT to use

- Broad session orientation → use `/origin:brief` instead.
- Storing a new memory → use `/origin:capture`.

## Hint: write specific queries

"Alice database preference" finds more than "database stuff". The semantic
matcher rewards specificity. If too many results return, add filters rather
than making the query longer.
