# syntax=docker/dockerfile:1
#
# TEE Match Server — GCP Confidential Space
#
# Runs the tee-match server with attestation, encryption, and ZK proving.
# TLS is terminated by GCP Load Balancer — app listens on plain HTTP.
#
# Build: docker build -f infra/Dockerfile -t tee-match .
# Run:   docker run -p 9721:9721 -e CER_DEK=<hex> -v $(pwd)/circuits/keys:/keys tee-match

# ── Stage 1: Build ─────────────────────────────────────────────
FROM rust:1.85-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /build

COPY Cargo.toml Cargo.lock ./
COPY tools/tee-match/ tools/tee-match/
COPY tools/rust-circuits/ tools/rust-circuits/

RUN cargo build --release --features secure \
    --manifest-path tools/tee-match/Cargo.toml

# ── Stage 2: Runtime ───────────────────────────────────────────
FROM debian:bookworm-slim AS release

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates && rm -rf /var/lib/apt/lists/*

RUN groupadd -r tee && useradd -r -g tee tee

COPY --from=builder /build/tools/tee-match/target/release/tee-match /usr/local/bin/tee-match

RUN mkdir -p /keys && chown -R tee:tee /keys

USER tee
EXPOSE 9721

ENV CER_DEK=""

HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
    CMD bash -c 'echo > /dev/tcp/localhost/9721' 2>/dev/null || exit 1

ENTRYPOINT ["tee-match"]
CMD ["ServeSecure", "--addr", "0.0.0.0:9721", "--db", "/keys/tee-db"]
