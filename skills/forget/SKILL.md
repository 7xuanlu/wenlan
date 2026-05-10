---
name: forget
description: >
  Delete a memory from Origin by ID. Destructive and cannot be undone —
  prefer `/capture` with `supersedes` for corrections. Invoked as
  `/forget <source_id>`.
---

# /forget

Permanently delete a memory by its `source_id`.

## How to invoke

You need the `source_id`. If the user did not provide it, call
`/recall` first to find the matching memory and confirm with the
user before deleting.

```
forget(memory_id="<source_id>")
```

## When to use

- User says "forget this", "delete that", "that's wrong, remove it".
- User explicitly identifies a memory by ID.

## When NOT to use

- For corrections, prefer storing a new memory with `supersedes` pointing
  at the old one. That preserves history. Use `/capture` with the
  `supersedes` arg instead.
- Bulk deletions — call `/review` first, confirm with the user,
  then delete one at a time.

## Safety

Deletion is destructive. Always confirm with the user before calling forget,
unless the user has already given an explicit, unambiguous instruction in
the same turn (e.g. they pasted the ID and said "delete this").
