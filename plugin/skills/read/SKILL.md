---
name: read
description: >
  Read a distilled wiki page inside Claude Code. Resolves a page by id
  or title and prints its markdown body inline. Invoked as
  `/read <page_id_or_title>`.
argument-hint: "<page_id_or_title>"
allowed-tools: ["Bash"]
---

# /read

Print a wiki page from `~/.origin/pages/` inline in the chat. The user
shouldn't need to open Finder or Obsidian just to see what was
synthesized.

## How to invoke

The skill never derives a slug or guesses a filename — slug logic lives
in the daemon (`KnowledgeWriter::slugify`) and any duplicate logic here
would drift on apostrophes, run-of-punctuation, and unicode normalization.
Always resolve through the API.

Two shapes:

1. **Page id** (starts with `page_` or `concept_`) → direct fetch.
2. **Title fragment / freeform word** → search, pick best match, fetch.

### 1. Direct id

```
Bash: curl -fsS http://127.0.0.1:7878/api/pages/<id> \
  | python3 -c "import json,sys; print(json.load(sys.stdin)['page']['content'])"
```

### 2. Title fragment / freeform word

Search first, then fetch the top hit by id:

```
Bash: id=$(curl -fsS -X POST http://127.0.0.1:7878/api/pages/search \
            -H 'Content-Type: application/json' \
            -d "{\"query\":\"<arg>\",\"limit\":1}" \
            | python3 -c "import json,sys; \
                d=json.load(sys.stdin); \
                hits=d.get('pages') or d.get('results') or []; \
                print(hits[0]['id'] if hits else '', end='')"); \
      [ -n "$id" ] && curl -fsS "http://127.0.0.1:7878/api/pages/$id" \
        | python3 -c "import json,sys; print(json.load(sys.stdin)['page']['content'])"
```

If `$id` is empty, tell the user "no page found matching `<arg>`" and
suggest `/distill <arg>` to create one.

## Output

Print the page content as a fenced markdown block so Claude Code
renders the source view. Prepend the page id and title:

```
Page: <title> (<id>)

<content>
```

Keep the body unchanged — wikilinks, citations, and headers should
survive verbatim.

## Auto-commit

None. `/read` is read-only.

## When to use

- User says "show me the page on X", "what's in <title>", "preview that".
- After `/distill` returns a new page id, follow up with `/read <id>`
  if the user asks to see it again later.
- Anytime the user wants to inspect a synthesized page without
  switching applications.

## When NOT to use

- Raw memory lookups → use `/recall` (returns memory chunks, not
  full pages).
- Listing recent pages → `curl /api/pages/recent` directly.
- Edits → `/read` is read-only; edit the file in `~/.origin/pages/`
  and the daemon will reconcile (page edit watcher: pending).
