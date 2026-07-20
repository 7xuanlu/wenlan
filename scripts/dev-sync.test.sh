#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT=$(cd "$(dirname "$0")/.." && pwd)
TMP_ROOT=$(mktemp -d)
trap 'rm -rf "$TMP_ROOT"' EXIT

make_fixture() {
  local name="$1"
  local root="$TMP_ROOT/$name"
  mkdir -p \
    "$root/scripts" \
    "$root/plugin/bin" \
    "$root/plugin-codex/bin" \
    "$root/target/debug" \
    "$root/fake-bin"
  cp "$REPO_ROOT/scripts/dev-sync.sh" "$root/scripts/dev-sync.sh"

  cat >"$root/fake-bin/git" <<'EOF'
#!/usr/bin/env bash
if [ "$1" = "rev-parse" ] && [ "$2" = "--show-toplevel" ]; then
  printf '%s\n' "$WENLAN_TEST_ROOT"
  exit 0
fi
exit 1
EOF
  cat >"$root/fake-bin/cargo" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$WENLAN_TEST_CARGO_LOG"
EOF
  cat >"$root/fake-bin/lsof" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$WENLAN_TEST_LSOF_LOG"
exit 0
EOF
  cat >"$root/fake-bin/launchctl" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$WENLAN_TEST_LAUNCHCTL_LOG"
if [ "$1" = "print" ] && [ "${WENLAN_TEST_LAUNCHD_LOADED:-0}" = "1" ]; then
  exit 0
fi
if [ "$1" = "list" ]; then
  exit 0
fi
exit 1
EOF
  cat >"$root/fake-bin/curl" <<'EOF'
#!/usr/bin/env bash
count=0
if [ -f "$WENLAN_TEST_CURL_COUNT" ]; then
  count=$(cat "$WENLAN_TEST_CURL_COUNT")
fi
count=$((count + 1))
printf '%s' "$count" >"$WENLAN_TEST_CURL_COUNT"
if [ "${WENLAN_TEST_CURL_FAIL_FIRST:-0}" = "1" ] && [ "$count" -eq 1 ]; then
  exit 56
fi
printf '{"status":"ok","version":"%s"}\n' "${WENLAN_TEST_DAEMON_VERSION:-0.13.2+gdeadbeef}"
EOF
  cat >"$root/fake-bin/sleep" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
  cat >"$root/fake-bin/nohup" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
  cat >"$root/target/debug/wenlan" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$WENLAN_TEST_WENLAN_LOG"
EOF
  cat >"$root/target/debug/wenlan-server" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
  cat >"$root/target/debug/wenlan-mcp" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
  chmod +x "$root/scripts/dev-sync.sh" "$root/fake-bin/"* "$root/target/debug/"*
  printf '%s\n' "$root"
}

run_fixture() {
  local root="$1"
  shift
  env \
    PATH="$root/fake-bin:/usr/bin:/bin" \
    WENLAN_TEST_ROOT="$root" \
    WENLAN_TEST_CARGO_LOG="$root/cargo.log" \
    WENLAN_TEST_LAUNCHCTL_LOG="$root/launchctl.log" \
    WENLAN_TEST_LSOF_LOG="$root/lsof.log" \
    WENLAN_TEST_CURL_COUNT="$root/curl.count" \
    WENLAN_TEST_WENLAN_LOG="$root/wenlan.log" \
    "$@" \
    bash "$root/scripts/dev-sync.sh"
}

failures=0

root=$(make_fixture build_cli)
if run_fixture "$root" >/dev/null 2>&1 \
  && grep -Eq '(^| )-p wenlan($| )' "$root/cargo.log"; then
  echo "PASS dev sync builds the service-management CLI"
else
  echo "FAIL dev sync must build the CLI it invokes for launchd handoff"
  failures=$((failures + 1))
fi

root=$(make_fixture launchd_probe)
if run_fixture "$root" WENLAN_TEST_LAUNCHD_LOADED=1 >/dev/null 2>&1 \
  && grep -qx 'background off' "$root/wenlan.log"; then
  echo "PASS dev sync detects a loaded launchd job via its domain"
else
  echo "FAIL dev sync missed a loaded launchd job omitted by legacy list"
  failures=$((failures + 1))
fi

root=$(make_fixture health_retry)
if run_fixture "$root" WENLAN_TEST_CURL_FAIL_FIRST=1 >/dev/null 2>&1 \
  && [ "$(cat "$root/curl.count")" -ge 2 ]; then
  echo "PASS dev sync retries transient health failures"
else
  echo "FAIL dev sync exited on the first transient health failure"
  failures=$((failures + 1))
fi

root=$(make_fixture listener_only)
if run_fixture "$root" >/dev/null 2>&1 \
  && grep -q -- '-sTCP:LISTEN' "$root/lsof.log"; then
  echo "PASS dev sync considers only the 7878 listener"
else
  echo "FAIL dev sync included MCP client connections in its port-owner check"
  failures=$((failures + 1))
fi

root=$(make_fixture release_version)
if run_fixture "$root" WENLAN_TEST_DAEMON_VERSION=0.13.2 >/dev/null 2>&1; then
  echo "FAIL dev sync accepted a release daemon instead of its dev build"
  failures=$((failures + 1))
else
  echo "PASS dev sync rejects a daemon without the dev-version suffix"
fi

root=$(make_fixture both_plugin_links)
if run_fixture "$root" >/dev/null 2>&1 \
  && [ -L "$root/plugin/bin/wenlan-mcp.local" ] \
  && [ -L "$root/plugin-codex/bin/wenlan-mcp.local" ]; then
  echo "PASS dev sync links both Claude and Codex MCP plugins"
else
  echo "FAIL dev sync left the Codex MCP plugin on the installed binary"
  failures=$((failures + 1))
fi

if [ "$failures" -ne 0 ]; then
  exit 1
fi
