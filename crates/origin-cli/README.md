# origin-cli

Origin's product CLI. Use it to set up the local runtime, manage the daemon service, search and recall memory, store new memories, configure models/API keys, and run doctor checks.

License: Apache-2.0.

## Install

```bash
cargo install --path crates/origin-cli
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

Show daemon, launchd service, model, and API key state.

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

Register or remove the macOS LaunchAgent. The service runs the sibling `origin-server` binary next to `origin`.

```bash
origin install
origin uninstall
```

### `origin doctor`

Diagnose daemon reachability, launchd state, model setup, and API key setup.

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

## Output formats

- `--format auto` (default): table on TTY, JSON when piped.
- `--format json`: pretty-printed JSON.
- `--format table`: human-readable table.
- `--quiet` / `-q`: suppress success output (errors still go to stderr).

## License

Apache-2.0. See top-level LICENSE.
