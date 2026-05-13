---
name: brief
description: >
  Session-start briefing from Origin. Loads identity, preferences, and
  topic-relevant memories so the agent walks in with context. Invoked as
  `/brief [topic]`. Call FIRST at session start, before any other
  Origin verb.
argument-hint: "[topic]"
allowed-tools: ["mcp__plugin_origin_origin__context", "mcp__plugin_origin_origin__recall", "mcp__plugin_origin_origin__search_pages"]
---

# /brief

Pull a curated session brief from Origin: who the user is, what they prefer,
and what's relevant to the current topic.

## How to invoke

**Step 1.** Call the `origin` MCP server's `context` tool. If the user passed a topic
argument, pass it through. Otherwise infer scope from the working directory and
the conversation so far — don't ask the user.

```
context(topic="<args or inferred>", domain=<inferred from cwd or recent turns>)
```

**Scope inference rules:**

- `topic`: if user omitted args, pass the most recent topic from the
  conversation (file or feature being discussed), or omit for a fresh
  general brief at session start.
- `domain`: from cwd. `~/Repos/origin/...` → `"origin"`. Other repos → repo
  name. Outside any repo → omit.

**Step 2.** Call `search_pages(page_type="recap", limit=3)` to load the 3 most recent
recap pages. If results, prepend a "Recent recaps" section to the brief output
showing each recap's title + 1-line summary. If the call returns no results or
errors, omit the section silently — recap pages may not exist yet on a fresh vault.

## When to use

- Session start — call BEFORE any other Origin tool.
- Major topic shift mid-session.
- User says "catch me up", "what's the background on X", "remind me about Y".
- Mid-session check-in to confirm assumptions.

## When NOT to use

- Specific factual lookup → use `/recall` (more targeted).
- Storing a new memory → use `/capture`.
- End of session → use `/handoff`.

## How to use the result

Model how the user thinks. Their preferences, corrections, and past decisions
tell you how they want to be helped, not just what they already know. Don't
just look things up: adjust your behavior.
