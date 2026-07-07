#!/usr/bin/env bash
# L7 manual live smoke test: distill merge-gating flow against a LIVE daemon
# with a REAL on-device model. This script is intentionally not CI-safe: it
# needs the qwen3-4b GGUF cached locally and exercises the daemon + model path.
# It uses an isolated WENLAN_PORT and WENLAN_DATA_DIR, then checks force-distill,
# birth-time unconfirmed + keep-card gating, machine refresh, keep-card accept,
# and the read-only formation sweep grid.
set -euo pipefail

PORT="${PORT:-17883}"
HOST="http://127.0.0.1:${PORT}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$ROOT/target/debug"

DATA_DIR="$(mktemp -d "${TMPDIR:-/tmp}/wenlan-smoke-distill.XXXXXX")"
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
    rm -rf "$DATA_DIR"
}
trap cleanup EXIT

fail() {
    echo "FAIL: $1" >&2
    echo "--- daemon log tail ---" >&2
    tail -80 "$DATA_DIR/daemon.log" >&2 || true
    exit 1
}

start_daemon() {
    (cd "$ROOT" && WENLAN_PORT="$PORT" WENLAN_DATA_DIR="$DATA_DIR" \
        HF_HUB_OFFLINE=1 \
        RUST_LOG=warn,wenlan_server=info,wenlan_core::maintenance=info,wenlan_core::synthesis=info \
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

MODEL_DIR="$HOME/.cache/huggingface/hub/models--unsloth--Qwen3-4B-Instruct-2507-GGUF"
[ -d "$MODEL_DIR" ] || fail "qwen3-4b not in the HF cache ($MODEL_DIR); run 'wenlan model install' once first"

echo "==> Building wenlan-server + wenlan"
(cd "$ROOT" && cargo build -p wenlan-server -p wenlan)

if lsof -ti ":${PORT}" >/dev/null 2>&1; then
    fail "port ${PORT} already in use; set PORT= to another free port"
fi

printf '{"setup_completed":true,"on_device_model":"qwen3-4b"}\n' >"$DATA_DIR/config.json"

echo "==> Starting isolated daemon"
start_daemon

AGENT="smoke-distill-agent"
echo "==> Step 1: seed related memories via /api/memory/store"
declare -a MEM_IDS=()
FACTS=(
    "The heliotrope distill lane stores source-backed merge notes in a local SQLite table named heliotrope_merge_notes."
    "The heliotrope distill lane requires keep-card review before a newly compiled page is trusted by a human."
    "The heliotrope distill lane refreshes machine-owned pages in place while keeping human-owned prose protected."
)
for fact in "${FACTS[@]}"; do
    STORE_RESP="$(curl -sf -X POST "$HOST/api/memory/store" \
        -H 'Content-Type: application/json' \
        -H "X-Agent-Name: $AGENT" \
        -d "$(jq -n --arg c "$fact" --arg a "$AGENT" '{content:$c, source_agent:$a}')")" || fail "memory store failed: $fact"
    MEM_ID="$(echo "$STORE_RESP" | jq -r '.source_id // empty')"
    [ -n "$MEM_ID" ] || fail "memory store response had no source_id: $STORE_RESP"
    MEM_IDS+=("$MEM_ID")
    sleep 1
done
MEMS_JSON="$(printf '%s\n' "${MEM_IDS[@]}" | jq -R . | jq -sc .)"
echo "    stored: ${MEM_IDS[*]}"

echo "==> Step 2: create page, then force distill it with the real model"
PAGE_ID="$(curl -sf -X POST "$HOST/api/pages" \
    -H 'Content-Type: application/json' \
    -H "X-Agent-Name: $AGENT" \
    -d "$(jq -n --argjson m "$MEMS_JSON" '{
        title:"Heliotrope Distill Lane",
        content:"The heliotrope distill lane stores merge notes, requires keep-card review, and refreshes machine-owned pages in place.",
        source_memory_ids:$m,
        creation_kind:"distilled"
    }')" | jq -r '.id // empty')"
[ -n "$PAGE_ID" ] || fail "create page returned no id"
echo "    created $PAGE_ID"

DISTILL_OK=""
for i in $(seq 1 60); do
    DISTILL_RESP="$(curl -sf -X POST "$HOST/api/distill" \
        -H 'Content-Type: application/json' \
        -d "$(jq -n --arg t "$PAGE_ID" '{target:$t, force:true}')" || true)"
    STATUS="$(echo "$DISTILL_RESP" | jq -r '.status // empty')"
    if [ "$STATUS" = "ok" ]; then
        UPDATED="$(echo "$DISTILL_RESP" | jq -r '.updated')"
        [ "$UPDATED" = "true" ] || fail "force distill returned ok but updated=false: $DISTILL_RESP"
        DISTILL_OK=1
        echo "    force distilled after $((i * 10))s"
        break
    fi
    sleep 10
done
[ -n "$DISTILL_OK" ] || fail "force distill did not return status=ok within 10min"

echo "==> Step 3: assert unconfirmed page + keep card"
PAGE_FILE="$DATA_DIR/page.json"
curl -sf "$HOST/api/pages/$PAGE_ID" -o "$PAGE_FILE" || fail "get page failed"
REVIEW_STATUS="$(jq -r '.page.review_status // empty' "$PAGE_FILE")"
[ "$REVIEW_STATUS" = "unconfirmed" ] || fail "page review_status should be unconfirmed, got $REVIEW_STATUS"
KEEP_QUEUE="$DATA_DIR/keep_queue.json"
curl -sf "$HOST/api/refinery/queue?action=page_keep_or_archive" -o "$KEEP_QUEUE" || fail "list keep cards failed"
KEEP_CARD_ID="$(jq -r --arg page "$PAGE_ID" '.proposals[]? | select(.action == "page_keep_or_archive" and (.source_ids | index($page))) | .id' "$KEEP_QUEUE" | head -1)"
[ -n "$KEEP_CARD_ID" ] || fail "no keep/archive card found for $PAGE_ID: $(cat "$KEEP_QUEUE")"
echo "    keep card: $KEEP_CARD_ID"

echo "==> Step 4: refresh machine-owned page in place"
REFRESH_BODY="The heliotrope distill lane keeps source-backed merge notes, keep-card review, and machine-owned refreshes in one flow."
REFRESH_RESP="$(curl -sf -X PUT "$HOST/api/pages/$PAGE_ID" \
    -H 'Content-Type: application/json' \
    -H "X-Agent-Name: $AGENT" \
    -d "$(jq -n --argjson m "$MEMS_JSON" --arg c "$REFRESH_BODY" '{content:$c, source_memory_ids:$m}')" || true)"
REFRESH_OK="$(echo "$REFRESH_RESP" | jq -r '.ok // empty')"
# PageWriteResponse.gated is skip_serializing_if=is_false: absent on the wire means false.
REFRESH_GATED="$(echo "$REFRESH_RESP" | jq -r '.gated // false')"
[ "$REFRESH_OK" = "true" ] || fail "refresh response not ok: $REFRESH_RESP"
[ "$REFRESH_GATED" = "false" ] || fail "machine refresh should not be gated: $REFRESH_RESP"
curl -sf "$HOST/api/pages/$PAGE_ID" -o "$PAGE_FILE" || fail "get refreshed page failed"
UPDATED_BODY="$(jq -r '.page.content' "$PAGE_FILE")"
USER_EDITED="$(jq -r '.page.user_edited' "$PAGE_FILE")"
EDITED_BY="$(jq -r '.page.last_edited_by // empty' "$PAGE_FILE")"
[ "$UPDATED_BODY" = "$REFRESH_BODY" ] || fail "refresh did not update page content"
[ "$USER_EDITED" = "false" ] || fail "machine refresh should leave user_edited=false"
[ "$EDITED_BY" = "agent_refresh" ] || fail "last_edited_by should be agent_refresh, got $EDITED_BY"

echo "==> Step 5: accept keep card and assert effect"
ACCEPT_RESP="$(curl -sf -X POST "$HOST/api/refinery/queue/$KEEP_CARD_ID/accept" \
    -H "X-Agent-Name: $AGENT")" || fail "accept keep card failed"
APPLIED="$(echo "$ACCEPT_RESP" | jq -r '.action_applied // empty')"
[ "$APPLIED" = "page_keep_or_archive" ] || fail "accept applied wrong action: $ACCEPT_RESP"
curl -sf "$HOST/api/pages/$PAGE_ID" -o "$PAGE_FILE" || fail "get archived page failed"
PAGE_STATUS="$(jq -r '.page.status // empty' "$PAGE_FILE")"
[ "$PAGE_STATUS" = "archived" ] || fail "keep-card accept should archive page, got status=$PAGE_STATUS"

echo "==> Step 6: sweep sanity returns the 4-point grid"
SWEEP_FILE="$DATA_DIR/sweep.json"
curl -sf -X POST "$HOST/api/distill" \
    -H 'Content-Type: application/json' \
    -d '{"sweep":true}' -o "$SWEEP_FILE" || fail "formation sweep failed"
GRID_LEN="$(jq '.thresholds | length' "$SWEEP_FILE")"
[ "$GRID_LEN" -eq 4 ] || fail "sweep thresholds length should be 4: $(cat "$SWEEP_FILE")"
GRID_POINTS="$(jq -r '[.thresholds[].formation_threshold] | @csv' "$SWEEP_FILE")"
[ "$GRID_POINTS" = "0.55,0.6,0.65,0.7" ] || fail "unexpected sweep grid points: $GRID_POINTS"

echo "==> PASS"
