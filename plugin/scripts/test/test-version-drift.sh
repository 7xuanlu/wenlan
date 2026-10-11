#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Bash tests for version-drift.sh. A loopback HTTP stub answers /api/health with
# a chosen daemon version; temp manifests carry the plugin version.

set -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DRIFT="$SCRIPT_DIR/../version-drift.sh"
tmpbase="$(mktemp -d)"

pass=0
fail=0

www="$tmpbase/www"
mkdir -p "$www/api"
# The stub binds port 0 itself and publishes the port once bound.
cat > "$tmpbase/server.py" <<'PY'
import http.server
import os
import pathlib
import sys

os.chdir(sys.argv[1])
server = http.server.HTTPServer(("127.0.0.1", 0), http.server.SimpleHTTPRequestHandler)
pathlib.Path(sys.argv[2]).write_text(str(server.server_address[1]))
server.serve_forever()
PY
python3 "$tmpbase/server.py" "$www" "$tmpbase/port" >/dev/null 2>&1 &
server=$!
trap 'kill "$server" 2>/dev/null; wait "$server" 2>/dev/null; rm -rf "$tmpbase"' EXIT
for _ in $(seq 1 50); do
    [ -s "$tmpbase/port" ] && break
    sleep 0.1
done
[ -s "$tmpbase/port" ] || { echo "FAIL stub daemon did not start" >&2; exit 1; }
port=$(cat "$tmpbase/port")
HEALTH="http://127.0.0.1:$port/api/health"
manifest="$tmpbase/plugin.json"

# run_case <name> <daemon ver> <manifest ver> <expected line> <expected rc> [flags...]
run_case() {
    name="$1"; dv="$2"; mv="$3"; want_line="$4"; want_rc="$5"; shift 5
    printf '{"status":"ok","db_initialized":true,"version":"%s"}' "$dv" > "$www/api/health"
    printf '{"name":"wenlan","version":"%s"}' "$mv" > "$manifest"
    out=$(bash "$DRIFT" --health-url "$HEALTH" --manifest "$manifest" "$@" 2>&1)
    rc=$?
    if [ "$out" = "$want_line" ] && [ "$rc" -eq "$want_rc" ]; then
        printf 'PASS  %s\n' "$name"; pass=$((pass + 1))
    else
        printf 'FAIL  %s\n  want: %s (rc %s)\n  got:  %s (rc %s)\n' "$name" "$want_line" "$want_rc" "$out" "$rc" >&2
        fail=$((fail + 1))
    fi
}

run_case 'same version' '0.18.16' '0.18.16' \
    'daemon=0.18.16 expected=0.18.16 release=0.18.16 result=same' 0
run_case 'daemon build metadata is not drift' '0.18.16+gabcdef12' '0.18.16' \
    'daemon=0.18.16+gabcdef12 expected=0.18.16 release=0.18.16 result=same' 0
run_case 'codex manifest +codex suffix stripped' '0.18.16' '0.18.16+codex' \
    'daemon=0.18.16 expected=0.18.16+codex release=0.18.16 result=same' 0
run_case 'daemon older minor' '0.17.9' '0.18.0' \
    'daemon=0.17.9 expected=0.18.0 release=0.18.0 result=older' 2
run_case 'daemon newer minor' '0.19.0' '0.18.16' \
    'daemon=0.19.0 expected=0.18.16 release=0.18.16 result=newer' 3
run_case 'numeric not lexicographic (0.9 older than 0.18)' '0.9.0' '0.18.0' \
    'daemon=0.9.0 expected=0.18.0 release=0.18.0 result=older' 2
run_case 'numeric patch (0.18.9 older than 0.18.16)' '0.18.9' '0.18.16' \
    'daemon=0.18.9 expected=0.18.16 release=0.18.16 result=older' 2
run_case 'patch drift counts by default' '0.18.15' '0.18.16' \
    'daemon=0.18.15 expected=0.18.16 release=0.18.16 result=older' 2
run_case 'patch drift ignored with --ignore-patch' '0.18.15' '0.18.16' \
    'daemon=0.18.15 expected=0.18.16 release=0.18.16 result=same' 0 --ignore-patch
run_case 'newer patch ignored with --ignore-patch' '0.18.20' '0.18.16' \
    'daemon=0.18.20 expected=0.18.16 release=0.18.16 result=same' 0 --ignore-patch
run_case 'minor drift still reported with --ignore-patch' '0.17.99' '0.18.0' \
    'daemon=0.17.99 expected=0.18.0 release=0.18.0 result=older' 2 --ignore-patch

# Manifest missing.
printf '{"status":"ok","version":"0.18.16"}' > "$www/api/health"
out=$(bash "$DRIFT" --health-url "$HEALTH" --manifest "$tmpbase/missing.json" 2>&1)
rc=$?
if [ "$out" = 'daemon=0.18.16 expected= release= result=unknown' ] && [ "$rc" -eq 4 ]; then
    printf 'PASS  %s\n' 'manifest missing is unknown'; pass=$((pass + 1))
else
    printf 'FAIL  manifest missing is unknown\n  got: %s (rc %s)\n' "$out" "$rc" >&2; fail=$((fail + 1))
fi

# Health down (closed port).
printf '{"version":"0.18.16"}' > "$manifest"
out=$(bash "$DRIFT" --health-url 'http://127.0.0.1:1/api/health' --manifest "$manifest" 2>&1)
rc=$?
if [ "$out" = 'daemon= expected=0.18.16 release=0.18.16 result=unknown' ] && [ "$rc" -eq 4 ]; then
    printf 'PASS  %s\n' 'health down is unknown'; pass=$((pass + 1))
else
    printf 'FAIL  health down is unknown\n  got: %s (rc %s)\n' "$out" "$rc" >&2; fail=$((fail + 1))
fi

# Health answers without a version.
printf '{"status":"ok"}' > "$www/api/health"
out=$(bash "$DRIFT" --health-url "$HEALTH" --manifest "$manifest" 2>&1)
rc=$?
if [ "$out" = 'daemon= expected=0.18.16 release=0.18.16 result=unknown' ] && [ "$rc" -eq 4 ]; then
    printf 'PASS  %s\n' 'health without version is unknown'; pass=$((pass + 1))
else
    printf 'FAIL  health without version is unknown\n  got: %s (rc %s)\n' "$out" "$rc" >&2; fail=$((fail + 1))
fi

printf '\n%d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
