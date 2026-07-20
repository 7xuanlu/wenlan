#!/usr/bin/env bash
# Sync the local Wenlan runtime to your current working tree in one shot:
# rebuild wenlan-server + wenlan-mcp + the service-management CLI, point the
# Claude and Codex plugins at the fresh MCP binary, and (re)start the daemon
# from the dev build.
# The daemon then reports a
# `+g<sha>` dev version, which silences the version-drift nudges while you work.
#
# Covers daemon + MCP only. The Claude Code plugin marketplace cache is refreshed
# separately with `/plugin update` inside Claude Code; wenlan-app is its own repo.
#
#   scripts/dev-sync.sh          rebuild + relink + restart on the dev runtime
#   scripts/dev-sync.sh --off    leave the dev runtime: drop the plugin override
#                                and hand the daemon back to launchd
#
# macOS-only (lsof / launchctl / nohup). set -euo pipefail: any step failing
# stops the script rather than leaving a half-synced runtime.
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
cd "$root"
local_links=(
  "$root/plugin/bin/wenlan-mcp.local"
  "$root/plugin-codex/bin/wenlan-mcp.local"
)

# Kill only a wenlan-server holding :7878 — never a random squatter (Fable).
kill_daemon() {
  local pid binname
  for pid in $(lsof -tiTCP:7878 -sTCP:LISTEN 2>/dev/null || true); do
    binname="$(ps -p "$pid" -o comm= 2>/dev/null || true)"
    case "$binname" in
      *wenlan-server*) kill -9 "$pid" 2>/dev/null || true ;;
      *)
        echo "!! :7878 is held by a non-wenlan process ($binname, pid $pid)." >&2
        echo "   Free the port and re-run." >&2
        exit 1
        ;;
    esac
  done
}

if [ "${1:-}" = "--off" ]; then
  echo "==> leaving dev runtime"
  for local_link in "${local_links[@]}"; do
    rm -f "$local_link" && echo "    removed plugin MCP override ($local_link)"
  done
  kill_daemon
  if [ -x "$root/target/debug/wenlan" ]; then
    "$root/target/debug/wenlan" background on >/dev/null 2>&1 \
      && echo "    handed daemon back to launchd" \
      || echo "    (could not start launchd daemon — run 'wenlan background on' yourself)"
  fi
  echo "==> done. Restart Claude Code so the plugin drops the local MCP binary."
  exit 0
fi

echo "==> building wenlan-server + wenlan-mcp + wenlan"
cargo build -p wenlan-server -p wenlan-mcp -p wenlan

# Point the plugin's local MCP override at the fresh debug binary. The runner
# (plugin/bin/wenlan-mcp-runner.sh) checks this symlink FIRST, before
# ~/.wenlan/bin and npx. It is gitignored (.gitignore) and survives plugin
# reloads — run `--off` when you're done to stop using the dev binary.
for local_link in "${local_links[@]}"; do
  ln -sf "$root/target/debug/wenlan-mcp" "$local_link"
done
echo "==> Claude + Codex plugin MCP -> target/debug/wenlan-mcp"

# If launchd is running a daemon from ~/.wenlan/bin, stop it first so it doesn't
# fight the dev binary for :7878 (wenlan-server exits if a healthy incumbent is
# already bound).
if launchctl print "gui/$(id -u)/com.wenlan.server" >/dev/null 2>&1; then
  echo "==> stopping launchd-managed daemon"
  ./target/debug/wenlan background off >/dev/null 2>&1 || true
fi

kill_daemon

log="${TMPDIR:-/tmp}/wenlan-dev-server.log"
echo "==> starting dev daemon (log: $log)"
nohup ./target/debug/wenlan-server >"$log" 2>&1 &

for _ in $(seq 1 30); do
  ver="$(curl -fsS -m 2 http://127.0.0.1:7878/api/health 2>/dev/null \
        | sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')" || true
  if [ -n "$ver" ]; then
    echo "==> daemon up: v$ver"
    case "$ver" in
      *+g*) echo "    dev version detected — drift nudges will stay quiet." ;;
      *)
        echo "!! :7878 is healthy but still serves release daemon v$ver, not this dev build." >&2
        exit 1
        ;;
    esac
    echo "    This dev daemon is not managed by launchd — it won't survive a reboot."
    echo "    Run 'scripts/dev-sync.sh --off' to return to the released runtime."
    exit 0
  fi
  sleep 1
done

echo "!! daemon did not report health within 30s — check $log" >&2
exit 1
