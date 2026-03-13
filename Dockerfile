# Build stage
FROM rust:1.93-slim AS build

WORKDIR /parrot

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    cmake \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Cache dependencies
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src && echo "fn main() {}" > src/main.rs
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release --locked

# Copy source and build
COPY src/ ./src/
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release --locked

# Final stage
FROM python:3.10-slim-trixie

RUN apt-get update && apt-get install -y --no-install-recommends \
    wget

# Copy binary
COPY --from=build /parrot/target/release/parrot /usr/local/bin/
COPY --chmod=0755 ./entrypoint.sh .

ENTRYPOINT ["./entrypoint.sh"]