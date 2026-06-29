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
the sync points, and an accidental `feat:` minor bump. Pure shell plus git,
sub-second, no build.

## Argument parsing

- arg1: target tag like `v0.7.1`. If omitted, default to `v$(cat version.txt)`.

## Steps

Run in order. Stop and report at the first hard failure.

### 1. Resolve the tag

```
Bash: TAG="${1:-v$(tr -d '[:space:]' < version.txt)}"; echo "checking against $TAG"
```

### 2. Run the canonical validator

This is the source of truth. Do not reimplement its logic.

```
Bash: RELEASE_TAG="$TAG" bash scripts/validate-versions.sh
```

`validate-versions.sh` checks version.txt, the workspace `Cargo.toml` version,
the `origin-types` / `origin-core` dep versions, `Cargo.lock` for all five
crates, both npm `package.json` files, and `plugin/.claude-plugin/plugin.json`.
All must equal `RELEASE_TAG` minus the leading `v`.

If it exits non-zero, surface the exact drift line it printed (`ERROR: version
drift` or `ERROR: Cargo.lock drift`) and stop. Tell the user which file is
wrong. Do not continue to the bump audit.

### 3. Bump-type audit

Scan commits since the last release tag for `feat:` which would trigger an
unwanted minor bump (AGENTS.md "feat: bumps minor, not patch"):

```
Bash: LAST="$(git describe --tags --abbrev=0 2>/dev/null || echo '')"; RANGE="${LAST:+$LAST..}HEAD"; echo "commits since ${LAST:-repo start}:"; git log --format='%s' $RANGE | grep -E '^feat(\(|:|!)' || echo "  no feat: commits (patch bump territory)"
```

If any `feat:` lines appear, warn: "These trigger a MINOR bump. If you meant a
patch, rename the squash-merge PR title to fix: before merging."

### 4. Summarize

PASS only if step 2 passed AND the bump type in step 3 matches intent. If the
validator failed, the result is FAIL regardless of the bump audit.

## When to use

- Right before merging the open release-please PR.
- After manually editing any version file.

## When NOT to use

- Mid-feature work. This is a release-gate skill.

## Cost

Pure shell plus git. Sub-second. No build.
