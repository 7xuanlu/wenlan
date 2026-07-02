---
name: forget
description: >
  Delete a Wenlan memory by exact id from Codex. Destructive and confirmation
  gated. Invoked as /forget <source_id>.
argument-hint: "<source_id>"
allowed-tools: ["Bash", "mcp__wenlan__forget", "mcp__wenlan__recall"]
user-invocable: true
---

# /forget

Delete one memory by exact `source_id`.

## Safety

This cannot be undone by Wenlan. Prefer `/capture` with a corrected memory when
history should be preserved.

Always confirm with the user before calling forget, unless the same user message
already includes an exact id and an unambiguous delete instruction.

If the id is missing, use `mcp__wenlan__recall` to find candidates or ask the
user for the exact id. The confirmation reply must include the action and id:

```text
delete <id>
```

A bare id is not confirmation.

## Call

After confirmation, call:

```text
mcp__wenlan__forget(memory_id="<source_id>")
```

Delete only one memory per tool call. For bulk cleanup, use `/curate captures`
so each item has its own decision.

## Auto-commit `~/.wenlan`

After a successful delete, snapshot any Markdown projection changes. Best
effort only:

```bash
git -C ~/.wenlan add -A && \
  git -C ~/.wenlan -c user.name=Wenlan -c user.email=daemon@wenlan.local \
    commit --quiet -m "forget: <source_id>" 2>/dev/null || \
(sleep 1 && git -C ~/.wenlan add -A && \
  git -C ~/.wenlan -c user.name=Wenlan -c user.email=daemon@wenlan.local \
    commit --quiet -m "forget: <source_id>" 2>/dev/null) || true
```

Do not fail the delete if this commit fails.

## When to use

- User says to forget or delete a specific memory and gives the id.
- User confirms `delete <id>` after recall found the matching memory.

## When not to use

- Corrections where history matters: capture a superseding memory.
- Page rebuilds: use `/distill rebuild <page-id>`.
