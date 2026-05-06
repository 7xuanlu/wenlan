# origin-cli

Origin CLI — talk to the local Origin daemon from your terminal.

License: Apache-2.0.

## Usage

```bash
origin status                    # daemon health + version
origin search "embedding model"  # full-text + vector search
origin recall "what we agreed on" # working memory bundle for query
origin store "remember this"     # store a memory
origin list --limit 10           # recent memories
origin agents list               # registered agents + trust levels
```

Set `ORIGIN_HOST` to point at a remote daemon (default `http://127.0.0.1:7878`).

## Build

```bash
cargo build -p origin
./target/debug/origin --help
```
