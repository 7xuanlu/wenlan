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

Pull pending memories via the `origin` MCP server's `list_pending` tool:

```
list_pending(limit=20)
```

For each memory, present:
- The content (full text)
- The auto-classified type + domain + entity
- The original source (chat, file, agent name)
- Confidence + quality

Then offer per-item:
- **Accept** → `confirm_memory(memory_id="<id>")`
- **Edit** → present a draft, on save call `capture` with the new content +
  `supersedes="<old_id>"` (then `forget(memory_id="<old_id>")` once the
  replacement lands)
- **Reject** → `forget(memory_id="<id>")`

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
