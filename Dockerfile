# syntax=docker/dockerfile:1
#
# TEE Match Server — GCP Confidential Space
#
# Runs the tee-match server with attestation, encryption, and ZK proving.
# TLS is terminated by GCP Load Balancer — app listens on plain HTTP.
#
# Build: docker build -f infra/Dockerfile -t tee-match .
# Run locally: docker run -p 9721:9721 -e CER_DEK=<hex> tee-match

# ── Stage 1: Build ─────────────────────────────────────────────
FROM rust:1.88-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /build

COPY tools/tee-match/Cargo.toml tools/tee-match/
COPY tools/tee-match/src/ tools/tee-match/src/
COPY tools/rust-circuits/Cargo.toml tools/rust-circuits/
COPY tools/rust-circuits/src/ tools/rust-circuits/src/

RUN cargo build --release --features secure \
    --manifest-path tools/tee-match/Cargo.toml

# ── Stage 2: Runtime ───────────────────────────────────────────
FROM debian:bookworm-slim AS release

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates && rm -rf /var/lib/apt/lists/*

RUN groupadd -r tee && useradd -r -g tee tee

COPY --from=builder /build/tools/tee-match/target/release/tee-match /usr/local/bin/tee-match

COPY circuits/keys/ /keys/

RUN chown -R tee:tee /keys

USER tee
EXPOSE 9720
EXPOSE 9721

HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
    CMD tee-match --help > /dev/null || exit 1

ENTRYPOINT ["tee-match", "--keys-dir", "/keys"]
CMD ["serve", "--addr", "0.0.0.0:9720", "--db", "/keys/tee-db", "--http-port", "9721"]
