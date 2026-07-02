---
name: handoff
description: >
  End a Codex session with Wenlan. Stores durable captures, writes a session
  log, updates project status, and previews pending captures. Invoked as
  /handoff.
allowed-tools: ["Bash", "mcp__wenlan__capture", "mcp__wenlan__list_pending"]
user-invocable: true
---

# /handoff

End-of-session ritual for Codex. Produce three artifacts:

1. Granular MCP captures in the daemon.
2. A narrative session log at `~/.wenlan/sessions/<YYYY-MM-DD-HHmm>-<slug>.md`.
3. Current project status at `~/.wenlan/sessions/_status/<project>.md` plus a
   timestamp JSON file.

## 1. Detect project and last handoff

Run:

```bash
repo="$(git -C "$PWD" rev-parse --show-toplevel 2>/dev/null || true)"
if [ -n "$repo" ]; then project="$(basename "$repo")"; else project="$(basename "$PWD")"; fi
status_json="$HOME/.wenlan/sessions/_status/handoff-${project}.json"
python3 - "$status_json" <<'PY'
import json, sys
from datetime import datetime, timedelta, timezone
path = sys.argv[1]
try:
    print(json.load(open(path)).get("lastHandoff", ""))
except FileNotFoundError:
    print((datetime.now(timezone.utc) - timedelta(hours=12)).strftime("%Y-%m-%dT%H:%M:%SZ"))
PY
```

Use the printed timestamp as `lastHandoff`.

## 2. Resolve the active space

Call the Codex resolver once:

```bash
resolved="$(plugin-codex/bin/resolve-space.sh --cwd "$PWD" 2>/dev/null)"
space="$(printf '%s\n' "$resolved" | cut -f1)"
source_layer="$(printf '%s\n' "$resolved" | cut -f2)"
```

If `space` is non-empty, print `Resolved space: <space> (from <source-layer>)`
and pass it to every `mcp__wenlan__capture` call. If it is empty, print
`Resolved space: none (unscoped)` and omit the `space` parameter.

## 3. Pending-captures preview

Call:

```text
mcp__wenlan__list_pending(limit=50)
```

Filter rows whose `created_at` is newer than `lastHandoff`. If none match, say
nothing. If any match, render:

```text
Pending-captures preview (<N> total, top 3 shown):

1. <source_id>  "<content>"  (untrusted source: <agent>)

Default: proceed. These captures stay pending. Run /curate captures before
/handoff if you want to walk them.
```

Do not prompt for per-item actions inside `/handoff`.

## 4. Gather session context

If `repo` is non-empty, collect:

```bash
git -C "$repo" log --oneline --since="$lastHandoff"
git -C "$repo" status --short
git -C "$repo" diff --stat HEAD~5..HEAD 2>/dev/null || true
git -C "$repo" worktree list
```

Use this together with conversation context. If there is no git repo, use the
conversation as the source.

## 5. MCP captures

Infer durable items without asking. Store only facts that will matter later.

| Label | `memory_type` | Use for |
|---|---|---|
| Decisions | `decision` | choices with rationale |
| Lessons | `lesson` | root cause, workaround, technical insight |
| Insights | `gotcha` | surprising behavior or sharp edge |
| Corrections | `preference` | user correction or style preference |
| Facts | `fact` | durable project, person, or tool fact |

For each item, call:

```text
mcp__wenlan__capture(
  content="<one self-contained sentence with why>",
  memory_type="<decision|lesson|gotcha|preference|fact>",
  space="<resolved if non-empty>"
)
```

Keep one memory per item. Skip file paths, commit hashes, and transient task
state the user can re-derive.

## 6. Write session log

Create `~/.wenlan/sessions/<YYYY-MM-DD-HHmm>-<slug>.md` with:

```markdown
# Session <YYYY-MM-DD HH:MM> - <slug>

**Project:** <project>
**Range:** <lastHandoff> -> <now>

## Accomplished
- <item>

## Decisions
- <decision and rationale>

## Lessons & Gotchas
- <root cause / workaround>

## Open Threads
- <what remains>

## Captures stored
- <source_id or short phrase>

## Git summary
<git log --oneline output>
```

Use a 2-4 word kebab-case slug.

## 7. Update project status

Overwrite `~/.wenlan/sessions/_status/<project>.md`:

```markdown
# <Project> - Current Status

## Last session (<date>)
- <accomplished bullet>

## Active
- <fresh next item> (added <YYYY-MM-DD>)
- <blocked item> (added <YYYY-MM-DD>) (gated: <trigger>)

## Backlog
- <older parked item> (added <YYYY-MM-DD>)
```

Active items are fresh next moves. Backlog items are parked but useful. Preserve
existing added dates when carrying items forward.

Also write `~/.wenlan/sessions/_status/handoff-<project>.json`:

```json
{
  "lastHandoff": "<ISO-8601 now>",
  "project": "<project>",
  "summary": "<one-line>"
}
```

## 8. Auto-commit `~/.wenlan`

Best effort only:

```bash
git -C ~/.wenlan add -A && \
  git -C ~/.wenlan -c user.name=Wenlan -c user.email=daemon@wenlan.local \
    commit --quiet -m "session: <slug>" 2>/dev/null || \
(sleep 1 && git -C ~/.wenlan add -A && \
  git -C ~/.wenlan -c user.name=Wenlan -c user.email=daemon@wenlan.local \
    commit --quiet -m "session: <slug>" 2>/dev/null) || true
```

Do not fail the handoff if this commit fails.

## 9. Report

Print:

```text
Handoff stored.
  Decisions:   <N>
  Lessons:     <N>
  Insights:    <N>
  Corrections: <N>
  Facts:       <N>
  Session:     ~/.wenlan/sessions/<filename>
  Status:      ~/.wenlan/sessions/_status/<project>.md
```

Only include non-empty labels.
