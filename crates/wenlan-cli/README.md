# wenlan-cli

Wenlan's product CLI. Use it to set up the local runtime, manage the background service, search and recall memory, capture new memories, configure models/API keys, and run doctor checks.

License: Apache-2.0.

## Install

Recommended user setup:

```bash
npx -y wenlan setup
```

The setup package supports macOS (arm64, x64), Linux (x64, arm64), and Windows (x64). It downloads the platform-matching release archive, installs `wenlan`, `wenlan-server`, and `wenlan-mcp` into `~/.wenlan/bin/`, configures local memory, registers the background runtime with the host's native service manager (launchd on macOS, systemd-user on Linux, Task Scheduler on Windows), and verifies status.

For local development:

```bash
cargo install --path crates/wenlan-cli
```

Or build from the workspace:

```bash
cargo build -p wenlan --release
./target/release/wenlan --help
```

## Configuration

Set `WENLAN_HOST` to point at a remote daemon:

```bash
export WENLAN_HOST=http://127.0.0.1:7878  # default
```

## Subcommands

### `wenlan status`

Show background process, native service (launchd / systemd-user / sc.exe), model, and API key state.

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

### `wenlan background <on|off>`

Register or remove the background runtime with the host's native service manager. The service runs the sibling `wenlan-server` binary next to `wenlan`.

- **macOS**: launchd user agent at `~/Library/LaunchAgents/com.wenlan.server.plist`.
- **Linux**: systemd user unit at `~/.config/systemd/user/wenlan-server.service`. `loginctl enable-linger` if you want it alive after logout.
- **Windows**: per-user Task Scheduler ONLOGON task.

```bash
wenlan background on
wenlan background off
wenlan restart
```

### `wenlan doctor`

Diagnose runtime reachability, native service state (launchd / systemd-user / sc.exe), model setup, and API key setup.

```bash
wenlan doctor
```

### `wenlan models`

Manage opt-in local models.

```bash
wenlan models list
wenlan models status
wenlan models install qwen3-4b
wenlan models reranker lite
```

### `wenlan keys`

Manage provider API keys.

```bash
wenlan keys status
wenlan keys set anthropic --env ANTHROPIC_API_KEY
wenlan keys clear anthropic
```

### `wenlan connect <client>`

Configure Wenlan MCP for a supported client. This is the MCP-only path for Claude Code users who do not want the plugin, and for Codex, Cursor, Claude Desktop, VS Code, and Gemini CLI.

```bash
wenlan connect claude-code
wenlan connect codex
wenlan connect cursor --dry-run
```

Supported clients: `claude-code`, `codex`, `gemini`, `cursor`, `claude-desktop`, `vscode`.

When `wenlan-mcp` is installed next to the `wenlan` CLI, the generated config points at that binary. Otherwise it falls back to `npx -y wenlan-mcp`.

Use `--dry-run` to preview JSON config edits before writing them:

```bash
wenlan connect cursor --dry-run
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

### `wenlan capture [text] [--file <path>] [--type <type>]`

Store a memory. Provide content positionally, via `--file`, or pipe via stdin.

```bash
wenlan capture "remember this insight" --type fact
wenlan capture --file notes.md --type page
echo "stdin pipe content" | wenlan capture --type quick_thought
```

### `wenlan memories [--limit N] [--type X]`

List recent memories.

```bash
wenlan memories
wenlan memories --limit 5
wenlan memories --type fact --format json
```

### `wenlan agents list/show/edit`

Manage registered agents.

```bash
wenlan agents list
wenlan agents show claude-code
wenlan agents edit claude-code --trust trusted --enabled true
```

### `wenlan spaces <list|add|default|move|show>`

Manage memory spaces (buckets).

```bash
wenlan spaces list
wenlan spaces add ideas --default
wenlan spaces show career
wenlan spaces default work
wenlan spaces move scratch career
```

### `wenlan sources add <path>`

Register or resync a file or folder source.

```bash
wenlan sources add ~/Notes
wenlan sources add ~/Notes/project.md
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
- [github.com/7xuanlu/wenlan](https://github.com/7xuanlu/wenlan) — source
- [github.com/7xuanlu/wenlan-app/releases/latest](https://github.com/7xuanlu/wenlan-app/releases/latest) — desktop app downloads

## License

Apache-2.0. See top-level LICENSE.
