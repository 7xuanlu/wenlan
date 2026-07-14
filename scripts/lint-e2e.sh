#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PY="$ROOT/scripts/lint-e2e.py"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/wenlan-lint-e2e.XXXXXX")"
GIT_TARGET="$WORK/git-target"
TARBALL_ROOT="$WORK/tarball"
TARBALL_TARGET="$WORK/tarball-target"
DAEMON_PID=""
FIXTURE_PID=""
DAEMON_LOG=""
HTTP_TIMEOUT=30
MAX_BODY_BYTES=8388608

cleanup() {
    for pid in "$FIXTURE_PID" "$DAEMON_PID"; do
        if [ -n "$pid" ]; then
            kill "$pid" >/dev/null 2>&1 || true
            wait "$pid" 2>/dev/null || true
        fi
    done
    rm -rf -- "$WORK"
}
trap cleanup EXIT

fail() {
    echo "FAIL: $1" >&2
    if [ -n "$DAEMON_LOG" ] && [ -f "$DAEMON_LOG" ]; then
        echo "--- daemon log tail ---" >&2
        tail -50 "$DAEMON_LOG" >&2 || true
    fi
    exit 1
}

resolve_cache() {
    if [ -n "${WENLAN_TEST_FASTEMBED_CACHE:-}" ]; then
        printf '%s\n' "$WENLAN_TEST_FASTEMBED_CACHE"
        return
    fi
    case "$(uname -s)" in
        Darwin) printf '%s\n' "$HOME/Library/Application Support/wenlan/memorydb/fastembed_cache" ;;
        *) printf '%s\n' "${XDG_DATA_HOME:-$HOME/.local/share}/wenlan/memorydb/fastembed_cache" ;;
    esac
}

resolve_ort() {
    local linked="" cache_root host library
    if [ -n "${WENLAN_TEST_ORT_LIB_LOCATION:-}" ]; then
        printf '%s\n' "$WENLAN_TEST_ORT_LIB_LOCATION"
        return
    fi
    linked="$(sed -n 's/^cargo:rustc-link-search=native=//p' "$ROOT"/target/debug/build/ort-sys-*/output 2>/dev/null | head -1 || true)"
    if [ -n "$linked" ] && [ -f "$linked/libonnxruntime.a" ]; then
        printf '%s\n' "$linked"
        return
    fi
    host="$(rustc -vV | sed -n 's/^host: //p')"
    case "$(uname -s)" in
        Darwin) cache_root="$HOME/Library/Caches/ort.pyke.io/dfbin/$host" ;;
        *) cache_root="${XDG_CACHE_HOME:-$HOME/.cache}/ort.pyke.io/dfbin/$host" ;;
    esac
    library="$(find "$cache_root" -type f -name 'libonnxruntime.a' -print 2>/dev/null | sort | head -1 || true)"
    [ -n "$library" ] && dirname "$library"
}

CACHE="$(resolve_cache)"
[ -d "$CACHE" ] || fail "offline FastEmbed cache missing: $CACHE"
[ -n "$(find "$CACHE" -type f -print -quit)" ] || fail "offline FastEmbed cache is empty"
ORT_LIB="$(resolve_ort)"
[ -f "$ORT_LIB/libonnxruntime.a" ] || fail "offline ONNX Runtime library missing"

HEAD="$(git -C "$ROOT" rev-parse HEAD)"
echo "==> Building exact git checkout $HEAD in a fresh target"
(
    cd "$ROOT"
    CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}" CARGO_NET_OFFLINE=true CARGO_TARGET_DIR="$GIT_TARGET" ORT_LIB_LOCATION="$ORT_LIB" cargo build --locked -p wenlan-server -p wenlan
)

SERVER="$GIT_TARGET/debug/wenlan-server"
CLI="$GIT_TARGET/debug/wenlan"
HOME_DIR="$WORK/home"
DATA_DIR="$WORK/data"
PAGES="$HOME_DIR/.wenlan/pages"
mkdir -p "$HOME_DIR" "$DATA_DIR"

start_daemon() {
    local server="$1" tag="$2" home data port_file
    home="$WORK/$tag-home"
    data="$WORK/$tag-data"
    port_file="$WORK/$tag-port"
    mkdir -p "$home" "$data"
    rm -f "$port_file"
    DAEMON_LOG="$WORK/$tag-daemon.log"
    HOME="$home" WENLAN_DATA_DIR="$data" WENLAN_TEST_FASTEMBED_CACHE="$CACHE" HF_HUB_OFFLINE=1 HF_HUB_DISABLE_TELEMETRY=1 TRANSFORMERS_OFFLINE=1 WENLAN_TEST_LINT_EPOCH=1900000000 WENLAN_BIND_ADDR=127.0.0.1:0 WENLAN_PORT_FILE="$port_file" RUST_LOG=warn "$server" >"$DAEMON_LOG" 2>&1 &
    DAEMON_PID=$!
    for _ in $(seq 1 120); do
        if [ -s "$port_file" ]; then
            PORT="$(cat "$port_file")"
            HOST="http://127.0.0.1:$PORT"
            HOME_DIR="$home"
            DATA_DIR="$data"
            PAGES="$home/.wenlan/pages"
            return
        fi
        kill -0 "$DAEMON_PID" 2>/dev/null || fail "$tag daemon exited during startup"
        sleep 1
    done
    fail "$tag daemon did not publish a port"
}

stop_daemon() {
    if [ -n "$DAEMON_PID" ]; then
        kill "$DAEMON_PID" >/dev/null 2>&1 || true
        wait "$DAEMON_PID" 2>/dev/null || true
        DAEMON_PID=""
    fi
}

fingerprint() {
    python3 "$PY" fingerprint "$DATA_DIR" "$HOME_DIR"
}

run_cli_bounded() {
    local host="$1" stdout="$2" stderr="$3" pid code=0
    shift 3
    HOME="$HOME_DIR" WENLAN_DATA_DIR="$DATA_DIR" WENLAN_HOST="$host" \
        "$CLI" "$@" >"$stdout" 2>"$stderr" &
    pid=$!
    for _ in $(seq 1 480); do
        if ! kill -0 "$pid" 2>/dev/null; then
            wait "$pid" || code=$?
            return "$code"
        fi
        sleep 0.25
    done
    kill "$pid" >/dev/null 2>&1 || true
    wait "$pid" 2>/dev/null || true
    return 124
}

run_pair() {
    local name="$1" query="$2" expected_exit="$3" before after code
    local cli_args=(--format json lint)
    if [ -n "$query" ]; then
        cli_args+=(--space "${query#?space=}")
    fi
    before="$(fingerprint)"
    curl --max-time "$HTTP_TIMEOUT" --max-filesize "$MAX_BODY_BYTES" -fsS "$HOST/api/lint$query" >"$WORK/$name-http.json"
    set +e
    run_cli_bounded "$HOST" "$WORK/$name-cli.json" "$WORK/$name-cli.err" \
        "${cli_args[@]}"
    code=$?
    set -e
    [ "$code" -eq "$expected_exit" ] || fail "$name CLI exit $code, expected $expected_exit"
    [ ! -s "$WORK/$name-cli.err" ] || fail "$name wrote diagnostics on success"
    python3 "$PY" compare "$WORK/$name-http.json" "$WORK/$name-cli.json"
    after="$(fingerprint)"
    [ "$before" = "$after" ] || fail "$name mutated persistent state"
}

run_fixture() {
    local name="$1" fixture="$2" expected_exit="$3" code
    [ "$(wc -c <"$fixture")" -le "$MAX_BODY_BYTES" ] || fail "$name fixture is oversized"
    python3 "$PY" serve-once "$fixture" "$WORK/$name-port" &
    FIXTURE_PID=$!
    for _ in $(seq 1 30); do [ -s "$WORK/$name-port" ] && break; sleep 0.1; done
    [ -s "$WORK/$name-port" ] || fail "$name fixture did not publish a port"
    set +e
    run_cli_bounded "http://127.0.0.1:$(cat "$WORK/$name-port")" \
        "$WORK/$name-cli.json" "$WORK/$name-cli.err" --format json lint
    code=$?
    set -e
    kill "$FIXTURE_PID" >/dev/null 2>&1 || true
    wait "$FIXTURE_PID" 2>/dev/null || true
    FIXTURE_PID=""
    [ "$code" -eq "$expected_exit" ] || fail "$name fixture exit $code, expected $expected_exit"
    [ ! -s "$WORK/$name-cli.err" ] || fail "$name fixture wrote stderr"
    python3 "$PY" compare "$fixture" "$WORK/$name-cli.json"
}

echo "==> Starting isolated exact-checkout daemon"
start_daemon "$SERVER" git
HOME="$HOME_DIR" WENLAN_DATA_DIR="$DATA_DIR" WENLAN_HOST="$HOST" \
    "$CLI" --quiet spaces add work

echo "==> Proving real-daemon baseline and producer receipt"
run_pair baseline "" 0
python3 "$PY" assert-report "$WORK/baseline-http.json" --complete true --scope global \
    --producer "$HEAD"

echo "==> Proving exit 0 with a canonical typed clean fixture"
python3 "$PY" clean-fixture "$WORK/baseline-http.json" "$WORK/clean.json"
run_fixture clean "$WORK/clean.json" 0

echo "==> Proving exit 1 with an independent typed actionable fixture"
python3 "$PY" actionable-fixture "$WORK/clean.json" "$WORK/actionable.json"
run_fixture actionable "$WORK/actionable.json" 1
python3 "$PY" assert-report "$WORK/actionable.json" --complete true --scope global \
    --producer "$HEAD" --finding serving.route_scope_contracts

echo "==> Seeding privacy and path canaries outside the measured lint window"
mkdir -p "$PAGES/.wenlan" "$PAGES/_sources"
printf '%s\n' '---' 'origin_id: PRIVATE_MALFORMED_ID_CANARY' 'origin_version: 2' '---' '# PRIVATE_TITLE_CANARY' '' 'PRIVATE_CONTENT_CANARY' >"$PAGES/PRIVATE_FILENAME_CANARY.md"
printf '%s\n' '---' 'origin_id: PRIVATE_SOURCE_ID_CANARY' '---' 'PRIVATE_SOURCE_CONTENT_CANARY' >"$PAGES/_sources/PRIVATE_SOURCE_FILENAME_CANARY.md"
printf '%s\n' '{"schema_version":2,"pages":{"PRIVATE_STATE_ID_CANARY":{"file":"/tmp/PRIVATE_ABSOLUTE_PATH_CANARY","version":2}}}' >"$PAGES/.wenlan/state.json"

CANARIES=(
    PRIVATE_FILENAME_CANARY PRIVATE_MALFORMED_ID_CANARY PRIVATE_TITLE_CANARY
    PRIVATE_CONTENT_CANARY PRIVATE_SOURCE_FILENAME_CANARY PRIVATE_SOURCE_ID_CANARY
    PRIVATE_SOURCE_CONTENT_CANARY PRIVATE_STATE_ID_CANARY PRIVATE_ABSOLUTE_PATH_CANARY
)

echo "==> Proving global, registered, and uncategorized parity"
run_pair global "" 0
run_pair registered "?space=work" 0
run_pair uncategorized "?space=uncategorized" 0
python3 "$PY" assert-report "$WORK/registered-http.json" --complete true --scope registered \
    --producer "$HEAD"
python3 "$PY" assert-report "$WORK/uncategorized-http.json" --complete true \
    --scope uncategorized --producer "$HEAD"
private_args=()
for canary in "${CANARIES[@]}"; do private_args+=(--canary "$canary"); done
echo "==> Proving unknown scope and forbidden wiki surfaces"
unknown_status="$(curl --max-time "$HTTP_TIMEOUT" --max-filesize "$MAX_BODY_BYTES" \
    -sS -o "$WORK/unknown-http.json" -w '%{http_code}' \
    "$HOST/api/lint?space=missing")"
[ "$unknown_status" = 422 ] || fail "unknown HTTP status $unknown_status"
set +e
run_cli_bounded "$HOST" "$WORK/unknown-cli.out" "$WORK/unknown-cli.err" \
    --format json lint --space missing
unknown_exit=$?
run_cli_bounded "$HOST" "$WORK/wiki.out" "$WORK/wiki.err" wiki check
wiki_exit=$?
set -e
[ "$unknown_exit" -eq 2 ] && [ ! -s "$WORK/unknown-cli.out" ] || fail "unknown CLI contract"
python3 "$PY" assert-error "$WORK/unknown-http.json" "$WORK/unknown-cli.err"
[ "$wiki_exit" -eq 2 ] && [ ! -s "$WORK/wiki.out" ] || fail "wiki command unexpectedly exists"
wiki_status="$(curl --max-time "$HTTP_TIMEOUT" --max-filesize "$MAX_BODY_BYTES" \
    -sS -o /dev/null -w '%{http_code}' "$HOST/api/wiki/check")"
[ "$wiki_status" = 404 ] || fail "wiki route unexpectedly exists"

echo "==> Proving typed incomplete precedence without mutation"
mv "$PAGES" "$PAGES.missing"
run_pair incomplete "" 2
python3 "$PY" assert-report "$WORK/incomplete-http.json" --complete false --scope global \
    --producer "$HEAD" --incomplete
python3 "$PY" precedence-fixture "$WORK/actionable.json" \
    "$WORK/incomplete-http.json" "$WORK/precedence.json"
run_fixture precedence "$WORK/precedence.json" 2
python3 "$PY" assert-report "$WORK/precedence.json" --complete false --scope global \
    --producer "$HEAD" --finding serving.route_scope_contracts --incomplete
mv "$PAGES.missing" "$PAGES"

echo "==> Building and booting a tarball-style checkout"
stop_daemon
mkdir -p "$TARBALL_ROOT"
git -C "$ROOT" archive HEAD | tar -x -C "$TARBALL_ROOT"
(
    cd "$TARBALL_ROOT"
    CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}" CARGO_NET_OFFLINE=true CARGO_TARGET_DIR="$TARBALL_TARGET" ORT_LIB_LOCATION="$ORT_LIB" cargo build --locked -p wenlan-server
)
start_daemon "$TARBALL_TARGET/debug/wenlan-server" tarball
curl --max-time "$HTTP_TIMEOUT" --max-filesize "$MAX_BODY_BYTES" -fsS \
    "$HOST/api/lint" >"$WORK/tarball-http.json"
python3 "$PY" assert-report "$WORK/tarball-http.json" --complete true --scope global \
    --producer null
python3 "$PY" actionable-fixture "$WORK/tarball-http.json" "$WORK/tarball-actionable.json"
run_fixture tarball-actionable "$WORK/tarball-actionable.json" 1
python3 "$PY" assert-report "$WORK/tarball-actionable.json" --complete true --scope global \
    --producer null --finding serving.route_scope_contracts
stop_daemon

output_paths=()
while IFS= read -r path; do output_paths+=("$path"); done \
    < <(find "$WORK" -maxdepth 1 -type f -print | sort)
python3 "$PY" assert-private "${private_args[@]}" "${output_paths[@]}"

echo "==> PASS: HTTP/CLI parity, exits 0/1/2, scopes, privacy, provenance, and zero mutation"
