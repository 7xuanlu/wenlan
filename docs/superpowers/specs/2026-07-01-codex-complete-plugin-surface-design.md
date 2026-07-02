# Codex Complete Plugin Surface Design

## Goal

Port the complete Wenlan Claude Code plugin command surface into the Codex
plugin while keeping Claude behavior unchanged and keeping the shared contract
strong enough to catch cross-surface drift.

## Context

Wenlan already has the right architecture split:

- `wenlan-mcp` is the local connector and tool layer.
- `plugin/` is the Claude Code workflow layer.
- `plugin-codex/` is the Codex workflow layer.
- `plugin-contract.json` records which skills are present on each surface and
  validates MCP command wiring, tool prefixes, agent names, and marketplace
  metadata.

PR #318 already ports `/pages` into Codex and makes it shared in the contract.
The remaining Claude-only skills are:

- `recall`
- `handoff`
- `debrief`
- `distill`
- `curate`
- `forget`
- `help`

The target is to port all of them now, in the same Codex PR, with Codex-native
guardrails instead of copying Claude-only assumptions.

## Non-Goals

- Do not change the daemon, HTTP API, `wenlan-mcp`, or shared wire types.
- Do not create a shared skill generator in this pass.
- Do not edit Claude plugin behavior under `plugin/`; validation may read
  Claude files but must not change their runtime behavior.
- Do not add native Codex UI widgets or browser companions.
- Do not make destructive actions silent.

## Design

### Surface Boundary

Claude stays the reference implementation for Claude Code:

- Claude skills keep `mcp__plugin_wenlan_wenlan__*` tool names.
- Claude skills keep `CLAUDE_PLUGIN_ROOT` and `plugin/bin/resolve-space.sh`.
- Claude skills can keep `AskUserQuestion` where that is a Claude-native UX.

Codex gets its own hand-authored skill files:

- Codex skills use `mcp__wenlan__*` tool names.
- Codex skills use `user-invocable: true`.
- Codex skills include `agents/openai.yaml` interface metadata.
- Codex skills use `plugin-codex/bin/resolve-space.sh`, a Codex-local copy of
  the Claude resolver semantics. This keeps Claude untouched while preserving
  resolver parity.
- Codex skills avoid `AskUserQuestion`, `CLAUDE_PLUGIN_ROOT`, and
  `mcp__plugin_wenlan_wenlan__*`.

This is intentionally duplicated at the skill-text layer. The behavior is not
yet stable enough to justify a generator, and the validator is cheaper than a
new abstraction.

### Skill Port Matrix

| Skill | Codex behavior |
|---|---|
| `recall` | Functional port. Parse optional `space:<name>`, resolve space with the Codex resolver script, call `mcp__wenlan__recall`, rerank results against the original query, render top hits with revision tags. |
| `handoff` | Functional port. Use Bash for git/session files, call `mcp__wenlan__list_pending` for preview and `mcp__wenlan__capture` for durable memories, write `~/.wenlan/sessions` files, then best-effort commit `~/.wenlan`. |
| `debrief` | Functional alias with duplicated handoff instructions. It must not rely on Codex loading another skill file implicitly. |
| `distill` | Functional port. Call `mcp__wenlan__distill`, synthesize pending clusters in-session, use `create_page`, `update_page`, and `get_page_sources` where needed, and best-effort commit `~/.wenlan`. |
| `curate` | Codex-safe structured fallback. No picker. For revisions and captures, show compact numbered batches and wait for an explicit user reply before mutating. Revision actions key on `revision_source_id`, not target id. |
| `forget` | Destructive port. Call `mcp__wenlan__forget` only after explicit same-turn confirmation, unless the user already provided an unambiguous id and delete instruction. Best-effort commit `~/.wenlan`. |
| `help` | Codex-specific one-screen reference. List the commands available in Codex and avoid Claude hook wording. |
| `pages` | Already ported in PR #318. Keep editor-first behavior and no page-body reads. |

### Interaction Rules

Codex lacks the Claude `AskUserQuestion` picker used by `/curate`. The Codex
surface should therefore use a compact textual decision protocol:

1. Render up to four items at a time.
2. Number each item and show only the fields needed to decide.
3. Offer explicit action syntax, for example:
   - `1 accept`
   - `2 dismiss`
   - `3 skip`
   - `4 edit: <replacement text>`
4. Perform no mutation until the user replies.
5. Treat cancellation, ambiguity, or no reply as no mutation.
6. Do not rely on hidden pagination state across turns. After applying a batch,
   re-list the pending surface before presenting another batch.

This preserves safety without turning review into long prose.

For `/curate revisions`, Codex must use the local `wenlan --format json curate`
CLI path and apply actions with `wenlan curate accept <revision_source_id>` or
`wenlan curate dismiss <revision_source_id>`. The MCP `accept_revision` and
`dismiss_revision` tools are target-keyed and are acceptable for `/brief`'s top
three daily flow, but they are not precise enough for a deep revision walk where
multiple proposed revisions can compete for one target memory.

### Destructive Actions

Two flows can destroy user data or user-authored prose:

- `/forget <source_id>`
- `/distill rebuild <page-id>`

Both require explicit confirmation unless the same user message already contains
the exact id and an unambiguous delete/rebuild instruction.

If the skill has to ask for a missing id, the follow-up prompt must require the
action verb and id in the same reply, such as `delete mem_abc` or
`rebuild page_abc`. A bare id is not confirmation.

The skill text must say what will be lost:

- `forget`: the memory row is deleted and cannot be restored by Wenlan.
- `distill rebuild`: user-edited page prose is wiped and regenerated from
  source memories.

### Space Resolution

Codex uses a local resolver script with the same layers as the Claude resolver:

1. `space:<name>` argument, for skills that accept it.
2. `WENLAN_SPACE`.
3. `~/.wenlan/spaces.toml` cwd-prefix mapping, longest prefix wins.
4. `~/.wenlan/spaces.toml` top-level `default`.
5. Current git repo basename.
6. Optional topic fallback when the skill has a meaningful topic string.
7. Unscoped output, represented as an empty space plus source layer `unscoped`.

Codex must not fall back to `personal` when the resolver returns unscoped.
Callers should omit the `space` parameter in that case.

For flows where unscoped behavior is more appropriate, the skill may omit a
space parameter when the user did not name one. The spec chooses conservative
defaults per skill:

- `recall`, `capture`, `brief`, `handoff`: use resolved space.
- `distill`: use explicit target first; otherwise use resolved space for bare
  `/distill`.
- `curate captures`: use all pending captures unless the user names a space.
- `curate revisions`: use the local CLI path keyed by `revision_source_id`;
  never use Claude-only tools.

### Contract and Validation

`plugin-contract.json` becomes a complete inventory:

- Every skill is `shared_now`.
- Every Codex skill must exist.
- Every Codex skill must be `user-invocable: true`.
- Every Codex skill must have `agents/openai.yaml` with interface metadata.
- Every Codex skill must avoid Claude-only tokens.
- Every Claude skill must avoid Codex MCP prefixes.

`scripts/validate-codex-plugin-slice.py` should evolve from a first-slice
validator into a complete Codex surface validator:

- `REQUIRED_SKILLS` includes all shared skills.
- Interface metadata is checked for every skill.
- Destructive skill guardrails are checked for `forget` and `distill`.
- Interactive fallback guardrails are checked for `curate`.
- CLI-only skills like `pages` and read-only menu skills like `help` are exempt
  from the MCP-tool-reference check.
- `debrief` must contain the handoff workflow instructions itself. The validator
  should reject a thin "run /handoff" pointer because Codex does not guarantee
  nested skill loading.
- The validator should derive the required Codex skill set from
  `plugin-contract.json` `shared_now` entries, or assert that any local
  `REQUIRED_SKILLS` tuple exactly equals that contract-derived set.

Guardrail checks should use explicit strings, matching the current `/pages`
style. Required phrases:

- `forget`: `cannot be undone`, `delete <id>`, and
  `Always confirm with the user before calling forget`.
- `distill`: `rebuild <page-id>`, `force=true`, and
  `user-edited page prose is wiped`.
- `curate`: `revision_source_id`, `Perform no mutation until the user replies`,
  and `Ambiguous replies do not mutate`.
- `debrief`: `Pending-captures preview`, `MCP captures`, and
  `Write session log`.

`scripts/validate-plugin-contract.test.sh` should keep negative tests for:

- Missing Codex skill metadata.
- Wrong MCP prefix in either surface.
- Untracked inventory drift.
- Marketplace path drift.
- Claude stdio default drift.
- Missing destructive confirmation wording.
- Resolver parity drift between `plugin/bin/resolve-space.sh` and
  `plugin-codex/bin/resolve-space.sh` for the documented layers.

### README and Install UX

The README Codex section should list the full command surface once all skills
are present:

```text
/init, /brief, /capture, /recall, /distill, /pages, /curate, /forget,
/handoff, /debrief, /help
```

It should keep the current local-development install flow and mention that a
new Codex thread is required after reinstall so slash skills and the MCP server
reload.

The Codex plugin cachebuster should be updated with the plugin-creator helper
after the skill files change.

### Error Handling

Common failures should be explicit and non-mutating:

- Missing `wenlan` CLI: tell the user to run `/init`.
- MCP call failure: report the tool failure and stop that flow.
- Ambiguous curation reply: ask for a clearer numbered action; do not mutate.
- Missing page id or memory id for destructive flows: call recall/search first
  or ask the user for the exact id plus action verb.
- `~/.wenlan` git commit failure: silently skip after the existing retry pattern;
  data writes must not fail only because the audit trail commit failed.

### Testing

Implementation must be test-first:

1. Add validator expectations for the full Codex surface and observe the
   validator fail before porting the missing skills.
2. Add per-skill guardrail checks for destructive and interactive flows.
3. Port the skills.
4. Run:
   - `python3 scripts/validate-codex-plugin-slice.py`
   - `python3 scripts/validate-plugin-contract.py`
   - `bash scripts/validate-plugin-contract.test.sh`
   - `cargo test -p wenlan-types --test plugin_distribution pages_skill_replaces_read`
   - `git diff --check && git diff --cached --check`
   - Codex plugin manifest validation inside a temporary PyYAML venv

PR CI must pass before the PR leaves draft.

## Review Plan

Before implementation planning, run a boule-style design review:

- Form phase: reviewers evaluate the design on the merits.
- Attack phase: reviewers identify concrete risks, missing constraints, or
  false assumptions, citing the spec or current files.
- Defend phase: reviewers distinguish real issues from overreach.
- Judge phase: summarize accepted changes and rejected objections.

The review must not use a forced "find everything wrong" stance. The goal is
truthful pressure-testing, not objection generation.

## Success Criteria

- Codex slash menu can expose every Wenlan command currently present in Claude.
- Claude plugin files and behavior are unchanged.
- Shared contract reports every skill as shared.
- Validators fail loudly if a future edit breaks Codex metadata, MCP prefixes,
  destructive confirmations, or inventory parity.
- PR remains easy to review: one complete Codex surface pass, no daemon changes,
  no generator.
