---
name: lint
description: Run Wenlan's read-only system diagnostics from Codex.
argument-hint: "[profile:general|deep] [global|uncategorized|space:<name>]"
allowed-tools: ["Bash", "mcp__wenlan__lint"]
user-invocable: true
---

# /lint

Run the canonical Wenlan lint report with exactly one lint MCP call through
`mcp__wenlan__lint`. Omitted profile means `general`.

Accept at most one `profile:general|deep` token and one scope selector:
`global`, `uncategorized`, or `space:<name>`. Reject unknown profiles, repeated
profiles, and mixed or repeated scope selectors before any tool call.

For `global`, omit `space`. For `uncategorized`, pass
`space="uncategorized"`. For `space:<name>`, pass that name. With no explicit
scope, call:

```bash
plugin-codex/bin/resolve-space.sh --cwd "$PWD"
```

Pass a non-empty resolved space; otherwise omit it. Make exactly one lint MCP
call. Render checks in canonical order. State is `incomplete` when `complete`
is false, otherwise `findings` when actionable findings are nonzero, otherwise
`clean`. Advisory findings remain visible but do not change the state to
findings. Do not infer repairs, rerun, or expose anything outside the report.
There is no CLI or HTTP fallback.
