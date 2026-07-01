---
name: init
description: >
  Frictionless Wenlan setup for Codex. Detects a missing daemon, installs or
  starts it, and verifies the plugin to MCP to daemon round-trip. Run when the
  user says "set up wenlan", "is wenlan working", or "fix wenlan".
allowed-tools: ["Bash", "mcp__wenlan__doctor", "mcp__wenlan__context"]
user-invocable: true
---

# /init

Self-healing setup for Codex. Default backend is local memory: no local model,
no API key, no prompt ceremony. Local model and Anthropic key are optional
upgrades after the basic path works.

## Steps

Run in order. Stop and report at the first failure that needs human attention.
Otherwise, push through automatically.

### 1. Daemon health probe

```bash
for i in 1 2 3; do
  curl -fsS -m 3 http://127.0.0.1:7878/api/health && break
  sleep 1
done
```

- 200 OK: continue to version drift probe.
- Anything else: continue to bootstrap.

### 2. Version drift probe

Compare daemon version to this plugin slice's expected runtime version:

```bash
EXPECTED_VER="0.9.5"
RESP="$(curl -fsS -m 3 http://127.0.0.1:7878/api/health)"
DAEMON_VER="$(printf '%s' "$RESP" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("version",""))')"
printf 'daemon=%s expected=%s\n' "$DAEMON_VER" "$EXPECTED_VER"
```

- Same version: continue to doctor.
- If the probe cannot run because the daemon is down: continue to bootstrap.
- If versions differ, repair the runtime:

```bash
EXPECTED_VER="0.9.5"
curl -fsSL "https://raw.githubusercontent.com/7xuanlu/wenlan/v${EXPECTED_VER}/install.sh" | bash
export PATH="$HOME/.wenlan/bin:$PATH"
wenlan setup --basic
wenlan install
```

Then re-probe daemon health.

### 3. Bootstrap

Detect whether the `wenlan` CLI is on PATH:

```bash
command -v wenlan >/dev/null 2>&1 && echo present || echo absent
```

If absent, install and configure local memory:

```bash
curl -fsSL https://raw.githubusercontent.com/7xuanlu/wenlan/v0.9.5/install.sh | bash
export PATH="$HOME/.wenlan/bin:$PATH"
wenlan setup --basic
wenlan install
```

If present but daemon is down:

```bash
wenlan setup --basic 2>/dev/null || true
wenlan install
```

### 4. Re-probe daemon health

```bash
for i in 1 2 3 4 5; do
  curl -fsS -m 3 http://127.0.0.1:7878/api/health && break
  sleep 1
done
```

If the daemon still is not reachable after about five seconds, surface the
error and stop. Likely causes: launchd load failure, port 7878 already in use,
or a local runtime crash.

### 5. Doctor

Call the Wenlan MCP `doctor` tool.

```text
doctor()
```

Expected: local memory configured. Capture the mode string for the final report.

### 6. MCP round-trip

Call the Wenlan MCP `context` tool with a small limit.

```text
context(limit=3)
```

If it fails, report: "wenlan-mcp did not respond through Codex. Start a new
Codex thread after reinstalling the plugin so Codex respawns the MCP server."

### 7. Ready report

Print:

```text
Wenlan ready.
  Daemon:   up on 127.0.0.1:7878
  Mode:     <mode from doctor()>
  MCP:      connected
  Data:     ~/.wenlan/
  Try:      /brief, /capture <thing>
```

## Optional upgrades

Mention these only if the user asks for richer synthesis:

- `wenlan model install` for local model-backed distillation.
- `wenlan key set anthropic` for stronger synthesis.

## Codex note

This Codex slice has no session-start hook. `/init` is the explicit health and
version check until Codex exposes a lifecycle hook this plugin can use.
