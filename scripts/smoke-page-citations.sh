#!/usr/bin/env bash
# Smoke test: per-claim verified citations in wiki pages — a LIVE daemon with
# a REAL on-device model on both the distill leg (fresh page, [N] markers +
# verified/unverified tiers) and the annotate-only backfill leg (legacy page,
# prose-unchanged guard). L7 manual: needs the qwen3-4b GGUF already in the
# HF cache (~2.5GB; `wenlan model install`), so it cannot run in CI. Isolated
# port + data dir per repo smoke-test policy.
#
# Two phases (mirrors smoke-doc-reconcile.sh's restart pattern):
#   A. daemon up, store+confirm 3 related memories, wait for the natural
#      Idle trigger (TriggerKind::Idle — the only trigger that runs
#      Phase::Emergence, i.e. new-page distillation from clusters; BurstEnd
#      only runs Recaps+RefinementQueue, see refinery/mod.rs TriggerKind::
#      runs_phase) to synthesize a page inline — the write path (Task 5):
#      [N] markers verified + persisted atomically. Idle needs 10min of
#      global write-quiet (IDLE_THRESHOLD, scheduler.rs) after the last
#      store, so this leg is the long pole of the smoke.
#   B. create a legacy page via POST /api/pages (citations IS NULL by
#      construction — create_page never links page_evidence), then restart
#      the daemon so the citation-backfill sweep's "first tick fires almost
#      immediately after boot" init (last_citation_sweep = now - 30min, same
#      trick as the enrichment/reconcile sweeps) picks it up within ~2min
#      instead of waiting out the real 30-min interval.
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
        RUST_LOG=warn,wenlan_server=info,wenlan_core::citations=info,wenlan_core::refinery=info \
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

echo "==> Phase A: daemon up (citation backfill default ON, harmless — no pages yet)"
start_daemon

AGENT="smoke-citation-agent"
echo "==> Storing + confirming 3 related memories (one cluster)"
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
    curl -sf -X POST "$HOST/api/memory/confirm/$MEM_ID" >/dev/null || fail "confirm failed for $MEM_ID"
    MEM_IDS+=("$MEM_ID")
    sleep 1
done
echo "    stored + confirmed: ${MEM_IDS[*]}"

echo "==> Waiting for Idle to synthesize a page (up to 20min: 10min quiet threshold + up to 10min steep deadline)"
# The DB is fresh (mktemp'd data dir) and the legacy-annotate leg's page is
# created strictly AFTER this loop, so any page appearing here is the one
# distilled from the 3 beacon memories — no need to pattern-match prose,
# which the LLM is free to paraphrase.
DISTILL_PAGE_ID=""
DISTILL_PAGE_JSON=""
for i in $(seq 1 240); do
    PAGES_RESP="$(curl -sf "$HOST/api/pages?limit=50" 2>/dev/null || true)"
    CANDIDATE="$(echo "$PAGES_RESP" | jq -c '.pages // [] | sort_by(.created_at) | last // empty' 2>/dev/null || true)"
    if [ -n "$CANDIDATE" ] && [ "$CANDIDATE" != "null" ]; then
        DISTILL_PAGE_JSON="$CANDIDATE"
        DISTILL_PAGE_ID="$(echo "$CANDIDATE" | jq -r '.id')"
        echo "    page $DISTILL_PAGE_ID after $((i * 5))s"
        break
    fi
    sleep 5
done
[ -n "$DISTILL_PAGE_ID" ] || fail "no page synthesized from the beacon memories within 20min"

echo "==> Asserting [N] markers + citations on the distilled page"
BODY="$(echo "$DISTILL_PAGE_JSON" | jq -r '.content')"
echo "$BODY" | grep -qE '\[[1-3]\]' || fail "distilled body has no [N] marker in 1..=3: $BODY"
CITE_COUNT="$(echo "$DISTILL_PAGE_JSON" | jq '.citations | length')"
[ "$CITE_COUNT" -ge 1 ] || fail "distilled page has zero citations: $DISTILL_PAGE_JSON"
VERIFIED_COUNT="$(echo "$DISTILL_PAGE_JSON" | jq '[.citations[] | select(.status == "verified")] | length')"
[ "$VERIFIED_COUNT" -ge 1 ] || fail "distilled page has zero verified citations: $DISTILL_PAGE_JSON"
echo "    body: $BODY"
echo "    citations: $CITE_COUNT total, $VERIFIED_COUNT verified"
echo "    score distribution: $(echo "$DISTILL_PAGE_JSON" | jq -c '[.citations[].score]')"

echo "==> Legacy-annotate leg: creating a page with no citation source"
LEGACY_CONTENT="The archived onboarding doc lives at /docs/legacy/onboarding.md and has not been updated since the citation feature shipped."
CREATE_RESP="$(curl -sf -X POST "$HOST/api/pages" \
    -H 'Content-Type: application/json' \
    -d "$(jq -n --arg t "Legacy Onboarding Note" --arg c "$LEGACY_CONTENT" '{title:$t, content:$c}')")" || fail "create_page failed"
LEGACY_PAGE_ID="$(echo "$CREATE_RESP" | jq -r '.id // empty')"
[ -n "$LEGACY_PAGE_ID" ] || fail "no id in create_page response: $CREATE_RESP"
echo "    created $LEGACY_PAGE_ID"

echo "==> Phase B: restart so the citation-backfill sweep's first tick fires almost immediately"
stop_daemon
start_daemon

echo "==> Waiting for the backfill sweep to touch the legacy page (up to 5min)"
# NOTE: `page.citations` cannot distinguish "citations IS NULL" (never
# processed) from "citations = '[]'" (poison-pilled after 3 rejections) —
# `row_to_page` collapses a NULL column to an empty Vec (crates/wenlan-core/
# src/db.rs, row_to_page), so both serialize to `"citations": []` over HTTP.
# The reliable observable signal is the changelog entry the sweep writes
# (`edited_by: "citation_backfill"`, see citations.rs build_backfill_changelog),
# surfaced as `page.last_edited_by`.
found=""
for i in $(seq 1 60); do
    PAGE_RESP="$(curl -sf "$HOST/api/pages/$LEGACY_PAGE_ID" 2>/dev/null || true)"
    EDITED_BY="$(echo "$PAGE_RESP" | jq -r '.page.last_edited_by // empty' 2>/dev/null || true)"
    if [ "$EDITED_BY" = "citation_backfill" ]; then
        found=1
        echo "    sweep touched the page after $((i * 5))s (last_edited_by=citation_backfill)"
        break
    fi
    sleep 5
done
[ -n "$found" ] || fail "legacy page never got a citation_backfill changelog entry within 5min"

echo "==> Asserting legacy prose is unchanged (annotate-only guard)"
FINAL_CONTENT="$(echo "$PAGE_RESP" | jq -r '.page.content')"
[ "$FINAL_CONTENT" = "$LEGACY_CONTENT" ] || fail "legacy prose changed: expected [$LEGACY_CONTENT] got [$FINAL_CONTENT]"

echo "==> PASS"
