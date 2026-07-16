# Set up Wenlan with your AI client

Use this guide when an AI assistant is setting up Wenlan on the user's behalf. Configure only the client in the current conversation unless the user explicitly asks for additional clients.

## Target outcome

Setup is complete only when:

1. The Wenlan runtime is healthy.
2. The current AI client has either the Wenlan plugin or an MCP connection.
3. A small capture can be recalled from the same client.

Do not report success after editing configuration alone. A live round trip is the proof.

## Claude Code

Install the Wenlan plugin from its public marketplace:

```bash
claude plugin marketplace add 7xuanlu/wenlan
claude plugin install wenlan@7xuanlu-wenlan
```

Start a new Claude Code session if requested, then run `/setup`. The setup skill installs or repairs the local runtime and verifies the MCP round trip. Detailed workflows: [Claude Code plugin](../plugin/.claude-plugin/README.md).

## Codex

Install the runtime and plugin:

```bash
npx -y wenlan setup
codex plugin marketplace add 7xuanlu/wenlan
codex plugin add wenlan@7xuanlu-wenlan
```

Start a new Codex task so the plugin and MCP server load, then run `/setup`. Detailed workflows: [Codex plugin](../plugin-codex/README.md).

## Cursor, Claude Desktop, VS Code, or Gemini CLI

Install the runtime, then configure only the client the user named:

```bash
npx -y wenlan setup
$HOME/.wenlan/bin/wenlan connect <client>
```

Supported values are `cursor`, `claude-desktop`, `vscode`, and `gemini`. The CLI makes a backup before replacing an existing JSON configuration. Full command reference: [CLI and MCP setup](../crates/wenlan-cli/README.md).

## Verify

First confirm the local runtime:

```bash
$HOME/.wenlan/bin/wenlan status
```

Then use the current client's Wenlan tools to capture a disposable sentence and recall it. Delete the test memory afterward if the client exposes `forget`. If the tools are not visible, start a new client session and verify again before declaring setup complete.
