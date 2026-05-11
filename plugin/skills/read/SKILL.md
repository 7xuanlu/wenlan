---
name: read
description: >
  Preview a distilled wiki page from inside Claude Code. Prints title,
  summary, source count, and the local md path. Full body lives on disk —
  open with the user's editor. Invoked as `/read <title_or_id>`.
argument-hint: "<title_or_id>"
allowed-tools: ["mcp__plugin_origin_origin__get_page", "Bash"]
---

# /read

Surface a page's identity so the user can decide whether to open it.
`/read` is **preview, not full text**. The body is on disk; preview
keeps chat scannable, dodges Bash output truncation, and respects the
"md is canonical, viewer is the user's editor" model.

## How to invoke

Two shapes accepted:

1. **Page id** (starts with `page_` or `concept_`) → direct fetch.
2. **Title or freeform word** → search, pick best match, fetch.

Both end with the same preview block.

### 1. Direct id

Call the MCP tool to fetch the page:

```
get_page(page_id="<id>")
```

The response is a JSON object wrapping `{ "page": {...} }`. Read
`title`, `summary`, `domain`, and `source_memory_ids` off the page,
then look up the md filename in `~/.origin/pages/.origin/state.json`:

```
Bash: python3 -c '
import json, os, sys
state_path = os.path.expanduser("~/.origin/pages/.origin/state.json")
pid = "<id>"
filename = None
try:
    with open(state_path) as f:
        filename = json.load(f).get("pages", {}).get(pid, {}).get("file")
except FileNotFoundError:
    pass
print(f"~/.origin/pages/{filename}" if filename else "(no md projection on disk)")
'
```

Combine the page fields with the resolved md path and emit the preview
block.

### 2. Title or freeform word

Search first, then fetch the top hit by id and run the same preview
block. Always resolve through `/api/pages/search` — never derive a slug
client-side (skill heuristics drift from the canonical `slugify()` on
apostrophes and punctuation).

```
Bash: id=$(curl -fsS -X POST http://127.0.0.1:7878/api/pages/search \
            -H 'Content-Type: application/json' \
            -d "{\"query\":\"<arg>\",\"limit\":1}" \
            | python3 -c "import json,sys; \
                d=json.load(sys.stdin); \
                hits=d.get('pages') or d.get('results') or []; \
                print(hits[0]['id'] if hits else '', end='')")
      [ -z "$id" ] && echo "no page found matching <arg> — try /distill <arg>" && exit 0
      # then call get_page with the resolved id and emit the preview block
```

If `$id` is empty after search, tell the user "no page found matching
`<arg>` — try `/distill <arg>` to create one" and stop.

## Output shape

Always print exactly these five lines (no body):

```
Title:    <title>
Summary:  <one sentence>
Sources:  <N> memories
Domain:   <domain or (none)>
Open:     ~/.origin/pages/<slug>.md
```

Don't paraphrase the title, don't trim the summary, don't decorate the
preview. The block is one screen, predictable, easy to skim.

If the user wants the full body, they open the md file in their editor
(Obsidian, VS Code, glow, bat, …). The plugin doesn't render markdown
better than their tools do.

## When to use

- User asks "show me the page on X", "what's in <title>", "preview that".
- After `/distill` finishes, follow up with `/read "<title>"` (titles
  survive any rendering surface; ids may visually truncate).

## When NOT to use

- Raw memory lookups → use `/recall`.
- Listing all pages → `curl /api/pages/recent` or
  `ls ~/.origin/pages/`.
- Reading the full body → open the md file in the user's editor.
