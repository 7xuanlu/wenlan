---
name: lint
description: Run Wenlan's read-only system diagnostics on demand.
argument-hint: "[profile:general|deep] [global|uncategorized|space:<name>]"
allowed-tools: ["Bash", "mcp__plugin_wenlan_wenlan__lint"]
---

# /lint

Run the canonical Wenlan lint report. Make exactly one lint MCP call through
`mcp__plugin_wenlan_wenlan__lint`. Omitted profile means `general`.

Accept at most one `profile:general|deep` token and one scope selector:
`global`, `uncategorized`, or `space:<name>`. Reject unknown profiles, repeated
profiles, or mixed scope selectors before calling any tool.

For `global`, omit `space`. For `uncategorized`, pass
`space="uncategorized"`. For `space:<name>`, pass that name. With no explicit
scope, run `$CLAUDE_PLUGIN_ROOT/bin/resolve-space.sh --cwd "$PWD"`; pass the
resolved non-empty space and otherwise omit it.

Render the canonical ordered report. State is `incomplete` when `complete` is
false, otherwise `findings` when actionable findings are nonzero, otherwise
`clean`. Show advisory findings without changing clean to findings. Do not
infer repairs or reveal anything beyond the report. There is no CLI or HTTP
fallback, no automatic rerun, and no second MCP call.
