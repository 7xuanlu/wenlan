---
name: review
description: >
  Review Origin's pending memories. Walks unconfirmed captures and lets the
  user accept, edit, or reject each one. Invoked as `/origin:review`. Use
  when the user wants to audit what was captured before it becomes
  authoritative.
---

# /origin:review

Walk through pending / unconfirmed memories so the user can accept, edit, or
reject each before they become authoritative.

## How to invoke

Pull pending memories from the daemon:

```
GET http://127.0.0.1:7878/api/memory/list?confirmed=false&limit=20
```

For each memory, present:
- The content (full text)
- The auto-classified type + domain + entity
- The original source (chat, file, agent name)
- Confidence + quality

Then offer per-item:
- **Accept** → `POST /api/memory/confirm/{id}` (marks confirmed)
- **Edit** → present a draft, on save `PUT` the new content
- **Reject** → `DELETE /api/memory/{id}` (or `POST /api/memory/{id}/archive`)

## When to use

- User says "review pending", "audit memories", "what got captured", "show
  me what's unconfirmed".
- After a bulk import — checks the auto-classification quality.
- Periodic hygiene — sweep unconfirmed batch every N captures.

## When NOT to use

- Single targeted edit → user knows the memory ID, use `/origin:recall` to
  find it then edit directly.
- Searching for facts → use `/origin:recall`.

## Cost

Read-only until user confirms / rejects. No LLM calls. Cheap.
