# origin-server

Local daemon for Origin. It owns the database, embeddings, search, refinery, page distillation, knowledge graph, and HTTP API on `127.0.0.1:7878`.

Most users install `origin-server` through the root installer:

```bash
curl -fsSL https://raw.githubusercontent.com/7xuanlu/origin/main/install.sh | bash
export PATH="$HOME/.origin/bin:$PATH"
origin setup
origin install
origin status
```

The installer downloads `origin-server` and `origin-mcp` into `~/.origin/bin/`, then creates a small `origin` launcher for setup and service commands.

## Setup Modes

```bash
origin setup                  # guided setup
origin setup --basic          # Basic Memory, no model or API key
origin model install          # opt into an on-device model
origin key set anthropic      # opt into Anthropic-backed extraction
origin doctor                 # diagnose daemon, model, and key state
```

Basic Memory works without a local LLM or API key: store, search, recall, and MCP memory are available immediately. On-device models and Anthropic keys unlock richer extraction, background refinement, and page synthesis.

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
