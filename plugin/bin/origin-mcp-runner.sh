#!/usr/bin/env bash
# Dispatch the origin MCP server.
#
# Default path: pull the published origin-mcp from npm via npx. This is what
# end users get after installing the plugin.
#
# Dev path: set ORIGIN_MCP_DEV_BIN to the absolute path of a locally-built
# origin-mcp binary (e.g. ~/Repos/origin/target/release/origin-mcp). The
# wrapper exec's it directly, so plugin changes to the MCP tool shape can be
# tested without publishing to npm first.
set -u

if [ -n "${ORIGIN_MCP_DEV_BIN:-}" ] && [ -x "${ORIGIN_MCP_DEV_BIN}" ]; then
  exec "${ORIGIN_MCP_DEV_BIN}" "$@"
fi

exec npx -y origin-mcp@^0.5.0 "$@"
