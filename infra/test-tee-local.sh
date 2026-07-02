#!/usr/bin/env bash
# Local TEE Test Harness
# Tests the secure (attestation + AEAD) endpoints without GCP hardware.
# Uses a mock DEK and plain HTTP.
# -----------------------------------------------------------------------
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

DEK_HEX="${CER_DEK:-$(openssl rand -hex 32)}"
echo "DEK: ${DEK_HEX:0:16}..."
export CER_DEK="$DEK_HEX"

# ── 1. Build tee-match with secure feature ──────────────────
echo "=== Building tee-match (secure feature) ==="
cargo build --release --features secure \
  --manifest-path "$ROOT/tools/tee-match/Cargo.toml"

# ── 2. Start server in background ───────────────────────────
rm -rf /tmp/tee-test-db
echo "=== Starting secure server on :9721 ==="
cargo run --release --features secure \
  --manifest-path "$ROOT/tools/tee-match/Cargo.toml" \
  -- ServeSecure --addr 127.0.0.1:9721 --db /tmp/tee-test-db &
SERVER_PID=$!
sleep 3

kill -0 $SERVER_PID 2>/dev/null || { echo "Server failed to start"; exit 1; }

# ── 3. Test /attestation endpoint ────────────────────────────
echo ""
echo "=== Test: GET /attestation ==="
curl -s http://127.0.0.1:9721/attestation?nonce=deadbeef | python3 -m json.tool 2>/dev/null || \
  echo "  (attestation stub — expected without GCP hardware)"

# ── 4. Test encrypted /init endpoint ─────────────────────────
echo ""
echo "=== Test: POST /init (encrypted) ==="

# Build the order init request (same format as TCP server)
PAYLOAD='{"cmd":"init","side":0,"price":100000,"size":5000000,"leverage":5,"asset":0,"nonce":42,"secret":12345}'

# Encrypt with AES-256-GCM using DEK
ENC=$(python3 -c "
import binascii, os, json, base64
from cryptography.hazmat.primitives.ciphers.aead import AESGCM

dek = binascii.unhexlify('$DEK_HEX')
nonce = os.urandom(12)
payload = json.dumps(json.loads('$PAYLOAD')).encode()
cipher = AESGCM(dek)
ct = cipher.encrypt(nonce, payload, None)
encrypted = nonce + ct
print(base64.b64encode(encrypted).decode())
" 2>/dev/null) || ENC=""

if [ -n "$ENC" ]; then
  curl -s -X POST http://127.0.0.1:9721/init \
    -H "Content-Type: application/json" \
    -d "{\"encrypted\":\"$ENC\"}" | python3 -m json.tool 2>/dev/null
else
  echo "  (encryption skipped — install 'cryptography' with: pip3 install cryptography)"
  echo "  Manual test:"
  echo "    curl -X POST http://127.0.0.1:9721/init -H 'Content-Type: application/json' \\"
  echo "      -d '{\"encrypted\":\"<base64(AES-256-GCM(dek, payload))>\"}'"
fi

# ── 5. Cleanup ───────────────────────────────────────────────
echo ""
echo "=== Cleanup ==="
kill $SERVER_PID 2>/dev/null || true
echo "Server stopped."
echo "Done."
