# syntax=docker/dockerfile:1
# Keepers: Oracle + Market Maker
FROM rust:1.89-slim-bookworm AS builder
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY tools/e2e/Cargo.toml tools/e2e/
COPY tools/e2e/src/ tools/e2e/src/
COPY tools/rust-circuits/Cargo.toml tools/rust-circuits/
COPY tools/rust-circuits/src/ tools/rust-circuits/src/
COPY keepers/Cargo.toml keepers/Cargo.toml
COPY keepers/Cargo.lock keepers/Cargo.lock
COPY keepers/src/ keepers/src/

RUN cargo build --release --manifest-path keepers/Cargo.toml

FROM debian:bookworm-slim AS release
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/*

# Install stellar CLI (same version as tee-match)
RUN curl -fsSL \
    https://github.com/stellar/stellar-cli/releases/download/v22.2.0/stellar-cli-22.2.0-x86_64-unknown-linux-gnu.tar.gz \
    | tar -xz -C /usr/local/bin/ stellar

COPY --from=builder /build/keepers/target/release/keepers /usr/local/bin/keepers

# Entrypoint script: writes ORACLE_SECRET to stellar identity file before starting keepers
COPY keepers/entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENTRYPOINT ["/entrypoint.sh"]
CMD []
