---
name: pr-hygiene
description: >
  Run CI's exact gates locally before you push: cargo fmt --check, clippy
  --workspace --all-targets -D warnings, cargo test --workspace --lib, and the
  origin-core no-tauri/axum boundary grep that ci.yml documents. Also flags
  feat: vs fix: bump intent. Every catch here is a CI round-trip avoided.
  Invoked as `/pr-hygiene`.
allowed-tools: ["Bash"]
---

# /pr-hygiene

Run the same gates CI (`ci.yml`) will run, locally, before you push. Mirrors the
`fmt`, `lint`, and `test` jobs plus the origin-core crate-boundary rule so the
required `conclusion` gate does not bounce your PR.

## Steps

Run in order. Steps 2-5 are hard gates. Steps 1 and 6 are advisory.

### 1. Title / bump intent (advisory)

Show the branch's commit subjects and warn on bump type:

```
Bash: git log --format='%s' origin/main..HEAD; echo "If the squash title starts with feat: it bumps MINOR. Use fix: for small changes."
```

### 2. Crate-boundary guard (hard gate)

AGENTS.md: origin-core must have NO tauri or axum dependencies. CI does not run
this grep directly, but the boundary is load-bearing. Expect zero hits:

```
Bash: if grep -rn "use tauri\|use axum" crates/origin-core/src/ ; then echo "FAIL: origin-core must not import tauri/axum"; exit 1; else echo "crate-boundary: ok"; fi
```

### 3. Format check (hard gate, mirrors CI `fmt` job)

CI runs `cargo fmt --check --all`. Mirror it exactly:

```
Bash: cargo fmt --all --check || { echo "FAIL: run cargo fmt --all"; exit 1; }
```

### 4. Clippy (hard gate, mirrors CI `lint` job)

CI runs `cargo clippy --workspace --all-targets -- -D warnings`. Mirror it
exactly:

```
Bash: cargo clippy --workspace --all-targets -- -D warnings
```

### 5. Library tests (hard gate, mirrors CI `test` workspace-lib step)

CI runs `cargo nextest run --workspace --lib`. Plain cargo is the portable
equivalent for local use:

```
Bash: cargo test --workspace --lib --quiet
```

### 6. Eval-number provenance on the diff vs main (advisory)

Same rule as the eval-citation guard hook (AGENTS.md single-run rule). A metric
added with no provenance token is a warning, not a hard fail:

```
Bash: git diff origin/main...HEAD | grep -E '^\+' | grep -iE '([0-9]+(\.[0-9]+)?[[:space:]]*%)|((f1|accuracy|recall|precision)[^0-9]{0,12}[0-9])' | grep -viE '(N[[:space:]]*[=>]|stddev|scaffold|repro:)' && echo "WARN: metric without provenance in diff (AGENTS.md single-run rule)" || echo "eval-citation: clean"
```

### 7. Summary

PASS only if steps 2-5 all passed. Steps 1 and 6 are warnings, not hard fails.

## Gates this skill does NOT mirror

- The `Integration tests origin-cli + origin-server`, `chat_import_e2e`, and
  `distillation_quality` CI steps. They need the fastembed model and run for
  minutes. CI owns them (AGENTS.md L4); pre-push deliberately skips them too.
- The Windows / macOS install round-trips and the embedding-only main canary.
  Platform-specific and main-only; not reproducible in a generic local push.

## When to use

- Before `git push` / opening a PR.

## When NOT to use

- Docs-only branches (pre-push already skips Rust gates for those).

## Cost

One clippy plus lib-test cycle. ~60-90s, same as the L3 pre-push hook.
