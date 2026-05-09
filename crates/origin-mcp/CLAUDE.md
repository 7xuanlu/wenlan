# CLAUDE.md

## What this is

`origin-mcp` is the MCP server for [Origin](https://github.com/7xuanlu/origin) — Where Personal AI Memory Compounds. It connects AI tools (Claude Code, Cursor, Claude Desktop, ChatGPT, Gemini CLI) to the Origin daemon via the Model Context Protocol (stdio and Streamable HTTP).

This is a separate repo from the main Origin app. It's MIT-licensed, lightweight, and depends only on `origin-types` (Apache-2.0) from crates.io.

## Build & Dev

```bash
# Build
cargo build

# Run (connects to Origin daemon on 127.0.0.1:7878)
cargo run

# Run with options
cargo run -- --origin-url http://localhost:7878

# Check + lint + test
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
```

**Requires a running Origin daemon.** Start one via the Origin desktop app or `origin-server` binary from the main repo. Without it, all tools return connection errors.

## Architecture

```
AI tool (stdio/HTTP) -> origin-mcp -> Origin daemon (HTTP :7878) -> libSQL + embeddings
```

| File | Purpose |
|------|---------|
| `src/main.rs` | Entry point, CLI args (clap), transport selection |
| `src/lib.rs` | Library root |
| `src/tools.rs` | `OriginMcpServer` struct, 5 MCP tools (capture/recall/context/forget/doctor), parameter structs, agent instructions. `capture` was renamed from `remember` in v0.4. |
| `src/client.rs` | `OriginClient` HTTP wrapper for the Origin daemon API |
| `src/types.rs` | Response types (kept minimal, forward-compatible with serde_json::Value where needed) |
| `src/serve.rs` | Streamable HTTP server (axum-based, for remote/tunnel access) |
| `src/auth.rs` | Bearer token auth for HTTP transport |
| `src/token.rs` | Token generation and validation |
| `npm/` | npm package wrapper (downloads binary on postinstall) |
| `tests/type_contract.rs` | Contract tests against origin-types + forward-compat tests |
| `tests/serve_integration.rs` | HTTP transport integration tests |

### Transport modes

- **Stdio** (default): local use with Claude Code, Cursor, etc. Full access to all tools.
- **HTTP** (Streamable HTTP): remote access via Cloudflare tunnel or direct. Blocks destructive operations (forget), injects `source_agent` from auth.

### Key design decisions

- **Forward-compatible context parsing**: `context_impl` parses the daemon response as `serde_json::Value` and extracts the `context` string, rather than strictly deserializing `ChatContextResponse`. This prevents breakage when the daemon schema changes ahead of the published `origin-types` crate.
- **origin-types from crates.io only**: CI gates against path dependencies. `origin-types` must always resolve from crates.io, never a local path. This ensures `cargo install origin-mcp` works for anyone.

## Distribution

This repo owns all origin-mcp distribution. The Origin repo does NOT publish origin-mcp.

| Channel | How | Automation |
|---------|-----|------------|
| **npm** (`origin-mcp`) | `npm/` wrapper, downloads binary on postinstall | Release workflow, Trusted Publishing (OIDC, no token) |
| **crates.io** (`origin-mcp`) | `cargo publish` | Release workflow, `CARGO_REGISTRY_TOKEN` secret |
| **Homebrew** (`7xuanlu/tap`) | Formula at `7xuanlu/homebrew-tap` | Release workflow auto-updates SHA256s, `HOMEBREW_TAP_TOKEN` secret |
| **GitHub Releases** | Binary tarballs (darwin-arm64, linux-x64) | Release workflow on `v*` tag push |

### Release process

1. Bump version in `Cargo.toml` and `npm/package.json`
2. Commit, tag `vX.Y.Z`, push tag
3. Release workflow builds, creates GitHub release, publishes to all channels

### Release gotchas

- **npm Trusted Publishing requires Node 24+.** The publish-npm job uses Node 24 because npm's OIDC handshake requires npm CLI 11.5.1+. Node 20 and 22 ship npm v10 which signs provenance (Sigstore) but fails the OIDC auth exchange with a misleading E404 (npm/cli#9088). Do not downgrade below Node 24.
- **SSH push may fail from CI or local.** If `git push origin` fails with SSH key errors, use HTTPS: `git push https://github.com/7xuanlu/origin-mcp.git main`
- **Re-tagging a failed release.** Delete the old tag locally and remotely, then re-tag on the fixed commit:
  ```bash
  git tag -d vX.Y.Z
  git push origin :refs/tags/vX.Y.Z
  git tag vX.Y.Z
  git push origin vX.Y.Z
  ```
  The GitHub Release from the previous run persists (build artifacts are fine if the build job succeeded). Only the publish jobs re-run.

### Platforms

- **darwin-arm64** (macOS Apple Silicon): primary target
- **linux-x64**: built and published, not yet tested in production
- **darwin-x64** (macOS Intel): dropped from CI due to severe GitHub Actions runner queue delays (30+ min). Intel Macs are EOL.

## CI

`.github/workflows/ci.yml` runs on every push to main and PRs:
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
- `cargo test`
- Gate: `origin-types` must resolve from crates.io (not a local path)
- `cargo publish --dry-run`

### CI gotchas

- **Rust version drift**: Local Rust (e.g., 1.93) may not catch lints that CI's stable (e.g., 1.95) does. Always check CI after pushing.
- **rmcp tool_router field**: The `tool_router!` macro requires a `tool_router` field on the server struct, but newer Rust flags it as dead_code. Keep `#[allow(dead_code)]` on it.

## Conventions

- **origin-types must stay lightweight.** Only serde + serde_json. No heavy deps.
- **Forward-compatible parsing.** When consuming daemon responses, prefer extracting specific fields from `serde_json::Value` over strict struct deserialization. The daemon evolves faster than the published crate.
- **No em-dashes** in user-facing text. Use periods, commas, colons, or restructure.
- **Parameterized SQL** in any query touching user input (inherited from Origin conventions).

## Secrets (GitHub Actions)

| Secret | Purpose |
|--------|---------|
| `CARGO_REGISTRY_TOKEN` | Publish to crates.io |
| `HOMEBREW_TAP_TOKEN` | Push formula updates to `7xuanlu/homebrew-tap` (fine-grained PAT, Contents read/write on that repo) |

npm uses Trusted Publishing (OIDC). No npm token secret needed.

## Related repos

- [origin](https://github.com/7xuanlu/origin): the desktop app, daemon, and core engine (AGPL-3.0 app, Apache-2.0 crates)
- [homebrew-tap](https://github.com/7xuanlu/homebrew-tap): Homebrew formula (auto-updated by release workflow)
