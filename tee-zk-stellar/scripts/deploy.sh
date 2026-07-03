#!/usr/bin/env bash
set -euo pipefail

# Deploy Soroban contract to Stellar testnet
#
# Prerequisites:
#   - stellar CLI installed
#   - .env file with STELLAR_SECRET_KEY
#
# Usage:
#   ./scripts/deploy.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

# Load .env
if [ -f "$PROJECT_DIR/.env" ]; then
  set -a
  source "$PROJECT_DIR/.env"
  set +a
fi

WASM_PATH="$PROJECT_DIR/contract/target/wasm32-unknown-unknown/release/stellar_verifier.wasm"

if [ ! -f "$WASM_PATH" ]; then
  echo "Building contract..."
  (cd "$PROJECT_DIR/contract" && cargo build --target wasm32-unknown-unknown --release)
fi

echo "Deploying to Stellar testnet..."
CONTRACT_ID=$(stellar contract deploy \
  --wasm "$WASM_PATH" \
  --source "$STELLAR_SECRET_KEY" \
  --network testnet \
  --rpc-url "${STELLAR_RPC_URL:-https://soroban-testnet.stellar.org}" \
  2>&1 | tail -1)

echo "Contract deployed: $CONTRACT_ID"
echo "Add to .env: CONTRACT_ID=$CONTRACT_ID"
