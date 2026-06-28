---
name: pages
description: >
  Browse and preview distilled wiki pages from inside Claude Code. With no
  argument, lists recent pages in a native picker; with a query, searches by
  title. Pick one to see its identity (title, summary, sources, links) and the
  local md path. Full body lives on disk — open with the user's editor.
  Invoked as `/pages [query]`.
argument-hint: "[query]"
allowed-tools: ["mcp__plugin_wenlan_wenlan__list_pages_recent", "mcp__plugin_wenlan_wenlan__search_pages", "mcp__plugin_wenlan_wenlan__get_page", "mcp__plugin_wenlan_wenlan__get_page_links", "AskUserQuestion", "Bash"]
---

# /pages

Browse the wiki, pick a page, preview it. `/pages` is **navigation +
preview, not full text**. The body is on disk; chat stays scannable.

Two halves: a **picker** (find the page) and a **preview block** (show
its identity). Never dump a page body into chat — that's what the
editor and the `Open:` path are for.

## 1. Build the candidate list

Resolve a short list of pages, metadata only (id + title + snippet) —
never the body.

- **No argument** → recent pages:

  ```
  list_pages_recent(limit=10)
  ```

  Returns a JSON array of activity items: `{ id, title, snippet,
  timestamp_ms, badge }`. No `content` field — that's the point.

- **Argument given** → search by title/topic:

  ```
  search_pages(query="<arg>", limit=10)
  ```

  Returns `{ "pages": [...] }`. The MCP tool already renders each hit
  as a one-line metadata row (`<id>  <title>  — <summary>`); it does
  **not** dump bodies. Read the `id` and `title` per row.

If the list is empty, tell the user "no pages found — run `/distill
[topic]` to synthesize one" and stop. Never derive a slug client-side
to guess a path; the daemon owns slugs.

## 2. Present the native picker

Show the candidates with the **native multiple-choice picker** (the
arrow-key prompt, same UI as a permission ask) via `AskUserQuestion` —
not a prose list. The user scans options, not paragraphs.

Rules:

- One option per page. **Label** = the page title (trim to a few words
  if long). **Description** = the snippet/summary + relative age
  (e.g. "mutex deadlock notes · 2d ago").
- Order by recency/relevance as returned; mark the **first** option
  recommended.
- The picker caps at 4 options. When more pages exist, show the top 3
  and **always** add a final option **"Search by title…"** so every
  page stays reachable despite the cap. When the user picks it, prompt
  for a query and re-run step 1 with `search_pages`.
- With ≤3 pages, list them all and still append "Search by title…".

The search escape is mandatory — it's what keeps the 4-option cap from
hiding pages.

## 3. Preview the picked page

Once the user selects a page, fetch it by id and emit the preview
block. Resolve the on-disk md path by id — never slugify client-side
(skill heuristics drift from the canonical `slugify()` on apostrophes
and punctuation).

```
get_page(page_id="<id>")
```

The response wraps `{ "page": {...} }`. Read `title`, `summary`,
`space`, `version`, `source_memory_ids`, `user_edited`, `stale_reason`,
and the edit fields off the page. Then look up the md filename in
`~/.wenlan/pages/.wenlan/state.json`:

```
Bash: python3 -c '
import json, os, sys
state_path = os.path.expanduser("~/.wenlan/pages/.wenlan/state.json")
pid = sys.argv[1]
filename = None
try:
    with open(state_path) as f:
        filename = json.load(f).get("pages", {}).get(pid, {}).get("file")
except FileNotFoundError:
    pass
print(f"~/.wenlan/pages/{filename}" if filename else "(no md projection on disk)")
' "<id>"
```

### Output shape

Always print exactly these lines (no body):

```
Title:    <title>
Version:  v<N> — <last_edited_by> <relative_time> (<last_delta_summary>)
Summary:  <one sentence>
Sources:  <N> memories
Space:    <space or (none)>
Links:    <N inbound, M outbound (<K> broken)>
Open:     ~/.wenlan/pages/<slug>.md
⚠ Stale: <stale_reason> — run /distill to refresh
```

**Lock banner rule:**

When `user_edited == true`, prepend this line as the very first line of
the rendered output, before the title:

```
🔒 You've edited this page. Auto-refresh paused. `/distill rebuild <page-id>` to unlock.
```

Substitute `<page-id>` with the actual `page.id`. When `user_edited` is
false or absent, omit this line. The lock means daemon distill cycles
will not auto-rewrite this page's prose from sources — edits stay until
the user runs `/distill rebuild` to wipe and regenerate.

**Version line rules:**

- Always show `v<N>`. When `version` is null or missing, omit the line.
- Append ` — <last_edited_by>` when populated (e.g. `re_distill`, `user`, `agent`).
- Append relative time when `last_edited_at` is set (e.g. `2h ago`, `3d ago`).
- Append `(<last_delta_summary>)` when the field is non-empty.
- Examples:
  - `v1 — synthesized 4h ago`
  - `v4 — re_distill 2h ago (+mem_xyz, +250 chars)`
  - `v3 — user 1d ago`

**Stale warning rule:**

- Emit the `⚠ Stale:` line only when `stale_reason` is non-null/non-empty.
- Render `stale_reason` verbatim (values like `source_updated`,
  `new_memories` are human-readable enough).

**Links line rules:** Call `get_page_links(page_id="<id>")` right after
`get_page` and count:

- inbound = `len(inbound)`
- outbound = `len(outbound)`
- broken = outbound entries where `target_page_id` is null

Omit the parenthetical when broken is zero. Drop the line entirely if
inbound + outbound is zero.

Don't paraphrase the title, don't trim the summary, don't decorate the
preview. The block is one screen, predictable, easy to skim.

If the user wants the full body, they open the md file in their editor
(Obsidian, VS Code, glow, bat, …). The plugin doesn't render markdown
better than their tools do.

## Shortcuts

- `/pages <id>` where `<arg>` starts with `page_` (or legacy `concept_`)
  → skip the picker, fetch that page directly, emit the preview block.
- `/pages` with a query that resolves to exactly one page → skip the
  picker, preview it directly.

## When to use

- User asks "show me the page on X", "what pages do I have", "browse my
  wiki", "preview that page".
- After `/distill` finishes, `/pages "<title>"` to inspect a changed page.

## When NOT to use

- Raw memory lookups → use `/recall`.
- Reading the full body → open the md file in the user's editor, or
  `ls ~/.wenlan/pages/` to see them all.
