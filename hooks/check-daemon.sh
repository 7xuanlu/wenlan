#!/usr/bin/env bash
# SessionStart hook: probe the local Origin daemon and surface a one-line
# warning if it's unreachable. Never blocks the session.
set -u

URL="http://127.0.0.1:7878/api/health"

if curl -fsS -m 1 "$URL" >/dev/null 2>&1; then
  exit 0
fi

cat <<MSG
[origin] daemon not reachable at $URL
[origin] start it with: origin install && origin status
[origin] without the daemon, /origin:* skills will fail
MSG

exit 0
