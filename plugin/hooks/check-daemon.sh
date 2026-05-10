#!/usr/bin/env bash
# SessionStart hook: probe the local Origin daemon and surface a one-line
# warning with the EXACT next step if anything is missing.
# Three states detected:
#   (a) daemon up and serving health        → silent
#   (b) daemon down but `origin` CLI on PATH → tell user `origin install`
#   (c) no daemon and no `origin` CLI       → print install one-liner
# Hook never blocks the session (always exit 0).
set -u

URL="http://127.0.0.1:7878/api/health"

if curl -fsS -m 1 "$URL" >/dev/null 2>&1; then
  exit 0
fi

if command -v origin >/dev/null 2>&1; then
  cat <<MSG
[origin] daemon not reachable at $URL
[origin] start it:  origin install && origin status
[origin] without the daemon, /capture /recall /brief etc. will fail
MSG
  exit 0
fi

cat <<MSG
[origin] daemon not running and \`origin\` CLI not found
[origin] install:    curl -fsSL https://raw.githubusercontent.com/7xuanlu/origin/main/install.sh | bash
[origin] then run:   export PATH="\$HOME/.origin/bin:\$PATH" && origin setup && origin install
[origin] without the daemon, /capture /recall /brief etc. will fail
MSG

exit 0
