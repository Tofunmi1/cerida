# syntax=docker/dockerfile:1
#
# TEE Prover Server
# 
# In production, this image runs inside GCP Confidential Space
# (AMD SEV-SNP). The hardware encrypts all container memory, so
# private keys and decrypted inputs are inaccessible to the host.
#
# In dev mode, it runs as a regular Docker container with a
# simulated TEE (no hardware encryption, same software logic).
#
# Build stages:
#   1. deps — install npm dependencies (cached layer)
#   2. build — compile TypeScript
#   3. release — minimal runtime image

# ── Stage 1: Dependencies ──────────────────────────────────────────
FROM node:20-slim AS deps

WORKDIR /app
COPY package.json package-lock.json* ./
RUN npm ci --omit=dev

# ── Stage 2: Build ─────────────────────────────────────────────────
FROM node:20-slim AS build

WORKDIR /app
COPY package.json package-lock.json* ./
RUN npm ci
COPY tsconfig.json ./
COPY src/ ./src/
RUN npx tsc

# ── Stage 3: Release ───────────────────────────────────────────────
FROM node:20-slim AS release

WORKDIR /app

# Security: run as non-root
RUN groupadd -r tee && useradd -r -g tee tee

# Copy runtime dependencies and compiled output
COPY --from=deps /app/node_modules ./node_modules
COPY --from=build /app/dist ./dist

# The circuit JSON is mounted at /circuit at runtime
# (built separately from circuit/)

EXPOSE 3000

USER tee

ENV NODE_ENV=production
ENV CIRCUIT_PATH=/circuit/position_proof.json

HEALTHCHECK --interval=30s --timeout=5s --start-period=15s \
  CMD node -e "require('http').get('http://localhost:3000/health', r => {process.exit(r.statusCode===200?0:1)})" \
  || exit 1

CMD ["node", "dist/index.js"]
