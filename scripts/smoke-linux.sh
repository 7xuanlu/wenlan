#!/usr/bin/env bash
set -euo pipefail

IMAGE_TAG="${IMAGE_TAG:-wenlan-server:smoke}"
CONTAINER="${CONTAINER:-wenlan-smoke}"
PORT="${PORT:-17878}"
# Default to the host arch: linux/arm64 on Apple Silicon devs (fast via OrbStack
# / Docker Desktop), linux/amd64 on CI (ubuntu-24.04 is amd64; arm64 emulation
# via QEMU adds 15-30 min). Override with PLATFORM= env var.
case "$(uname -m)" in
    arm64|aarch64) PLATFORM="${PLATFORM:-linux/arm64}" ;;
    *)             PLATFORM="${PLATFORM:-linux/amd64}" ;;
esac

cleanup() {
    docker rm -f "$CONTAINER" >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo "==> Building ${IMAGE_TAG} for ${PLATFORM}"
# Optional buildx GHA cache (CI sets BUILDX_CACHE_FROM/_TO). Outside CI the
# variables are empty and the build runs without remote cache.
CACHE_ARGS=()
[ -n "${BUILDX_CACHE_FROM:-}" ] && CACHE_ARGS+=(--cache-from "$BUILDX_CACHE_FROM")
[ -n "${BUILDX_CACHE_TO:-}" ] && CACHE_ARGS+=(--cache-to "$BUILDX_CACHE_TO")
docker buildx build --platform "$PLATFORM" --load \
    "${CACHE_ARGS[@]}" \
    -f docker/Dockerfile.daemon -t "$IMAGE_TAG" .

echo "==> Starting container on port ${PORT}"
# Deliberately omit --rm: if the daemon crashes before /api/health
# responds, `docker logs` needs the container record to still exist.
# The cleanup trap removes it with `docker rm -f` regardless.
#
# No host -v mount: container runs as `nonroot` (UID 65532) and a
# host bind would inherit the runner's UID, breaking writes to /data.
# Smoke wants ephemeral storage anyway; the named VOLUME in the image
# gives the container a writable anonymous mount.
docker run -d --name "$CONTAINER" -p "${PORT}:7878" "$IMAGE_TAG"

echo "==> Waiting for /api/health"
for i in $(seq 1 30); do
    if curl -sf "http://127.0.0.1:${PORT}/api/health" >/dev/null; then
        echo "    healthy after ${i}s"
        break
    fi
    sleep 1
    if [ "$i" = "30" ]; then
        echo "FAIL: daemon did not become healthy" >&2
        echo "--- container state ---" >&2
        docker ps -a --filter "name=${CONTAINER}" --format 'id={{.ID}} status={{.Status}} exit={{.RunningFor}}' >&2 || true
        echo "--- container logs ---" >&2
        docker logs "$CONTAINER" >&2 || true
        exit 1
    fi
done

echo "==> Store a memory"
STORE_RESP=$(curl -sf -X POST "http://127.0.0.1:${PORT}/api/memory/store" \
    -H 'Content-Type: application/json' \
    -d '{"content":"This is a Linux Docker smoke test memory verifying the daemon stores notes correctly across cross-platform builds.","memory_type":"lesson"}')
echo "    $STORE_RESP"

echo "==> Search for it"
SEARCH_RESP=$(curl -sf -X POST "http://127.0.0.1:${PORT}/api/memory/search" \
    -H 'Content-Type: application/json' \
    -d '{"query":"Linux Docker smoke","limit":3}')
echo "    $SEARCH_RESP"
echo "$SEARCH_RESP" | grep -q "smoke test memory" || {
    echo "FAIL: search did not return stored memory" >&2
    exit 1
}

echo "==> Status"
curl -sf "http://127.0.0.1:${PORT}/api/status" | head -c 200
echo

echo "==> PASS"
