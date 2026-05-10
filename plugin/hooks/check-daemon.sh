#!/usr/bin/env bash
# SessionStart hook: probe the local Origin daemon and surface two issues:
#   1. Daemon not running → point user at /origin:init (it auto-installs).
#   2. Daemon version mismatches the plugin manifest → print the upgrade
#      one-liner directly so the user can copy-paste.
# Hook never blocks (always exit 0) and never prints command soup.
set -u

URL="http://127.0.0.1:7878/api/health"
PLUGIN_JSON="${CLAUDE_PLUGIN_ROOT:-}/.claude-plugin/plugin.json"

RESP=$(curl -fsS -m 1 "$URL" 2>/dev/null) || {
  cat <<MSG
[origin] daemon not running. Run /origin:init to set up.
MSG
  exit 0
}

# Compare daemon version vs plugin manifest version. Silent unless mismatch.
[ -r "$PLUGIN_JSON" ] || exit 0

extract_version() {
  sed -nE 's/.*"version"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/p' "$@" | head -1
}

DAEMON_VER=$(printf '%s' "$RESP" | extract_version /dev/stdin)
EXPECTED_VER=$(extract_version "$PLUGIN_JSON")

if [ -n "$DAEMON_VER" ] && [ -n "$EXPECTED_VER" ] && [ "$DAEMON_VER" != "$EXPECTED_VER" ]; then
  cat <<MSG
[origin] daemon v${DAEMON_VER}, plugin expects v${EXPECTED_VER}. Upgrade:
  curl -fsSL https://raw.githubusercontent.com/7xuanlu/origin/v${EXPECTED_VER}/install.sh | bash
MSG
fi

exit 0
