# Weekly self-review - scheduled Claude Code (web) routine

A scheduled remote routine that runs every Sunday and puts an honest weekly mirror in front of you, with a
5-line plan you can accept or ignore. Built on the two tools already in this kit. It is read-and-draft only;
it never pushes code.

## How to wire it

Claude Code on the web supports scheduled/triggered sessions (see
https://code.claude.com/docs/en/claude-code-on-the-web). Create a scheduled session against the `origin` repo
with the prompt below, cadence weekly (Sunday evening). Pick a network policy that allows the GitHub API so
the radar can run; if your policy blocks it, the dashboard still works and the radar section degrades to a
note.

## The session prompt (paste this as the scheduled task)

```
Run my weekly self-review. Steps:

1. Run: bash overnight/automations/weekly-review/weekly-review.sh
   (If a script is missing, run overnight/tools/self-dashboard.sh 7 and
   overnight/automations/competitor-radar/radar.sh directly.)

2. Read the output. Then write me a SHORT review with exactly these sections:
   - One sentence: was this an inward week or an outward week? (Use the
     dashboard's inward %. Over 50% = inward.)
   - The single most notable change in the competitor radar vs what I'd expect
     (a new entrant, someone passing me on stars, nobody moved).
   - Three lines max: the ONE outward task to do this week, why, and how I'll
     know it worked. Bias toward: publish writing, reply to a real user, ship a
     thing to humans. Never recommend "tune retrieval" or "improve CI."
   - If I shipped nothing outward two weeks running, say so bluntly in one line.

3. Open a draft Gmail with the review as the body, subject
   "Weekly self-review <date>". Do NOT send it. Do NOT push any code.

Keep it under 150 words total. Be honest, not encouraging.
```

## Why this routine and not a dashboard you forget to open

The dashboard already exists. The problem is not data, it is that nobody runs it. A scheduled session removes
the discipline requirement: the mirror shows up whether or not you feel like looking. The "never recommend
tune retrieval / improve CI" instruction is deliberate. Left open, you will ask the assistant for the inward
task, because it is the comfortable one. This routine refuses to hand it to you.

## VERIFICATION
- weekly-review.sh: `bash -n` clean; it shells out to the two tools already verified to run in this kit
  (self-dashboard.sh ran live; radar.sh ran live). The radar branch degrades gracefully on network block.
- The scheduled-session capability and the Gmail-draft-only constraint match the documented web-session
  feature set (see the docs link). The prompt is text the user pastes; nothing here executes automatically
  until the user creates the schedule. [INFERRED from product docs; the user confirms by creating it]
- Safety: the prompt explicitly says draft-only Gmail and no code push, matching this run's own rules.
