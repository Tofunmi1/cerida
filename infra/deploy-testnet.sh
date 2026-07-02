#!/usr/bin/env bash
# Testnet Deployment — 6-Market Setup
# -----------------------------------------------------------------------
# Deploys orderbook + perp-engine, registers 6 assets, sets oracle prices.
#
# Prerequisites:
#   stellar CLI installed + funded source account named "e2e"
#   make build-contracts (runs automatically)
# -----------------------------------------------------------------------
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
COLOR_GREEN="\033[32m"
COLOR_RESET="\033[0m"

log() { echo -e "${COLOR_GREEN}===${COLOR_RESET} $*"; }

# ── 1. Build WASM contracts ─────────────────────────────────
log "Building WASM contracts..."
cd "$ROOT" && make build-contracts

# ── 2. Deploy + register assets via e2e tool ─────────────────
log "Deploying contracts + registering 6 markets..."
export SOROBAN_RPC_URL="${SOROBAN_RPC_URL:-https://stellar-testnet.g.alchemy.com/v2/FqjaGAy9IMENhdv2i_3UUVDPZnNClYNq}"

cargo run --manifest-path tools/e2e/Cargo.toml -- \
  --wasm-dir target/wasm32v1-none/release \
  deploy

echo ""
log "Deployment complete."
echo ""
echo "Next steps:"
echo "  1. Start TEE server:"
echo "     cargo run --manifest-path tools/tee-match/Cargo.toml -- serve \\"
echo "       --addr 0.0.0.0:9720 --db /tmp/tee-testnet-db"
echo ""
echo "  2. Run benchmark:"
echo "     cargo run --manifest-path tools/e2e/Cargo.toml -- benchmark --mms 2 --traders 2"
echo ""
echo "  3. Frontend env vars:"
echo "     (copy contract IDs from deploy output above)"
