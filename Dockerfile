# Build image
FROM rust:1.93-slim AS build

# Install build dependencies and clean up in the same layer
RUN apt-get update && apt-get install --no-install-recommends -y \
    build-essential autoconf automake cmake libtool libssl-dev pkg-config

WORKDIR /parrot

# Cache cargo dependencies by copying only Cargo files and creating dummy source
COPY Cargo.toml Cargo.lock ./

# Create dummy source files to satisfy Cargo (supports both binary and library crates)
RUN mkdir -p src && \
    echo "fn main() {}" > src/main.rs

# Build dependencies - this layer will be cached as long as Cargo.toml/Cargo.lock don't change
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release --locked

# Copy only source files needed for compilation (exclude entrypoint.sh and other non-build files)
COPY src/ ./src/

# Build the actual application
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release --locked

# Release image
FROM debian:trixie-slim

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends ffmpeg wget \
    && rm -rf /var/lib/apt/lists/*

COPY --from=build /parrot/target/release/parrot /usr/local/bin/
COPY --chmod=0755 ./entrypoint.sh .

ENTRYPOINT ["./entrypoint.sh"]