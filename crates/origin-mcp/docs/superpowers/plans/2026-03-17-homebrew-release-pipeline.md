# Homebrew Formula & Release Pipeline Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create a GitHub Actions release pipeline that builds origin-mcp binaries and publishes to Homebrew, npm, and crates.io from a single tag push.

**Architecture:** Tag push triggers a build matrix (3 platforms), then fan-out jobs create a GitHub Release, update a Homebrew tap repo, publish to crates.io, and publish to npm. The Homebrew formula downloads `.tar.gz` archives; npm downloads bare binaries.

**Tech Stack:** GitHub Actions, Homebrew (Ruby formula), Rust cross-compilation, npm, cargo publish

**Spec:** `docs/superpowers/specs/2026-03-17-homebrew-release-pipeline-design.md`

---

### Task 1: Update prerequisites in Cargo.toml and npm/package.json

**Files:**
- Modify: `Cargo.toml:1-6`
- Modify: `npm/package.json:15-18`

- [ ] **Step 1: Add `repository` field to Cargo.toml**

Add after the `license` line:

```toml
repository = "https://github.com/7xuanlu/origin-mcp"
```

Result — lines 1-7 of `Cargo.toml` should be:
```toml
[package]
name = "origin-mcp"
version = "0.1.0"
edition = "2021"
description = "MCP server for Origin — personal agent memory layer"
license = "MIT"
repository = "https://github.com/7xuanlu/origin-mcp"
```

- [ ] **Step 2: Update npm/package.json repository URL**

Change `repository.url` from `https://github.com/7xuanlu/origin` to `https://github.com/7xuanlu/origin-mcp`:

```json
"repository": {
  "type": "git",
  "url": "https://github.com/7xuanlu/origin-mcp"
}
```

- [ ] **Step 3: Verify Cargo.toml parses correctly**

Run: `cargo check 2>&1 | head -5`
Expected: no parse errors (may show compile warnings, that's fine)

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml npm/package.json
git commit -m "chore: add repository field and fix repo URLs for release pipeline"
```

---

### Task 2: Fix npm installer to point to origin-mcp repo

**Files:**
- Modify: `npm/install.js:10,27`

- [ ] **Step 1: Update REPO constant**

Change line 10 from:
```javascript
const REPO = "7xuanlu/origin";
```
to:
```javascript
const REPO = "7xuanlu/origin-mcp";
```

- [ ] **Step 2: Update download URL tag format**

Change line 27 from:
```javascript
const url = `https://github.com/${REPO}/releases/download/mcp-v${VERSION}/${binary}`;
```
to:
```javascript
const url = `https://github.com/${REPO}/releases/download/v${VERSION}/${binary}`;
```

- [ ] **Step 3: Verify the constructed URL is correct**

Run: `node -e "const p = require('./npm/package.json'); console.log('https://github.com/7xuanlu/origin-mcp/releases/download/v' + p.version + '/origin-mcp-darwin-arm64')"`

Expected: `https://github.com/7xuanlu/origin-mcp/releases/download/v0.1.0/origin-mcp-darwin-arm64`

- [ ] **Step 4: Commit**

```bash
git add npm/install.js
git commit -m "fix(npm): point installer to origin-mcp repo releases"
```

---

### Task 3: Create GitHub Actions release workflow

**Files:**
- Create: `.github/workflows/release.yml`

- [ ] **Step 1: Create the workflow file**

```yaml
name: Release

on:
  push:
    tags:
      - "v*"

permissions:
  contents: write

env:
  CARGO_TERM_COLOR: always

jobs:
  build:
    name: Build ${{ matrix.name }}
    runs-on: ${{ matrix.runner }}
    strategy:
      matrix:
        include:
          - name: darwin-arm64
            target: aarch64-apple-darwin
            runner: macos-latest
            os: macos
          - name: darwin-x64
            target: x86_64-apple-darwin
            runner: macos-13
            os: macos
          - name: linux-x64
            target: x86_64-unknown-linux-gnu
            runner: ubuntu-latest
            os: linux
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust toolchain
        run: |
          rustup toolchain install stable --profile minimal
          rustup target add ${{ matrix.target }}

      - name: Build release binary
        run: cargo build --release --target ${{ matrix.target }}

      - name: Ad-hoc sign macOS binary
        if: matrix.os == 'macos'
        run: codesign --sign - --force target/${{ matrix.target }}/release/origin-mcp

      - name: Package artifacts
        run: |
          tar czf origin-mcp-${{ matrix.name }}.tar.gz -C target/${{ matrix.target }}/release origin-mcp
          cp target/${{ matrix.target }}/release/origin-mcp origin-mcp-${{ matrix.name }}

      - name: Upload artifacts
        uses: actions/upload-artifact@v4
        with:
          name: origin-mcp-${{ matrix.name }}
          path: |
            origin-mcp-${{ matrix.name }}.tar.gz
            origin-mcp-${{ matrix.name }}

  release:
    name: Create GitHub Release
    needs: build
    runs-on: ubuntu-latest
    steps:
      - name: Download all artifacts
        uses: actions/download-artifact@v4
        with:
          merge-multiple: true

      - name: Create release
        uses: softprops/action-gh-release@v2
        with:
          generate_release_notes: true
          files: |
            origin-mcp-darwin-arm64.tar.gz
            origin-mcp-darwin-x64.tar.gz
            origin-mcp-linux-x64.tar.gz
            origin-mcp-darwin-arm64
            origin-mcp-darwin-x64
            origin-mcp-linux-x64

  publish-crates:
    name: Publish to crates.io
    needs: release
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust toolchain
        run: rustup toolchain install stable --profile minimal

      - name: Publish
        run: cargo publish
        env:
          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}

  update-homebrew:
    name: Update Homebrew tap
    needs: release
    runs-on: ubuntu-latest
    steps:
      - name: Download artifacts
        uses: actions/download-artifact@v4
        with:
          merge-multiple: true

      - name: Compute checksums and update formula
        env:
          HOMEBREW_TAP_TOKEN: ${{ secrets.HOMEBREW_TAP_TOKEN }}
        run: |
          VERSION="${GITHUB_REF_NAME#v}"

          SHA_DARWIN_ARM64=$(sha256sum origin-mcp-darwin-arm64.tar.gz | cut -d' ' -f1)
          SHA_DARWIN_X64=$(sha256sum origin-mcp-darwin-x64.tar.gz | cut -d' ' -f1)
          SHA_LINUX_X64=$(sha256sum origin-mcp-linux-x64.tar.gz | cut -d' ' -f1)

          git clone https://x-access-token:${HOMEBREW_TAP_TOKEN}@github.com/7xuanlu/homebrew-tap.git tap
          mkdir -p tap/Formula

          cat > tap/Formula/origin-mcp.rb << FORMULA
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
              assert_match "origin-mcp", shell_output("#{bin}/origin-mcp --help")
            end
          end
          FORMULA

          # Fix indentation (heredoc adds leading spaces from YAML nesting)
          sed -i 's/^          //' tap/Formula/origin-mcp.rb

          cd tap
          git config user.name "github-actions[bot]"
          git config user.email "github-actions[bot]@users.noreply.github.com"
          git add Formula/origin-mcp.rb
          git commit -m "origin-mcp ${VERSION}"
          git push origin HEAD:main

      - name: Verify formula syntax
        run: |
          ruby -c tap/Formula/origin-mcp.rb

  publish-npm:
    name: Publish to npm
    needs: release
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: actions/setup-node@v4
        with:
          node-version: "20"
          registry-url: "https://registry.npmjs.org"

      - name: Publish
        working-directory: npm
        run: npm publish --access public
        env:
          NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}
```

**Note on the heredoc:** The formula generation uses a single unquoted heredoc (`<< FORMULA`). Shell expands `${VERSION}` and `${SHA_*}` variables. Ruby's `#{version}` and `#{bin}` syntax passes through unchanged because `#{}` is not bash interpolation syntax — only `${}` and backticks are expanded in unquoted heredocs.

**Note on --help exit code:** The original spec had `shell_output("#{bin}/origin-mcp --help", 2)` but clap-based CLIs exit 0 on `--help`. The formula uses `shell_output("#{bin}/origin-mcp --help")` which defaults to expecting exit code 0.

- [ ] **Step 2: Validate YAML syntax**

Run: `ruby -ryaml -e "YAML.safe_load(File.read('.github/workflows/release.yml'))" && echo "YAML OK"`
Expected: `YAML OK`

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci: add release workflow for Homebrew, npm, and crates.io"
```

---

### Task 4: Create placeholder Homebrew formula for tap repo bootstrapping

This file is a reference copy — the real formula lives in `7xuanlu/homebrew-tap` and is auto-generated by CI. This copy serves as documentation and can be used to bootstrap the tap repo.

**Files:**
- Create: `homebrew/origin-mcp.rb`

- [ ] **Step 1: Write the placeholder formula**

```ruby
# This formula is auto-generated by the release workflow.
# It lives in 7xuanlu/homebrew-tap/Formula/origin-mcp.rb
# This copy is for reference only.
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
    assert_match "origin-mcp", shell_output("#{bin}/origin-mcp --help")
  end
end
```

- [ ] **Step 2: Validate Ruby syntax**

Run: `ruby -c homebrew/origin-mcp.rb`
Expected: `Syntax OK`

- [ ] **Step 3: Commit**

```bash
git add homebrew/origin-mcp.rb
git commit -m "docs: add reference Homebrew formula for tap bootstrapping"
```

---

### Task 5: Manual setup checklist (not code — human steps)

These steps must be done by the repo owner before the first release:

- [ ] **Step 1: Create `7xuanlu/homebrew-tap` repo on GitHub**

Go to GitHub → New Repository → `homebrew-tap` under `7xuanlu`. Initialize with a README. The release workflow will push the formula on first tag.

- [ ] **Step 2: Create a GitHub PAT for the tap repo**

Settings → Developer settings → Personal access tokens → Generate new token (classic).
Scope: `repo` (full control of private repositories).
Save as `HOMEBREW_TAP_TOKEN` secret in `7xuanlu/origin-mcp` repo settings.

- [ ] **Step 3: Add remaining secrets to origin-mcp repo**

In `7xuanlu/origin-mcp` → Settings → Secrets and variables → Actions:
- `CARGO_REGISTRY_TOKEN` — from https://crates.io/settings/tokens
- `NPM_TOKEN` — from `npm token create` (with publish access to `@origin-memory` scope)
- `HOMEBREW_TAP_TOKEN` — the PAT from step 2

- [ ] **Step 4: Cut the first release**

The release process for every version (including the first):

1. Update `version` in `Cargo.toml`
2. Update `version` in `npm/package.json` to match
3. Commit: `git commit -am "release: v0.1.0"`
4. Tag: `git tag v0.1.0`
5. Push: `git push && git push --tags`

For `v0.1.0` specifically, both files already show `0.1.0`, so steps 1-2 are no-ops. Just tag and push.

- [ ] **Step 5: Verify the release**

Monitor the Actions tab. Verify:
- All 3 binaries build successfully
- GitHub Release is created with 6 assets (3 `.tar.gz` + 3 bare binaries)
- `homebrew-tap` repo gets a commit with the formula
- crates.io shows the package
- npm shows `@origin-memory/mcp@0.1.0`

If any step fails, delete the tag (`git push --delete origin v0.1.0 && git tag -d v0.1.0`), fix, and re-tag.
