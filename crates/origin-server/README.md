# origin-server

Local daemon for Origin. It owns the database, embeddings, search, refinery, page distillation, knowledge graph, and HTTP API on `127.0.0.1:7878`.

## Headless install

Most users install Origin through the [Claude Code plugin](../../plugin/.claude-plugin/README.md), which auto-runs the install script the first time `/init` runs. This page is for the other 10% — automation, servers, CI, or pre-flight installs without the plugin.

```bash
curl -fsSL https://raw.githubusercontent.com/7xuanlu/origin/main/install.sh | bash
export PATH="$HOME/.origin/bin:$PATH"
origin setup --basic       # non-interactive Basic Memory mode (default)
origin install             # register the launchd service
origin status              # verify
```

The installer downloads `origin-server` and `origin-mcp` into `~/.origin/bin/` and creates a small `origin` launcher.

## Setup modes

```bash
origin setup                  # interactive (1=Basic, 2=on-device model, 3=Anthropic key)
origin setup --basic          # non-interactive Basic Memory — used by the plugin's /init
origin model install          # opt into a local Qwen model (llama.cpp + Metal)
origin key set anthropic      # opt into Anthropic-backed extraction (BYOK)
origin doctor                 # diagnose daemon, model, and key state
```

Basic Memory works without a local LLM or API key: store, search, recall, and MCP memory are available immediately. On-device models and Anthropic keys unlock background refinery — auto entity extraction, page synthesis, recaps, knowledge-graph rethink.

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

The libSQL store itself lives at `~/Library/Application Support/origin/memorydb/origin_memory.db` (macOS app convention). `~/.origin/db` is a symlink so everything user-facing is browseable from a single tree. Use `open ~/.origin/`, `code ~/.origin/`, or symlink `~/.origin/pages/` into an Obsidian vault for the graph view.

## Service Commands

```bash
origin install      # install and load the launchd service
origin uninstall    # unload and remove the launchd service
origin status       # service + runtime status
```

The daemon listens on `127.0.0.1:7878`. MCP clients, the Claude Code plugin, and local tools call that local API.

## Build From Source

```bash
cargo build -p origin-server
cargo run -p origin-server
```

Or build a release binary and install it as a launchd service:

```bash
cargo build --release -p origin-server
./target/release/origin-server install
./target/release/origin-server status
```

First build takes several minutes while `llama.cpp` compiles for Metal.

## Main HTTP Surfaces

- Health/status: `/api/health`, `/api/status`, `/api/setup/status`
- Memory ingest/search: `/api/memory/store`, `/api/memory/search`, `/api/chat-context`
- Review and pages: `/api/memory/list`, `/api/memory/confirm/{id}`, `/api/distill`
- Model/key setup: `/api/on-device-model`, `/api/on-device-model/download`, `/api/setup/anthropic-key`

See [CLAUDE.md](../../CLAUDE.md) for the full route and module map.

## License

Apache-2.0.
