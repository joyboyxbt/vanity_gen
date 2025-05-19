# Multi-stage Dockerfile for Solana Vanity Seed Generator (CPU executor)
# Stage 1: Build the Rust binary
FROM rust:latest AS builder
WORKDIR /app
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        build-essential pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

# Stage 2: Create a minimal runtime image
FROM debian:buster-slim
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/solana-vanity-seed /usr/local/bin/solana-vanity-seed
ENTRYPOINT ["solana-vanity-seed"]