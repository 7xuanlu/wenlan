# Contributing to Origin

Origin is a local-first personal AI memory layer. We welcome bug fixes, features, tests, docs, and design feedback.

This repo holds the daemon (`origin-server`), the CLI (`origin`), and the shared types/core (`origin-types`, `origin-core`). The Tauri desktop app lives in [7xuanlu/origin-app](https://github.com/7xuanlu/origin-app); the MCP server lives in [7xuanlu/origin-mcp](https://github.com/7xuanlu/origin-mcp). Bug reports for any of those pieces are welcome here or on the corresponding repo.

## Development Setup

**Requirements:** macOS Apple Silicon (M1+), [Xcode Command Line Tools](https://developer.apple.com/xcode/resources/), [Rust](https://rustup.rs/) (stable).

```bash
git clone https://github.com/7xuanlu/origin.git
cd origin
cargo build -p origin-server
```

Run the daemon directly:

```bash
cargo run -p origin-server
```

Or install as a launchd service:

```bash
cargo build --release -p origin-server
./target/release/origin-server install
./target/release/origin-server status
```

> First build can take several minutes while `llama.cpp` compiles for Metal.

### Running Tests

```bash
# Workspace tests
cargo test --workspace

# Per-crate
cargo test -p origin-types
cargo test -p origin-core --lib
cargo test -p origin-server
cargo test -p origin
```

### Linting

```bash
cargo fmt --check --all
cargo clippy --workspace --all-targets -- -D warnings
```

## Architecture Overview

- **Shared types**: `crates/origin-types` (Apache-2.0). Lightweight wire types shared with `origin-mcp` and `origin-app` via crates.io.
- **Core logic**: `crates/origin-core` (Apache-2.0). DB, embeddings, LLM engine, search, knowledge graph, refinery, eval. No tauri / no axum dependencies.
- **HTTP daemon**: `crates/origin-server` (Apache-2.0), serves `127.0.0.1:7878`.
- **CLI binary**: `crates/origin-cli` (Apache-2.0). The `origin` command for setup, install, search, recall, etc.
- **Desktop app** (separate repo): [7xuanlu/origin-app](https://github.com/7xuanlu/origin-app), AGPL-3.0-only.
- **Database**: libSQL (vectors + knowledge graph + FTS).

See `CLAUDE.md` for a full module-by-module breakdown.

## Finding Work

Look for issues labeled [`good first issue`](https://github.com/7xuanlu/origin/labels/good%20first%20issue) or [`help wanted`](https://github.com/7xuanlu/origin/labels/help%20wanted).

## Pull Request Process

1. Fork the repo and create a branch from `main`
2. Make your changes — keep PRs small and focused (one logical change per PR)
3. Ensure all tests pass and linting is clean
4. Open a PR using the template — describe what and how to test

CI runs `cargo fmt --check --all`, `cargo clippy --workspace --all-targets`, and `cargo test` across all daemon crates.

## Code Conventions

These conventions keep the codebase consistent. See `CLAUDE.md` for the full list.

- **SQL safety**: Always use parameterized queries — never interpolate user input into SQL strings
- **NULL semantics**: Store `Option<T>` as SQL NULL, not empty string
- **UTF-8 safety**: Never byte-index Rust strings (`&s[..n]`) — use `chars().take(n)` instead
- **Batch SQL**: Wrap multi-row insert/delete loops in `BEGIN`/`COMMIT` transactions
- **License headers**: The workspace is still normalizing SPDX headers after the package split. For new files, use the header that matches the package/file license even if nearby legacy files have not been cleaned up yet.

## Docs Layout

- In-repo docs live under `docs/` (especially `docs/plans/` for historical implementation context).
- Some personal/internal notes may exist outside the repository and are not required for contributors.

## License

This repo is Apache-2.0: `crates/origin-types`, `crates/origin-core`, `crates/origin-server`, and `crates/origin-cli`. The desktop app in [origin-app](https://github.com/7xuanlu/origin-app) is AGPL-3.0-only. The MCP server in [origin-mcp](https://github.com/7xuanlu/origin-mcp) is MIT.

By contributing, you agree that your changes will be licensed under the license that applies to the files you modify.
