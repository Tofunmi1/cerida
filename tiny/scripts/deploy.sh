#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TINY_DIR="$(dirname "$SCRIPT_DIR")"
TARGET="$TINY_DIR/target/wasm32v1-none/release"
VK_JSON="$TINY_DIR/circuit-keys/verification_key.json"

echo "=== Build contracts ==="
(cd "$TINY_DIR" && VERIFIER_VK_JSON="$VK_JSON" cargo build --target wasm32v1-none --release -p tiny-verifier)
(cd "$TINY_DIR" && cargo build --target wasm32v1-none --release -p tiny-pool)

echo "=== Deploy verifier (VK embedded in WASM) ==="
VERIFIER_ID=$(stellar contract deploy --wasm "$TARGET/tiny_verifier.wasm" --source e2e --network testnet)

echo "=== Deploy pool ==="
POOL_ID=$(stellar contract deploy --wasm "$TARGET/tiny_pool.wasm" --source e2e --network testnet)

echo "=== Save config ==="
mkdir -p "$TINY_DIR/deployments/testnet"
cat > "$TINY_DIR/deployments/testnet/deployments.json" <<EOF
{
  "verifier": "$VERIFIER_ID",
  "pool": "$POOL_ID",
  "network": "testnet"
}
EOF

echo ""
echo "Done!"
echo "  Verifier: $VERIFIER_ID"
echo "  Pool:     $POOL_ID"
