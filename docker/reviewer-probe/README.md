# Reviewer probe (local feasibility lane only)

Builds the real `wenlan-server` + `wenlan-mcp` sources plus the compiled
`seed_reviewer_library` example (`crates/wenlan-mcp/examples/
seed_reviewer_library.rs`, seed content in `support/reviewer_seed.rs`,
scope `atlas-review`) into a linux/amd64 Debian 13 container.
No personal library content is involved.

## Run (lead inspects/runs; loopback publish only)

```sh
docker build --platform linux/amd64 -f docker/reviewer-probe/Dockerfile -t reviewer-probe:local .
docker run --rm --name reviewer-probe-local \
  -p 127.0.0.1:8080:8080 \
  -e REVIEWER_BEARER_TOKEN \
  reviewer-probe:local
curl -H "Authorization: Bearer $REVIEWER_BEARER_TOKEN" \
  http://127.0.0.1:8080/health
```

Cleanup (named owned container only; never touch other services):

```sh
docker stop reviewer-probe-local
```

## Truthful constraints

- The seed example exclusively creates `/runtime/synthetic` (mode 0700,
  `create_new` config, `WENLAN_DATA_DIR=root`, knowledge at `pages/`);
  the entrypoint refuses any pre-existing root and creates nothing itself.
- Daemon binds container loopback (`127.0.0.1:7878`) only; the sole
  `0.0.0.0` listener in-container is the bearer-authenticated query-only MCP
  (`brief`, `recall`, `get_page_sources`) with the strict `WENLAN_SPACE`
  pin fixed to `atlas-review` (`crates/wenlan-mcp/src/serve.rs`).
- Token arrives only via `REVIEWER_BEARER_TOKEN` at runtime; never baked
  in, never echoed; failure tails are bounded to 20 lines.
- Either owned child exiting (even 0) ends the container nonzero.
- Cold first-boot embedding downloads are a known cost, NOT readiness:
  health checks alone do not prove tool correctness or review readiness.
  Run real Brief, recall, provenance, scope-denial and lifecycle checks too.
- Does NOT implement: Cloudflare Containers adapter, OAuth, public
  routing, stable reviewer login, persistent search history, cold-start
  SLO. Build and run receipts are separate from this implementation.

Set a generated synthetic bearer token in the calling shell environment before
running; never put a real credential or personal library in this probe. The
container is removed by `--rm` when stopped. Recreate rather than restart it,
since a previously seeded root is deliberately refused.

`node --test docker/reviewer-probe/entrypoint.test.mjs` checks invalid-token
fail-closed behavior only, not real MCP calls or container isolation.

For the real container acceptance check, set `WENLAN_CONTAINER_PROBE_IMAGE` to
the locally built `sha256:` image ID and `WENLAN_RELAY_TOOLCHAIN` to the existing
relay test-toolchain directory, then run:

```sh
node --test docker/reviewer-probe/container.test.mjs
```

It creates one randomly named owned container, never pulls an image, exposes
only a random host-loopback MCP port, and runs the existing
`relay/tests/real-backend-check.mjs` suite against the real database. OAuth runs
in the isolated local Worker fixture, not the deployed Cloudflare service.
It checks no host mounts, anonymous denial, shutdown, and fail-closed reuse of
an existing root, then removes only its labeled container. The fixed test token
is synthetic and must never be used for a publicly reachable service.

## Offline model packaging probe

`Dockerfile.offline` extends the existing locally built probe image (override
`PROBE_IMAGE` when needed). It downloads public model assets in a discarded
build stage using the real seed, checks the expected upstream model revision,
and copies only the model cache and source/license notices into the final image.
No host build-context files, build-time database, bearer or account is copied.
This is a local feasibility artifact, not a deployable Cloudflare integration.
Review and pin the base image and all dependencies before production packaging.

```sh
docker build --platform linux/amd64 --pull=false \
  -f docker/reviewer-probe/Dockerfile.offline \
  -t wenlan-reviewer-offline-probe:0913 .
docker image inspect wenlan-reviewer-offline-probe:0913 --format '{{.Id}}'
```

Set `WENLAN_OFFLINE_PROBE_IMAGE` to that explicit `sha256:` image ID, then run
`node --test docker/reviewer-probe/offline.test.mjs`. This opt-in test uses
`--network none`, no host mounts or published ports, and real authenticated MCP
calls through container loopback. It checks Brief, recall, provenance and
cross-Space denial, then stops and removes only its owned container. It does not
prove hosted cold-start latency, stable reviewer login or ChatGPT availability.
It also recreates a fresh container with the same synthetic backend credential,
requires the previous MCP session to fail with404, initializes a new session,
and checks the real Brief again before a second graceful stop and cleanup.
The existing `container.test.mjs` can also exercise this image through the local
OAuth Worker fixture; that fixture is separate from the offline test.
