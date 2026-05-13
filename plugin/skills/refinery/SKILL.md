---
name: refinery
description: >
  Walk Origin's pending refinement queue. Lists daemon-queued proposals (entity merges,
  relation conflicts, contradictions, entity suggestions) and lets the user dismiss noise.
  Invoked as `/refinery`. Use when the user wants to audit what the daemon's refinery
  has queued for review.
allowed-tools: ["mcp__plugin_origin_origin__list_refinements", "mcp__plugin_origin_origin__reject_refinement", "mcp__plugin_origin_origin__accept_refinement"]
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
`dedup_merge`. (`suggest_entity` is reserved for a future producer and not
emitted by any current path.)

For each proposal, present:
- **Action** (one of the 4 variants above)
- **Confidence** (0.0 - 1.0)
- **Source ids** (the memories or entities the proposal references)
- **Payload summary** — pattern-match on the typed `payload` field:
  - `entity_merge`: "Merge entity `<existing_id>` ↔ `<new_id>` (similarity `<similarity>`)"
  - `relation_conflict`: "Relation `<existing_id>` vs `<new_id>` from `<from>` → `<to>`, type `<old_type>` → `<new_type>`"
  - `detect_contradiction`: source ids list the conflicting memories
  - `dedup_merge`: no payload — historical (auto-dismissed by daemon)

Then offer per-item:
- **Accept** → `accept_refinement(id="<id>")`: applies the change with sensible defaults (see below)
- **Dismiss** → `reject_refinement(id="<id>")`

**Only dismiss when you are confident the proposal is wrong.** Do not develop a
habit of dismissing-by-default; that would skew the queue toward attrition.

## Accept a proposal

Use `accept_refinement(id)` to apply the proposed change using sensible defaults.

- **entity_merge:** existing entity wins as canonical. The new entity folds in as an alias. All relations, observations, and memories re-point to the canonical entity.
- **relation_conflict:** new relation supersedes. Old relation is deleted.
- **detect_contradiction:** previously-stored memory flagged with `pending_revision=1` for human revisit. The new memory is left unchanged. Accept here means "the contradiction is genuine and the established memory needs attention", not "the new memory wins".

If the default is wrong for your case, reject instead and edit manually via the relevant memory, entity, or relation endpoint.

`suggest_entity` and `dedup_merge` proposals return 422 on accept. `suggest_entity` is reserved for a future producer. `dedup_merge` is deprecated (distillation handles it now). Reject those instead.

## When to use

- User says "check refinery", "pending proposals", "what's queued", "audit
  daemon decisions", "review entity merges".
