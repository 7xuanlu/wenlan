---
name: curate
description: >
  Power-user audit of Wenlan's pending surfaces: accept or dismiss pending
  revisions (conflicts/merges), or audit unconfirmed captures. Most users want
  `/brief` for revisions; that handles the daily flow. Use `/curate` only for
  explicit deep-walk audits after bulk imports, or to walk the full queue rather
  than the top 3 shown in /brief.
  Invoked as `/curate captures` or `/curate revisions`.
argument-hint: "captures | revisions"
allowed-tools: ["Bash", "AskUserQuestion", "mcp__plugin_wenlan_wenlan__list_pending", "mcp__plugin_wenlan_wenlan__confirm_memory", "mcp__plugin_wenlan_wenlan__forget", "mcp__plugin_wenlan_wenlan__capture", "mcp__plugin_wenlan_wenlan__recall"]
---

# /curate

Power-user audit lever. Most users do not need /curate in daily flow:

- **Pending revisions** surface in `/brief` automatically (top 3 with inline accept/dismiss).
- **Pending captures from this session** surface in `/handoff`'s preview block (top 3).
- **Orphan wikilinks** surface in `/distill`'s topic-suggestion block.

Use /curate only when you want the deep walk those skills intentionally do not force.

**Captures are meant to decay.** A capture lands unconfirmed and, if nothing ever
conflicts with it, it quietly fades — that is by design, not a backlog you owe the
system. The only items that *need* a human are genuine conflicts (a contradiction,
or a richer re-capture worth merging); those surface as pending revisions. `/curate
captures` is an **opt-in audit** for when you *want* to seal a batch (e.g. after a
bulk import), not a daily chore to zero out.

## Decide in native cards, not prose

Walk the queue with `AskUserQuestion` — the same native picker `/pages` uses.
**Never narrate items one-by-one in chat prose.** A card is a real decision; prose
is a wall of text the user has to reply to by hand.

`AskUserQuestion` takes **up to 4 questions per card** (one per item) and **2-4
options** each, plus an auto "Other" free-text slot. So batch the queue in groups
of ≤4:

1. List the pending items (per section below).
2. Take the next ≤4. Build ONE `AskUserQuestion` call:
   - one **question** per item — `question` is the item's content, trimmed to ~1-2 lines.
   - `header`: a short tag, ≤12 chars (the memory type, or `Revision 3`).
   - `options`: the actions for that section (below).
   - The card always adds an "Other" slot.
3. Apply each answer (below), then repeat with the next ≤4 until the queue is empty.
4. Cancel / close the card = **no mutation**. "Skip" is the safe per-item escape.

## `/curate revisions` (and bare `/curate`) → via the `wenlan` CLI

Revisions go through the **CLI**, not MCP — no deferred-tool round-trip, so the
picker comes up fast. Resolve the binary once, then list:

```
W="$(command -v wenlan || echo "$HOME/.wenlan/bin/wenlan")"
"$W" --format json curate
```

That prints a JSON array; each element is ONE logical revision (the CLI already
groups the per-chunk rows and joins their text, so you never see a mid-sentence
fragment):

- `target_source_id` — the memory this revision would replace; the **action key**.
- `content` — the full revised text.
- `source_agent` — who proposed it (or `null` = daemon).

Walk it as native cards (≤4 per card). Per revision, options:

- **Accept** → the revision replaces the original memory.
- **Dismiss** → drop the revision, keep the original.
- **Skip** → nothing.

After the card returns, apply the picks in ONE Bash call (skips run nothing):

```
"$W" curate accept <target_source_id>      # for each Accept
"$W" curate dismiss <target_source_id>     # for each Dismiss
```

If the array is empty, say so in one line ("Nothing needs you — no pending
conflicts.") and stop. Captures are meant to decay; mention `/curate captures`
only as an opt-in deep audit the user can run if they *want* to, never as a backlog.

The list carries the revised text but **not the original**. If the user wants to
compare before deciding, `recall` the `target_source_id` first and show both.
(Surfacing the original inline is a daemon follow-up.)

## `/curate captures` (opt-in audit, MCP)

The captures path is the rare opt-in audit, so it stays on MCP. `list_pending`
lists every unconfirmed memory (pass `space` only if the user named one, e.g.
"curate work captures"; otherwise leave it off for all pending). Each memory has a
`source_id`. Per item, options:

- **Accept** → `confirm_memory(memory_id=<source_id>)`
- **Reject** → `forget(memory_id=<source_id>)`
- **Skip** → nothing
- *Other (typed text)* → edit: `capture(content=<text>, supersedes=<source_id>)`,
  then `forget(memory_id=<source_id>)`.

## When to use

- After a bulk import (ChatGPT, Obsidian dump) when you want to audit every
  auto-classification before sealing.
- When `/brief` shows ">3 pending revisions" and you want to clear the full
  queue, not just the top 3.

## When NOT to use

- Daily session work. `/brief` handles the surface that matters today.
- Specific factual lookup: use `/recall`.

## Cost

Read-only until the user picks an action in a card. Revisions go through the
local `wenlan` CLI (daemon round-trip ~milliseconds, no LLM). Cheap.
