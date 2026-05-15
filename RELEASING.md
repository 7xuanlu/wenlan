# Releasing Origin (daemon side)

This document covers releases of the local runtime: `origin` CLI, `origin-server` daemon, `origin-mcp` connector, and shared crates (`origin-types`, `origin-core`). The desktop app ships from [7xuanlu/origin-app](https://github.com/7xuanlu/origin-app) on its own release cadence.

## How release-please works

Merge conventional commits to `main` (e.g. `feat:`, `fix:`, `chore:`). The `release-please` workflow opens a "Release PR" automatically, bumping the version and updating `CHANGELOG.md`. Merge that PR to cut the release. Release-please then creates the git tag, which triggers the `release.yml` build workflow.

The `.release-please-manifest.json` is the canonical version source. The release-please workflow syncs Cargo manifests, npm package manifests, plugin metadata, and pinned install URLs from `version.txt`.

## Manual override: bump-version.sh

If you need to cut a release without release-please (hotfix, first release, version correction):

```bash
bash scripts/bump-version.sh 0.2.0
```

This updates workspace Cargo versions, npm package manifests, plugin metadata, and pinned plugin URLs. Review the diff, stage the files, and push. Then create and push the tag manually:

```bash
git tag v0.2.0
git push origin v0.2.0
```

The `release.yml` workflow triggers on any `v*` tag push.

## Version consistency gate

The `release.yml` workflow validates that the pushed tag version matches `version.txt`, workspace Cargo, npm package manifests, and plugin metadata before building. If out of sync, the build fails with instructions to run `bump-version.sh`.

## What the release workflow does

1. Validates version consistency.
2. Builds `origin`, `origin-server`, and `origin-mcp` for `aarch64-apple-darwin`.
3. Smoke-tests `origin --help` and `origin-server --help`.
4. Creates the GitHub release with standalone binaries attached.
5. Publishes `origin-types` and `origin-mcp` to crates.io.
6. Publishes `origin-mcp` and `@7xuanlu/origin` to npm.
7. Updates the Homebrew tap for `origin-mcp`.

`origin-mcp` now lives in this monorepo under `crates/origin-mcp` and shares the workspace Apache-2.0 license. The desktop DMG is still built from [origin-app](https://github.com/7xuanlu/origin-app); see its `RELEASING.md` for that pipeline.

## Required secrets

Configure these in the repository settings (Settings, Secrets and variables, Actions):

| Secret | Purpose |
| ------ | ------- |
| `CARGO_REGISTRY_TOKEN` | Publish `origin-types` to crates.io. Create at crates.io under Account Settings, API Tokens. |
| `RELEASE_TOKEN` | Fine-grained PAT (contents:write) used by release-please-action so its push triggers the next workflow run. GITHUB_TOKEN-driven pushes never fire downstream workflows. |
| `GITHUB_TOKEN` | Built-in. Used for GitHub release creation and release-please PR management. No setup needed. |
