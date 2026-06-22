# origin-types

Shared wire-format types for [Origin](https://github.com/7xuanlu/origin) — a personal agent memory system.

This crate defines the HTTP API request/response types and core enums used by:
- `origin-server` (HTTP backend daemon)
- `origin-mcp` (MCP server wrapper for AI tools)
- `origin` (product CLI)
- downstream local clients that talk to the Origin daemon

## Stability

Pre-1.0. Expect minor version bumps to include breaking changes, per Rust 0.x convention. Types marked `#[doc(hidden)]` are not part of the stability contract and may change without a version bump.

## Links

- [useorigin.app](https://useorigin.app) — project home
- [useorigin.app/docs](https://useorigin.app/docs) — install + daily workflow
- [origin-mcp on crates.io](https://crates.io/crates/origin-mcp) — sibling crate
- [github.com/7xuanlu/origin](https://github.com/7xuanlu/origin) — source

## License

Apache-2.0
