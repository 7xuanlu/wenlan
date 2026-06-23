# wenlan-cli

Wenlan's product CLI. Use it to set up the local runtime, manage the daemon service, search and recall memory, store new memories, configure models/API keys, and run doctor checks.

License: Apache-2.0.

## Install

Recommended user setup:

```bash
npx -y @7xuanlu/wenlan setup
```

The setup package supports macOS (arm64, x64), Linux (x64, arm64), and Windows (x64). It downloads the platform-matching release archive, installs `origin`, `wenlan-server`, and `wenlan-mcp` into `~/.wenlan/bin/`, configures local memory, registers the daemon with the host's native service manager (launchd on macOS, systemd-user on Linux), and verifies status. On Windows, `wenlan install` is not supported in v1 (daemon does not yet speak the Windows Service Control Protocol); run `wenlan-server.exe` manually or via Task Scheduler.

For local development:

```bash
cargo install --path crates/wenlan-cli
```

Or build from the workspace:

```bash
cargo build -p origin --release
./target/release/wenlan --help
```

## Configuration

Set `ORIGIN_HOST` to point at a remote daemon:

```bash
export ORIGIN_HOST=http://127.0.0.1:7878  # default
```

## Subcommands

### `wenlan status`

Show daemon, native service (launchd / systemd-user / sc.exe), model, and API key state.

```bash
wenlan status
wenlan status --format json
```

### `wenlan setup`

Configure Wenlan's runtime mode.

```bash
wenlan setup                  # interactive
wenlan setup --basic          # local memory, no local model or API key
wenlan setup --model qwen3-4b # download/select a local model
wenlan setup --anthropic-api-key-env ANTHROPIC_API_KEY
```

### `wenlan install` / `wenlan uninstall`

Register or remove the daemon with the host's native service manager. The service runs the sibling `wenlan-server` binary next to `origin`.

- **macOS**: launchd user agent at `~/Library/LaunchAgents/com.wenlan.server.plist`.
- **Linux**: systemd user unit at `~/.config/systemd/user/wenlan-server.service`. `loginctl enable-linger` if you want it alive after logout.
- **Windows**: not yet supported in v1. The console-app daemon does not implement the Windows Service Control Protocol; sc.exe start times out. Run `wenlan-server.exe` manually or register a Task Scheduler logon task. Tracked follow-up.

```bash
wenlan install
wenlan uninstall
```

### `wenlan doctor`

Diagnose daemon reachability, native service state (launchd / systemd-user / sc.exe), model setup, and API key setup.

```bash
wenlan doctor
```

### `wenlan model`

Manage opt-in local models.

```bash
wenlan model list
wenlan model status
wenlan model install qwen3-4b
```

### `wenlan key`

Manage provider API keys.

```bash
wenlan key status
wenlan key set anthropic --env ANTHROPIC_API_KEY
wenlan key clear anthropic
```

### `origin mcp add <client>`

Configure Wenlan MCP for a supported client. This is the MCP-only path for Claude Code users who do not want the plugin, and for Codex, Cursor, Claude Desktop, VS Code, and Gemini CLI.

```bash
origin mcp add claude-code
origin mcp add codex
origin mcp add cursor --dry-run
```

Supported clients: `claude-code`, `codex`, `gemini`, `cursor`, `claude-desktop`, `vscode`.

When `wenlan-mcp` is installed next to the `origin` CLI, the generated config points at that binary. Otherwise it falls back to `npx -y wenlan-mcp`.

Use `--dry-run` to preview JSON config edits before writing them:

```bash
origin mcp add cursor --dry-run
```

### `wenlan search <query>`

Search memories (vector + FTS hybrid).

```bash
wenlan search "embedding model"
wenlan search "rust" --limit 5
wenlan search "rust" --format json | jq '.results[].score'
```

### `wenlan recall <query>`

Get the working memory bundle for a query (pages + decisions + relevant memories + graph context).

```bash
wenlan recall "what we agreed on for the API"
wenlan recall "memory layer" --format json
```

### `wenlan store [text] [--file <path>] [--type <type>]`

Store a memory. Provide content positionally, via `--file`, or pipe via stdin.

```bash
wenlan store "remember this insight" --type fact
wenlan store --file notes.md --type page
echo "stdin pipe content" | wenlan store --type quick_thought
```

### `wenlan list [--limit N] [--type X]`

List recent memories.

```bash
wenlan list
wenlan list --limit 5
wenlan list --type fact --format json
```

### `wenlan agents list/show/edit`

Manage registered agents.

```bash
wenlan agents list
wenlan agents show claude-code
wenlan agents edit claude-code --trust trusted --enabled true
```

### `wenlan space <list|add|default|move|show>`

Manage memory spaces (buckets).

```bash
wenlan space list
wenlan space add ideas --default
wenlan space show career
wenlan space default work
wenlan space move scratch career
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
