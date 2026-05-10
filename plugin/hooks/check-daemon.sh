#!/usr/bin/env bash
# SessionStart hook: probe the local Origin daemon. If it's down, point the
# user at `/origin:init` — that skill auto-installs and starts the daemon
# end-to-end. Hook never blocks (always exit 0) and never prints command
# soup. The skill owns the install logic.
set -u

URL="http://127.0.0.1:7878/api/health"

if curl -fsS -m 1 "$URL" >/dev/null 2>&1; then
  exit 0
fi

cat <<MSG
[origin] daemon not running. Run /origin:init to set up.
MSG

exit 0
