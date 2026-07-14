# Wenlan Codex plugin

The Codex plugin wraps the shared `wenlan-mcp` server with Codex slash skills.
Use it when you want the session workflow commands in Codex, not just raw MCP
tools.

## Install from this repo

```bash
npx -y wenlan setup
codex plugin marketplace add .
codex plugin add wenlan@wenlan-local
```

Start a new Codex thread after installing so the skills and MCP server load.
Then try `/setup`, `/brief`, `/capture <memory>`, `/recall <query>`,
`/pages <query>`, or `/handoff`.

## Development

After editing `plugin-codex`, reinstall it into Codex's plugin cache:

```bash
python3 ~/.codex/skills/.system/plugin-creator/scripts/update_plugin_cachebuster.py plugin-codex
codex plugin add wenlan@wenlan-local
```

The plugin runner uses `~/.wenlan/bin/wenlan-mcp` when available and falls back
to `npx -y wenlan-mcp@^0.13.2`. It passes `--agent-name codex` so captures are
labeled as Codex writes.

Before changing plugin skills, manifests, MCP runner wiring, or the local
marketplace, run the shared Claude/Codex contract checks:

```bash
python3 scripts/validate-codex-plugin-slice.py
python3 scripts/validate-plugin-contract.py
bash scripts/validate-plugin-contract.test.sh
```
