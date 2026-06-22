# origin-cli

Origin's product CLI. Use it to set up the local runtime, manage the daemon service, search and recall memory, store new memories, configure models/API keys, and run doctor checks.

License: Apache-2.0.

## Install

Recommended user setup:

```bash
npx -y @7xuanlu/origin setup
```

The setup package supports macOS (arm64, x64), Linux (x64, arm64), and Windows (x64). It downloads the platform-matching release archive, installs `origin`, `origin-server`, and `origin-mcp` into `~/.origin/bin/`, configures local memory, registers the daemon with the host's native service manager (launchd on macOS, systemd-user on Linux), and verifies status. On Windows, `origin install` is not supported in v1 (daemon does not yet speak the Windows Service Control Protocol); run `origin-server.exe` manually or via Task Scheduler.

For local development:

```bash
cargo install --path crates/wenlan-cli
```

Or build from the workspace:

```bash
cargo build -p origin --release
./target/release/origin --help
```

## Configuration

Set `ORIGIN_HOST` to point at a remote daemon:

```bash
export ORIGIN_HOST=http://127.0.0.1:7878  # default
```

## Subcommands

### `origin status`

Show daemon, native service (launchd / systemd-user / sc.exe), model, and API key state.

```bash
origin status
origin status --format json
```

### `origin setup`

Configure Origin's runtime mode.

```bash
origin setup                  # interactive
origin setup --basic          # local memory, no local model or API key
origin setup --model qwen3-4b # download/select a local model
origin setup --anthropic-api-key-env ANTHROPIC_API_KEY
```

### `origin install` / `origin uninstall`

Register or remove the daemon with the host's native service manager. The service runs the sibling `origin-server` binary next to `origin`.

- **macOS**: launchd user agent at `~/Library/LaunchAgents/com.origin.server.plist`.
- **Linux**: systemd user unit at `~/.config/systemd/user/origin-server.service`. `loginctl enable-linger` if you want it alive after logout.
- **Windows**: not yet supported in v1. The console-app daemon does not implement the Windows Service Control Protocol; sc.exe start times out. Run `origin-server.exe` manually or register a Task Scheduler logon task. Tracked follow-up.

```bash
origin install
origin uninstall
```

### `origin doctor`

Diagnose daemon reachability, native service state (launchd / systemd-user / sc.exe), model setup, and API key setup.

```bash
origin doctor
```

### `origin model`

Manage opt-in local models.

```bash
origin model list
origin model status
origin model install qwen3-4b
```

### `origin key`

Manage provider API keys.

```bash
origin key status
origin key set anthropic --env ANTHROPIC_API_KEY
origin key clear anthropic
```

### `origin mcp add <client>`

Configure Origin MCP for a supported client. This is the MCP-only path for Claude Code users who do not want the plugin, and for Codex, Cursor, Claude Desktop, VS Code, and Gemini CLI.

```bash
origin mcp add claude-code
origin mcp add codex
origin mcp add cursor --dry-run
```

Supported clients: `claude-code`, `codex`, `gemini`, `cursor`, `claude-desktop`, `vscode`.

When `origin-mcp` is installed next to the `origin` CLI, the generated config points at that binary. Otherwise it falls back to `npx -y origin-mcp`.

Use `--dry-run` to preview JSON config edits before writing them:

```bash
origin mcp add cursor --dry-run
```

### `origin search <query>`

Search memories (vector + FTS hybrid).

```bash
origin search "embedding model"
origin search "rust" --limit 5
origin search "rust" --format json | jq '.results[].score'
```

### `origin recall <query>`

Get the working memory bundle for a query (pages + decisions + relevant memories + graph context).

```bash
origin recall "what we agreed on for the API"
origin recall "memory layer" --format json
```

### `origin store [text] [--file <path>] [--type <type>]`

Store a memory. Provide content positionally, via `--file`, or pipe via stdin.

```bash
origin store "remember this insight" --type fact
origin store --file notes.md --type page
echo "stdin pipe content" | origin store --type quick_thought
```

### `origin list [--limit N] [--type X]`

List recent memories.

```bash
origin list
origin list --limit 5
origin list --type fact --format json
```

### `origin agents list/show/edit`

Manage registered agents.

```bash
origin agents list
origin agents show claude-code
origin agents edit claude-code --trust trusted --enabled true
```

### `origin space <list|add|default|move|show>`

Manage memory spaces (buckets).

```bash
origin space list
origin space add ideas --default
origin space show career
origin space default work
origin space move scratch career
```

## Output formats

- `--format auto` (default): table on TTY, JSON when piped.
- `--format json`: pretty-printed JSON.
- `--format table`: human-readable table.
- `--quiet` / `-q`: suppress success output (errors still go to stderr).

## Links

- [useorigin.app](https://useorigin.app) — project home
- [useorigin.app/docs/get-started](https://useorigin.app/docs/get-started) — install + verify the first memory loop
- [useorigin.app/docs/commands](https://useorigin.app/docs/commands) — Claude Code commands and MCP tools reference
- [useorigin.app/docs/troubleshooting](https://useorigin.app/docs/troubleshooting) — common failure modes
- [github.com/7xuanlu/origin](https://github.com/7xuanlu/origin) — source

## License

Apache-2.0. See top-level LICENSE.
