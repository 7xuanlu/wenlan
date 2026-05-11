---
name: distill
description: >
  Synthesize wiki pages from related memories. One endpoint, one flow:
  daemon clusters and synthesizes what it can; agent finishes whatever
  the daemon couldn't (no LLM or cluster too big). Invoked as
  `/distill [target]`.
argument-hint: "[page_id_or_entity_or_domain]"
allowed-tools: ["mcp__plugin_origin_origin__recall", "mcp__plugin_origin_origin__distill", "Bash"]
---

# /distill

Force a distillation pass now. The daemon's background refinery runs
on its own clock; `/distill` is the explicit user-triggered pass.

## Single flow

One POST to the daemon. Response splits into:

- `pages_created` / `created_ids`: pages the daemon synthesized itself
  (only when daemon has an LLM).
- `pending`: clusters the daemon couldn't finish. The agent
  synthesizes each in this session and POSTs them back to `/api/pages`.

Trigger timing is the only thing that differs between the background
refinery and this skill. Code path is the same; daemon hands back
clusters when it can't synthesize; whoever called fills in the rest.

## Flow

### 1. Pick the scope

For bare `/distill`, infer a target from cwd:

```
Bash: top=$(git -C "$PWD" rev-parse --show-toplevel 2>/dev/null); \
      common=$(git -C "$PWD" rev-parse --git-common-dir 2>/dev/null); \
      if [ -n "$common" ]; then \
        case "$common" in /*) root=$(dirname "$common");; *) root=$(cd "$top" && cd "$(dirname "$common")" && pwd);; esac; \
        basename "$root"; \
      fi
```

- Output → use it (e.g. `origin`).
- Not a git repo → fall back to `basename "$PWD"`.
- Reserved keyword `deep` → no scope (global pass).

For `/distill <arg>` → forward `<arg>` to `target`.

### 2. Call the daemon

**TEMP** — using curl while MCP `distill` tool is still on the old npm
release. Once `origin-mcp` ships a version that returns the daemon's
JSON verbatim, swap this for `distill(target="<scope>")` via the MCP
tool.

```
Bash: curl -fsS -X POST http://127.0.0.1:7878/api/distill \
  -H 'Content-Type: application/json' \
  -d '{"target":"<scope>"}'
```

Parse the JSON response. Possible shapes:

```
{
  "pages_created": 0,
  "scoped": true,
  "created_ids": [],
  "pending": [
    { "source_ids": [...], "contents": [...], "entity_id": ...,
      "entity_name": ..., "domain": ..., "estimated_tokens": ... },
    ...
  ]
}
```

The route never invokes the daemon LLM. `created_ids` is always empty
when called from this skill; `pending` carries every cluster the
daemon found. The agent synthesizes them in this session — that's why
the LLM choice is consistent with how the user invoked the skill.

`unresolved` + `hint`: relay to user verbatim and stop.

### 3. Finish each `pending` cluster

For each cluster in `pending`:

- Title: short noun phrase (use `entity_name` if present, otherwise a
  short phrase summarizing the cluster).
- Summary: one sentence — the durable claim.
- Body: 3-7 paragraphs of wiki prose. Use `[[wikilinks]]`. Cite source
  ids inline with `(source: mem_XXX)`.
- Then POST to `/api/pages`:

```
Bash: curl -fsS -X POST http://127.0.0.1:7878/api/pages \
  -H 'Content-Type: application/json' \
  -d '{"title":"...","summary":"...","content":"...",
       "entity_id":"<cluster.entity_id or null>","domain":"<cluster.domain>",
       "source_memory_ids":[...]}'
```

The daemon's `handle_create_page` writes both the DB row and the md
file atomically. No second step needed.

### 4. Report terse

Synthesize every cluster the daemon returned — that's the whole pending
list. Then report once:

```
Distilled N page(s) from <total> memories in scope `<scope>`:
  - <Title>  (~/.origin/pages/<slug>.md)
  - <Title>  (~/.origin/pages/<slug>.md)
  ...
```

The daemon already filtered out clusters that don't qualify (too small,
too thin), so there is no "Skipped" section. If the pass produced
fewer pages than the user expected, it's because the daemon's
clustering thresholds are conservative — most memories sit alone
without enough nearby peers to form a cluster of 3+. Capture more on
the same topic to grow those latent clusters.

Rules:
- **Titles, not page ids.** Ids visually truncate; titles read clean.
- One line per synthesized page. No body in chat — `/read "<title>"`
  for that.
- `<total>` = sum of `source_ids.len()` across all returned clusters
  (the memories the daemon actually drew on, not the full DB).

## Auto-commit ~/.origin/

```
Bash: cd ~/.origin 2>/dev/null && [ -d .git ] && git add -A && \
      git -c user.name=Origin -c user.email=daemon@origin.local \
          commit --quiet -m "distill: <N> pages" \
          || true
```

Skip when no diff.

## When to use

- User says "distill", "synthesize", "rebuild the page on X".
- After a bulk import — daemon refinery handles this in the background;
  user can force a pass for immediate visibility.

## When NOT to use

- Trivial / one-off interactions. The background scheduler covers
  periodic refresh.
- Single memory write → daemon's post-ingest enrichment already
  covers it.

## Cost

Each cluster the agent synthesizes counts against this session's
tokens. Daemon-side clusters (when an LLM is present) cost daemon LLM
tokens instead (cents on API, seconds on-device). Either way, keep
cluster sizes reasonable — the daemon already enforces a per-cluster
token budget via its tuning config.
