---
name: handoff
description: >
  End-of-session ritual. Captures decisions, lessons, gotchas, and open
  threads. Writes a narrative session log to ~/.origin/sessions/ and stores
  granular memories via Origin MCP. Invoked as `/handoff`.
allowed-tools: ["Bash", "mcp__plugin_origin_origin__capture"]
---

# /handoff

End-of-session debrief. Three artifacts each pass:

1. **Granular MCP captures** — one per decision/lesson/gotcha (DB authoritative).
2. **Session log md** — narrative thread at `~/.origin/sessions/<YYYY-MM-DD-HHmm>-<slug>.md`.
3. **Project status md + json** — current goals + last-handoff timestamp at `~/.origin/sessions/_status/`.

These are orthogonal: captures are queryable atoms, session log is the
narrative thread, status file lets the next session see where we left off.

## Steps

### 1. Detect project + last handoff time

```
Bash: cd_repo=$(git -C "$PWD" rev-parse --show-toplevel 2>/dev/null); echo "${cd_repo:-no-git}"
```

- If output is a path → use the basename as `<project>` (e.g. `origin`).
- If `no-git` → use the cwd basename. Skip git steps below; rely entirely
  on conversation context.

Read `~/.origin/sessions/_status/handoff-<project>.json` for `lastHandoff`
timestamp (ISO-8601). If file missing, default to "12 hours ago".

### 2. Gather session context (parallel, only if git repo)

```
Bash: git -C <repo> log --oneline --since=<lastHandoff>
Bash: git -C <repo> status --short
Bash: git -C <repo> diff --stat HEAD~5..HEAD 2>/dev/null
Bash: git -C <repo> worktree list
```

Capture output. Use it alongside conversation history to infer what
happened. If not a git repo, skip — conversation context is the source.

### 3. Infer, do not ask

Synthesize silently from git output + conversation. Categorize each item:

- **Decisions** (architectural choice, tool/pattern selection — with WHY).
- **Lessons** (root cause, workaround, gotcha, technical insight).
- **Insights** (debugging discovery, unexpected behavior).
- **Open threads** (started but not finished, blockers).
- **Corrections** (things the user pushed back on).
- **Preferences** (workflow patterns observed).

Skip purely mechanical facts already in git (file paths, function names,
config values). The commit log preserves those.

### 4. MCP captures (one per item)

For each non-trivial item, call:

```
capture(content="<one self-contained sentence with WHY>", topic="<project>")
```

Atomic: one decision per call. Don't merge multiple items into one
memory. The daemon dedups against existing knowledge, so re-storing
known facts is a no-op.

Only surface items to the user BEFORE storing if they meet one of these
bars:

- Contradicts an existing memory (recall returned a conflicting fact).
- Marks a critical incident, irreversible action, or production change.
- You are uncertain whether the item is durable vs transient.

Otherwise just store and report counts at the end.

### 5. Write session log

Bash heredoc to `~/.origin/sessions/<YYYY-MM-DD-HHmm>-<slug>.md`:

```markdown
# Session <YYYY-MM-DD HH:MM> — <slug>

**Project:** <project>
**Range:** <lastHandoff> → <now>

## Accomplished
- <item>

## Decisions
- <decision and rationale>

## Lessons & Gotchas
- <root cause / workaround>

## Open Threads
- <what's unfinished>

## Captures stored
- <source_id_or_brief_summary>

## Git summary
<git log --oneline output>
```

`<slug>` = kebab-case 2-4 word summary (`session-handoff-md-writer`).

### 6. Update project status

Overwrite `~/.origin/sessions/_status/<project>.md`:

```markdown
# <Project> — Current Status

## Last session (<date>)
- <accomplished bullet>

## Open
- <unfinished item>

## Next
1. <next step>
2. <next step>
```

Single file per project. New session overwrites — this is the *current*
state, not a log.

### 7. Write timestamp

Overwrite `~/.origin/sessions/_status/handoff-<project>.json`:

```json
{
  "lastHandoff": "<ISO-8601 now>",
  "project": "<project>",
  "summary": "<one-line>"
}
```

Per-project file prevents parallel sessions from clobbering each other.

### 8. Auto-commit ~/.origin/

After writing the files above, snapshot the change so the user can `git
log` their memory's life timeline. Defensive — silent skip if `git` is
missing or `~/.origin/` is not a repo yet.

```
Bash: git -C ~/.origin add -A && \
      git -C ~/.origin -c user.name=Origin -c user.email=daemon@origin.local \
          commit --quiet -m "session: <slug>" 2>/dev/null || \
      (sleep 1 && git -C ~/.origin add -A && \
       git -C ~/.origin -c user.name=Origin -c user.email=daemon@origin.local \
           commit --quiet -m "session: <slug>" 2>/dev/null) || true
```

The retry handles index.lock races — the daemon may be writing to
`~/.origin/` at the same moment (auto-commit from captures). One-second
wait is enough for the daemon to release the lock.

### 9. Confirm

Print one summary block with captures broken out by category:

```
Handoff stored.
  Decisions: <N> (brief list)
  Lessons:   <N> (brief list)
  Insights:  <N> (brief list)
  Session:   ~/.origin/sessions/<filename>
  Status:    ~/.origin/sessions/_status/<project>.md
  Git:       <commit hash> session: <slug>
```

Show each category only if non-empty. List items as short phrases, not
full sentences — the session log has the details.

## When to use

- "Wrapping up", "let's call it", "we're done".
- Session about to close and useful state would otherwise be lost.

## When NOT to use

- Mid-flow capture during work → use `/capture` (single memory).
- Search / lookup → use `/recall`.
- One-off chat with no decisions or lessons — captures alone are enough.

## Notes on the three artifact classes

- **Memories** (MCP captures) live in the daemon DB only. Confirmation flips
  a `stability` flag — they never get exported to md.
- **Pages** are wiki-style syntheses written to `~/.origin/pages/` by the
  daemon when `/distill` runs. Citations link back to source memory ids.
- **Sessions** (this skill) live only at `~/.origin/sessions/`. They are
  the narrative axis: chronological, not topical. Browse them as a
  changelog of your work.
