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

### 3. Overlap check (skip duplicates)

Before synthesizing anything, fetch existing pages and check each
cluster against them. Daemon doesn't dedupe at the route level — the
old Jaccard check used to run inside the daemon's synth path which
this route bypasses. Without this step, repeated `/distill` calls
re-create the same pages.

```
Bash: curl -fsS "http://127.0.0.1:7878/api/pages?limit=100"
```

For each `cluster` in `pending`, compute
`overlap = |cluster.source_ids ∩ page.source_memory_ids| / |cluster.source_ids ∪ page.source_memory_ids|`
against every existing page. If `overlap > 0.5` for any page, mark
the cluster as a duplicate of that page and skip it. Save the existing
page title and overlap count for the report.

### 4. Synthesize each non-duplicate cluster

For each remaining cluster:

- Title: short noun phrase. Prefer `cluster.entity_name` when it's
  specific; if the entity name is generic (e.g. the project name
  itself), derive a more specific title from the first memory's
  content.
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

### 5. Report terse

Two output shapes — one for "actually distilled something", one for
"nothing new, scope is up to date".

**If at least one new page synthesized:**

```
Distilled N page(s) from <total> memories in scope `<scope>`:
  - <Title>  (~/.origin/pages/<slug>.md)
  ...
```

Add a one-line note only when at least one cluster was also skipped
as a duplicate, e.g.: `(M cluster(s) already covered by existing
pages.)`

**If zero new pages and every cluster was a duplicate:**

```
Scope `<scope>` is up to date — N existing pages already cover the
candidate clusters. Capture more on new topics to grow latent
clusters.
```

That's the whole output. Do **not** list the matched existing pages —
the user already has them; restating five titles they own makes
"nothing happened" look like a failure.

**If `pending` was empty in the daemon response:**

```
Scope `<scope>` has no candidate clusters yet — capture a few more
related memories to bootstrap distillation.
```

Rules:
- **Titles, not page ids.** Ids visually truncate; titles read clean.
- One line per synthesized page. No body in chat — `/read "<title>"`
  for that.
- `<total>` = sum of `source_ids.len()` across the clusters that were
  actually synthesized (not the duplicates).
- If the pass produced fewer pages than expected, it's the clustering
  thresholds. Most memories sit alone without enough peers to form a
  cluster of 3+. Capture more on the same topic to grow them.

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
