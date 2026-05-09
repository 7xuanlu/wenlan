# Homebrew Formula & Release Pipeline for origin-mcp

**Date:** 2026-03-17
**Status:** Draft
**Priority:** P0

## Context

origin-mcp is an MCP server for Origin, distributed as a Rust binary. It currently has an npm distribution channel (`@origin-memory/mcp`) that downloads pre-built binaries from GitHub releases. The npm installer currently points to the `7xuanlu/origin` repo for binaries — this needs to move to `7xuanlu/origin-mcp` so the source repo owns its own releases.

This spec covers adding Homebrew as a second distribution channel and building the CI/CD release pipeline that serves all three channels:

1. **Homebrew** — `brew install 7xuanlu/tap/origin-mcp`
2. **npm** — `npx @origin-memory/mcp` (existing, needs repo pointer fix)
3. **crates.io** — `cargo install origin-mcp` (source build)

## Decision: Releases from `origin-mcp` repo

Binaries are built and released from `7xuanlu/origin-mcp`, not `7xuanlu/origin`. This decouples the MCP server's release cycle from the main Origin app.

## Prerequisites

Before the first release:
- Add `repository = "https://github.com/7xuanlu/origin-mcp"` to `Cargo.toml` (`cargo publish` requires it)
- Update `npm/package.json` `repository.url` from `7xuanlu/origin` to `7xuanlu/origin-mcp`
- Create the `7xuanlu/homebrew-tap` repo on GitHub

## 1. Homebrew Tap Repo

**New repo:** `7xuanlu/homebrew-tap`

Structure:
```
homebrew-tap/
  Formula/
    origin-mcp.rb
  README.md
```

Users install via:
```bash
brew install 7xuanlu/tap/origin-mcp
```

Or equivalently:
```bash
brew tap 7xuanlu/tap
brew install origin-mcp
```

Homebrew requires taps to be a separate repo named `homebrew-*`. This is enforced by `brew tap` which clones `github.com/<user>/homebrew-<name>`.

## 2. Binary packaging

Release binaries are packaged as `.tar.gz` archives, not bare binaries. Each archive contains a single `origin-mcp` executable. This is the standard Homebrew convention — it allows `stable.url` to work correctly and Homebrew's built-in extraction to handle the file.

Archive naming:
- `origin-mcp-darwin-arm64.tar.gz`
- `origin-mcp-darwin-x64.tar.gz`
- `origin-mcp-linux-x64.tar.gz`

The release workflow creates these by: `tar czf origin-mcp-<platform>.tar.gz origin-mcp`

## 3. Formula (`origin-mcp.rb`)

```ruby
class OriginMcp < Formula
  desc "MCP server for Origin — personal agent memory layer"
  homepage "https://github.com/7xuanlu/origin-mcp"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/7xuanlu/origin-mcp/releases/download/v#{version}/origin-mcp-darwin-arm64.tar.gz"
      sha256 "PLACEHOLDER"
    end
    on_intel do
      url "https://github.com/7xuanlu/origin-mcp/releases/download/v#{version}/origin-mcp-darwin-x64.tar.gz"
      sha256 "PLACEHOLDER"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/7xuanlu/origin-mcp/releases/download/v#{version}/origin-mcp-linux-x64.tar.gz"
      sha256 "PLACEHOLDER"
    end
  end

  def install
    bin.install "origin-mcp"
  end

  test do
    assert_match "origin-mcp", shell_output("#{bin}/origin-mcp --help", 2)
  end
end
```

Key details:
- Platform-specific `.tar.gz` download (Homebrew extracts automatically)
- Each archive contains a single `origin-mcp` binary, so `bin.install "origin-mcp"` works directly
- SHA256 checksums are computed and injected by the release workflow
- Test block verifies the binary runs (exit code 2 expected since no Origin server is running)

## 4. Release Workflow (`.github/workflows/release.yml`)

**Trigger:** Push a tag matching `v*` (e.g., `v0.1.0`)

### Steps

#### 4a. Build matrix

Cross-compile the Rust binary for each target:

| Target | Rust triple | Runner | Archive name |
|--------|------------|--------|-------------|
| macOS ARM64 | `aarch64-apple-darwin` | `macos-latest` (ARM64) | `origin-mcp-darwin-arm64.tar.gz` |
| macOS x64 | `x86_64-apple-darwin` | `macos-13` (Intel) | `origin-mcp-darwin-x64.tar.gz` |
| Linux x64 | `x86_64-unknown-linux-gnu` | `ubuntu-latest` | `origin-mcp-linux-x64.tar.gz` |

**Important:** `macos-latest` is ARM64 only. Intel builds use `macos-13`, which is the last Intel runner. This avoids cross-compilation issues.

Each job:
1. Checks out the repo
2. Installs the Rust toolchain for the target
3. Runs `cargo build --release --target <triple>`
4. Ad-hoc signs macOS binaries: `codesign --sign - --force target/<triple>/release/origin-mcp` (prevents Gatekeeper quarantine blocks on unsigned binaries)
5. Creates the tar.gz archive: `tar czf origin-mcp-<platform>.tar.gz -C target/<triple>/release origin-mcp`
6. Copies the bare binary with platform name: `cp target/<triple>/release/origin-mcp origin-mcp-<platform>`
7. Uploads both the `.tar.gz` archive and bare binary as workflow artifacts

#### 4b. Create GitHub Release

After all build jobs complete (runs on `ubuntu-latest`):
1. Downloads all artifacts
2. Creates a GitHub Release for the tag
3. Uploads `.tar.gz` archives as release assets (for Homebrew)
4. Uploads bare binaries as release assets (for npm)

#### 4c. Publish to crates.io

Runs `cargo publish` using a `CARGO_REGISTRY_TOKEN` secret. Requires `repository` field in `Cargo.toml` (see Prerequisites).

#### 4d. Update Homebrew tap

Runs on `ubuntu-latest`.

1. Downloads each `.tar.gz` artifact
2. Computes SHA256 for each: `sha256sum origin-mcp-*.tar.gz` (runs on Ubuntu where `sha256sum` is available)
3. Generates the formula using a shell heredoc that injects version and checksums:

```bash
VERSION="${GITHUB_REF_NAME#v}"  # strips 'v' prefix from tag

cat > Formula/origin-mcp.rb << FORMULA
class OriginMcp < Formula
  desc "MCP server for Origin — personal agent memory layer"
  homepage "https://github.com/7xuanlu/origin-mcp"
  version "${VERSION}"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/7xuanlu/origin-mcp/releases/download/v#{version}/origin-mcp-darwin-arm64.tar.gz"
      sha256 "${SHA_DARWIN_ARM64}"
    end
    on_intel do
      url "https://github.com/7xuanlu/origin-mcp/releases/download/v#{version}/origin-mcp-darwin-x64.tar.gz"
      sha256 "${SHA_DARWIN_X64}"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/7xuanlu/origin-mcp/releases/download/v#{version}/origin-mcp-linux-x64.tar.gz"
      sha256 "${SHA_LINUX_X64}"
    end
  end

  def install
    bin.install "origin-mcp"
  end

  test do
    assert_match "origin-mcp", shell_output("#{bin}/origin-mcp --help", 2)
  end
end
FORMULA
```

4. Clones `7xuanlu/homebrew-tap`, commits the updated formula, pushes using `HOMEBREW_TAP_TOKEN`

**Note:** The heredoc approach generates the formula from scratch each time. This is more reliable than sed-based substitution — no placeholder drift risk. The `#{version}` inside the Ruby string is Ruby interpolation (resolved at brew install time), while `${VERSION}` and `${SHA_*}` are shell variables (resolved at generation time).

#### 4e. Publish npm package

1. Runs `npm publish --access public` from the `npm/` directory using `NPM_TOKEN` secret
2. The version in `npm/package.json` must already be correct (updated manually before tagging — see release process)

### Secrets required

| Secret | Purpose |
|--------|---------|
| `CARGO_REGISTRY_TOKEN` | Publish to crates.io |
| `HOMEBREW_TAP_TOKEN` | Push formula updates to `homebrew-tap` repo (GitHub PAT with `repo` scope) |
| `NPM_TOKEN` | Publish `@origin-memory/mcp` to npm |

`GITHUB_TOKEN` (built-in) is used for creating the release on `origin-mcp`.

## 5. Changes to npm distribution

### `npm/install.js`

Two changes:
1. `REPO` from `"7xuanlu/origin"` to `"7xuanlu/origin-mcp"`
2. Tag format from `mcp-v${VERSION}` to `v${VERSION}`

```javascript
// Before
const REPO = "7xuanlu/origin";
// url: github.com/7xuanlu/origin/releases/download/mcp-v0.1.0/origin-mcp-darwin-arm64

// After
const REPO = "7xuanlu/origin-mcp";
// url: github.com/7xuanlu/origin-mcp/releases/download/v0.1.0/origin-mcp-darwin-arm64
```

**Note:** npm downloads bare binaries (not `.tar.gz`). The release workflow uploads both the `.tar.gz` archives (for Homebrew) and bare binaries (for npm) as release assets.

### `npm/package.json`

Update `repository.url` to `https://github.com/7xuanlu/origin-mcp`.

### Backwards compatibility

No published versions of `@origin-memory/mcp` exist on npm yet, so this is not a breaking change. Once published, the npm install URL is baked into each version — old versions would still point to `7xuanlu/origin` if they existed.

## 6. Release process (manual steps)

To cut a release:
1. Update `version` in `Cargo.toml`
2. Update `version` in `npm/package.json` to match
3. Commit: `release: v0.1.0`
4. Tag: `git tag v0.1.0`
5. Push: `git push && git push --tags`
6. CI handles: build, GitHub Release, crates.io, Homebrew tap, npm publish

Both `Cargo.toml` and `npm/package.json` versions are bumped manually before tagging to keep them in sync. The tagged commit contains the correct versions for all channels.

## 7. macOS code signing

All macOS binaries are ad-hoc signed in CI before archiving:
```bash
codesign --sign - --force target/<triple>/release/origin-mcp
```

This prevents macOS Gatekeeper from blocking the binary with "developer cannot be verified" errors. Ad-hoc signing is sufficient for Homebrew distribution — Apple Developer ID signing is not required for tap formulas.

**Note:** This only protects the Homebrew and npm install paths. Users who download binaries directly from GitHub Releases may still see Gatekeeper warnings (they can bypass with `xattr -d com.apple.quarantine origin-mcp`).

## Deliverables

| Item | Location |
|------|----------|
| Homebrew formula | `7xuanlu/homebrew-tap/Formula/origin-mcp.rb` |
| Release workflow | `.github/workflows/release.yml` |
| npm installer fix | `npm/install.js`, `npm/package.json` |
| Cargo.toml update | Add `repository` field |

## Out of scope

- Homebrew Core submission (requires notability criteria; use tap for now)
- Linux ARM64 support (can be added later to the build matrix)
- Windows support
- Auto-bumping versions (manual for now — could add a `release.sh` script later)
