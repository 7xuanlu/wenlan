---
name: init
description: >
  End-to-end setup check for Origin in this workspace. Verifies the daemon
  is up, MCP is wired, and a real round-trip works — then prints "Ready"
  or the exact failing step. Use after `/plugin install origin@7xuanlu`
  or any time the user says "set up origin", "is origin working", "did
  origin install correctly", "/init".
allowed-tools: ["Bash", "mcp__plugin_origin_origin__doctor", "mcp__plugin_origin_origin__context"]
---

# /init

End-to-end setup check. Goal: 30 seconds from plugin install to a
provably-working Origin. No guessing.

## How to invoke

Run these four steps in order. STOP and report at the FIRST failure with
the exact next step the user should take. Do not proceed past a failure.

### Step 1: daemon health

```
Bash: curl -fsS -m 1 http://127.0.0.1:7878/api/health
```

- Pass → continue.
- Fail → say:
  > Daemon not running. Run `origin install && origin status`. If
  > `origin` is not on PATH, run the install one-liner:
  > `curl -fsSL https://raw.githubusercontent.com/7xuanlu/origin/main/install.sh | bash`,
  > then `export PATH="$HOME/.origin/bin:$PATH"`.

### Step 2: backend / setup mode

Call the `origin` MCP server's `doctor` tool:

```
doctor()
```

- Pass (any of: Basic Memory / On-device Qwen / Anthropic API configured)
  → continue, name the mode in the final report.
- Unconfigured → say:
  > Pick a backend:
  > - Basic Memory (no LLM): nothing to do, fastest path.
  > - On-device Qwen: `origin model install`.
  > - Anthropic API: `origin key set anthropic`.

### Step 3: MCP round-trip

Call `context()` once (no topic):

```
context()
```

- Pass → continue.
- Fail → MCP server not wired. Say:
  > origin-mcp didn't respond. Restart Claude Code so the plugin's
  > `.mcp.json` re-spawns the server. If still failing, check
  > `npx -y origin-mcp` runs without error in a terminal.

### Step 4: ready report

Print exactly:

```
Origin ready.
  Daemon:  up on 127.0.0.1:7878
  Mode:    <mode from doctor()>
  MCP:     connected
  Try:     /brief, /capture <thing to remember>, /recall <query>, /help
```

Then dispatch `/help` once for the new user (skip if user already saw it
this session).

## When to use

- Right after `/plugin install origin@7xuanlu`.
- User says "set up origin", "verify origin", "is it working".
- Hook printed a daemon warning and user wants to confirm fix.

## When NOT to use

- Daemon already verified this session → use `/brief` instead.
- Changing one config field — use `origin doctor` or edit settings directly.
