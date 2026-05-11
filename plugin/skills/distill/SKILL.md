---
name: distill
description: >
  Synthesize wiki pages from related memories. Daemon does the work; the
  skill just forwards the user's target. Invoked as
  `/distill [page_id_or_entity_or_domain]`.
argument-hint: "[page_id_or_entity_or_domain]"
allowed-tools: ["mcp__plugin_origin_origin__distill", "mcp__plugin_origin_origin__recall", "Bash"]
---

# /distill

Force a distillation pass now. The daemon's refinery already runs
distillation in the background — `/distill` is for when the user wants
the wiki view refreshed immediately. Pages emerge automatically; the
user never has to name topics or manage clusters.

## How to invoke

Default is **scoped to the current repo**, not a global pass. Global
distillation runs continuously in the daemon's background refinery;
`/distill` is for an on-demand refresh of what the user is working on
right now.

```
distill(target="<inferred-domain>")    # bare /distill — agent infers cwd domain
distill(target="<arg>")                # /distill <arg> — user supplies target
distill()                              # /distill deep — global deep pass (no target)
```

### Resolving the target

For bare `/distill`, the agent infers a target from cwd:

```
Bash: git -C "$PWD" rev-parse --show-toplevel 2>/dev/null
```

- If output is a path → use its basename (e.g. `~/Repos/origin/...` → `origin`).
- If not a git repo → fall back to the cwd basename.
- If even that is empty → call `distill()` with no `target` (full pass, slow).

For `/distill <arg>`:

- **`/distill deep`** → reserved keyword for the global deep pass. Call
  `distill()` with **no** `target`. This is the slow Karpathy-style "let
  the daemon cluster everything" sweep — runs the deep refinement step
  too. Use sparingly; the background scheduler covers it on a clock.
- **`/distill <anything else>`** → forward unchanged as `target`.

If a user has an entity or domain literally named `deep`, they need to
disambiguate (e.g. quote it or use the exact entity id). That's a price
worth paying for a single short verb.

### How the daemon resolves `target`

- `page_*` / `concept_*` → re-distill that single page from its sources.
- exact entity name → scope clustering to that entity.
- exact domain value → scope to that domain.
- anything else → daemon returns `unresolved` + hint; relay the hint to
  the user, do not retry blindly.

When `target` is supplied the daemon skips the deep refinement sweep —
only the scoped clusters are touched. Full deep passes happen in the
background scheduler or when the user explicitly invokes a global
pass (no target).

## Basic Memory fallback (no daemon LLM)

If the MCP `distill` response indicates no LLM is available (Basic
Memory mode), the agent does the synthesis itself: fetch a cluster,
write a page, post it back.

Fetch candidate memories via MCP `recall`:

```
recall(query="<topic>", domain="<topic>", limit=50)
```

Read the result. Cluster by shared entities or sub-topic. Write the
page in wiki-prose style:

- Title: short noun phrase (e.g. "Origin daemon architecture").
- Summary: one sentence — the durable claim the page supports.
- Body: 3-8 paragraphs of encyclopedia-style prose. Use `[[wikilinks]]`
  to reference other pages or entities. Cite source memory ids inline
  with `(source: mem_XXX)`.
- Durable: write what would still be true in six months, not the
  current state of in-progress work.

POST the page back. MCP `distill` doesn't accept an agent-written page
body, so fall through to the HTTP page endpoint:

```
Bash: curl -fsS -X POST http://127.0.0.1:7878/api/pages \
  -H 'Content-Type: application/json' \
  -d '{"title":"<Title>","content":"<page body>","summary":"<one line>",
       "entity_id":"<primary_entity_id_or_null>","domain":"<topic>",
       "source_memory_ids":["mem_X","mem_Y","mem_Z"]}'
```

## Auto-commit ~/.origin/

After distillation, snapshot page changes:

```
Bash: cd ~/.origin 2>/dev/null && [ -d .git ] && git add -A && \
      git -c user.name=Origin -c user.email=daemon@origin.local \
          commit --quiet -m "distill: <N> pages" \
          || true
```

Skip the commit if no diff — `git commit` with empty staging fails.

## When to use

- User says "distill", "synthesize", "rebuild the page on X", "refresh
  the knowledge view".
- After bulk import — daemon refinery handles this in the background,
  but user can force a pass for immediate visibility.

## When NOT to use

- LLM-equipped daemon already runs distillation periodically. Don't
  trigger redundantly during normal flow.
- Single memory write → daemon's post-ingest enrichment already covers
  it; manual distill is over-eager.

## Cost

LLM path: one LLM call per cluster (seconds on-device, cents on API).
Agent path: counts against the current Claude session's tokens — keep
clusters small (≤ 20 source memories per page) to control cost.
