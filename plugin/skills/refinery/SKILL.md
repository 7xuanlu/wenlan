---
name: refinery
description: >
  Walk Origin's pending refinement queue. Lists daemon-queued proposals (entity merges,
  relation conflicts, contradictions, entity suggestions) and lets the user dismiss noise.
  Invoked as `/refinery`. Use when the user wants to audit what the daemon's refinery
  has queued for review.
allowed-tools: ["mcp__plugin_origin_origin__list_refinements", "mcp__plugin_origin_origin__reject_refinement"]
---

# /refinery

Surface Origin's refinement queue and walk each pending proposal with the user.

## How to invoke

Pull pending proposals via the `origin` MCP server's `list_refinements` tool:

```
list_refinements(limit=20)
```

Optional action filter to narrow:

```
list_refinements(action="entity_merge")
```

Valid action values: `entity_merge`, `relation_conflict`, `detect_contradiction`,
`suggest_entity`, `dedup_merge`.

For each proposal, present:
- **Action** (one of the 5 variants above)
- **Confidence** (0.0 - 1.0)
- **Source ids** (the memories or entities the proposal references)
- **Payload summary** — pattern-match on the typed `payload` field:
  - `entity_merge`: "Merge entity `<existing_id>` ↔ `<new_id>` (similarity `<similarity>`)"
  - `relation_conflict`: "Relation `<existing_id>` vs `<new_id>` from `<from>` → `<to>`, type `<old_type>` → `<new_type>`"
  - `detect_contradiction`: source ids list the conflicting memories
  - `suggest_entity`: name hint `<name_hint>`
  - `dedup_merge`: no payload — historical (auto-dismissed by daemon)

Then offer per-item:
- **Keep** → no-op (see notice below)
- **Dismiss** → `reject_refinement(id="<id>")`

## Accept is not in scope yet

This skill exposes **list + dismiss only**. There is no accept verb in this version.
Keeping a proposal is a no-op — it stays in the queue for a future session (or for
the desktop app, when the accept-side spec lands).

**Only dismiss when you are confident the proposal is wrong.** Do not develop a
habit of dismissing-by-default; that would skew the queue toward attrition.

## When to use

- User says "check refinery", "pending proposals", "what's queued", "audit
  daemon decisions", "review entity merges".
