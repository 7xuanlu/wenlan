# Verification

Checks run 2026-05-31 against the repo. Every command below was executed, not assumed.

## JSON validity

- `jq . hooks/eval-citation-guard.json` -> PASS. Valid JSON. Single `PreToolUse`
  key, matcher `Edit|Write`, one command hook.

## Bash syntax

- `bash -n hooks/eval-citation-guard.sh` -> PASS. No syntax errors under
  `set -euo pipefail`.

## Hook event name and matcher schema

Confirmed against `https://code.claude.com/docs/en/hooks` (WebFetch, 2026-05-31):

- `PreToolUse` is a valid event name. PASS.
- Config schema matches docs: `hooks.PreToolUse[].matcher` (string, supports
  `Edit|Write` alternation), `hooks[].type = "command"`, `hooks[].command`,
  `timeout`. PASS.
- Block path matches docs: exit 0 with stdout JSON
  `hookSpecificOutput.{hookEventName:"PreToolUse", permissionDecision:"deny",
  permissionDecisionReason:"..."}`. The script emits exactly this. PASS.
- Input field: docs say `tool_input` carries tool args. Script reads
  `.tool_input.new_string` (Edit) with `.tool_input.content` (Write) fallback.
  PASS.

## Functional hook behavior (executed)

- Edit adding `accuracy 71.4%` with no provenance -> DENY JSON emitted. PASS.
- Edit adding `71.4% (N>=3, stddev 0.8, repro: ...)` -> exit 0, no output =
  allow. PASS.
- Write of plain prose, no metric -> exit 0, no output = allow. PASS.

## SKILL.md frontmatter

Compared against existing `plugin/skills/init/SKILL.md` and
`plugin/skills/capture/SKILL.md`. Valid keys: `name`, `description`,
`allowed-tools`, optional `argument-hint`.

- `skills/release-check/SKILL.md`: keys `name`, `description`, `argument-hint`,
  `allowed-tools`. PASS. (`argument-hint` present, matches `capture` which takes
  an arg.)
- `skills/pr-hygiene/SKILL.md`: keys `name`, `description`, `allowed-tools`.
  PASS. (`argument-hint` omitted because the skill takes no args, matching
  `init` which also omits it. Optional key, valid to drop.)

Also satisfies the CI `plugin` job check (`head -5` must contain `^name:` and
`^description:`): both files have `name:` on line 2 and `description:` on line 3.

## pr-hygiene dry-run vs ci.yml

Mapped each pr-hygiene step to its ci.yml gate:

| pr-hygiene step | ci.yml gate | Mirrored? |
|---|---|---|
| 2 crate-boundary grep `use tauri\|use axum` | AGENTS.md rule (not a literal CI step) | Yes. Extra protection. CI does not run this grep, stated in the skill. |
| 3 `cargo fmt --all --check` | `fmt` job: `cargo fmt --check --all` | Yes. Same flags, order differs (both valid). |
| 4 `cargo clippy --workspace --all-targets -- -D warnings` | `lint` job: identical | Yes, exact. |
| 5 `cargo test --workspace --lib` | `test` job: `cargo nextest run --workspace --lib` | Yes, equivalent. Plain cargo is the portable local form; nextest is a runner, not a different gate. |
| 6 eval-provenance diff scan | AGENTS.md single-run rule | Advisory only, matches hook. |

### Gates NOT mirrored (stated honestly)

- `lint` job also runs a `cargo metadata` check that origin-mcp uses a PATH dep
  for origin-types. Not mirrored. It is a packaging invariant unrelated to a
  normal feature push; pre-push does not check it either.
- `test` job integration steps: `Integration tests origin-cli + origin-server`,
  `chat_import_e2e`, `distillation_quality`. Need the fastembed model and run
  for minutes. Not mirrored by design (the skill says so; pre-push skips them
  too).
- Windows / macOS install round-trips and the main-only embedding canary.
  Platform/branch specific, not reproducible in a generic local push.
- detect-changes path filtering and the `conclusion` aggregate gate are CI
  orchestration, not local gates.

Conclusion: pr-hygiene mirrors the three correctness gates (fmt, clippy,
workspace-lib tests) exactly, adds the crate-boundary grep, and clearly
documents the integration/platform gates it cannot reproduce locally.

## Summary

| File | Result |
|---|---|
| `skills/release-check/SKILL.md` | PASS (frontmatter, wraps validate-versions.sh, feat:/fix: audit) |
| `skills/pr-hygiene/SKILL.md` | PASS (mirrors CI fmt/clippy/test + boundary grep) |
| `hooks/eval-citation-guard.json` | PASS (valid JSON, valid event + matcher) |
| `hooks/eval-citation-guard.sh` | PASS (valid syntax, deny/allow behavior verified) |
| `README.md` | PASS (install paths + per-artifact purpose and risk) |

## Commands run

```
jq . hooks/eval-citation-guard.json
bash -n hooks/eval-citation-guard.sh
grep -q '"PreToolUse"' hooks/eval-citation-guard.sh
grep -q 'permissionDecision' hooks/eval-citation-guard.sh
jq -r '.hooks.PreToolUse[0].matcher' hooks/eval-citation-guard.json
printf ... | bash hooks/eval-citation-guard.sh   # deny + 2 allow cases
sed -n '1,11p' skills/*/SKILL.md | grep -E '^(name|description|argument-hint|allowed-tools):'
WebFetch https://code.claude.com/docs/en/hooks   # event name + schema confirm
```
