#!/usr/bin/env bash
# Dispatch the wenlan MCP server.
#
# Resolution order (most specific first):
#   1. Sibling file `bin/wenlan-mcp.local` next to this script — typically a
#      symlink to a locally-built wenlan-mcp binary. Filesystem-based so it
#      survives plugin reloads that don't re-read settings.json env.
#   2. WENLAN_MCP_DEV_BIN env var (primary) or ORIGIN_MCP_DEV_BIN (fallback) —
#      secondary, kept for shells that already export them. Requires Claude Code
#      to inherit the var at startup. Accepts both for backward compatibility.
#   3. ~/.wenlan/bin/wenlan-mcp — the path install.sh places binaries at.
#      Preferred over npx because (a) it's already on disk so MCP host
#      handshake is instant, and (b) it sidesteps the EPERM class of npx
#      failures when ~/.npm/_cacache contains root-owned files left over
#      from older npm versions (npx exits before responding to initialize,
#      MCP host then waits 30s and times out).
#   4. npx -y wenlan-mcp@^0.13.0 — fallback for users who installed the
#      plugin without running install.sh.

# Don't enable `set -u` here: if Claude Code (or any MCP host) invokes the
# script through a shell that doesn't populate BASH_SOURCE, `set -u` halts
# before we even get to the npx fallback. Fall back to $0 instead.
here="$(cd -- "$(dirname -- "${BASH_SOURCE[0]:-$0}")" 2>/dev/null && pwd -P)"
local_bin="${here}/wenlan-mcp.local"

if [ -x "${local_bin}" ]; then
  exec "${local_bin}" "$@"
fi

dev_bin="${WENLAN_MCP_DEV_BIN:-${ORIGIN_MCP_DEV_BIN:-}}"
if [ -n "${dev_bin}" ] && [ -x "${dev_bin}" ]; then
  exec "${dev_bin}" "$@"
fi

installed_bin="${HOME}/.wenlan/bin/wenlan-mcp"
if [ -x "${installed_bin}" ]; then
  exec "${installed_bin}" "$@"
fi

exec npx -y wenlan-mcp@^0.13.0 "$@"
