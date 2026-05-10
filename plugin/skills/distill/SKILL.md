---
name: distill
description: >
  Trigger Origin's synthesis pass — clusters related memories into pages,
  surfaces patterns, and rebuilds the wiki view. Invoked as
  `/distill [page_id]`. Run on demand when the user wants the
  knowledge view refreshed; otherwise the daemon does this in the
  background.
argument-hint: "[page_id]"
allowed-tools: ["mcp__plugin_origin_origin__distill"]
---

# /distill

Run Origin's synthesis (distillation) pass. Without an arg, distills any
clusters with new sources. With a `page_id` arg, re-distills that specific
page from its current sources.

## How to invoke

Call the `origin` MCP server's `distill` tool. Without an arg, distills any
clusters with new sources. With `page_id`, re-distills that one page.

```
distill()                       # full pass
distill(page_id="<page_id>")    # single page
```

## What distillation does

- Clusters memories that share entities or topics into pages
- Synthesizes prose summaries from raw memories with citations
- Refreshes the wiki view when sources change
- Surfaces patterns across multiple memories (future: "Insight" type)

## When to use

- User says "distill", "synthesize", "rebuild the page on X", "refresh the
  knowledge view".
- After a bulk import — daemon refinery handles this automatically, but
  user can force a pass for immediate visibility.
- After editing many memories — re-distill affected pages.

## When NOT to use

- Daemon scheduler runs distillation periodically. Don't trigger
  redundantly during normal flow.
- Single memory write → daemon's post-ingest enrichment already runs;
  manual distill is over-eager.

## Cost

Distillation calls the LLM once per cluster. With on-device Qwen, expect
seconds per page. With API LLM, expect cents.
