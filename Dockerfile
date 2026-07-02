# syntax=docker/dockerfile:1.7
#
# GCP Confidential Space TEE image for tee-match.
# Builds the secure Rust server and ships a minimal runtime image.
#
# Build:
#   docker build -f Dockerfile -t tee-match-tee .
# Run:
#   docker run --rm -p 9721:9721 \
#     -e CER_DEK=<hex> \
#     -v $(pwd)/circuits/keys:/keys \
#     -v tee-db:/data \
#     tee-match-tee

FROM rust:1.85-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
	build-essential \
	ca-certificates \
	libssl-dev \
	pkg-config \
	&& rm -rf /var/lib/apt/lists/*

WORKDIR /workspace

COPY tools/tee-match/Cargo.toml tools/tee-match/Cargo.toml
COPY tools/rust-circuits/Cargo.toml tools/rust-circuits/Cargo.toml
COPY tools/tee-match/src/ tools/tee-match/src/
COPY tools/rust-circuits/src/ tools/rust-circuits/src/

RUN cargo build --release --features secure --manifest-path tools/tee-match/Cargo.toml

FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
	ca-certificates \
	&& rm -rf /var/lib/apt/lists/*

RUN groupadd -r tee && useradd -r -g tee -m -d /home/tee tee

WORKDIR /app

COPY --from=builder /workspace/tools/tee-match/target/release/tee-match /usr/local/bin/tee-match

RUN mkdir -p /keys /data && chown -R tee:tee /keys /data /app

USER tee

EXPOSE 9721

ENV RUST_LOG=info
ENV CER_DEK=

HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
	CMD tee-match --help > /dev/null || exit 1

ENTRYPOINT ["tee-match"]
CMD ["--keys-dir", "/keys", "ServeSecure", "--addr", "0.0.0.0:9721", "--db", "/data/tee-db"]