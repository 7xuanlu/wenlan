---
name: distill
description: >
  Synthesize wiki pages from related memories. The agent does the LLM
  work (Claude in this session); the daemon stores the result. Invoked
  as `/distill [page_id_or_entity_or_domain]`.
argument-hint: "[page_id_or_entity_or_domain]"
allowed-tools: ["mcp__plugin_origin_origin__recall", "mcp__plugin_origin_origin__distill", "Bash"]
---

# /distill

Force a distillation pass now. Pages emerge automatically; the user
never has to name topics or manage clusters. The Claude session does
the synthesis — the daemon does not load its own LLM for this skill.

## Why agent-driven by default

The daemon may have an on-device LLM (Qwen, etc.) loaded for the
background refinery. The fans on it are *real*. When the user
invokes `/distill` interactively, the Claude session is already in
context and can synthesize for free. Keep the on-device model quiet.

If you ever want the daemon to do it instead (e.g. unattended bulk
runs), call MCP `distill` directly. That path stays available; it is
just not the default.

## Default flow

Always run the three steps below. No "try MCP first, fall back".

### 1. Pick the scope

For bare `/distill`, infer a target from cwd. Use `--git-common-dir`
so the result stays the same inside a git worktree (otherwise the
worktree directory name leaks in instead of the parent repo):

```
Bash: g=$(git -C "$PWD" rev-parse --git-common-dir 2>/dev/null); \
      [ -n "$g" ] && basename "$(dirname "$g")"
```

- If output is a name → use it (e.g. `/Users/lucian/Repos/origin/.git` → `origin`).
- If not a git repo → fall back to the cwd basename.
- For `/distill <arg>` → use `<arg>`.
- For `/distill deep` (reserved keyword) → no scope (full pass over
  every memory). Slow. Use only when the user explicitly asks for it.

### 2. Fetch candidate memories

```
recall(query="<scope>", domain="<scope>", limit=50)
```

Read the result. Cluster mentally by shared entities or sub-topic.
Pick one cluster per page. Semantic ranking biases results toward the
scope query, which is fine for distillation — the goal is finding
related material.

### 3. Synthesize and post

Write each page in wiki-prose style:

- **Title**: short noun phrase (e.g. "Origin daemon architecture").
- **Summary**: one sentence — the durable claim the page supports.
- **Body**: 3-8 paragraphs of encyclopedia-style prose. Use
  `[[wikilinks]]` to reference other pages or entities. Cite source
  memory ids inline with `(source: mem_XXX)`.
- **Durable**: write what would still be true in six months, not the
  current state of in-progress work.

POST the page back:

```
Bash: curl -fsS -X POST http://127.0.0.1:7878/api/pages \
  -H 'Content-Type: application/json' \
  -d '{"title":"<Title>","content":"<page body>","summary":"<one line>",
       "entity_id":"<primary_entity_id_or_null>","domain":"<scope>",
       "source_memory_ids":["mem_X","mem_Y","mem_Z"]}'
```

Repeat for each cluster, one POST per page.

### 4. Echo the page in chat

After each POST, fetch the rendered md so the user can read what just
got created without leaving Claude Code. Cat the local file (faster
than another HTTP round trip) and include it inline in the reply:

```
Bash: slug=$(echo "<Title>" | tr '[:upper:]' '[:lower:]' | sed -E 's/[^a-z0-9]+/-/g; s/^-|-$//g'); \
      cat "$HOME/.origin/pages/${slug}.md"
```

Wrap the content in a fenced block when reporting back so the
rendered output preserves the source view. If the file isn't there
yet (e.g. KnowledgeWriter not wired into the POST route yet), GET
`/api/pages/<id>` and print `.page.content` instead.

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
- After bulk import — daemon refinery handles this in the background;
  user can force a pass for immediate visibility.

## When NOT to use

- Daemon scheduler runs distillation periodically (on-device LLM).
  Don't trigger redundantly during normal flow.
- Single memory write → daemon's post-ingest enrichment already covers
  it; manual distill is over-eager.

## Cost

Agent path: counts against the current Claude session's tokens. Keep
clusters small (≤ 20 source memories per page) to control cost.

Daemon path (MCP `distill`): one on-device LLM call per cluster. Off
by default in this skill; call MCP `distill` directly if you want it.
