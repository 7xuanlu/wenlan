---
name: origin-forget
description: >
  Delete a memory from Origin by ID. Wraps the origin MCP `forget` tool.
  Invoked as `/origin-forget <source_id>`. Destructive and cannot be undone —
  prefer `/origin-store` with `supersedes` for corrections.
---

# /origin-forget

Permanently delete a memory by its `source_id`.

## How to invoke

You need the `source_id`. If the user did not provide it, call `/origin-recall`
first to find the matching memory and ask the user to confirm before
deleting.

```
forget(memory_id="<source_id>")
```

## When to use

- User says "forget this", "delete that", "that's wrong, remove it".
- User explicitly identifies a memory by ID.

## When NOT to use

- For corrections, prefer storing a new memory with `supersedes` pointing at
  the old one. That preserves history. Use `/origin-store` with the
  `supersedes` arg instead.
- Bulk deletions — call `/origin-recall` first, confirm with the user, then
  delete one at a time.

## Safety

Deletion is destructive. Always confirm with the user before calling forget,
unless the user has already given an explicit, unambiguous instruction in the
same turn (e.g. they pasted the ID and said "delete this").
