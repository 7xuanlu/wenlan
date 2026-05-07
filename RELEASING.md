# Releasing Origin (daemon side)

This document covers releases of the daemon (`origin-server`), CLI (`origin`), and shared crates (`origin-types`, `origin-core`). The desktop app ships from [7xuanlu/origin-app](https://github.com/7xuanlu/origin-app) on its own release cadence; the MCP server ships from [7xuanlu/origin-mcp](https://github.com/7xuanlu/origin-mcp).

## How release-please works

Merge conventional commits to `main` (e.g. `feat:`, `fix:`, `chore:`). The `release-please` workflow opens a "Release PR" automatically, bumping the version and updating `CHANGELOG.md`. Merge that PR to cut the release. Release-please then creates the git tag, which triggers the `release.yml` build workflow.

The `.release-please-manifest.json` is the canonical version source. The release-please workflow syncs all daemon `Cargo.toml` files via the `# x-release-please-version` marker.

## Manual override: bump-version.sh

If you need to cut a release without release-please (hotfix, first release, version correction):

```bash
bash scripts/bump-version.sh 0.2.0
```

This updates all daemon `Cargo.toml` files, regenerates `Cargo.lock`, and shows a diff summary. Review the diff, stage the files, and push. Then create and push the tag manually:

```bash
git tag v0.2.0
git push origin v0.2.0
```

The `release.yml` workflow triggers on any `v*` tag push.

## Version consistency gate

The `release.yml` workflow validates that the pushed tag version matches `crates/origin-server/Cargo.toml` before building. If out of sync, the build fails with instructions to run `bump-version.sh`.

## What the release workflow does

1. Validates version consistency (tag vs. `crates/origin-server/Cargo.toml`).
2. Builds `origin-server` for `aarch64-apple-darwin`.
3. Builds `origin-mcp` from the separate repo via `cargo install --git`.
4. Smoke-tests the daemon binary (`--help`).
5. Creates the GitHub release with standalone binaries attached.
6. After the release job succeeds, `publish-crates` publishes `origin-types` to crates.io if it changed since the previous tag.

**Note:** The `origin-mcp` npm package is published from the [origin-mcp repo](https://github.com/7xuanlu/origin-mcp), not from this repo. That repo owns all origin-mcp distribution: npm, crates.io, and Homebrew. The desktop DMG is built from [origin-app](https://github.com/7xuanlu/origin-app); see its `RELEASING.md` for that pipeline.

## Cross-repo coordination with origin-mcp

`origin-mcp` lives in a separate repo (`~/Repos/origin-mcp`, MIT license). The release workflow pulls its binary directly from that repo via `cargo install --git`. There is no automated version pinning between the two repos. Steps for a coordinated release:

1. Release `origin-mcp` first (or ensure `main` is in a good state).
2. Tag and push `origin` as described above.
3. The workflow will install the latest `origin-mcp` from `main` of that repo.

If you need to pin to a specific `origin-mcp` commit or tag, edit the `cargo install --git` step in `release.yml`.

## Required secrets

Configure these in the repository settings (Settings, Secrets and variables, Actions):

| Secret | Purpose |
| ------ | ------- |
| `CARGO_REGISTRY_TOKEN` | Publish `origin-types` to crates.io. Create at crates.io under Account Settings, API Tokens. |
| `RELEASE_TOKEN` | Fine-grained PAT (contents:write) used by release-please-action so its push triggers the next workflow run. GITHUB_TOKEN-driven pushes never fire downstream workflows. |
| `GITHUB_TOKEN` | Built-in. Used for GitHub release creation and release-please PR management. No setup needed. |
