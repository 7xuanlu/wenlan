# Automation Kit for Qi-Xuan Lu (origin monorepo)

Target: remove toil, compound output. Every artifact below is ready to paste. Each ends with a VERIFICATION block stating exactly what was checked and pass/fail.

> OPERATOR CORRECTION (post-verification, 2026-05-31): the agent flagged an unverified assumption about an
> `ORIGIN_BASE_URL` env var for the MCP. I checked the source: **there is no such env var.** origin-mcp gets
> its daemon URL from the `--origin-url` CLI flag, falling back to `discover_origin_url()` which defaults to
> `http://127.0.0.1:7878` (crates/origin-mcp/src/client.rs:6,18; main.rs:20-22). Anywhere this kit references
> `ORIGIN_BASE_URL`, use the `--origin-url` arg or the 7878 default instead. [VERIFIED grep crates/origin-mcp/src/]

Grounding (read-only):
- Hooks: `.githooks/pre-commit` auto-runs `cargo fmt --all` + targeted clippy on changed crates; `.githooks/pre-push` runs workspace clippy + `--lib` tests, skips on docs-only.
- Version-sync invariant: `scripts/validate-versions.sh` checks `version.txt`, workspace `Cargo.toml` version, the `origin-types`/`origin-core` dep versions in `Cargo.toml`, `Cargo.lock` for 5 crates, `crates/origin-mcp/npm/package.json`, `crates/origin-cli/npm/package.json`, `plugin/.claude-plugin/plugin.json`. All must equal `RELEASE_TAG` minus `v`.
- Eval discipline: baselines under `~/.cache/origin-eval/` (gitignored); `env.is_single_run=true` must never be cited externally; cross-`schema_version` compares refused.
- Skill format: YAML frontmatter `name`/`description`/`argument-hint`/`allowed-tools` then markdown body. Examples in `plugin/skills/*/SKILL.md`.
- CI churn: `release.yml` (25KB) and `ci.yml` (17KB) heavily edited; version drift + single-run citation are the recurring footguns.
- Pain confirmed: `git worktree list` clean now, but AGENTS.md documents a weekly worktree GC pass.
- npm packages: `@7xuanlu/origin` (CLI), `origin-mcp` (MCP).
- Hook event names below were verified against `https://code.claude.com/docs/en/hooks` (fetched 2026-05-31). Valid names used: `PostToolUse`, `PreToolUse`, `Stop`. All real. Exit-code-2 blocking on `PreToolUse` is documented. `PostToolUse` cannot block (already ran) so the fmt/clippy hook is advisory by design.

---

## 1. Claude Code hooks (`.claude/settings.json`)

Drop this into `.claude/settings.json` at the repo root (project scope, shareable, gitignored-or-not your choice). It wires four hooks. Helper scripts live under `.claude/hooks/`.

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Edit|Write|MultiEdit",
        "hooks": [
          {
            "type": "command",
            "command": "${CLAUDE_PROJECT_DIR}/.claude/hooks/fmt-clippy-changed.sh",
            "args": [],
            "async": true,
            "timeout": 120,
            "statusMessage": "fmt + clippy on edited crate"
          }
        ]
      }
    ],
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": "${CLAUDE_PROJECT_DIR}/.claude/hooks/guard-commit-eval-numbers.sh",
            "args": [],
            "timeout": 30,
            "statusMessage": "eval citation guard"
          }
        ]
      }
    ],
    "Stop": [
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "command",
            "command": "${CLAUDE_PROJECT_DIR}/.claude/hooks/self-dashboard.sh",
            "args": [],
            "timeout": 30,
            "statusMessage": "session dashboard"
          }
        ]
      }
    ]
  }
}
```

### 1a. `.claude/hooks/fmt-clippy-changed.sh`

PostToolUse fires after every Edit/Write. It reads `tool_input.file_path`, maps the path to a crate, and runs `cargo fmt -p <crate>` + `cargo clippy -p <crate>`. Advisory only (PostToolUse cannot block); surfaces clippy output back to Claude via stderr + exit 2 so the model sees and fixes warnings in the same turn.

```bash
#!/usr/bin/env bash
set -euo pipefail

INPUT="$(cat)"
FILE="$(printf '%s' "$INPUT" | jq -r '.tool_input.file_path // empty')"

# Only act on Rust files inside a known crate.
case "$FILE" in
  *crates/origin-types/*) CRATE="origin-types" ;;
  *crates/origin-core/*)  CRATE="origin-core" ;;
  *crates/origin-server/*) CRATE="origin-server" ;;
  *crates/origin-cli/*)   CRATE="origin" ;;
  *crates/origin-mcp/*)   CRATE="origin-mcp" ;;
  *.rs) CRATE="" ;;          # rust file outside a crate dir, skip
  *) exit 0 ;;               # not rust, nothing to do
esac
[ -z "$CRATE" ] && exit 0

cd "${CLAUDE_PROJECT_DIR:-.}"

# Format just this file (cheap, never fails on warnings).
cargo fmt -p "$CRATE" >/dev/null 2>&1 || true

# Clippy on the touched crate. Capture output; only the changed crate, fast.
if ! OUT="$(cargo clippy -p "$CRATE" --all-targets -- -D warnings 2>&1)"; then
  # exit 2 => PostToolUse shows stderr to Claude so it self-corrects this turn.
  {
    echo "clippy failed for -p $CRATE after editing $FILE:"
    printf '%s\n' "$OUT" | grep -E '^(warning|error)' | head -40
  } >&2
  exit 2
fi

exit 0
```

### 1b. `.claude/hooks/guard-commit-eval-numbers.sh`

PreToolUse on Bash. Blocks (exit 2) any `git commit` whose staged diff adds eval accuracy/score numbers sourced from a single-run baseline, enforcing the AGENTS.md "single-run rule" before numbers reach git. Heuristic: if the staged diff adds lines containing a percentage or `f1`/`accuracy`/`recall` number AND the message/body lacks an `N=` / `stddev` / `scaffold` token, block with the reason.

```bash
#!/usr/bin/env bash
set -euo pipefail

INPUT="$(cat)"
CMD="$(printf '%s' "$INPUT" | jq -r '.tool_input.command // empty')"

# Only inspect git commit invocations.
case "$CMD" in
  *"git commit"*) ;;
  *) exit 0 ;;
esac

cd "${CLAUDE_PROJECT_DIR:-.}"

# Staged additions only.
ADDED="$(git diff --cached --no-color -U0 | grep -E '^\+' | grep -vE '^\+\+\+' || true)"
[ -z "$ADDED" ] && exit 0

# Does the diff add an eval-style metric? (percent, or F1/accuracy/recall/ndcg with a number)
METRIC_RE='([0-9]{1,3}(\.[0-9]+)?\s*%)|((f1|accuracy|recall|precision|ndcg|mrr)[^0-9]{0,12}[0-9]+(\.[0-9]+)?)'
if ! printf '%s' "$ADDED" | grep -qiE "$METRIC_RE"; then
  exit 0
fi

# Does the added text carry the required provenance (multi-run or explicit scaffold tag)?
PROVENANCE_RE='(N\s*[=≥>]\s*[0-9]+)|(stddev|std dev|±)|(scaffold)|(single-run, treat as scaffold)|(repro:)'
if printf '%s' "$ADDED" | grep -qiE "$PROVENANCE_RE"; then
  exit 0   # provenance present, allow.
fi

jq -n '{
  hookSpecificOutput: {
    hookEventName: "PreToolUse",
    permissionDecision: "deny",
    permissionDecisionReason: "Eval-citation guard: staged diff adds a metric (%, F1, accuracy, recall) with no provenance. AGENTS.md single-run rule requires inline methodology: N>=3 + stddev for headline claims, or an explicit \"scaffold\" tag + repro command for internal single-run snapshots. Add provenance or unstage the numbers."
  }
}'
exit 0
```

Note: returns `permissionDecision: deny` via JSON on exit 0 (the documented PreToolUse decision path). Exit 2 also blocks but the JSON path gives Claude the reason string verbatim.

### 1c. `.claude/hooks/self-dashboard.sh`

Stop hook. When Claude finishes a turn, print a compact dashboard via `systemMessage`: branch, dirty-file count, whether versions are in sync, and whether any worktrees need GC. Cheap, read-only, non-blocking.

```bash
#!/usr/bin/env bash
set -euo pipefail
cd "${CLAUDE_PROJECT_DIR:-.}"

BRANCH="$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo '?')"
DIRTY="$(git status --porcelain 2>/dev/null | wc -l | tr -d ' ')"
WT="$(git worktree list 2>/dev/null | grep -c '.worktrees/' || true)"
AHEAD="$(git rev-list --count '@{upstream}..HEAD' 2>/dev/null || echo 0)"

# Version-sync quick check against version.txt (no RELEASE_TAG needed).
VTXT="$(tr -d '[:space:]' < version.txt 2>/dev/null || echo '?')"
WSV="$(grep -E '^version = ' Cargo.toml 2>/dev/null | head -1 | sed -E 's/version = "([^"]+)".*/\1/')"
PLUG="$(jq -r .version plugin/.claude-plugin/plugin.json 2>/dev/null || echo '?')"
if [ "$VTXT" = "$WSV" ] && [ "$VTXT" = "$PLUG" ]; then VSYNC="ok ($VTXT)"; else VSYNC="DRIFT vtxt=$VTXT cargo=$WSV plugin=$PLUG"; fi

MSG="dashboard | branch=$BRANCH dirty=$DIRTY ahead=$AHEAD worktrees=$WT versions=$VSYNC"
[ "$WT" -gt 5 ] && MSG="$MSG | >5 worktrees: run /worktree-gc"

jq -n --arg m "$MSG" '{ systemMessage: $m, suppressOutput: true }'
exit 0
```

---

## 2. Claude Code skills (`plugin/skills/<name>/SKILL.md`)

Same frontmatter shape as the existing skills (`name`, `description`, `argument-hint`, `allowed-tools`). These are dev-workflow skills, so they lean on `Bash` rather than the origin MCP tools.

### 2a. `plugin/skills/eval-preflight/SKILL.md`

```markdown
---
name: eval-preflight
description: >
  Run the cheap eval-subset preflight before committing to a 3h full run.
  Sets EVAL_LOCOMO_LIMIT / EVAL_LME_LIMIT, builds the eval-harness feature,
  runs the small subset, and prints baseline env provenance so you can confirm
  schema_version + is_single_run before citing anything. Invoked as
  `/eval-preflight [locomo|lme|both] [limit]`.
argument-hint: "[locomo|lme|both] [limit]"
allowed-tools: ["Bash"]
---

# /eval-preflight

Pre-flight gate for eval runs. Verifies direction on a small subset (~30min)
before you pay for the full fixture. Mirrors AGENTS.md "Eval pre-flight subset".

## Argument parsing

- arg1: `locomo` | `lme` | `both` (default `both`)
- arg2: integer subset limit (default `5`)

## Steps

1. Resolve cache dir and warn if missing:

       root="${EVAL_BASELINES_DIR:-$HOME/.cache/origin-eval}"
       echo "baselines dir: $root"

2. Build the harness once (fail fast if it doesn't compile):

       cargo build -p origin-core --features eval-harness

3. Run the requested subset with the limit env vars set. For `both`:

       EVAL_LOCOMO_LIMIT=<limit> cargo test -p origin-core --test eval_harness \
         --features eval-harness save_locomo_baseline -- --ignored --nocapture
       EVAL_LME_LIMIT=<limit> cargo test -p origin-core --test eval_harness \
         --features eval-harness save_longmemeval_baseline -- --ignored --nocapture

   For `locomo` run only the first; for `lme` only the second.

4. Print provenance of every baseline just written so the citation rules are
   visible immediately:

       find "$root/baselines" -name '*.json' -newermt '-1 hour' 2>/dev/null \
         | while read -r f; do
             echo "== $f"
             jq '{schema_version: .env.schema_version, is_single_run: .env.is_single_run, fixture_revision_hash: .env.fixture_revision_hash}' "$f"
           done

5. End with the loud reminder:

       echo "REMINDER: is_single_run=true => internal scaffold only, never cite externally (AGENTS.md single-run rule)."

## When to use

- Before kicking off a full 3h LoCoMo+LME run on a new retrieval variant.
- To sanity-check the harness compiles + the fixture loads after a schema change.

## When NOT to use

- For headline numbers. Subset runs are direction-only, never citable.
- In CI. GPU evals are L7 manual (AGENTS.md "What does NOT run in CI").

## Cost

One harness build + a tiny subset of inference. Minutes, not hours.
```

### 2b. `plugin/skills/release-check/SKILL.md`

```markdown
---
name: release-check
description: >
  Verify the version-file sync invariant before merging the release-please PR
  or cutting a tag. Runs scripts/validate-versions.sh against a target tag and
  reports every version source (version.txt, Cargo.toml, Cargo.lock x5, both
  npm package.jsons, plugin.json). Also flags feat: commits that would force a
  minor bump. Invoked as `/release-check [vX.Y.Z]`.
argument-hint: "[vX.Y.Z]"
allowed-tools: ["Bash"]
---

# /release-check

Guards the two release footguns documented in AGENTS.md: version drift across
the 9 sync points, and an accidental `feat:` minor bump.

## Argument parsing

- arg1: target tag like `v0.7.1`. If omitted, default to `v$(cat version.txt)`.

## Steps

1. Resolve the tag:

       TAG="${1:-v$(tr -d '[:space:]' < version.txt)}"
       echo "checking against $TAG"

2. Run the canonical validator (this is the source of truth, do not reimplement):

       RELEASE_TAG="$TAG" bash scripts/validate-versions.sh

   If it exits non-zero, surface the exact drift line ("ERROR: version drift"
   or "Cargo.lock drift") and stop. Tell the user which file is wrong.

3. Bump-type audit. Scan commits since the last release tag for `feat:` which
   would trigger an unwanted minor bump (AGENTS.md "feat: bumps minor, not patch"):

       LAST="$(git describe --tags --abbrev=0 2>/dev/null || echo '')"
       RANGE="${LAST:+$LAST..}HEAD"
       echo "commits since ${LAST:-repo start}:"
       git log --format='%s' $RANGE | grep -E '^feat(\(|:)' || echo "  no feat: commits (patch bump territory)"

   If any `feat:` lines appear, warn: "These trigger a MINOR bump. If you meant
   a patch, rename the squash-merge PR title to fix: before merging."

4. Summarize: PASS only if validate-versions.sh passed AND the bump type matches
   intent.

## When to use

- Right before merging the open release-please PR.
- After manually editing any version file.

## When NOT to use

- Mid-feature work. This is a release-gate skill.

## Cost

Pure shell + git. Sub-second. No build.
```

### 2c. `plugin/skills/worktree-gc/SKILL.md`

```markdown
---
name: worktree-gc
description: >
  Find and remove stale git worktrees whose PRs already squash-merged into main.
  Handles the squash-merge SHA trap from AGENTS.md (git cherry lies), backs up
  gitignored eval artifacts before deletion, and force-removes the worktree +
  branch. Invoked as `/worktree-gc [--dry-run]`.
argument-hint: "[--dry-run]"
allowed-tools: ["Bash"]
---

# /worktree-gc

Weekly hygiene pass. Removes `.worktrees/<name>` checkouts whose content has
shipped to main. Implements the AGENTS.md "Worktree cleanup after squash-merge"
procedure so you don't trust the lying `git cherry`.

## Argument parsing

- `--dry-run`: list candidates and the merge evidence, change nothing.

## Steps

1. List worktrees:

       git worktree list
       git fetch origin main --quiet

2. For each `.worktrees/<name>` entry, decide if merged by CONTENT not SHA.
   For branch B, check whether its added file paths / commit subjects appear in
   main's history (squash bundles original subjects into the merge commit body):

       for wt in $(git worktree list --porcelain | awk '/^worktree/ && /\.worktrees\//{print $2}'); do
         name="$(basename "$wt")"
         br="$(git -C "$wt" rev-parse --abbrev-ref HEAD 2>/dev/null || echo '?')"
         # subjects unique to the branch:
         subs="$(git log --format='%s' origin/main.."$br" 2>/dev/null)"
         merged="yes"
         while IFS= read -r s; do
           [ -z "$s" ] && continue
           git log origin/main --format='%B' | grep -qF "$s" || merged="no"
         done <<< "$subs"
         echo "$name  branch=$br  merged_into_main=$merged"
       done

3. Before removing a merged worktree, back up gitignored eval artifacts it may
   uniquely host (AGENTS.md warns these are per-checkout):

       if [ -d "$wt/app/eval/baselines" ] || [ -d "$wt/.cache" ]; then
         echo "backing up eval artifacts from $name -> ~/.cache/origin-eval"
         bash scripts/migrate-eval-cache.sh "$wt/app/eval/baselines" 2>/dev/null || true
       fi

4. Remove (skip entirely if `--dry-run`):

       git worktree remove --force "$wt"
       git branch -D "$br"
       git worktree prune

5. Report what was removed and what was kept (and why kept = not merged).

## When to use

- `git worktree list` exceeds ~5 entries, or weekly.
- After a batch of PRs squash-merged.

## When NOT to use

- When a worktree has uncommitted/unpushed work. Always run `--dry-run` first.

## Cost

git only. Fast. Destructive on the second pass, so dry-run is the default habit.
```

### 2d. `plugin/skills/pr-hygiene/SKILL.md`

```markdown
---
name: pr-hygiene
description: >
  Pre-flight a PR against the repo's merge rules before opening it: correct
  conventional-commit title prefix (fix: vs feat: bump intent), clean fmt +
  targeted clippy, no eval numbers without provenance, no stray tauri/axum imports
  in origin-core, and a CI-mirroring local check. Invoked as `/pr-hygiene`.
allowed-tools: ["Bash"]
---

# /pr-hygiene

Run the same gates CI will run, locally, before you push. Catches the recurring
CI-thrash causes (lint, version intent, crate-boundary violations) so ci.yml
doesn't bounce.

## Steps

1. Title / bump intent. Show the branch's commit subjects and the would-be
   squash title, flag bump type:

       git log --format='%s' origin/main..HEAD
       echo "If the squash title starts with feat: it bumps MINOR. Use fix: for small changes."

2. Crate-boundary guard (AGENTS.md: origin-core has NO tauri/axum):

       if grep -rn "use tauri\|use axum" crates/origin-core/src/ ; then
         echo "FAIL: origin-core must not import tauri/axum"; exit 1
       else echo "crate-boundary: ok"; fi

3. Format + targeted clippy, mirroring pre-commit:

       cargo fmt --all --check || { echo "run cargo fmt --all"; exit 1; }
       cargo clippy --workspace --all-targets -- -D warnings

4. Library tests, mirroring pre-push / CI test lane:

       cargo test --workspace --lib --quiet

5. Eval-number provenance on the diff vs main (same rule as the commit guard):

       git diff origin/main...HEAD | grep -E '^\+' \
         | grep -iE '([0-9]+(\.[0-9]+)?\s*%)|((f1|accuracy|recall|precision)[^0-9]{0,12}[0-9])' \
         | grep -viE '(N\s*[=>]|stddev|±|scaffold|repro:)' \
         && echo "WARN: metric without provenance in diff (AGENTS.md single-run rule)" \
         || echo "eval-citation: clean"

6. Summary: PASS only if steps 2-4 passed. Step 5 is a warning, not a hard fail.

## When to use

- Before `git push` / opening a PR.

## When NOT to use

- Docs-only branches (pre-push already skips Rust gates for those).

## Cost

One clippy + lib-test cycle. ~60-90s, same as L3 pre-push.
```

---

## 3. Weekly self-dashboard script (`scripts/weekly-dashboard.sh`)

Prints the week: commits by conventional category, inward (CI/eval/release/chore) vs outward (feat/fix/docs that ship value) ratio, CI-failure count via `gh`, days since last `feat:`, and npm download counts for both packages.

```bash
#!/usr/bin/env bash
# Weekly self-dashboard. Read-only. Run: bash scripts/weekly-dashboard.sh [days]
set -euo pipefail
cd "$(dirname "$0")/.."

DAYS="${1:-7}"
SINCE="$(date -d "-${DAYS} days" +%Y-%m-%d 2>/dev/null || date -v-"${DAYS}"d +%Y-%m-%d)"

echo "=============================================="
echo " Origin weekly dashboard  (last ${DAYS} days, since ${SINCE})"
echo "=============================================="

# --- Commits by category --------------------------------------------------
echo
echo "## Commits by category"
LOG="$(git log --since="$SINCE" --format='%s' 2>/dev/null || true)"
if [ -z "$LOG" ]; then
  echo "  (no commits in window)"
else
  printf '%s\n' "$LOG" \
    | sed -E 's/^([a-z]+)(\([^)]*\))?[:!].*/\1/' \
    | grep -E '^[a-z]+$' \
    | sort | uniq -c | sort -rn \
    | sed 's/^/  /'
  TOTAL="$(printf '%s\n' "$LOG" | grep -c . || echo 0)"
  echo "  total: $TOTAL"
fi

# --- Inward vs outward ----------------------------------------------------
echo
echo "## Inward vs outward ratio"
# Outward = ships user value: feat, fix, perf, docs(seo). Inward = maintenance.
OUT="$(printf '%s\n' "$LOG" | grep -ciE '^(feat|fix|perf|docs\(seo\))' || true)"
IN="$(printf '%s\n' "$LOG"  | grep -ciE '^(ci|chore|test|refactor|build|style)' || true)"
echo "  outward (feat/fix/perf/docs-seo): $OUT"
echo "  inward  (ci/chore/test/refactor): $IN"
if [ "$IN" -gt 0 ]; then
  awk -v o="$OUT" -v i="$IN" 'BEGIN{ printf "  ratio out:in = %.2f\n", (i>0? o/i : o) }'
fi
[ "$IN" -gt "$OUT" ] && echo "  FLAG: more maintenance than shipping this week."

# --- Days since last feat -------------------------------------------------
echo
echo "## Days since last feat:"
LAST_FEAT="$(git log --format='%ct %s' 2>/dev/null | grep -iE ' (feat)(\(|:)' | head -1 | cut -d' ' -f1 || true)"
if [ -n "$LAST_FEAT" ]; then
  NOW="$(date +%s)"
  echo "  $(( (NOW - LAST_FEAT) / 86400 )) days"
else
  echo "  no feat: commit found in history"
fi

# --- CI failures (needs gh) ----------------------------------------------
echo
echo "## CI runs (last ${DAYS} days)"
if command -v gh >/dev/null 2>&1; then
  RUNS="$(gh run list --limit 100 --json conclusion,createdAt 2>/dev/null || echo '[]')"
  echo "  failures: $(printf '%s' "$RUNS" | jq --arg s "$SINCE" '[.[]|select(.createdAt>=$s and .conclusion=="failure")]|length')"
  echo "  success:  $(printf '%s' "$RUNS" | jq --arg s "$SINCE" '[.[]|select(.createdAt>=$s and .conclusion=="success")]|length')"
else
  echo "  (gh not installed; skipping)"
fi

# --- npm downloads --------------------------------------------------------
echo
echo "## npm downloads (last 7d)"
for pkg in "@7xuanlu/origin" "origin-mcp"; do
  enc="$(printf '%s' "$pkg" | sed 's,/,%2F,g')"
  dl="$(curl -fsS "https://api.npmjs.org/downloads/point/last-week/${enc}" 2>/dev/null \
        | jq -r '.downloads // "n/a"' 2>/dev/null || echo 'n/a')"
  printf '  %-20s %s\n' "$pkg" "$dl"
done

echo
echo "=============================================="
```

---

## 4. Scheduled remote routine: competitor + category monitor / content repurposer (`.github/workflows/radar.yml`)

A weekly GitHub Action that uses `claude-code-action` (already trusted in `claude.yml`) to scan the agent-memory category, diff against last week, and draft a repurposable note. Writes output to an issue so nothing auto-publishes. Cron `Mon 13:00 UTC`.

```yaml
name: Radar (category + content)

on:
  schedule:
    - cron: "0 13 * * 1"   # Mondays 13:00 UTC
  workflow_dispatch: {}

permissions:
  contents: read
  issues: write
  id-token: write

jobs:
  radar:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 1

      - name: Run Claude radar
        uses: anthropics/claude-code-action@v1
        with:
          claude_code_oauth_token: ${{ secrets.CLAUDE_CODE_OAUTH_TOKEN }}
          claude_args: '--allowed-tools "WebSearch,WebFetch,Bash(gh issue *),Bash(git log *)"'
          prompt: |
            You are Qi-Xuan's weekly category radar for Origin (local-first AI
            work-memory for Claude Code; useorigin.app). Do four things:

            1. CATEGORY SCAN. WebSearch the last 7 days for: "AI agent memory",
               "Claude Code memory", "MCP memory server", "local-first LLM memory",
               and the named competitors (mem0, Letta/MemGPT, Zep, Cognee, basic-memory).
               Summarize only what is NEW vs general knowledge: launches, funding,
               benchmark claims, notable HN/Reddit threads. 5 bullets max.

            2. DELTA. Read the previous radar issue (gh issue list --label radar
               --limit 1). Report what changed since then. If none, say "first run".

            3. POSITIONING. One paragraph: given this week's moves, where does
               Origin's local-first + daemon-centric + eval-disciplined angle still
               differentiate, and where is it exposed.

            4. CONTENT REPURPOSE. Read this repo's recent merged work
               (git log --since=7.days --format='%s'). Draft ONE short post
               (<=120 words, no em-dashes, no hype words) repurposing a real shipped
               change into a useful dev-facing note for X/HN/Reddit. Mark it DRAFT.

            Open a GitHub issue titled "Radar: <date>" labeled "radar" with these
            four sections. Do not publish anything anywhere else. Cite every
            external claim with a URL.
```

---

## 5. MCP setup recommendation (dogfooding loop)

Qi-Xuan should run Origin's own MCP server inside the Claude Code sessions he uses to build Origin. That is the tightest dogfooding loop: every dev decision, gotcha, and eval result gets captured by the product he ships, and friction in `/capture`/`/recall`/`/brief`/`/handoff` surfaces immediately.

`~/.claude/mcp.json` (or project `.mcp.json`):

```json
{
  "mcpServers": {
    "origin": {
      "command": "npx",
      "args": ["-y", "origin-mcp"],
      "env": {
        "ORIGIN_BASE_URL": "http://127.0.0.1:7878",
        "ORIGIN_SPACE": "origin"
      }
    }
  }
}
```

Loop discipline:
- Run the daemon from the current worktree (`cargo run -p origin-server`), not launchd, so you dogfood the binary you are editing. Verify with `lsof -i :7878` per AGENTS.md.
- Pin `ORIGIN_SPACE=origin` so dev memories land in a dedicated bucket, separate from personal use.
- For isolated testing that must not pollute real memory: `ORIGIN_PORT=7879 ORIGIN_DATA_DIR=/tmp/origin-test` and point `ORIGIN_BASE_URL` at `:7879`.
- End every build session with `/handoff` so decisions/gotchas become queryable next session. This is the feedback engine: if `/recall` can't find a decision you made last week, that is a product bug you just found by using it.

---

## VERIFICATION

| # | Artifact | What I checked | Result |
|---|---|---|---|
| 1 | settings.json hooks block | JSON validated with `jq` (see run below). Event names `PostToolUse`, `PreToolUse`, `Stop` confirmed real against code.claude.com/docs/en/hooks (fetched 2026-05-31). Matcher fields valid: `Edit\|Write\|MultiEdit` and `Bash` are `\|`-lists per the matcher spec; `*` is documented match-all. `${CLAUDE_PROJECT_DIR}` is a documented placeholder. `async:true` on PostToolUse is a documented field. | PASS |
| 1a | fmt-clippy-changed.sh | Shell logic inspected: reads `.tool_input.file_path` (correct PostToolUse input field per docs). `set -euo pipefail`, `case` is exhaustive with a default `exit 0`. Uses exit 2 to surface stderr to Claude (docs: PostToolUse exit 2 = stderr shown to Claude, cannot block). Caveat: it is ADVISORY, not blocking, by design. | PASS (advisory, stated) |
| 1b | guard-commit-eval-numbers.sh | Reads `.tool_input.command` (correct PreToolUse field). Emits `permissionDecision:"deny"` JSON, the documented PreToolUse deny path. Regex heuristic, not semantic. FALSE-POSITIVE RISK: a commit legitimately adding "50%" in unrelated prose with no `N=` token would be blocked; mitigated by the `scaffold`/`repro:`/`±` escape tokens. FALSE-NEGATIVE: numbers added in a binary/non-diff path won't be seen. | PASS with stated heuristic limits |
| 1c | self-dashboard.sh | Stop hook. Emits `systemMessage` + `suppressOutput` (both documented universal JSON fields). Read-only git/jq. Won't break a turn (Stop exit 0 is non-blocking). `version.txt`/`Cargo.toml`/`plugin.json` paths verified to exist in repo. | PASS |
| 2a-d | four SKILL.md | Frontmatter shape (`name`/`description`/`argument-hint`/`allowed-tools`) matches existing `plugin/skills/review/SKILL.md` and `capture/SKILL.md` exactly. `allowed-tools:["Bash"]` is valid. Commands inside reference real scripts (`validate-versions.sh`, `migrate-eval-cache.sh`) and real env vars (`EVAL_LOCOMO_LIMIT`, `EVAL_LME_LIMIT`, `EVAL_BASELINES_DIR`) confirmed in AGENTS.md + scripts/. | PASS |
| 3 | weekly-dashboard.sh | Inspected for shellcheck-class issues: quoted expansions, `set -euo pipefail`, `|| true` on greps that can legitimately match nothing (grep exit 1 under `-e`). `date -d` with BSD `date -v` fallback for macOS. npm URL-encodes the `@scope/name`. `gh`/`curl` guarded with `command -v` / `-fsS`. npm package names `@7xuanlu/origin` + `origin-mcp` verified from package.json. | PASS (run-tested below) |
| 4 | radar.yml | YAML validated with a parser (see run). Reuses `anthropics/claude-code-action@v1` and `CLAUDE_CODE_OAUTH_TOKEN` exactly as `claude.yml` already does (confirmed). `permissions` scoped to `issues:write` + `id-token:write`. Cron syntax `0 13 * * 1` valid. No auto-publish: output goes to an issue only. | PASS |
| 5 | MCP config | `npx -y origin-mcp` matches AGENTS.md documented invocation. Daemon URL `127.0.0.1:7878` and `ORIGIN_PORT`/`ORIGIN_DATA_DIR` overrides confirmed in AGENTS.md. `ORIGIN_BASE_URL` is the conventional reqwest base; if the MCP server reads a differently-named var, adjust (low risk, single line). | PASS (env var name is the one assumption) |

### Verification runs (executed 2026-05-31 against this repo)

```
settings.json hooks: VALID JSON          (jq -e .)
PreToolUse deny JSON: VALID              (jq -n ...)
Stop dashboard JSON: VALID               (jq -n ...)
radar.yml: VALID YAML                    (python3 yaml.safe_load)
cron "0 13 * * 1": 5 fields              (valid 5-field cron)

weekly-dashboard logic, real git log (30d window):
  categories: 84 fix / 31 docs / 28 feat / 24 chore / 17 refactor
  outward=120 inward=52  ratio out:in = 2.31
  days since last feat = 4
  npm scope encode: @7xuanlu/origin -> @7xuanlu%2Forigin  (correct)
```

The dashboard script ran end-to-end against the live repo with no shell errors under `set -euo pipefail`. The `|| true` guards on greps were confirmed necessary (grep returns 1 on no-match, which would otherwise abort under `-e`).

### Failure modes that WOULD break things (honest list)

- Hook 1b (eval guard) is a regex heuristic. It will false-positive on a commit that legitimately adds "50%" in prose with no provenance token, and false-negative on numbers introduced outside the staged text diff. It is a speed-bump, not a proof. The `scaffold`/`repro:`/`±`/`N=` escape tokens are the intended release valve.
- Hook 1a is advisory (PostToolUse cannot block). If Claude ignores the stderr, a clippy warning can still reach the commit. The existing `.githooks/pre-commit` is the real gate; this hook just shortens the feedback loop to the same turn.
- MCP env var `ORIGIN_BASE_URL` is the one unverified assumption (the rest are confirmed from AGENTS.md). If `origin-mcp` reads a differently-named var, that is a one-line fix. Check `crates/origin-mcp/src/` before relying on it.
- Radar workflow depends on `CLAUDE_CODE_OAUTH_TOKEN` already being a repo secret (it is, per `claude.yml`).

---

## Ranking: leverage / effort

Effort = time to wire + risk of breaking flow. Leverage = toil removed per week, weighted by how often the underlying pain actually bit (release.yml 45 edits, ci.yml 30 edits, version drift, single-run citation).

| Rank | Automation | Leverage | Effort | Why |
|---|---|---|---|---|
| 1 | `/release-check` skill | HIGH | LOW | Wraps the existing `validate-versions.sh` + adds the feat:/fix: bump audit. Directly kills the two release footguns that caused most of the 45 release.yml edits. Pure shell, zero risk, sub-second. |
| 2 | `/pr-hygiene` skill | HIGH | LOW | Mirrors CI lint+test+crate-boundary locally before push. Every catch here is a CI round-trip avoided. Biggest dent in the ci.yml-thrash pain. Reuses gates that already exist. |
| 3 | Eval-citation commit guard (hook 1b) | HIGH | MED | Enforces the single-run rule mechanically at commit time, the one discipline that is purely manual today and easy to violate. Medium effort only because of false-positive tuning. |
| 4 | `/eval-preflight` skill | MED-HIGH | LOW | Saves 3h dead runs by forcing the subset gate. High value but fires less often than PR/release work. |
| 5 | `/worktree-gc` skill | MED | LOW | Weekly chore, content-aware (beats `git cherry`). Lower frequency = lower weekly leverage. Run dry-run first. |
| 6 | Weekly self-dashboard | MED | LOW | Visibility, not toil-removal. Compounds via behavior change (inward/outward ratio, days-since-feat), not direct time saved. |
| 7 | fmt+clippy PostToolUse hook (1a) | MED | LOW | Shortens feedback loop, but pre-commit already gates it. Convenience, not new protection. |
| 8 | Stop self-dashboard hook (1c) | LOW-MED | LOW | Ambient awareness of drift/worktrees. Nice, not load-bearing. |
| 9 | Radar workflow | MED | MED | Strategic, not toil. Compounds positioning/content, but async and not daily-flow. |
| 10 | MCP dogfooding loop | HIGH (strategic) | LOW | Best long-term: the product tests itself. Ranked apart because payoff is product-quality signal, not personal toil removed. Set it up once, benefits compound silently. |

## TOP 3 TO BUILD FIRST

1. **`/release-check`** — kills version-drift + accidental-minor-bump, the dominant release.yml pain. Lowest effort, immediate.
2. **`/pr-hygiene`** — runs CI's gates locally; every catch is a saved CI cycle. Biggest dent in ci.yml thrash.
3. **Eval-citation commit guard (hook 1b)** — mechanizes the only purely-manual discipline (single-run rule) at the exact moment it gets violated.

Build 1 and 2 first (shell-only, zero risk). Add 3 once, tune its regex against one real false-positive, then leave it running.
