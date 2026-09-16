# syntax=docker/dockerfile:1.7
# ---------------------------------------------------------------------------
# Multi-stage. Ảnh cuối ~35 MB, chỉ có binary + CA certs, chạy non-root.
# Build đa kiến trúc (x86_64 build box + GX10 aarch64):
#   docker buildx build --platform linux/amd64,linux/arm64 -t <registry>/brighto-router:0.1.0 --push .
# ---------------------------------------------------------------------------
ARG RUST_VERSION=1.98.1

# ---------- 1. chef: cache dependency riêng khỏi source, đổi 1 dòng code không build lại 300 crate ----------
FROM rust:${RUST_VERSION}-bookworm AS chef
RUN apt-get update && apt-get install -y --no-install-recommends cmake clang libclang-dev \
 && rm -rf /var/lib/apt/lists/* \
 && cargo install cargo-chef --version 0.1.78 --locked
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ---------- 2. builder ----------
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# chỉ dependency — layer này cache cho tới khi Cargo.toml/Cargo.lock đổi
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --release --locked \
 && strip -g target/release/brighto-router

# ---------- 3. runtime ----------
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates tzdata \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --home /nonexistent --shell /usr/sbin/nologin router \
 && mkdir -p /var/lib/brighto-router && chown router:router /var/lib/brighto-router
COPY --from=builder /app/target/release/brighto-router /usr/local/bin/brighto-router
COPY --from=builder /app/migrations /app/migrations
USER router
ENV LISTEN_ADDR=0.0.0.0:8080 \
    LEDGER_FALLBACK_FILE=/var/lib/brighto-router/ledger-fallback.jsonl \
    RUST_LOG=brighto_router=info
VOLUME ["/var/lib/brighto-router"]
EXPOSE 8080
HEALTHCHECK --interval=10s --timeout=3s --start-period=5s --retries=3 \
  CMD ["/usr/local/bin/brighto-router", "healthcheck"]
STOPSIGNAL SIGTERM
ENTRYPOINT ["/usr/local/bin/brighto-router"]
