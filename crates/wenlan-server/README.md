# origin-server

Local daemon for Origin. It owns the database, embeddings, search, distill cycles, page distillation, knowledge graph, and HTTP API on `127.0.0.1:7878`.

## Headless install

Most users install Origin through the [Claude Code plugin](../../plugin/.claude-plugin/README.md), which auto-runs the install script the first time `/init` runs. This page is for daemon internals. For terminal setup, use the product CLI:

```bash
npx -y @7xuanlu/origin setup
```

The installer downloads `origin`, `origin-server`, and `origin-mcp` into `~/.origin/bin/`. Cross-platform: macOS (arm64, x64), Linux (x64, arm64; glibc), Windows (x64). The `origin` CLI owns setup and service management; `origin-server` is the daemon binary the host's service manager runs (launchd on macOS, systemd-user on Linux, Task Scheduler ONLOGON task on Windows).

## Setup modes

```bash
origin setup                  # interactive (1=local memory, 2=on-device model, 3=Anthropic key)
origin setup --basic          # non-interactive local memory setup used by the plugin's /init
origin model install          # opt into a local Qwen model (llama.cpp + Metal)
origin key set anthropic      # opt into Anthropic-backed extraction (BYOK)
origin doctor                 # diagnose daemon, model, and key state
```

Local memory works without a local model or API key: store, search, recall, and MCP memory are available immediately. On-device models and Anthropic keys unlock distill cycles: auto entity extraction, page synthesis, recaps, and knowledge-graph rethink.

## Data layout

All user-facing data lives under `~/.origin/`:

```text
~/.origin/pages/               wiki pages distilled from your memories (md)
~/.origin/sessions/            session logs by date (md, written by /handoff)
~/.origin/sessions/_status/    current per-project goals + last-handoff timestamps
~/.origin/db/                  symlink to the libSQL store
~/.origin/bin/                 installed binaries
~/.origin/.git/                git repo — skills auto-commit per logical batch
```

The libSQL store lives under the platform data directory (`dirs::data_local_dir()/origin/memorydb/origin_memory.db`). On macOS that resolves to `~/Library/Application Support/origin/memorydb/`; on Linux `~/.local/share/origin/memorydb/`; on Windows `%LOCALAPPDATA%\origin\memorydb\`. `~/.origin/db` is a symlink on macOS + Linux so the user-facing tree stays single-rooted (Windows skips the symlink since it needs Developer Mode or admin and the alias is cosmetic). Use `open ~/.origin/`, `code ~/.origin/`, or symlink `~/.origin/pages/` into an Obsidian vault for the graph view.

## Service Commands

```bash
origin install      # register with the host service manager (launchd / systemd-user / schtasks)
origin uninstall    # remove from the service manager
origin status       # service + runtime status
```

On Windows the install path uses `schtasks.exe` to register a per-user `OriginServer` ONLOGON task and triggers it immediately. origin-server stays a plain console app — no Windows Service Control Protocol dispatcher is needed.

The daemon listens on `127.0.0.1:7878`. MCP clients, the Claude Code plugin, and local tools call that local API.

## Build From Source

```bash
cargo build -p origin-server
cargo run -p origin-server
```

Or build a release binary and register it with the host service manager:

```bash
cargo build --release -p origin -p origin-server
./target/release/origin setup --basic
./target/release/origin install
./target/release/origin status
```

First build on macOS takes several minutes while `llama.cpp` compiles for Metal. On Linux + Windows the build is CPU-only and finishes faster.

## Main HTTP Surfaces

- Health/status: `/api/health`, `/api/status`, `/api/setup/status`
- Memory ingest/search: `/api/memory/store`, `/api/memory/search`, `/api/chat-context`
- Review and pages: `/api/memory/list`, `/api/memory/confirm/{id}`, `/api/distill`
- Model/key setup: `/api/on-device-model`, `/api/on-device-model/download`, `/api/setup/anthropic-key`

See [CLAUDE.md](../../CLAUDE.md) for the full route and module map.

## Links

- [useorigin.app](https://useorigin.app) — project home
- [useorigin.app/docs/get-started](https://useorigin.app/docs/get-started) — install + verify the first memory loop
- [useorigin.app/docs/mcp-clients](https://useorigin.app/docs/mcp-clients) — connect Claude Code, Cursor, Codex, Claude Desktop, Gemini CLI
- [github.com/7xuanlu/origin](https://github.com/7xuanlu/origin) — source

## License

Apache-2.0.
