#!/usr/bin/env bash
# Smoke test: the doc-reconcile sweep (doc-grounded revisions, L3) fires in a
# LIVE daemon with a REAL on-device judge — the two seams the e2e suite stubs
# (scheduler wiring + real model output through the parser). L7 manual: needs
# the qwen3-4b GGUF already in the HF cache (~2.5GB; `wenlan model install`),
# so it cannot run in CI. Isolated port + data dir per repo smoke-test policy.
#
# Two phases (the sweep ticks at most once per 30min, so seed first):
#   A. daemon with the sweep DISABLED: store + confirm a capture, ingest a
#      contradicting doc.
#   B. restart with the sweep ENABLED: the first tick (~30s in) judges the
#      pair; assert a pending revision appears, grounded in the doc, with
#      content that actually differs from the capture (no-op echoes are
#      dropped by the guard — if the judge echoes, that is a real FAIL of
#      live rewrite quality, and the log tail says so).
set -euo pipefail

PORT="${PORT:-17881}"
HOST="http://127.0.0.1:${PORT}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$ROOT/target/debug"

DATA_DIR="$(mktemp -d)"
FIXTURE_DIR="$(mktemp -d)"
DAEMON_PID=""

cleanup() {
    if [ -n "$DAEMON_PID" ]; then
        kill -9 "$DAEMON_PID" >/dev/null 2>&1 || true
        wait "$DAEMON_PID" 2>/dev/null || true
    fi
    for _ in $(seq 1 10); do
        if ! lsof -ti ":${PORT}" >/dev/null 2>&1; then
            break
        fi
        sleep 1
    done
    rm -rf "$DATA_DIR" "$FIXTURE_DIR"
}
trap cleanup EXIT

fail() {
    echo "FAIL: $1" >&2
    echo "--- daemon log tail ---" >&2
    tail -40 "$DATA_DIR/daemon.log" >&2 || true
    exit 1
}

start_daemon() { # $1 = WENLAN_ENABLE_DOC_RECONCILE value
    (cd "$ROOT" && WENLAN_PORT="$PORT" WENLAN_DATA_DIR="$DATA_DIR" \
        WENLAN_ENABLE_DOC_RECONCILE="$1" HF_HUB_OFFLINE=1 \
        RUST_LOG=warn,wenlan_server=info,wenlan_core::reconcile=info \
        exec "$BIN/wenlan-server" >>"$DATA_DIR/daemon.log" 2>&1) &
    DAEMON_PID=$!
    local healthy=""
    for i in $(seq 1 120); do
        if curl -sf "$HOST/api/health" >/dev/null 2>&1; then
            echo "    healthy after ${i}s"
            healthy=1
            break
        fi
        kill -0 "$DAEMON_PID" 2>/dev/null || fail "daemon exited during startup"
        sleep 1
    done
    [ -n "$healthy" ] || fail "daemon did not become healthy within 120s"
}

stop_daemon() {
    kill -9 "$DAEMON_PID" >/dev/null 2>&1 || true
    wait "$DAEMON_PID" 2>/dev/null || true
    DAEMON_PID=""
    for _ in $(seq 1 10); do
        lsof -ti ":${PORT}" >/dev/null 2>&1 || break
        sleep 1
    done
}

MODEL_DIR="$HOME/.cache/huggingface/hub/models--unsloth--Qwen3-4B-Instruct-2507-GGUF"
[ -d "$MODEL_DIR" ] || fail "qwen3-4b not in the HF cache ($MODEL_DIR); run 'wenlan model install' once first"

echo "==> Building wenlan-server + wenlan"
(cd "$ROOT" && cargo build -p wenlan-server -p wenlan)

if lsof -ti ":${PORT}" >/dev/null 2>&1; then
    fail "port ${PORT} already in use; set PORT= to another free port"
fi

# The judge needs an on-device model; point the isolated config at the cached one.
printf '{"setup_completed":true,"on_device_model":"qwen3-4b"}\n' >"$DATA_DIR/config.json"

echo "==> Phase A: daemon with the sweep disabled (seeding)"
start_daemon 0

echo "==> Storing + confirming the capture"
CAPTURE="The staging server lives at staging.example.test on port 8080."
STORE_RESP="$(curl -sf -X POST "$HOST/api/memory/store" \
    -H 'Content-Type: application/json' \
    -d "{\"content\":\"$CAPTURE\",\"source_agent\":\"smoke-agent\"}")" || fail "store failed"
MEM_ID="$(echo "$STORE_RESP" | sed -n 's/.*"source_id":"\([^"]*\)".*/\1/p')"
[ -n "$MEM_ID" ] || fail "no source_id in store response: $STORE_RESP"
curl -sf -X POST "$HOST/api/memory/confirm/$MEM_ID" >/dev/null || fail "confirm failed"

echo "==> Ingesting the contradicting doc"
cat >"$FIXTURE_DIR/infra.md" <<'EOF'
The staging server lives at staging.example.test on port 9090.
EOF
WENLAN_HOST="$HOST" "$BIN/wenlan" ingest "$FIXTURE_DIR" >/dev/null

echo "==> Phase B: restart with the sweep enabled (default), first tick ~30s in"
stop_daemon
start_daemon 1

echo "==> Waiting for a doc-grounded pending revision (up to 6min: model load + tick + judge)"
found=""
for i in $(seq 1 72); do
    REVS="$(WENLAN_HOST="$HOST" "$BIN/wenlan" curate 2>/dev/null || true)"
    if echo "$REVS" | grep -q '"grounded_in"'; then
        found=1
        echo "    revision after $((i * 5))s"
        break
    fi
    sleep 5
done
if [ -z "$found" ]; then
    if grep -q "skipping no-op proposal" "$DATA_DIR/daemon.log"; then
        fail "judge echoed the capture (guard dropped it) — live rewrite quality regressed"
    fi
    fail "no doc-grounded revision within 6min"
fi

echo "==> Asserting grounding + a real rewrite"
echo "$REVS" | grep -q 'infra.md' || fail "revision not grounded in infra.md: $REVS"
echo "$REVS" | grep -q '"source_agent": *"reconcile"' || fail "revision not from the reconcile sweep: $REVS"
# The staged content must differ from the capture (the no-op guard's contract,
# now proven against the real model).
REV_CONTENT="$(echo "$REVS" | sed -n 's/.*"content": *"\([^"]*\)".*/\1/p' | head -1)"
[ -n "$REV_CONTENT" ] || fail "could not extract revision content: $REVS"
[ "$REV_CONTENT" != "$CAPTURE" ] || fail "staged revision equals the capture verbatim (no-op leaked through)"
echo "    capture:  $CAPTURE"
echo "    revision: $REV_CONTENT"

echo "==> Capture stays visible until human accept"
SEARCH="$(curl -sf -X POST "$HOST/api/memory/search" \
    -H 'Content-Type: application/json' \
    -d '{"query":"staging server port","limit":5}')" || fail "search failed"
echo "$SEARCH" | grep -q "port 8080" || fail "capture no longer retrievable before accept"

echo "==> PASS"
