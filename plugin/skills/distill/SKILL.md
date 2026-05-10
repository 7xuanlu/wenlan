---
name: distill
description: >
  Synthesize wiki pages from related memories. With an LLM daemon, calls
  the MCP `distill` tool. Without one (Basic Memory mode), the agent reads
  the cluster, writes the page, and posts it back. Invoked as
  `/distill [topic_or_page_id]`.
argument-hint: "[topic_or_page_id]"
allowed-tools: ["mcp__plugin_origin_origin__distill", "mcp__plugin_origin_origin__recall", "Bash"]
---

# /distill

Synthesize a wiki page from related memories. Two paths:

1. **LLM-equipped daemon** → MCP tool `distill` does the clustering and
   synthesis server-side.
2. **Basic Memory** (no LLM in daemon) → the agent does the synthesis
   itself: read the cluster via HTTP, write the page, POST it back.

The agent picks the path. The MCP tool returns an error or empty result
when no LLM is available — fall through to Path B then.

## How to invoke

### Try the MCP tool first

```
distill()                       # full pass — when user types bare /distill
distill(page_id="<page_id>")    # single page re-distill
```

If the response indicates LLM unavailable, or the daemon is in Basic
Memory mode, switch to the agent-driven path below.

### Agent-driven synthesis (Basic Memory)

Decide which cluster to distill:

- User typed bare `/distill` → pick the topic with the most recent unsynthesized memories (use `recall` with the current domain to surface candidates).
- User typed `/distill <topic>` → use that topic.
- User typed `/distill <page_id>` (starts with `page_` or `concept_`) → re-distill that page from its current sources.

Fetch the source memories:

```
Bash: curl -fsS -X POST http://127.0.0.1:7878/api/memory/list \
  -H 'Content-Type: application/json' \
  -d '{"domain":"<topic>","limit":50}'
```

Read the result. Cluster by shared entities or sub-topic. Pick one
cluster per page.

Write the page in wiki-prose style:

- Title: short noun phrase (e.g. "Origin daemon architecture").
- Summary: one sentence — the durable claim the page supports.
- Body: 3-8 paragraphs of encyclopedia-style prose. Use `[[wikilinks]]`
  to reference other pages or entities. Cite source memory ids inline
  with `(source: mem_XXX)` where the claim came from.
- Keep it durable — write what would still be true in six months, not
  the current state of in-progress work.

POST the page back:

```
Bash: curl -fsS -X POST http://127.0.0.1:7878/api/pages \
  -H 'Content-Type: application/json' \
  -d '{"title":"<Title>","content":"<page body>","summary":"<one line>",
       "entity_id":"<primary_entity_id_or_null>","domain":"<topic>",
       "source_memory_ids":["mem_X","mem_Y","mem_Z"]}'
```

Repeat for each cluster, one POST per page. Report the page ids back to
the user.

## Auto-commit ~/.origin/

After distillation (either path), snapshot page changes:

```
Bash: cd ~/.origin 2>/dev/null && [ -d .git ] && git add -A && \
      git -c user.name=Origin -c user.email=daemon@origin.local \
          commit --quiet -m "distill: <N> pages" \
          || true
```

Skip the commit if no diff — `git commit` with empty staging fails.

## When to use

- User says "distill", "synthesize", "rebuild the page on X", "refresh the
  knowledge view".
- After a bulk import — when the daemon is LLM-equipped its refinery
  handles this automatically; in Basic Memory mode the user must trigger
  it.
- After editing many memories — re-distill affected pages.

## When NOT to use

- LLM-equipped daemon already runs distillation periodically. Don't
  trigger redundantly during normal flow.
- Single memory write → daemon's post-ingest enrichment already runs
  (when LLM present); manual distill is over-eager.

## Cost

LLM path: one LLM call per cluster (seconds on-device, cents on API).
Agent path: counts against the current Claude session's tokens — keep
clusters small (≤ 20 source memories per page) to control cost.
