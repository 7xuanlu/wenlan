---
name: init
description: >
  Frictionless setup. Detects missing daemon, installs it, configures local
  memory, and verifies the full plugin → MCP → daemon round-trip. Run
  after `/plugin install wenlan@7xuanlu`, or any time the user says "set up
  wenlan", "is wenlan working", "fix wenlan".
allowed-tools: ["Bash", "mcp__plugin_wenlan_wenlan__doctor", "mcp__plugin_wenlan_wenlan__context"]
---

# /init

Self-healing setup. Goal: 30 seconds, two user actions max (install plugin,
type /init). Default backend is local memory — no local model, no API key, no
prompts. Local model and Anthropic key are opt-in upgrades documented in
`/help`.

## Steps

Run in order. Stop and report at the first failure that needs human
attention. Otherwise, push through automatically.

### 1. Daemon health probe

```
Bash: for i in 1 2 3; do curl -fsS -m 3 http://127.0.0.1:7878/api/health && break; sleep 1; done
```

- 200 OK → continue to version drift probe.
- Anything else → step 2.

### 1.5. Version drift probe

Compare daemon version vs plugin manifest version:

```
Bash: PLUGIN_JSON="${CLAUDE_PLUGIN_ROOT:-}/.claude-plugin/plugin.json"; if command -v python3 >/dev/null 2>&1 && [ -r "$PLUGIN_JSON" ]; then RESP="$(curl -fsS -m 3 http://127.0.0.1:7878/api/health)"; DAEMON_VER="$(printf '%s' "$RESP" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("version",""))')"; EXPECTED_VER="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("version",""))' "$PLUGIN_JSON")"; printf 'daemon=%s expected=%s\n' "$DAEMON_VER" "$EXPECTED_VER"; else echo "version_check=skipped"; fi
```

- Same version → skip to step 4.
- If `PLUGIN_JSON` is unreadable or `python3` is missing, skip to step 4;
  the daemon is healthy and the hook will keep surfacing a mismatch if one
  exists.
- If mismatch, repair the runtime (no human prompts):

```
Bash: PLUGIN_JSON="${CLAUDE_PLUGIN_ROOT:-}/.claude-plugin/plugin.json"; EXPECTED_VER="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("version",""))' "$PLUGIN_JSON")"; curl -fsSL https://raw.githubusercontent.com/7xuanlu/wenlan/v${EXPECTED_VER}/install.sh | bash
Bash: export PATH="$HOME/.wenlan/bin:$PATH" && wenlan setup --basic && wenlan install
```

Then continue to step 3.

### 2. Bootstrap (auto-install if missing)

Detect whether the `wenlan` CLI is on PATH:

```
Bash: command -v wenlan >/dev/null 2>&1 && echo present || echo absent
```

If `absent`, run the installer (no human prompts):

```
Bash: curl -fsSL https://raw.githubusercontent.com/7xuanlu/wenlan/v0.9.3/install.sh | bash
```

Then add it to PATH for the current session and configure local memory
non-interactively:

```
Bash: export PATH="$HOME/.wenlan/bin:$PATH" && wenlan setup --basic && wenlan install
```

If `present` (CLI exists, daemon down), just install + start:

```
Bash: wenlan setup --basic 2>/dev/null || true; wenlan install
```

`wenlan setup --basic` is idempotent — safe to re-run. `wenlan install`
writes the launchd plist and starts the daemon.

### 3. Re-probe daemon health

```
Bash: for i in 1 2 3 4 5; do curl -fsS -m 3 http://127.0.0.1:7878/api/health && break; sleep 1; done
```

If the daemon still isn't reachable after ~5s, surface the error and stop.
Likely cause: launchd plist load failure, port 7878 occupied by another
process, or macOS Tahoe Metal init issue (daemon degrades but still binds —
check `lsof -ti :7878`).

### 4. Doctor (verify backend)

Call the `wenlan` MCP server's `doctor` tool:

```
doctor()
```

Expected: local memory configured (no model, no key). Capture the mode
string for the final report.

### 5. MCP round-trip

```
context()
```

Pass → continue. Fail → MCP not wired. Tell user:
"wenlan-mcp didn't respond. Restart Claude Code so the plugin's
`.mcp.json` re-spawns the server."

### 6. Ready report

Print:

```
Wenlan ready.
  Daemon:   up on 127.0.0.1:7878
  Mode:     <mode from doctor()>
  MCP:      connected
  Data:     ~/.wenlan/  (pages, sessions, db symlink)
  Try:      /brief, /capture <thing>, /recall <query>, /help
```

If this was the first /init invocation in the session, dispatch `/help`
once so the user sees the verb cheat-sheet without asking.

## Optional upgrades (don't auto-run)

Mention these in the ready report only if the user explicitly asks for
"richer features" or asks about model-backed extraction:

- `wenlan model install` — local Qwen for distill cycles.
- `wenlan key set anthropic` — Anthropic for stronger synthesis.

Default flow ignores both. Storage, search, recall, and MCP memory all
work in local memory mode.

## When to use

- Right after `/plugin install wenlan@7xuanlu`.
- Hook printed "daemon down — run /wenlan:init".
- Hook printed "Run /wenlan:init to repair" for a version mismatch.
- User says "set up wenlan", "is it working", "reinstall wenlan".

## When NOT to use

- Daemon already verified this session → `/brief` instead.
- Editing one config field → `wenlan doctor` or settings file directly.
