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

For bare `/distill`, infer a target from cwd. Walk the worktree up to
its parent repo so the same scope is used whether the user is sitting
in `~/Repos/origin/.worktrees/feature/...` or in `~/Repos/origin`:

```
Bash: top=$(git -C "$PWD" rev-parse --show-toplevel 2>/dev/null); \
      common=$(git -C "$PWD" rev-parse --git-common-dir 2>/dev/null); \
      if [ -n "$common" ]; then \
        case "$common" in /*) root=$(dirname "$common");; *) root=$(cd "$top" && cd "$(dirname "$common")" && pwd);; esac; \
        basename "$root"; \
      fi
```

- Both subshells succeed → use the parent repo basename (works for
  both main checkouts and worktrees; `git-common-dir` resolves to the
  primary `.git` either way).
- Not a git repo → fall back to `basename "$PWD"`.
- For `/distill <arg>` → use `<arg>`.
- For `/distill deep` (reserved keyword) → no scope (full pass over
  every memory). Slow. Use only when the user explicitly asks for it.

### 2. Fetch candidate memories

```
recall(query="<scope>", domain="<scope>", limit=50)
```

Use the full `limit=50`. A narrow recall hides clusters and produces a
one-page pass that looks like a no-op. Take the time to pull the wider
net.

Read the result. Cluster mentally by shared entities or sub-topic. A
cluster needs at least 3 related memories to be worth synthesizing —
singletons and pairs go in the "skipped" report below, not into a
page. Plan to emit every qualifying cluster, not just the strongest
one.

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

Repeat for each qualifying cluster, one POST per page.

### 4. Report the pass terse

After all POSTs land, report the pass with one block — no wall of
text. The user already has the md on disk and can open `/read <id>`
when they want to see the body. Format:

```
Distilled N page(s):
  - <Title 1>  →  /read <id>  (·  ~/.origin/pages/<slug>.md)
  - <Title 2>  →  /read <id>
  ...

Skipped M cluster(s):
  - <topic hint>  (<N> memories, no other peers yet)
```

Rules for the report:
- One line per page; don't include the page body.
- Mention the md path once per page so users can open it in Obsidian
  / VS Code without re-asking.
- Always include the "Skipped" section when at least one candidate
  cluster fell below the 3-memory floor — silence here makes the
  user think there was nothing else to do.
- Omit "Skipped" only when every memory ended up in a page.

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
