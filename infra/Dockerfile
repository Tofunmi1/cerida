# syntax=docker/dockerfile:1
#
# TEE Match Server — GCP Confidential Space
#
# Runs the tee-match server with attestation, encryption, and ZK proving.
# TLS is terminated by GCP Load Balancer — app listens on plain HTTP.
#
# Build: docker build -f infra/Dockerfile -t tee-match .
# Run locally: docker run -p 9721:9721 -e CER_DEK=<hex> tee-match
# Env vars:
#   STELLAR_SOURCE_SECRET=S...   — signing key for match/oracle submissions
#   STELLAR_RELAYER_SECRET=S...  — signing key for relay/open_position (separate account, pays fees)
#   SOROBAN_RPC_URL              — optional override (default: testnet)

# ── Stage 1: Build ─────────────────────────────────────────────
FROM rust:1.89-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev curl && rm -rf /var/lib/apt/lists/*

# Install stellar CLI (used by tee-match to submit on-chain transactions)
RUN curl -fsSL \
    https://github.com/stellar/stellar-cli/releases/download/v22.2.0/stellar-cli-22.2.0-x86_64-unknown-linux-gnu.tar.gz \
    | tar -xz -C /usr/local/bin/ stellar

WORKDIR /build

COPY tools/tee-match/Cargo.toml tools/tee-match/Cargo.toml
COPY tools/tee-match/Cargo.lock tools/tee-match/Cargo.lock
COPY tools/tee-match/src/ tools/tee-match/src/
COPY tools/rust-circuits/Cargo.toml tools/rust-circuits/
COPY tools/rust-circuits/src/ tools/rust-circuits/src/

RUN cargo build --release --features secure \
    --manifest-path tools/tee-match/Cargo.toml

# ── Stage 2: Runtime ───────────────────────────────────────────
FROM debian:bookworm-slim AS release

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl && rm -rf /var/lib/apt/lists/*

RUN groupadd -r tee && useradd -r -m -g tee tee

# Cloudflare Tunnel for HTTPS proxy (Vercel → TEE)
RUN curl -fsSL https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-amd64 \
    -o /usr/local/bin/cloudflared && chmod +x /usr/local/bin/cloudflared

COPY --from=builder /build/tools/tee-match/target/release/tee-match /usr/local/bin/tee-match
COPY --from=builder /usr/local/bin/stellar /usr/local/bin/stellar

COPY circuits/keys/ /keys/

RUN chown -R tee:tee /keys

USER tee
EXPOSE 9720
EXPOSE 9721

HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
    CMD tee-match --help > /dev/null || exit 1

ENTRYPOINT ["tee-match", "--keys-dir", "/keys"]
CMD ["serve", "--addr", "0.0.0.0:9720", "--db", "/keys/tee-db", "--http-port", "9721"]
