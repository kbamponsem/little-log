# syntax=docker/dockerfile:1
#
# Little Log server image.
#
#   docker build -f docker/Dockerfile -t little-log-server .
#
# Four stages: the web UI, a cargo-chef planner, the Rust build (cached layers
# for dependencies), and a slim non-root runtime that serves both the API and
# the built UI.

########################################################################### web UI
FROM node:24-bookworm-slim AS web

WORKDIR /web
# Dependencies first: this layer only changes when package/package-lock change.
COPY web/package.json web/package-lock.json ./
RUN npm ci --no-audit --no-fund

COPY web/ ./
RUN npm run build

##################################################################### Rust planner
FROM rust:1.99-bookworm AS chef

RUN cargo install cargo-chef --locked --version 0.1.78
WORKDIR /build

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo chef prepare --recipe-path recipe.json

####################################################################### Rust build
FROM chef AS builder

# Dependencies compile in their own layer, so source edits rebuild only our code.
COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release --locked --package little-log-server

########################################################################## runtime
FROM debian:bookworm-slim AS runtime

# ca-certificates is all we need: the healthcheck is the binary itself, so
# there is no curl/wget in the image.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home-dir /data --shell /usr/sbin/nologin littlelog

WORKDIR /app

COPY --from=builder /build/target/release/little-log-server /usr/local/bin/little-log-server
COPY --from=web /web/dist /app/web

ENV PORT=8080 \
    DATA_DIR=/data \
    WEB_DIR=/app/web \
    RUST_LOG=info

RUN mkdir -p /data && chown -R littlelog:littlelog /data /app

USER littlelog

EXPOSE 8080
VOLUME ["/data"]

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD ["little-log-server", "healthcheck"]

ENTRYPOINT ["little-log-server"]
