#!/usr/bin/env bash
# Reviewer-probe entrypoint: the compiled seed_reviewer_library example
# exclusively creates /runtime/synthetic; the loopback daemon and the
# authenticated query-only MCP start only after it succeeds.
# No canned responses; any failure exits nonzero with visible logs.
set -Eeuo pipefail

DAEMON_PID=""
MCP_PID=""
SEED_PID=""

ROOT="/runtime/synthetic"
DATA="$ROOT"
KNPATH="$ROOT/pages"

fail() { echo "reviewer-probe: ERROR: $1" >&2; exit 1; }

tail_log() {
  local f="$1"
  if [[ -f "$f" ]]; then
    echo "reviewer-probe: --- last 20 lines of $f ---" >&2
    tail -n 20 "$f" >&2 || true
  fi
}

cleanup_children() {
  for pid in "$DAEMON_PID" "$MCP_PID" "$SEED_PID"; do
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
      kill -TERM "$pid" 2>/dev/null || true
    fi
  done
  for _ in $(seq 1 50); do
    local alive=0
    for pid in "$DAEMON_PID" "$MCP_PID" "$SEED_PID"; do
      if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then alive=1; fi
    done
    [[ "$alive" -eq 0 ]] && break
    sleep 0.1
  done
  for pid in "$DAEMON_PID" "$MCP_PID" "$SEED_PID"; do
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
      kill -KILL "$pid" 2>/dev/null || true
    fi
  done
  wait 2>/dev/null || true
}

on_term() { exit 143; }
trap on_term TERM INT
trap cleanup_children EXIT

# 1. Fixed synthetic scope: unconditional, inherited env cannot override.
export REVIEWER_RUNTIME_ROOT="$ROOT"
export HOME="$ROOT/home"
export WENLAN_DATA_DIR="$ROOT"
export WENLAN_MCP_CACHE_DIR="$ROOT/mcp-cache"
export WENLAN_NO_AUTOSTART=1
export WENLAN_SPACE="atlas-review"
export WENLAN_RERANKER_MODE=off
export WENLAN_BIND_ADDR="127.0.0.1:7878" WENLAN_PORT="7878"
export WENLAN_ENABLE_ENTITY_SWEEP=0
unset WENLAN_DEFAULT_SPACE || true

# 2. Bearer [REDACTED] validated with shell builtins only (never echoed,
# never passed to external tools). 32-128 chars of A-Za-z0-9_-.
token="${REVIEWER_BEARER_TOKEN:-}"
[[ -n "$token" ]] || fail "REVIEWER_BEARER_TOKEN is empty or unset; pass -e REVIEWER_BEARER_TOKEN=... at runtime."
[[ "$token" =~ ^[A-Za-z0-9_-]{32,128}$ ]] || fail "REVIEWER_BEARER_TOKEN malformed (need 32-128 chars of A-Za-z0-9_-)."
token=""

# 3. Fresh root only: the real seed exclusively creates it, never us.
[[ -e "$ROOT" ]] && fail "runtime root $ROOT already exists; start from an empty container filesystem."
[[ -d "/runtime" ]] || fail "/runtime parent missing; image build is broken."

# 4. Real seed as a tracked child so TERM during seeding cleans it up.
env -u REVIEWER_BEARER_TOKEN /usr/local/bin/seed_reviewer_library "$ROOT" >&2 &
SEED_PID=$!
if ! wait "$SEED_PID"; then
  SEED_PID=""
  fail "seed_reviewer_library failed; refusing to serve."
fi
SEED_PID=""

# 5. Verify the seed's documented layout before starting any listener.
[[ -d "$KNPATH" ]] || fail "expected knowledge_path $KNPATH missing after seed."
grep -q '"/runtime/synthetic/pages"' "$ROOT/config.json" || fail "config.json knowledge_path is not /runtime/synthetic/pages after seed."
[[ -n "$(ls -A "$KNPATH")" ]] || fail "seed left $KNPATH empty."

# 6. Daemon on container loopback only (unauthenticated server surface).
env -u REVIEWER_BEARER_TOKEN /usr/local/bin/wenlan-server >>"$ROOT/daemon.log" 2>&1 &
DAEMON_PID=$!
i=0
until curl --max-time 2 -fsS http://127.0.0.1:7878/api/health >/dev/null 2>&1; do
  kill -0 "$DAEMON_PID" 2>/dev/null || { tail_log "$ROOT/daemon.log"; fail "daemon exited before readiness."; }
  i=$((i + 1))
  if [[ "$i" -ge 60 ]]; then
    tail_log "$ROOT/daemon.log"
    fail "daemon unhealthy after 60s."
  fi
  sleep 1
done

# 7. Authenticated query-only MCP is the ONLY 0.0.0.0 listener in-container.
/usr/local/bin/wenlan-mcp serve \
  --tool-profile query-only \
  --host 0.0.0.0 --port 8080 \
  --token-env REVIEWER_BEARER_TOKEN \
  --origin-url http://127.0.0.1:7878 >>"$ROOT/mcp.log" 2>&1 &
MCP_PID=$!

# 8. Either child exiting (even 0) ends the container nonzero; errors
# propagate instead of hiding behind a health check.
set +e
wait -n
status=$?
set -e
tail_log "$ROOT/daemon.log"
tail_log "$ROOT/mcp.log"
echo "reviewer-probe: owned child exited (status=$status); container exiting nonzero." >&2
if [[ "$status" -ne 0 ]]; then exit "$status"; fi
exit 1
