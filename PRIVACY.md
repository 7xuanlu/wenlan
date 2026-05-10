# Privacy Policy

Origin is a local-first personal memory system. This policy covers the Origin daemon, CLI, MCP server, and Claude Code plugin.

## What data Origin stores

Only what you explicitly capture: decisions, lessons, observations, project context, and wiki pages synthesized from those memories. Origin does not monitor, scrape, or ingest anything automatically.

## Where data is stored

All data stays on your machine:

- `~/.origin/pages/` -- wiki pages (Markdown)
- `~/.origin/sessions/` -- session logs (Markdown)
- `~/.origin/db/` -- symlink to the libSQL database at `~/Library/Application Support/origin/memorydb/`
- `~/.origin/bin/` -- installed binaries

The daemon listens on `127.0.0.1:7878` (localhost only). No data is sent to any remote server by default.

## Third-party services

None by default. Two opt-in integrations exist:

- **Anthropic API (BYOK):** If you run `origin key set anthropic`, your memories are sent to the Anthropic API for richer extraction and synthesis. Anthropic's privacy policy applies to that data. Origin does not store or relay your API key beyond the local config file.
- **On-device model:** If you run `origin model install`, a Qwen model is downloaded from Hugging Face Hub. No memory data leaves your machine in this mode.

## Telemetry

None. Origin collects no usage analytics, crash reports, or diagnostics.

## Data deletion

- Delete individual memories: `/forget` skill or `origin` CLI.
- Delete everything: remove `~/.origin/` and `~/Library/Application Support/origin/`.
- Uninstall the daemon: `origin uninstall`.

## Contact

Questions or concerns: open an issue at https://github.com/7xuanlu/origin/issues.

Last updated: 2026-05-10.
