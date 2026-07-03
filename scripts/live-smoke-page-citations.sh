#!/usr/bin/env bash
# Live smoke test: per-claim verified citations in wiki pages — a LIVE daemon
# with a REAL on-device model on both the distill leg (fresh page, [N] markers +
# verified tiers) and the annotate-only backfill leg (legacy page,
# prose-unchanged guard). L7 manual: needs the qwen3-4b GGUF already in the
# HF cache (~2.5GB; `wenlan model install`), so it cannot run in CI. Isolated
# port + data dir per repo smoke-test policy.
#
# Two legs, both DETERMINISTIC (no Idle-trigger wait, no similarity roulette —
# the natural Emergence path needs unconfirmed memories clustering at cosine
# >= 0.73 while the store gate rejects >= ~0.9 near-dupes, a band too narrow
# to hit reliably with synthetic facts; first smoke iterations failed there):
#   A. store 3 facts -> POST /api/pages with source_memory_ids -> force
#      redistill (POST /api/distill {target, force:true} -> deep_distill_single,
#      the same DISTILL_PAGE write path the refinery uses) -> assert markers,
#      per-occurrence citations, >= 1 verified, score distribution logged.
#   B. create a second evidence-backed page (citations stays NULL by
#      construction on the create path), restart the daemon so the citation
#      backfill sweep's first tick fires at the first 30s poll
#      (last_citation_sweep inits to now - 30min) -> the REAL
#      ANNOTATE_CITATIONS prompt + prose guard run against the real model ->
#      assert the sweep touched the page AND marker-stripped prose is
#      byte-identical to the original (the annotate-only invariant).
set -euo pipefail

PORT="${PORT:-17882}"
HOST="http://127.0.0.1:${PORT}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$ROOT/target/debug"

DATA_DIR="$(mktemp -d "${TMPDIR:-/tmp}/wenlan-smoke-citations.XXXXXX")"
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
    tail -60 "$DATA_DIR/daemon.log" >&2 || true
    exit 1
}

start_daemon() {
    (cd "$ROOT" && WENLAN_PORT="$PORT" WENLAN_DATA_DIR="$DATA_DIR" \
        HF_HUB_OFFLINE=1 \
        RUST_LOG=warn,wenlan_server=info,wenlan_core::citations=info,wenlan_core::synthesis=info \
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

# The distill + annotate paths both need an on-device model; point the
# isolated config at the cached one.
printf '{"setup_completed":true,"on_device_model":"qwen3-4b"}\n' >"$DATA_DIR/config.json"

echo "==> Phase A: daemon up"
start_daemon

AGENT="smoke-citation-agent"
echo "==> Storing 3 related memories"
declare -a MEM_IDS=()
FACTS=(
    "The wenlan-citation-beacon service listens on port 4471 and exposes a /status endpoint for health checks."
    "The wenlan-citation-beacon service authenticates callers with a rotating HMAC token refreshed every 15 minutes."
    "The wenlan-citation-beacon service logs every request to a local SQLite audit table named beacon_requests."
)
for fact in "${FACTS[@]}"; do
    STORE_RESP="$(curl -sf -X POST "$HOST/api/memory/store" \
        -H 'Content-Type: application/json' \
        -d "$(jq -n --arg c "$fact" --arg a "$AGENT" '{content:$c, source_agent:$a}')")" || fail "store failed: $fact"
    MEM_ID="$(echo "$STORE_RESP" | jq -r '.source_id // empty')"
    [ -n "$MEM_ID" ] || fail "no source_id in store response: $STORE_RESP"
    MEM_IDS+=("$MEM_ID")
    sleep 1
done
echo "    stored: ${MEM_IDS[*]}"

echo "==> Creating a page linked to the 3 memories"
MEMS_JSON="$(printf '%s\n' "${MEM_IDS[@]}" | jq -R . | jq -sc .)"
DISTILL_PAGE_ID="$(curl -sf -X POST "$HOST/api/pages" \
    -H 'Content-Type: application/json' \
    -d "$(jq -n --argjson m "$MEMS_JSON" '{title:"Wenlan Citation Beacon Service", content:"Placeholder body about the beacon service.", source_memory_ids:$m}')" \
    | jq -r '.id // empty')"
[ -n "$DISTILL_PAGE_ID" ] || fail "create_page (distill leg) returned no id"
echo "    created $DISTILL_PAGE_ID"

echo "==> Force redistill (deep_distill_single, real model; retries while the model loads)"
DISTILL_OK=""
for i in $(seq 1 60); do
    RESP="$(curl -sf -X POST "$HOST/api/distill" \
        -H 'Content-Type: application/json' \
        -d "$(jq -n --arg t "$DISTILL_PAGE_ID" '{target:$t, force:true}')" || true)"
    STATUS="$(echo "$RESP" | jq -r '.status // empty')"
    if [ "$STATUS" = "ok" ]; then
        UPDATED="$(echo "$RESP" | jq -r '.updated')"
        [ "$UPDATED" = "true" ] || fail "force distill ran but wrote nothing: $RESP"
        DISTILL_OK=1
        echo "    distilled after $((i * 10))s"
        break
    fi
    sleep 10
done
[ -n "$DISTILL_OK" ] || fail "force distill never reached status=ok within 10min (model load?)"

echo "==> Asserting [N] markers + citations on the distilled page"
PAGE_FILE="$DATA_DIR/distill_page.json"
curl -sf "$HOST/api/pages/$DISTILL_PAGE_ID" -o "$PAGE_FILE" || fail "get_page failed"
BODY="$(jq -r '.page.content' "$PAGE_FILE")"
echo "$BODY" | grep -qE '\[[1-3]\]' || fail "distilled body has no [N] marker in 1..=3: $BODY"
CITE_COUNT="$(jq '.page.citations | length' "$PAGE_FILE")"
[ "$CITE_COUNT" -ge 1 ] || fail "distilled page has zero citations: $(cat "$PAGE_FILE")"
VERIFIED_COUNT="$(jq '[.page.citations[] | select(.status == "verified")] | length' "$PAGE_FILE")"
[ "$VERIFIED_COUNT" -ge 1 ] || fail "distilled page has zero verified citations: $(cat "$PAGE_FILE")"
echo "    citations: $CITE_COUNT total, $VERIFIED_COUNT verified"
echo "    records: $(jq -c '[.page.citations[] | {marker, status, scope, score}]' "$PAGE_FILE")"

echo "==> Legacy-annotate leg: evidence-backed page, citations NULL by construction"
LEGACY_CONTENT="The wenlan-citation-beacon service logs every request to a local SQLite audit table named beacon_requests."
LEGACY_PAGE_ID="$(curl -sf -X POST "$HOST/api/pages" \
    -H 'Content-Type: application/json' \
    -d "$(jq -n --argjson m "$MEMS_JSON" --arg c "$LEGACY_CONTENT" '{title:"Beacon Audit Logging", content:$c, source_memory_ids:$m}')" \
    | jq -r '.id // empty')"
[ -n "$LEGACY_PAGE_ID" ] || fail "create_page (annotate leg) returned no id"
echo "    created $LEGACY_PAGE_ID"

echo "==> Phase B: restart so the backfill sweep's first tick fires at the first poll"
stop_daemon
start_daemon

echo "==> Waiting for the backfill sweep to process the legacy page (up to 10min)"
# Success = changelog entry (edited_by citation_backfill). Guard-rejection =
# a '[citation_backfill] ... guard rejected' log line. EITHER proves the real
# annotate prompt + guard ran live; the poison-pill needs 3 ticks (90min) and
# is e2e-covered, so a single rejection is a smoke PASS for wiring.
SWEEP_SEEN=""
for i in $(seq 1 120); do
    LP="$DATA_DIR/legacy_page.json"
    curl -sf "$HOST/api/pages/$LEGACY_PAGE_ID" -o "$LP" 2>/dev/null || true
    EDITED_BY="$(jq -r '.page.last_edited_by // empty' "$LP" 2>/dev/null || true)"
    if [ "$EDITED_BY" = "citation_backfill" ]; then
        SWEEP_SEEN="changelog"
        break
    fi
    if grep -q "citation_backfill.*guard rejected" "$DATA_DIR/daemon.log" 2>/dev/null; then
        SWEEP_SEEN="guard-rejected"
        break
    fi
    sleep 5
done
[ -n "$SWEEP_SEEN" ] || fail "backfill sweep never touched the legacy page within 10min"
echo "    sweep outcome: $SWEEP_SEEN after $((i * 5))s"

echo "==> Asserting legacy prose unchanged (annotate-only invariant)"
FINAL_CONTENT="$(jq -r '.page.content' "$DATA_DIR/legacy_page.json")"
STRIPPED="$(echo "$FINAL_CONTENT" | sed -E 's/\[[0-9]+\]//g' | tr -s ' ' | sed -E 's/ +$//')"
EXPECTED="$(echo "$LEGACY_CONTENT" | tr -s ' ' | sed -E 's/ +$//')"
[ "$STRIPPED" = "$EXPECTED" ] || fail "legacy prose changed: expected [$EXPECTED] got [$STRIPPED]"
if [ "$SWEEP_SEEN" = "changelog" ]; then
    echo "    legacy citations: $(jq -c '[.page.citations[]? | {marker, status, scope}]' "$DATA_DIR/legacy_page.json")"
fi

echo "==> PASS"
