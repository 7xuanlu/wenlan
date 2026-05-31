# Automations (ready to install)

Three deliverables turned from `overnight/05-automation-kit.md` into real files.
Nothing here is wired into the repo. You copy each file into place yourself.

## Artifacts

| File | Purpose | Risk |
|---|---|---|
| `skills/release-check/SKILL.md` | `/release-check [vX.Y.Z]` wraps `scripts/validate-versions.sh` and audits feat:/fix: bump intent before a release. | None. Read-only shell plus git. No build, no writes. |
| `skills/pr-hygiene/SKILL.md` | `/pr-hygiene` runs CI's exact gates locally before push (fmt --check, clippy --workspace --all-targets -D warnings, test --workspace --lib, origin-core no-tauri/axum grep). | Low. Runs clippy + lib tests (~60-90s). No writes. |
| `hooks/eval-citation-guard.json` + `hooks/eval-citation-guard.sh` | PreToolUse hook that blocks an Edit/Write adding an eval metric line with no provenance, enforcing the AGENTS.md single-run rule. | Medium. Regex heuristic. Can false-positive on a legit "50%" in prose and false-negative on numbers added outside an Edit/Write. Escape tokens: `scaffold`, `repro:`, `N>=`, `stddev`, `+/-`. |

## Install

### Skills

Copy each skill directory into the plugin skills tree:

```
cp -r overnight/automations/skills/release-check plugin/skills/release-check
cp -r overnight/automations/skills/pr-hygiene    plugin/skills/pr-hygiene
```

They become `/release-check` and `/pr-hygiene`. Both lean on `Bash` only, so
they work without the Origin MCP server running. CI's `plugin` job validates
`SKILL.md` frontmatter (`name:` / `description:` present in the first 5 lines),
which both files satisfy.

### Eval-citation guard hook

The hook needs two pieces: the config merged into your settings, and the script
on disk at the path the config points to.

1. Put the script where the config expects it (project scope):

   ```
   mkdir -p .claude/hooks
   cp overnight/automations/hooks/eval-citation-guard.sh .claude/hooks/eval-citation-guard.sh
   chmod +x .claude/hooks/eval-citation-guard.sh
   ```

   For user scope instead, use `~/.claude/hooks/` and change the `command` path
   in the config to `${HOME}/.claude/hooks/eval-citation-guard.sh`.

2. Merge the `hooks` block from `hooks/eval-citation-guard.json` into your
   settings file. Project scope is `.claude/settings.json`; user scope is
   `~/.claude/settings.json`. If a `hooks` key already exists, merge the
   `PreToolUse` array rather than overwriting it.

The config uses `${CLAUDE_PROJECT_DIR}` so it resolves the script relative to
the repo root in project scope. The script reads `tool_input.new_string` (Edit)
or `tool_input.content` (Write) from stdin, and denies via the documented
`hookSpecificOutput` / `permissionDecision: "deny"` path on exit 0.

`jq` must be on PATH for the hook to run.

## Notes

- The guard matches `Edit|Write` only. `MultiEdit` is intentionally not covered
  (its `edits[]` array shape differs); add a separate matcher if you want it.
- The skills mirror gates that already exist (`pre-commit`, `pre-push`,
  `ci.yml`). They shorten the feedback loop; they do not replace the real gates.
