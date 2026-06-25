#!/usr/bin/env bash
# SessionStart hook: probe the local Wenlan daemon and surface two issues:
#   1. Daemon not running → point user at /wenlan:init (it auto-installs).
#   2. Daemon version mismatches the plugin manifest → point user at
#      /wenlan:init (it upgrades/restarts and verifies the runtime).
# Hook never blocks (always exit 0) and never prints command soup.
set -u

URL="http://127.0.0.1:7878/api/health"
PLUGIN_JSON="${CLAUDE_PLUGIN_ROOT:-}/.claude-plugin/plugin.json"

RESP=""
for i in 1 2 3; do
  RESP=$(curl -fsS -m 3 "$URL" 2>/dev/null) && break
  sleep 1
done

if [ -z "$RESP" ]; then
  cat <<MSG
[wenlan] daemon not running. Run /wenlan:init to set up.
MSG
  exit 0
fi

# Compare daemon version vs plugin manifest version. Silent unless mismatch.
[ -r "$PLUGIN_JSON" ] || exit 0
command -v python3 >/dev/null 2>&1 || exit 0  # fail closed without python3

extract_version() {
  python3 -c 'import json,sys; print(json.load(sys.stdin).get("version",""))' 2>/dev/null
}

DAEMON_VER=$(printf '%s' "$RESP" | extract_version)
EXPECTED_VER=$(extract_version <"$PLUGIN_JSON")

if [ -n "$DAEMON_VER" ] && [ -n "$EXPECTED_VER" ] && [ "$DAEMON_VER" != "$EXPECTED_VER" ]; then
  cat <<MSG
[wenlan] daemon v${DAEMON_VER}, plugin expects v${EXPECTED_VER}.
  Run /wenlan:init to repair. It will upgrade/restart the runtime and verify MCP.
MSG
fi

exit 0
