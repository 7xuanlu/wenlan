# agent-rigor

The discipline layer for agent-assisted engineering.

AI coding agents (Claude Code, Cursor, Codex, Copilot, Aider, Zed) are fast and confident. They also hallucinate "done," skip tests, gold-plate features nobody asked for, quote single cherry-picked benchmark numbers, and leave half-finished work behind. agent-rigor is the set of rules, file conventions, and git hooks that keep an agent honest while it works in your repo.

It is not a framework and not a dependency. It is a small, copy-in kit: one template file, two git hooks, and two short guides. You read them, adapt them, commit them. The agent then reads them on every run and is held to them mechanically.

## Provenance (be honest about where this came from)

agent-rigor is extracted from a real project: **Origin**, a local-first Rust memory daemon built over ~6 weeks across ~314 commits, roughly 37% of which went into eval, CI, and process discipline. The methodology below was not designed in the abstract. It was the working operating system of an actual codebase that ships a daemon, a CLI, a typed wire-format crate, and an MCP server.

This kit strips the Origin-specific identifiers (crate names, the database engine, the bind port) and keeps the transferable structure. Every rule in `AGENTS.template.md` is tagged `[VERIFIED AGENTS.md section]` and traces to a real section of the source `AGENTS.md`. Nothing here is invented discipline the author does not actually practice. If a rule could not be traced to the source, it is not in the template.

## Who this is for

- **Solo developers** running a coding agent on a real codebase who keep getting plausible-but-wrong output and want a mechanical floor.
- **Teams** standardizing how their agents behave across Claude Code, Cursor, Codex, and the rest. One `AGENTS.md` at the repo root, re-imported by vendor-specific files, so every agent reads the same rules.
- **Anyone doing eval-driven development** who needs an honest-metric-reporting standard that survives being quoted on Hacker News, in a deck, or in a README.

## The philosophy

Six load-bearing ideas, all drawn from the source project's Design Philosophy:

1. **Simple and elegant over clever.** Prefer the straightforward solution. Surprising code is usually wrong code.
2. **No speculative surface.** No features beyond what was asked. No abstraction for single-use code. No error handling for impossible cases.
3. **Surgical changes.** Touch only what the task requires. No "while I'm here" refactors, no adjacent cleanups, no formatting churn outside the diff.
4. **Challenge assumptions.** Do not follow the user's framing uncritically. If two interpretations exist, surface both instead of silently picking one. Push back when the approach is wrong.
5. **Verify before claiming done.** Run the tests, check the build, confirm the behavior. Evidence before assertions. This is the rule agents violate most.
6. **Honest metrics.** No benchmark number leaves the building without methodology attached. Single-run results are scaffold, not evidence.

The first five keep the agent from making a mess in your code. The sixth keeps you from making a mess in public.

## What is in the box

| File | What it is |
|---|---|
| `AGENTS.template.md` | A project-agnostic `AGENTS.md` you drop at your repo root and fill in. Distills the Design Philosophy, the L1-L8 test-layer model, eval citation discipline, the surgical-changes rule, and module-boundary checks. Every rule cited back to source. |
| `docs/eval-citation-discipline.md` | The standalone guide to honest metric reporting: single-run ban, N>=3 + stddev, schema-version refusal, receipt-only, layer attribution, per-case visibility. The rare and valuable part. |
| `docs/test-layers.md` | The L1-L8 model generalized into a reusable framework for splitting work across dev loop, pre-commit, pre-push, CI, and manual lanes, with the 3-question rule for deciding where a check belongs. |
| `hooks/pre-commit` | Auto-format on staged files plus targeted lint on the changed module. Generalized from the source `.githooks/pre-commit`. |
| `hooks/pre-push` | Workspace lint plus library tests, with a docs-only skip. Generalized from the source `.githooks/pre-push`. |
| `hooks/setup-hooks.sh` | One-line installer that points `core.hooksPath` at the kit. |
| `WHY.md` | The "why this exists" essay angle, tied to the reputation play. |

## Quickstart

```bash
# 1. Copy the kit into your repo
cp agent-rigor/AGENTS.template.md ./AGENTS.md
mkdir -p docs && cp agent-rigor/docs/*.md ./docs/
cp -r agent-rigor/hooks ./.githooks

# 2. Fill in the four bracketed placeholders in AGENTS.md
#    (build command, lint command, test command, module list)

# 3. If your agent vendor uses its own file (CLAUDE.md, .cursorrules),
#    point it at AGENTS.md with a one-line import so the rules stay in sync.
#    Example CLAUDE.md body: "@AGENTS.md"

# 4. Activate the hooks
bash .githooks/setup-hooks.sh

# 5. Tell the agent: "Read AGENTS.md before you start. Follow it exactly."
```

That is the whole setup. The template carries TODO markers where your project's specifics go. The hooks are language-agnostic in shape; swap the format/lint/test commands for your toolchain (the defaults shown assume Cargo, but the structure is the point, not the tool).

## Adapting the hooks to your language

The shipped hooks call `cargo fmt`, `cargo clippy`, and `cargo test` because that is what the source project uses. The reusable structure is:

- **pre-commit**: detect staged files of your language, auto-format them, re-stage, then run a fast lint scoped to the changed module. Fast means seconds.
- **pre-push**: skip entirely on docs-only pushes, otherwise run the full linter plus the fast (library/unit) test tier. Aim for under 90 seconds. Do not run coverage or slow integration tests here.

Replace `cargo fmt --all` with `prettier`/`black`/`gofmt`, `cargo clippy` with `eslint`/`ruff`/`golangci-lint`, and `cargo test --lib` with your fast test command. The split (auto-fix at commit, gate at push, heavy stuff in CI) is what transfers.

## VERIFICATION

This scaffold was built against two read-only inputs and verified as follows.

- **Source traced.** Every rule in `AGENTS.template.md` carries a `[VERIFIED AGENTS.md section]` tag naming the exact source section it came from (Design Philosophy, Local vs CI test responsibilities, the L1-L8 table, Eval Citation Discipline, Crate boundaries, Async and locking, Worktree cleanup after squash-merge). No rule appears in the template that is not present in the source `AGENTS.md`.
- **No invented discipline.** Rules the source does not practice were not added. Where the source is project-specific (libSQL connection pattern, the `Arc<MemoryDB>` sharing primitive, the 7878 bind port, release-please version-sync), those were either generalized into a project-agnostic form (module-boundary check, single-writer rule) or dropped, never copied verbatim as if universal.
- **Identifiers stripped.** No `origin-core`, `origin-server`, `origin-types`, `libSQL`, `axum`, `tauri`, `launchd`, or port `7878` survives in `AGENTS.template.md`. They are replaced with bracketed placeholders or generic terms (CORE module, HTTP layer). Spot-check: `grep -iE 'origin-core|libsql|7878|launchd' AGENTS.template.md` returns nothing.
- **Hooks grounded.** `hooks/pre-commit` and `hooks/pre-push` are generalized from the real `.githooks/pre-commit` and `.githooks/pre-push` (the staged-file detection, auto-format-and-restage, targeted-vs-workspace lint, and docs-only skip are all preserved from the originals). `hooks/setup-hooks.sh` mirrors the real `scripts/setup-hooks.sh`.
- **Internal consistency.** The template's test-layer references (L1-L8), its eval citation rules, and its boundary checks all point at the two docs (`test-layers.md`, `eval-citation-discipline.md`), and those docs do not contradict the template. The 3-question rule appears once (in `test-layers.md`) and is referenced, not duplicated, from the template.
