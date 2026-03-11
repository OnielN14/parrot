# Build image
# Necessary dependencies to build Parrot
FROM rust:1.93-slim AS build

RUN apt-get update && apt-get install  --no-install-recommends -y \
    build-essential autoconf automake cmake libtool libssl-dev pkg-config

WORKDIR "/parrot"

# Cache cargo build dependencies by creating a dummy source
RUN mkdir src
RUN echo "fn main() {}" > src/main.rs
COPY Cargo.toml ./
COPY Cargo.lock ./
RUN cargo build --release --locked

COPY . .
RUN cargo build --release --locked

# Release image
# Necessary dependencies to run Parrot
FROM debian:stable-slim

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends ffmpeg wget && rm -rf /var/lib/apt/lists/*

COPY --from=build /parrot/target/release/parrot .
COPY ./entrypoint.sh .
ENTRYPOINT ["sh", "./entrypoint.sh"]
