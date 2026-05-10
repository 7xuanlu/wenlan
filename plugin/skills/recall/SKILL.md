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
ranked by hybrid vector + FTS search, then re-ordered by the agent if it
helps.

## Two phases

When the daemon has an LLM it can rerank and expand server-side. In Basic
Memory mode it cannot. The skill always does **agent-side expansion and
rerank** itself — cheap, makes results good in both modes.

### Phase 1 — expand the query (agent-side)

Before calling `recall`, rewrite the user's query into a more
search-friendly form:

- Replace pronouns with the referent ("it" → the actual thing).
- Expand abbreviations the embedder is unlikely to know.
- Add the obvious synonym when the original term is too narrow (e.g.
  "auth" → "auth OR authentication").

Don't over-expand. If the query is already specific, leave it alone.
When in doubt, issue two recall calls — one with the original query and
one with the expanded form — and merge the results in Phase 3.

### Phase 2 — call the MCP tool

```
recall(query="<expanded query>", domain=<inferred>, memory_type=<inferred>)
```

Inferences (do not ask the user):

- `domain`: current working directory (e.g. `~/Repos/origin/...` → `"origin"`),
  the topic being discussed, or whatever space was mentioned in recent turns.
  Omit if no clear signal.
- `memory_type`: only when the query itself names a type ("decision on X",
  "lesson about Y", "preference for Z"). Otherwise omit and let hybrid
  search rank.
- `limit`: default 10. Use 3-5 for quick lookups, 10-20 for exploration.

### Phase 3 — rerank (agent-side)

The daemon returns hits ranked by hybrid search. That ranking is good but
not perfect — it doesn't know the user's exact intent.

Re-read the returned memories against the *original* query. Promote the
ones that directly answer the question; demote ones that just share
keywords. If you issued multiple recall calls in Phase 1, merge and
de-dup by `source_id` before reranking.

Show the user the top 3-5 reranked hits. Surface the rest only if asked.

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
