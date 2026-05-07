---
name: origin-status
description: >
  Diagnose the local Origin daemon. Wraps the origin MCP `doctor` tool.
  Invoked as `/origin-status`. Use only when Origin tools fail, when onboarding
  a new MCP client, or when the user asks why setup, extraction, or background
  refinement is paused.
---

# /origin-status

Report the daemon's health and setup state.

## How to invoke

Call the `origin` MCP server's `doctor` tool with no parameters.

```
doctor()
```

## What it reports

- Daemon reachability on `127.0.0.1:7878`
- Setup mode (Basic Memory, On-device Model, Anthropic API)
- API key state (configured or not)
- On-device model state (selected, loaded, cached)

## When to use

- User says "is Origin running", "why isn't memory working", "check Origin".
- Memory tool calls error out unexpectedly.
- Onboarding a new MCP client and verifying the daemon is reachable.

## When NOT to use

- This is not part of the normal memory loop. Don't call it during routine
  recall / store flows.
