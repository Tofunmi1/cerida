#!/usr/bin/env bash
set -euo pipefail

# Integration test: run the full E2E demo
#
# 1. Compile circuit
# 2. Build contract
# 3. Start TEE prover Docker container
# 4. Deploy contract to testnet
# 5. Run demo client

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

echo "=== TEE + ZK + Stellar E2E Test ==="
echo ""

# Step 1: Compile circuit
echo " [1/5] Compiling Noir circuit..."
(cd "$PROJECT_DIR/circuit" && nargo compile)

# Step 2: Build contract
echo " [2/5] Building Soroban contract..."
(cd "$PROJECT_DIR/contract" && cargo build --target wasm32-unknown-unknown --release)

# Step 3: Start TEE prover
echo " [3/5] Starting TEE prover Docker container..."
(cd "$PROJECT_DIR" && docker compose build && docker compose up -d)
sleep 3

# Health check
curl -sf http://localhost:3000/health > /dev/null || {
  echo " TEE prover failed to start"
  docker compose logs
  exit 1
}
echo " TEE prover is healthy"

# Step 4: Deploy contract
if [ -n "${STELLAR_SECRET_KEY:-}" ]; then
  echo " [4/5] Deploying contract..."
  CONTRACT_ID=$(bash "$PROJECT_DIR/scripts/deploy.sh" 2>&1 | tail -1)
  echo " Contract: $CONTRACT_ID"
fi

# Step 5: Run demo
echo " [5/5] Running demo client..."
(cd "$PROJECT_DIR/client" && npm install && npx tsx src/demo.ts)

echo ""
echo "=== Test complete ==="
