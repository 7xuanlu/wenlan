---
name: distill
description: >
  Synthesize wiki pages from related memories. One endpoint, one flow:
  daemon clusters and synthesizes what it can; agent finishes whatever
  the daemon couldn't (no LLM or cluster too big). Invoked as
  `/distill [target]`.
argument-hint: "[page_id_or_entity_or_domain]"
allowed-tools: ["mcp__plugin_origin_origin__recall", "mcp__plugin_origin_origin__distill", "mcp__plugin_origin_origin__create_page", "mcp__plugin_origin_origin__delete_page", "Bash"]
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

### 2. Call the MCP tool

```
distill(target="<scope>")
```

The tool returns the daemon's full JSON payload as text. Parse it as
JSON. Possible shapes:

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

### 3. Synthesize each `pending` cluster

The daemon route filters out clusters fully covered by an existing
page (subset or Jaccard ≥ 0.8). What remains is either:

- A **brand-new cluster** (no existing page) → create a new page.
- A **refresh candidate** (`existing_page_id` is set) → the cluster
  has new memories beyond what's in the matched page. The agent has
  LLM access, so the right move is to refresh the existing page in
  the same pass.

Cluster shape:

```
pending: [
  {
    source_ids, contents, entity_id, entity_name, domain,
    estimated_tokens,
    existing_page_id?, existing_page_title?, new_memory_count?
  },
  ...
]
```

For each cluster, first run a **coherence check** before synthesizing:

- Skim every memory in `cluster.contents`.
- If the cluster has ≥ ~4 memories and the topics scatter (entity
  shared but the memories cover unrelated sub-topics — e.g. all tagged
  `Origin` but spanning RwLock bugs, schema choices, onboarding UI,
  migrations, and CSS), the cluster is **incoherent**. Skip
  synthesizing it. Record it for the report under "Skipped (low
  coherence)" with the existing page title (if refresh) or a short
  topic hint (if new).
- Coherent cluster (memories share an actual topic, not just an entity
  tag) → proceed to synthesis.

The coherence judgement is something only the agent can do — it needs
to read the prose. Daemon clustering is heuristic; agent is the
final filter against producing a grab-bag page.

For each coherent cluster:

- Title: short noun phrase. Use `existing_page_title` when refreshing
  unless the new memories materially change the topic. For new
  clusters: `cluster.entity_name` if specific, otherwise derive from
  the first memory's content.
- Summary: one sentence — the durable claim.
- Body: 3-7 paragraphs of wiki prose. Use `[[wikilinks]]`. Cite source
  ids inline with `(source: mem_XXX)`.

**New cluster** (no `existing_page_id`) — call the MCP tool:

```
create_page(title="...", summary="...", content="...",
            entity_id="<cluster.entity_id or omit>",
            domain="<cluster.domain>",
            source_memory_ids=[...])
```

**Refresh candidate** (`existing_page_id` is set) — replace the old
page with the refreshed one via delete then create.

⚠ The two calls are NOT atomic as a pair. `delete_page` cleans DB + md
atomically, and `create_page` writes the new pair atomically, but a
daemon restart or network blip between them leaves the page gone with
no replacement. Recovery is simple: re-run `/distill` — clustering
will rediscover the same memories and the flow restarts. Note that
the new page gets a fresh page id; any external reference to the old
id breaks. A proper `PUT /api/pages/{id}` route would close both gaps
but is tracked separately.

```
delete_page(page_id="<existing_page_id>")
create_page(title="...", summary="...", content="...",
            source_memory_ids=cluster.source_ids, ...)
```

### 4. Report terse

Three output shapes. Pick the one that matches what happened.

**If `pending` is empty (every cluster already fully covered):**

```
Scope `<scope>` is up to date — no new memories to distill.
```

**If at least one cluster was synthesized:**

```
Distilled N page(s) from <total> memories in scope `<scope>`:
  - <Title>  (~/.origin/pages/<slug>.md)
  - <Title>  (~/.origin/pages/<slug>.md, refreshed)
  ...
```

Tag refreshed pages with `, refreshed` so the user can tell which
replaced an existing page vs which are brand new.

**If at least one cluster was skipped on the coherence check:**

```
Skipped M cluster(s) — low coherence (memories share entity but
topics scatter; would produce a grab-bag page):
  - "<existing_page_title or topic hint>"  (<N> memories)
  ...
```

When both happened in the same pass — some synthesized, some skipped
— emit both blocks back-to-back.

When the only outcome is skipped clusters (and `pending` was
non-empty), still emit the Skipped block. Do **not** report "up to
date" in that case — the scope isn't up to date, the candidates were
just too low quality.

Rules:
- **Titles, not page ids.** Ids visually truncate; titles read clean.
- One line per synthesized page. No body in chat — `/read "<title>"`
  for that.
- `<total>` = sum of `source_ids.len()` across the clusters that were
  actually synthesized.
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
