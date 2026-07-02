---
name: debrief
description: >
  End a Codex session using the brief/debrief naming pair. Same artifact
  contract as handoff: captures, session log, and project status. Invoked as
  /debrief.
allowed-tools: ["Bash", "mcp__wenlan__capture", "mcp__wenlan__list_pending"]
user-invocable: true
---

# /debrief

End-of-session ritual for users who prefer `/brief` at the start and
`/debrief` at the end. This skill contains the full workflow directly so Codex
does not need to load another skill implicitly.

Artifacts:

1. Granular daemon memories from MCP captures.
2. Session log Markdown under `~/.wenlan/sessions/`.
3. Current project status under `~/.wenlan/sessions/_status/`.

## Detect project and range

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

Use the timestamp as the start of the session range.

## Resolve space

Call the Codex resolver:

```bash
resolved="$(plugin-codex/bin/resolve-space.sh --cwd "$PWD" 2>/dev/null)"
space="$(printf '%s\n' "$resolved" | cut -f1)"
source_layer="$(printf '%s\n' "$resolved" | cut -f2)"
```

If `space` is non-empty, pass it to every capture. If empty, omit `space`.

## Pending-captures preview

Call:

```text
mcp__wenlan__list_pending(limit=50)
```

Filter to captures newer than the session range start. If any exist, show only
the top three:

```text
Pending-captures preview (<N> total, top 3 shown):
1. <source_id> "<content>" (untrusted source: <agent>)

Default: continue. Run /curate captures later if you want to audit them.
```

Do not mutate pending captures here.

## Collect evidence

If in a git repo, read:

```bash
git -C "$repo" log --oneline --since="$lastHandoff"
git -C "$repo" status --short
git -C "$repo" diff --stat HEAD~5..HEAD 2>/dev/null || true
git -C "$repo" worktree list
```

Combine that evidence with conversation context.

## MCP captures

Store durable memories only:

```text
mcp__wenlan__capture(
  content="<one self-contained sentence with why>",
  memory_type="<decision|lesson|gotcha|preference|fact>",
  space="<resolved if non-empty>"
)
```

Map content types:

- Decisions -> `decision`
- Lessons -> `lesson`
- Insights and sharp edges -> `gotcha`
- Corrections -> `preference`
- Durable project facts -> `fact`

Skip transient task state and facts already obvious from git.

## Write session log

Write `~/.wenlan/sessions/<YYYY-MM-DD-HHmm>-<slug>.md`:

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

## Update status

Overwrite `~/.wenlan/sessions/_status/<project>.md`:

```markdown
# <Project> - Current Status

## Last session (<date>)
- <accomplished bullet>

## Active
- <fresh next item> (added <YYYY-MM-DD>)

## Backlog
- <older parked item> (added <YYYY-MM-DD>)
```

Write `~/.wenlan/sessions/_status/handoff-<project>.json` with `lastHandoff`,
`project`, and `summary`.

## Commit and report

Best-effort commit:

```bash
git -C ~/.wenlan add -A && \
  git -C ~/.wenlan -c user.name=Wenlan -c user.email=daemon@wenlan.local \
    commit --quiet -m "session: <slug>" 2>/dev/null || \
(sleep 1 && git -C ~/.wenlan add -A && \
  git -C ~/.wenlan -c user.name=Wenlan -c user.email=daemon@wenlan.local \
    commit --quiet -m "session: <slug>" 2>/dev/null) || true
```

Then print:

```text
Debrief stored.
  Session: ~/.wenlan/sessions/<filename>
  Status:  ~/.wenlan/sessions/_status/<project>.md
```
