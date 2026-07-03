# Command Surface Cleanup Design

## Goal

Make Wenlan's CLI and plugin commands read as user jobs instead of implementation details. This is a pre-1.0 breaking cleanup: old command names are removed, not kept as deprecated aliases.

## Decisions

- Replace `/init` with `/setup`. The command sets up and repairs Wenlan; it is not a project initializer.
- `/setup` reads the plugin manifest/runtime version instead of hardcoding an old release tag. This keeps repair tied to the installed plugin slice.
- Remove `/debrief`. `/handoff` remains the single end-of-session plugin command.
- Replace `wenlan install` / `wenlan uninstall` with `wenlan background on` / `wenlan background off`.
- Keep `wenlan restart`. Users understand this after update or config changes.
- Replace `wenlan mcp add <client>` with `wenlan connect <client>`.
- Replace `wenlan store` with `wenlan capture`.
- Replace `wenlan list` with `wenlan memories`.
- Replace `wenlan space ...` with `wenlan spaces ...`.
- Replace `wenlan model ...` with `wenlan models ...`.
- Replace `wenlan key ...` with `wenlan keys ...`.
- Move top-level `wenlan reranker <mode>` under `wenlan models reranker <mode>`.
- Replace `wenlan ingest <path>` with `wenlan sources add <path>`. The user job is adding a folder or file source, not thinking about ingestion internals.
- Keep `/curate` and `wenlan curate` for this pass. Prior context shows `/review` collides with Claude/Codex review semantics.
- Keep `/distill` and `wenlan pages` for this pass. Folding distillation into pages is a larger workflow redesign and should not be mixed into this packaging/runtime cleanup.

## Architecture

The CLI remains a thin HTTP/service client. This change reshapes the Clap command tree and documentation only; it will not add daemon endpoints or move business logic into the CLI.

Service management continues to use the existing `commands::service::{install, uninstall, restart}` functions internally, but the user-facing command becomes `background on/off`. MCP setup continues to use the existing config writers internally, but the public command becomes `connect`.

## Plugin Surface

The plugin skill directory will expose `/setup`, `/brief`, `/capture`, `/recall`, `/distill`, `/pages`, `/curate`, `/forget`, `/handoff`, and `/help`.

The plugin will not expose `/init` or `/debrief`. Hook text and skill docs will point users to `/setup` for repair.

## Compatibility

No deprecated aliases. Running the old commands will fail through Clap's normal unknown-command path. Docs and plugin contract tests will make that intentional so drift is loud.

## Testing

- Update plugin distribution tests to require `/setup`, reject `/init` and `/debrief`, and verify setup repair text uses `wenlan background on`.
- Require both Claude and Codex setup skills to use manifest/current-version repair instead of stale hardcoded release tags.
- Add CLI command contract tests for the public help output:
  - accepted commands include `background`, `connect`, `capture`, `memories`, `spaces`, `sources`, `models`, and `keys`;
  - removed commands are absent from help output: `install`, `uninstall`, `mcp`, `store`, `list`, `space`, `model`, `key`, `reranker`, and `ingest`;
  - `models --help` exposes `reranker`.
- Run targeted CLI and plugin contract tests after implementation.
