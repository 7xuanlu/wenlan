---
name: capture
description: >
  Save a durable memory to Wenlan from Codex. Use proactively when the user
  states a preference, makes a decision, corrects you, or shares a durable
  fact. Invoked as /capture <content>.
argument-hint: "<content>"
allowed-tools: ["Bash", "mcp__wenlan__capture", "mcp__wenlan__recall", "mcp__wenlan__create_entity", "mcp__wenlan__create_relation", "mcp__wenlan__list_rejections", "mcp__wenlan__accept_revision", "mcp__wenlan__dismiss_revision"]
user-invocable: true
---

# /capture

Capture one memory in the moment. Write the memory as a self-contained
statement with the reason it matters.

## Resolve the active space

Accept one optional inline token of the form `space:<name>` anywhere in the
argument string. The Codex resolver strips it and returns the rest as the
content. It prints the resolved Space line itself:

```bash
raw_args="<the full argument string passed to /capture>"
# The resolver ships inside the installed plugin; the relative path only
# exists in a wenlan checkout.
resolver="$(find "$HOME/.codex/plugins/cache" -path '*/wenlan/*/bin/resolve-space.sh' 2>/dev/null | head -1)"
resolved="$("${resolver:-plugin-codex/bin/resolve-space.sh}" --cwd "$PWD" --args "$raw_args")"
space="$(printf '%s\n' "$resolved" | cut -f1)"
source_layer="$(printf '%s\n' "$resolved" | cut -f2)"
content="$(printf '%s\n' "$resolved" | cut -f3-)"
```

If `content` is empty, ask the user what they want to capture. If `space` is
non-empty, pass it to the `capture` MCP tool; if empty, omit the `space`
parameter.

After capture, relay the tool's returned `space`, `space_source`, and
`write_outcome`; those fields describe what the daemon actually persisted and
are authoritative.

## How to invoke

Call the Wenlan MCP `capture` tool with the user's content as a complete,
self-contained statement. Attach topic from cwd or the conversation when useful.

```text
capture(
  content="<content, written as a full sentence with WHY>",
  memory_type="<picked from the 6 types>",
  entity="<primary entity name, if any>",
  space="<resolved if non-empty>"
)
```

## Pick `memory_type`

The daemon classifies when a model or API key is configured. In local memory
mode it may not, so pick the type from the content itself.

| Type | Use for |
|---|---|
| `identity` | Durable facts about the user |
| `preference` | A habit, correction, or stylistic choice with a reason |
| `decision` | A specific choice with rationale |
| `lesson` | Root cause, workaround, or technical insight |
| `gotcha` | Sharp edge or surprising behavior |
| `fact` | Durable info about people, projects, tools, or places |

If two types fit, pick the one closest to why the memory matters.

## Extract `entity`

Pick the single most important named anchor: person, project, tool, or place.
Use the exact name. If there is no named anchor, omit `entity`.

Pass the single most important named anchor through `capture.entity`. Ordinary
captures stop there; daemon enrichment handles routine extraction. Do not call
`mcp__wenlan__create_entity` for ordinary captures. Never infer a relation the
user did not state.

Use the explicit KG tools only when the user explicitly states a durable relation:

1. Call `mcp__wenlan__create_entity` for both named endpoints first and collect
   their stable ids. The call is idempotent and may return an existing id.
2. Call `mcp__wenlan__capture` with the complete relation statement and pass
   the primary entity name as `entity` so the memory resolves and links to it.
3. Call `mcp__wenlan__create_relation` with `from_entity_id`, `to_entity_id`,
   `relation_type`, and the capture result's required `source_memory_id`.

For a durable named entity explicitly established by the user,
`mcp__wenlan__create_entity` may also be used alone when its stable id is
needed. `Entity <id> ready` does not imply that a new row was created.

## What to capture, what to skip, how to write it

The `capture` tool description is the single source for this policy: what to
store proactively, what never to store (tool output, agent operating rules,
transient state, world facts), one atomic idea per call, and declarative
wording. Follow it as written; do not keep a second copy here.

If a capture the user expected is missing or the user explicitly asks why it
was rejected, call `mcp__wenlan__list_rejections`. Do not list rejections after
successful ordinary captures.

## Post-capture contradiction signal

After `capture` returns, check `triggered_revisions` and `auto_superseded`.

If `auto_superseded` is non-empty, surface it as informational. No follow-up
tool call is needed.

If `triggered_revisions` is non-empty and `auto_superseded` is empty, render:

```text
Stored <new id>.

This capture topic-matches a protected memory now flagged for revision:
  - <target id>

Action: accept (replace original) | dismiss (drop revision) | leave (decide later)
```

Inline verb map:

- accept: `accept_revision(target_source_id="<target id>")`
- dismiss: `dismiss_revision(target_source_id="<target id>")`
- leave: no call
