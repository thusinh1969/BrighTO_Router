# syntax=docker/dockerfile:1.7
# ---------------------------------------------------------------------------
# Multi-stage image. The runtime image contains only the binary, CA certs,
# timezone data, and a non-root user.
# Multi-architecture build example:
#   docker buildx build --platform linux/amd64,linux/arm64 -t <registry>/brighto-router:0.1.0 --push .
# ---------------------------------------------------------------------------
ARG RUST_VERSION=1.98.1

# ---------- 1. chef: cache dependencies separately from application source ----------
FROM --platform=$BUILDPLATFORM rust:${RUST_VERSION}-bookworm AS chef
ARG BUILDARCH
ARG TARGETARCH
RUN apt-get update && apt-get install -y --no-install-recommends cmake clang libclang-dev \
 && if [ "$BUILDARCH" != "$TARGETARCH" ]; then \
      case "$TARGETARCH" in \
        arm64) apt-get install -y --no-install-recommends gcc-aarch64-linux-gnu g++-aarch64-linux-gnu ;; \
        amd64) apt-get install -y --no-install-recommends gcc-x86-64-linux-gnu g++-x86-64-linux-gnu ;; \
        *) exit 1 ;; \
      esac; \
    fi \
 && rm -rf /var/lib/apt/lists/* \
 && cargo install cargo-chef --version 0.1.78 --locked
# Compile natively even for ARM releases; emulation is only needed to test them.
RUN case "$TARGETARCH" in \
      amd64) target=x86_64-unknown-linux-gnu; linker=x86_64-linux-gnu-gcc; cargo_linker=CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER ;; \
      arm64) target=aarch64-unknown-linux-gnu; linker=aarch64-linux-gnu-gcc; cargo_linker=CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER ;; \
      *) exit 1 ;; \
    esac \
 && rustup target add "$target" \
 && printf 'export CARGO_BUILD_TARGET=%s\nexport BRIGHTO_STRIP=strip\n' "$target" > /usr/local/share/brighto-target.env \
 && if [ "$BUILDARCH" != "$TARGETARCH" ]; then \
      printf 'export %s=%s\nexport BRIGHTO_STRIP=%s\n' "$cargo_linker" "$linker" "${linker%-gcc}-strip" >> /usr/local/share/brighto-target.env; \
    fi
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ---------- 2. builder ----------
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# Dependency layer stays cached until Cargo.toml or Cargo.lock changes.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    . /usr/local/share/brighto-target.env \
 && cargo chef cook --release --target "$CARGO_BUILD_TARGET" --recipe-path recipe.json
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    . /usr/local/share/brighto-target.env \
 && cargo build --release --locked \
 && mkdir -p /out \
 && cp "target/$CARGO_BUILD_TARGET/release/brighto-router" /out/brighto-router \
 && "$BRIGHTO_STRIP" -g /out/brighto-router

# ---------- 3. runtime ----------
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates tzdata \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --home /nonexistent --shell /usr/sbin/nologin router \
 && mkdir -p /var/lib/brighto-router/provider_keys && chown -R router:router /var/lib/brighto-router
COPY --from=builder /out/brighto-router /usr/local/bin/brighto-router
COPY --from=builder /app/migrations /app/migrations
USER router
ENV LISTEN_ADDR=0.0.0.0:8080 \
    LEDGER_FALLBACK_FILE=/var/lib/brighto-router/ledger-fallback.jsonl \
    DATA_DIR=/var/lib/brighto-router \
    RUST_LOG=brighto_router=info
VOLUME ["/var/lib/brighto-router"]
EXPOSE 8080
HEALTHCHECK --interval=10s --timeout=3s --start-period=5s --retries=3 \
  CMD ["/usr/local/bin/brighto-router", "healthcheck"]
STOPSIGNAL SIGTERM
ENTRYPOINT ["/usr/local/bin/brighto-router"]
