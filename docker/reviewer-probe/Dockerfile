# syntax=docker/dockerfile:1.7
#
# Local feasibility probe: synthetic reviewer container (NOT production).
# Builds the real wenlan-server + wenlan-mcp sources plus the compiled
# `seed_reviewer_library` example (crates/wenlan-mcp/examples/
# seed_reviewer_library.rs + support/reviewer_seed.rs) on Debian 13 glibc.
# Debug build, debuginfo off. No Cloudflare adapter, OAuth, public routing,
# or persistent history is implemented here.

ARG RUST_VERSION=1.95.0

FROM rust:${RUST_VERSION}-trixie AS builder
RUN apt-get update && apt-get install -y --no-install-recommends \
        cmake pkg-config libssl-dev clang lld curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src

# Allowlisted build context only: root manifests plus the exact source
# inputs cargo needs (never `COPY .`, which would drag .git/.wrangler/
# node_modules/data/credentials/eval/contracts/testdata into the build).
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates/wenlan-server/Cargo.toml crates/wenlan-server/Cargo.toml
COPY crates/wenlan-server/build.rs crates/wenlan-server/build.rs
COPY crates/wenlan-server/src/ crates/wenlan-server/src/
COPY crates/wenlan-mcp/Cargo.toml crates/wenlan-mcp/Cargo.toml
COPY crates/wenlan-mcp/src/ crates/wenlan-mcp/src/
COPY crates/wenlan-mcp/examples/ crates/wenlan-mcp/examples/
COPY crates/wenlan-core/Cargo.toml crates/wenlan-core/Cargo.toml
COPY crates/wenlan-core/build.rs crates/wenlan-core/build.rs
COPY crates/wenlan-core/src/ crates/wenlan-core/src/
COPY crates/wenlan-types/Cargo.toml crates/wenlan-types/Cargo.toml
COPY crates/wenlan-types/src/ crates/wenlan-types/src/
# Workspace members required for manifest resolution (not built here).
COPY crates/wenlan-cli/Cargo.toml crates/wenlan-cli/Cargo.toml
COPY crates/wenlan-cli/src/ crates/wenlan-cli/src/
COPY app/Cargo.toml app/Cargo.toml
COPY app/build.rs app/build.rs
COPY app/src/ app/src/

# Feasibility probe: plain debug build, debug info disabled. Bounded jobs.
ENV CARGO_PROFILE_DEV_DEBUG=0 CARGO_BUILD_JOBS=2
RUN cargo build --locked -p wenlan-server -p wenlan-mcp \
 && cargo build --locked -p wenlan-mcp --example seed_reviewer_library
# Current Linux ORT links statically. Retain staged shared libraries if a
# compatible dependency configuration emits them, without requiring one.
RUN mkdir /runtime-libs && for lib in target/debug/libonnxruntime.so*; do \
      if [ -e "$lib" ]; then cp -L "$lib" /runtime-libs/; fi; \
    done

FROM debian:13-slim@sha256:020c0d20b9880058cbe785a9db107156c3c75c2ac944a6aa7ab59f2add76a7bd AS runtime
RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends \
        libgomp1 bash curl ca-certificates; \
    rm -rf /var/lib/apt/lists/*; \
    useradd --uid 10001 --create-home --shell /bin/bash reviewer; \
    mkdir -p /runtime; \
    chown 10001:10001 /runtime
COPY --from=builder /src/target/debug/wenlan-server /usr/local/bin/wenlan-server
COPY --from=builder /src/target/debug/wenlan-mcp /usr/local/bin/wenlan-mcp
COPY --from=builder /src/target/debug/examples/seed_reviewer_library /usr/local/bin/seed_reviewer_library
COPY --from=builder /runtime-libs/ /usr/local/lib/
COPY docker/reviewer-probe/entrypoint.sh /opt/reviewer-probe/entrypoint.sh
RUN chmod 0555 /opt/reviewer-probe/entrypoint.sh
ENV LD_LIBRARY_PATH=/usr/local/lib
RUN set -eu; for binary in wenlan-server wenlan-mcp seed_reviewer_library; do \
      dependencies="$(ldd "/usr/local/bin/$binary")"; \
      case "$dependencies" in *"not found"*) printf '%s\n' "$dependencies"; exit 1;; esac; \
    done
# Fixed synthetic scope; the entrypoint re-exports these unconditionally so
# inherited container env cannot override them. The Bearer [REDACTED] stays
# runtime-only (`-e REVIEWER_BEARER_TOKEN=...`) and is never baked in.
ENV REVIEWER_RUNTIME_ROOT=/runtime/synthetic \
    HOME=/runtime/synthetic/home \
    WENLAN_DATA_DIR=/runtime/synthetic \
    WENLAN_MCP_CACHE_DIR=/runtime/synthetic/mcp-cache \
    WENLAN_NO_AUTOSTART=1 \
    WENLAN_SPACE=atlas-review \
    WENLAN_RERANKER_MODE=off \
    WENLAN_BIND_ADDR=127.0.0.1:7878 \
    WENLAN_PORT=7878
USER 10001
EXPOSE 8080
ENTRYPOINT ["/opt/reviewer-probe/entrypoint.sh"]
