# TEE + ZK + Stellar Example

A complete working example showing how to combine a **TEE** (Docker-simulated), **ZK proofs** (Noir/UltraHonk), and **Stellar testnet** (Soroban) for privacy-preserving perpetual trading.

## Architecture

```
User's Browser
  │
  │ 1. Fetch TEE pubkey
  │ 2. Encrypt trade inputs (NaCl Box)
  │ 3. POST /prove with ciphertext
  ▼
┌─────────────────────────────────┐
│    Docker Container ("TEE")      │
│  ┌─────────────────────────────┐ │
│  │  AMD SEV-SNP (simulated)    │ │
│  │                             │ │
│  │  - Keypair in enclave memory│ │
│  │  - Decrypts inputs inside   │ │
│  │  - Generates UltraHonk proof│ │
│  │  - Returns proof + outputs  │ │
│  └─────────────────────────────┘ │
└────────────────┬────────────────┘
                 │
                 │ 4. Submit proof + commitments to Stellar
                 ▼
┌─────────────────────────────────┐
│       Stellar Testnet            │
│  ┌─────────────────────────────┐ │
│  │  Soroban Verifier Contract  │ │
│  │                             │ │
│  │  open(commitment,           │ │
│  │      collateral)            │ │
│  │  close(commitment,          │ │
│  │        nullifier, proof)    │ │
│  │                             │ │
│  │  BN254 verification via     │ │
│  │  Protocol 26 host functions │ │
│  └─────────────────────────────┘ │
└─────────────────────────────────┘
```

### Why This Architecture

| Problem | Solution |
|---------|----------|
| Server operator sees private inputs | TEE encrypts memory — operator sees only ciphertext |
| Need to prove validity on-chain | ZK proof is cryptographically binding |
| Proof generation is expensive | Server (in TEE) does the heavy lifting |
| Must work with Stellar | Soroban contract verifies via Protocol 26 BN254 |

### Privacy Guarantees

| Layer | Sees | Cannot See |
|-------|------|------------|
| Docker host (simulated TEE operator) | Encrypted ciphertexts | Private inputs (amount, direction, etc.) |
| Real TEE operator (GCP SEV-SNP) | Encrypted ciphertexts | Private inputs (HW-encrypted memory) |
| Stellar validators | Commitment hash, nullifier, proof | Position details |
| User | Everything | Nothing (full privacy) |

## How It Works

### Prover Server (inside TEE)

```
On startup:
  └─ Generate NaCl X25519 keypair (private key never leaves enclave)

GET  /pubkey
  └─ Return base64 public key

GET  /attestation (real TEE)
  └─ Return GCP OIDC signed token (proves container identity)

POST /prove
  Body: { ephemeralPubkey, nonce, ciphertext } // NaCl Box
  1. Decrypt with enclave's private key
  2. Run Noir witness generator (fast, <100ms)
  3. Generate UltraHonk proof via bb.js (~5-15s)
  4. Return { commitment, nullifier, proof, publicInputs }
```

### Client Flow

```
1. Fetch pubkey from TEE server
2. (Optional) Verify attestation token
3. Encrypt trade inputs with pubkey (NaCl Box)
4. Send to POST /prove
5. Receive proof + commitment + nullifier
6. Sign Stellar tx: open(commitment, collateral)
7. Later: Sign Stellar tx: close(commitment, nullifier, proof)
```

### Soroban Contract

```
open(trader, commitment, collateral):
  - Store commitment → trader mapping
  - Store collateral amount

close(trader, commitment, nullifier, proof):
  - Check nullifier not spent
  - Verify UltraHonk proof via BN254 host functions
  - Return collateral
  - Mark nullifier as spent
```

## Prerequisites

```bash
# Stellar CLI (Soroban)
cargo install --locked stellar-cli

# Noir (circuit compiler)
curl -L https://raw.githubusercontent.com/noir-lang/noirup/main/install | bash
noirup

# Docker
# (Docker Desktop for Mac)
```

## Quick Start

```bash
# 1. Set up environment
cp .env.example .env
# Edit .env with your testnet keys

# 2. Compile the Noir circuit
cd circuit && nargo compile && cd ..

# 3. Build the Soroban contract
cd contract && cargo build --target wasm32-unknown-unknown --release && cd ..

# 4. Start the TEE prover server
docker compose up -d

# 5. Deploy contract to testnet
./scripts/deploy.sh

# 6. Run the demo
cd client && npx tsx src/demo.ts
```

## Production TEE Deployment

To deploy on real GCP Confidential Space (AMD SEV-SNP):

1. Build the Docker image and push to Artifact Registry
2. Create a Confidential Space instance with the image
3. The instance automatically gets:
   - Memory encrypted by AMD SEV-SNP hardware
   - An OIDC attestation endpoint at instance metadata
   - Hardware-bound key generation
4. Clients verify the OIDC token before sending encrypted inputs

See: [GCP Confidential Space docs](https://cloud.google.com/confidential-computing)

## Project Structure

```
├── README.md                  # This file
├── Makefile                   # Build/test commands
├── docker-compose.yml         # TEE prover + Stellar Quickstart
├── .env.example               # Environment template
├── circuit/                   # Noir ZK circuit
│   ├── Nargo.toml
│   └── src/main.nr           # Position commitment + nullifier
├── tee-prover/                # Prover server (runs in TEE)
│   ├── Dockerfile
│   ├── package.json
│   └── src/
│       ├── index.ts           # Fastify entry point
│       ├── enclave.ts         # TEE keypair + decryption
│       ├── prover.ts          # UltraHonk proof generation
│       ├── crypto.ts          # NaCl Box utilities
│       └── types.ts           # Shared types
├── client/                    # Demo client
│   ├── package.json
│   └── src/demo.ts           # Full E2E demo
├── contract/                  # Soroban verifier contract
│   ├── Cargo.toml
│   └── src/lib.rs            # open/close with BN254 verification
└── scripts/
    ├── deploy.sh             # Deploy to testnet
    └── test.sh               # Integration test
```

## Key Files

| File | What It Does |
|------|-------------|
| `circuit/src/main.nr` | Noir circuit: Poseidon2 commitment + nullifier |
| `tee-prover/src/enclave.ts` | TEE boundary: keypair generation, decryption, attestation |
| `tee-prover/src/prover.ts` | bb.js UltraHonk proof generation inside enclave |
| `tee-prover/src/crypto.ts` | NaCl Box encrypt/decrypt for input privacy |
| `contract/src/lib.rs` | Soroban contract: open/close with BN254 verification |
| `client/src/demo.ts` | Full E2E demo: open → prove → close |
| `docker-compose.yml` | TEE container + Stellar Quickstart for local dev |
