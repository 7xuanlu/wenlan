---
name: pages
description: >
  Browse distilled wiki pages from inside Claude Code and open one. With no
  argument, lists recent pages in a native picker; with a query, searches by
  title. Picking one opens its markdown file in your default editor; on a
  headless terminal it prints the prose. The body is never rendered as a
  metadata card. Invoked as `/pages [query]`.
argument-hint: "[query]"
allowed-tools: ["mcp__plugin_wenlan_wenlan__list_pages_recent", "mcp__plugin_wenlan_wenlan__search_pages", "AskUserQuestion", "Bash"]
---

# /pages

Find a distilled page and **open it**. `/pages` is a finder, not a viewer —
its job is to resolve which page you mean and hand the markdown to your editor.
Pages live as `.md` files in `~/.wenlan/pages/`; your editor renders and
searches them better than chat can.

The flow: **resolve candidates → pick (native picker) → open the file**. No
metadata card, no body dumped into chat.

## 1. Resolve candidates (metadata only)

Never read the body here — only id + title + snippet.

- **`/pages <id>`** where the arg starts with `page_` (or legacy `concept_`)
  → skip the picker, go straight to step 3 with that id.

- **`/pages <query>`** → search by title/topic (daemon does real semantic search):

  ```
  search_pages(query="<arg>", limit=10)
  ```

  Returns `{ "pages": [...] }`, rendered one metadata row per hit
  (`<id>  <title>  — <summary>`) — no bodies. Parse `id` + `title` per row.
  If exactly **one** hit, skip the picker → step 3. If several → step 2.

- **`/pages`** (no argument) → recent pages:

  ```
  list_pages_recent(limit=10)
  ```

  Returns activity items `{ id, title, snippet, timestamp_ms, badge }` — no
  `content` field. Go to step 2.

If the list is empty, say "no pages found — run `/distill [topic]` to
synthesize one" and stop. Never derive a slug client-side; the daemon owns it.

## 2. Native picker (the search lives in "Other")

Present the candidates with `AskUserQuestion` — the arrow-key picker, not a
prose list.

Rules:

- One option per page. **Label** = the title (trim long ones). **Description**
  = the snippet/summary. Order by recency/relevance as returned; mark the
  **first** option recommended. Keep a label→id map in mind — the picker
  returns the chosen label, you open its id.
- The picker caps at **4 options**. Show up to 4 real pages. There is **no
  "Search by title…" option** — an explicit option can't capture typed text.
  Instead, put the search hint in the **question text**:

  > `Which page to open? (or pick **Other** and type a title/topic to search)`

  The native **"Other"** choice is the only free-text box a skill gets. When
  the user picks Other and types text, treat it as a **search query** → re-run
  `search_pages(query=<that text>, limit=10)` → re-present this picker. This is
  the no-prose search escape; it keeps every page reachable past the 4-cap.

## 3. Open the page

Resolve the on-disk filename **by id** (never slugify client-side — skill
heuristics drift from the daemon's `slugify()` on apostrophes/punctuation),
then open it. Run one Bash block with the id substituted:

```bash
PID="<id>"
P=$(python3 -c '
import json, os, sys
state_path = os.path.expanduser("~/.wenlan/pages/.wenlan/state.json")
pid = sys.argv[1]
filename = None
try:
    with open(state_path) as f:
        filename = json.load(f).get("pages", {}).get(pid, {}).get("file")
except FileNotFoundError:
    pass
print(os.path.expanduser(f"~/.wenlan/pages/{filename}") if filename else "")
' "$PID")
if [ -z "$P" ]; then
  echo "(no md on disk for $PID — run /distill to (re)synthesize it)"
else
  # Open in your OS default app for .md (you pick it once in Finder > Get Info >
  # Open With). No editor hardcoded. cat only on a headless box.
  open "$P" 2>/dev/null || xdg-open "$P" 2>/dev/null || cat "$P"
  echo "Opened $P"
fi
```

- **Run this block with the command sandbox DISABLED.** `open` / `xdg-open`
  launch a GUI app through LaunchServices, which a sandboxed shell cannot reach —
  it fails with `kLSUnknownErr (-10810) "Couldn't communicate with a helper
  application"`, and the chain then falls through to `cat`, dumping the whole
  body into chat (the thing this skill exists to avoid). With the sandbox off the
  editor actually opens; only a genuinely headless box (no window server) then
  falls to `glow` / `cat`. One-time: allowlist `open` so it stops prompting.
- Opens in your **OS default `.md` app** — `open` (macOS) / `xdg-open` (Linux) /
  `cat` (headless). No editor is hardcoded; you control which app via Finder ▸ Get
  Info ▸ Open With. If `open` appears to do nothing, your `.md` default is a
  no-window app (e.g. Xcode) — repoint it once to your editor and every page opens
  there.
- Print exactly the one `Opened <path>` line the block emits. Don't add a
  metadata card, version/links/stale block, or summary — the file is the view.

## When to use

- "show me the page on X", "open that page", "what pages do I have", "browse my wiki".
- After `/distill`, `/pages "<title>"` to open a changed page in your editor.

## When NOT to use

- Raw memory lookups → `/recall`.
- Just listing files → `ls ~/.wenlan/pages/`.
