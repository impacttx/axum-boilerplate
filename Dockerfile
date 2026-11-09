# syntax=docker/dockerfile:1
# -----------------------------------------------------------------------------
# Stage 0: cargo-chef base (dependency-layer caching)
# -----------------------------------------------------------------------------
FROM lukemathwalker/cargo-chef:latest-rust-1-slim-bookworm AS chef

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config ca-certificates libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# -----------------------------------------------------------------------------
# Stage 1: Compute dependency recipe
# -----------------------------------------------------------------------------
FROM chef AS planner

COPY Cargo.toml Cargo.lock ./
COPY migration ./migration
COPY src ./src
RUN cargo chef prepare --recipe-path recipe.json

# -----------------------------------------------------------------------------
# Stage 2: Build release binaries
# -----------------------------------------------------------------------------
FROM chef AS builder

ENV CARGO_INCREMENTAL=0

# Build dependencies only; cached until Cargo manifests change
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo chef cook --release --workspace --recipe-path recipe.json

COPY Cargo.toml Cargo.lock ./
COPY migration ./migration
COPY src ./src

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --release --workspace

# -----------------------------------------------------------------------------
# Stage 3: Prepare a writable upload directory owned by the non-root user
# -----------------------------------------------------------------------------
FROM chef AS assets
RUN mkdir -p /app/uploads && chown -R 65532:65532 /app

# -----------------------------------------------------------------------------
# Stage 4: Production runtime image (distroless: no shell, no package manager)
# -----------------------------------------------------------------------------
FROM gcr.io/distroless/cc-debian12 AS base

# CA certificates for outbound HTTPS (Google OAuth, object storage, email)
COPY --from=chef /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
# Application binary (dynamically linked against glibc/libstdc++ from distroless)
COPY --from=builder /app/target/release/axum-boilerplate /usr/local/bin/axum-boilerplate
# Writable upload directory owned by the non-root user
COPY --from=assets --chown=65532:65532 /app/uploads /app/uploads

WORKDIR /app
USER 65532:65532

ENV HOST=0.0.0.0 PORT=8000 RUST_LOG=info,tower_http=info

EXPOSE 8000

ENTRYPOINT ["/usr/local/bin/axum-boilerplate"]
