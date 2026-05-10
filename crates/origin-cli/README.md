# origin-cli

Origin CLI. Talk to the local Origin daemon from your terminal.

Note: the release installer also creates an `origin` launcher for setup and service commands such as `origin setup`, `origin install`, and `origin doctor`. This crate is the source-built developer CLI for search, recall, store, list, and agent-management commands.

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

Show daemon health + version.

```bash
origin status
origin status --format json
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
