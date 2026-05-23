#!/usr/bin/env bash
set -euo pipefail

IMAGE_TAG="${IMAGE_TAG:-origin-server:smoke}"
CONTAINER="${CONTAINER:-origin-smoke}"
PORT="${PORT:-17878}"
DATA_DIR="$(mktemp -d -t origin-smoke.XXXXXX)"
# Default to the host arch: linux/arm64 on Apple Silicon devs (fast via OrbStack
# / Docker Desktop), linux/amd64 on CI (ubuntu-24.04 is amd64; arm64 emulation
# via QEMU adds 15-30 min). Override with PLATFORM= env var.
case "$(uname -m)" in
    arm64|aarch64) PLATFORM="${PLATFORM:-linux/arm64}" ;;
    *)             PLATFORM="${PLATFORM:-linux/amd64}" ;;
esac

cleanup() {
    docker rm -f "$CONTAINER" >/dev/null 2>&1 || true
    rm -rf "$DATA_DIR"
}
trap cleanup EXIT

echo "==> Building ${IMAGE_TAG} for ${PLATFORM}"
docker buildx build --platform "$PLATFORM" --load \
    -f docker/Dockerfile.daemon -t "$IMAGE_TAG" .

echo "==> Starting container on port ${PORT}, data ${DATA_DIR}"
docker run --rm -d --name "$CONTAINER" -p "${PORT}:7878" \
    -v "${DATA_DIR}:/data" "$IMAGE_TAG"

echo "==> Waiting for /api/health"
for i in $(seq 1 30); do
    if curl -sf "http://127.0.0.1:${PORT}/api/health" >/dev/null; then
        echo "    healthy after ${i}s"
        break
    fi
    sleep 1
    if [ "$i" = "30" ]; then
        echo "FAIL: daemon did not become healthy" >&2
        docker logs "$CONTAINER" >&2
        exit 1
    fi
done

echo "==> Store a memory"
STORE_RESP=$(curl -sf -X POST "http://127.0.0.1:${PORT}/api/memory/store" \
    -H 'Content-Type: application/json' \
    -d '{"content":"Smoke test memory from macOS host"}')
echo "    $STORE_RESP"

echo "==> Search for it"
SEARCH_RESP=$(curl -sf -X POST "http://127.0.0.1:${PORT}/api/memory/search" \
    -H 'Content-Type: application/json' \
    -d '{"query":"smoke test","limit":3}')
echo "    $SEARCH_RESP"
echo "$SEARCH_RESP" | grep -q "Smoke test memory" || {
    echo "FAIL: search did not return stored memory" >&2
    exit 1
}

echo "==> Status"
curl -sf "http://127.0.0.1:${PORT}/api/status" | head -c 200
echo

echo "==> PASS"
