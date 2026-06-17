# TEE + ZK + Stellar: Complete Architecture Explanation

## What This Is

A privacy-preserving perpetual futures DEX for tokenized real-world assets (RWAs) on Stellar. Traders can open, manage, and close positions without revealing their position size, direction, entry price, or strategy to anyone — not to the server operator, not to Stellar validators, not to other traders.

Three technologies work together to achieve this:

| Layer | Technology | Job |
|-------|-----------|------|
| **TEE** | GCP Confidential Space (AMD SEV-SNP) | Protects private inputs during proof generation |
| **ZK** | Noir/UltraHonk (Poseidon2 + BN254) | Proves validity on-chain without revealing data |
| **Stellar** | Soroban smart contract (Protocol 26) | Verifies ZK proofs, manages positions |

---

## The Problem: Privacy in RWA Trading

When you trade tokenized real-world assets — commodities, bonds, real estate — on a public blockchain, every detail is visible:

- Your position size
- Your entry price
- Your liquidation level
- Your P&L

For institutions and serious traders, this is unacceptable. Positions reveal strategy. Large orders get front-run. Competitors see your book.

**The goal**: a DEX where positions are mathematically proven to be valid without ever revealing the underlying data.

---

## Solution Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│ User's Browser                                                      │
│                                                                      │
│  1. Compute commitment locally (<100ms)                              │
│     comm = Poseidon2(amount, direction, price, salt)                │
│                                                                      │
│  2. Encrypt (amount, direction, price, salt, secret)                 │
│     with TEE's public key (NaCl Box)                                │
│                                                                      │
│  3. Send encrypted ciphertext to TEE server                         │
└────────────────────────┬────────────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────────────┐
│ TEE Prover Server (GCP Confidential Space / Docker)                  │
│                                                                      │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │              AMD SEV-SNP Encrypted Memory                     │    │
│  │                                                               │    │
│  │  - Private key generated INSIDE enclave, never leaves         │    │
│  │  - Decrypts ciphertext INSIDE enclave                         │    │
│  │  - Generates UltraHonk ZK proof INSIDE enclave                │    │
│  │  - Host operator sees only encrypted bytes in, proof out      │    │
│  │                                                               │    │
│  │  Steps:                                                       │    │
│  │    1. NaCl Box decrypt => recover trade inputs                │    │
│  │    2. Noir.execute() => compute commitment + nullifier        │    │
│  │    3. UltraHonkBackend.generateProof() => bind witnesses      │    │
│  │    4. Return { commitment, nullifier, proof, publicInputs }   │    │
│  │                                                               │    │
│  └─────────────────────────────────────────────────────────────┘    │
└────────────────────────┬────────────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────────────┐
│ Stellar Testnet (Soroban)                                           │
│                                                                      │
│  open(commitment, collateral):                                      │
│    - Store commitment => trader mapping                              │
│    - Lock collateral                                                 │
│                                                                      │
│  close(commitment, nullifier, proof, publicInputs):                  │
│    - Verify nullifier not spent (prevent double-close)              │
│    - Verify UltraHonk proof via Protocol 26 BN254 host functions    │
│      (g1_msm => g1_add => pairing_check)                             │
│    - Return collateral                                               │
│    - Mark nullifier as spent                                         │
│                                                                      │
│  The proof cryptographically demonstrates:                          │
│    "I know {amount, direction, price, salt, secret} such that:      │
│       Poseidon2(amount, direction, price, salt) = commitment  AND   │
│       Poseidon2(commitment, secret) = nullifier"                    │
│    without revealing any of the private inputs.                     │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Component Deep Dive

### 1. The Noir Circuit (`circuits/position/`)

The circuit is the heart of the ZK system. It defines the statement being proved in zero knowledge.

```noir
fn main(
    amount: Field,      // private -- never revealed
    direction: Field,   // private -- 0=LONG, 1=SHORT
    entry_price: Field, // private -- never revealed
    salt: Field,        // private -- random nonce for commitment
    secret: Field,      // private -- used to derive nullifier
) -> pub [Field; 2] {   // public -- visible on-chain
    assert(direction * (1 - direction) == 0, "direction must be 0 or 1");

    let commitment = Poseidon2::hash([amount, direction, entry_price, salt], 4);
    let nullifier = Poseidon2::hash([commitment, secret], 2);

    [commitment, nullifier]
}
```

**Why Poseidon2?**
- SHA-256 in a ZK circuit: ~25,000 constraints
- Poseidon2 in a ZK circuit: ~200 constraints
- **100x cheaper** to prove
- Stellar has native Poseidon2 host functions (Protocol 25+), ensuring on-chain and in-circuit hashing are identical

**Why commitment + nullifier?**
- **Commitment**: binds the trade details. Posted on-chain at `open()`. Anyone can see the hash, but not the contents.
- **Nullifier**: prevents double-spending. At `close()`, the nullifier is marked as spent. If anyone tries to close the same position again, it's rejected.
- **Binding**: the ZK proof ties the nullifier to the commitment cryptographically. You cannot create a valid nullifier for a commitment you didn't open.

### 2. The UltraHonk Prover (`examples/tee-zk-stellar/tee-prover/src/prover.ts`)

The prover runs inside the TEE. It uses two libraries:

1. **`@noir-lang/noir_js`**: executes the circuit's witness generator (pure arithmetic, ~100ms). This computes the commitment and nullifier values that satisfy the circuit constraints.

2. **`@aztec/bb.js`** (UltraHonkBackend): Barretenberg's WASM prover that generates the cryptographic proof (~5-15s). This proof binds the private witnesses to the public outputs using UltraHonk (a variant of the Plonk proving system).

**Why inside a TEE?**
- The proof generation needs the private inputs (amount, direction, price, salt, secret)
- If the prover runs on a regular server, the operator sees these inputs
- Inside a TEE (GCP Confidential Space with AMD SEV-SNP), memory is encrypted by hardware
- The operator sees: encrypted request in, proof bytes out -- never the plaintext inputs

### 3. The TEE Boundary (`examples/tee-zk-stellar/tee-prover/src/enclave.ts`)

```
On startup:
  1. Generate NaCl X25519 keypair
     - Private key lives in enclave memory only
     - In SEV-SNP, this memory is physically encrypted by the CPU
     - Not even the hypervisor (GCP) can read it

  2. Expose endpoints:
     GET /pubkey => base64 X25519 public key
     GET /attestation => GCP OIDC token (proves container identity)
     POST /prove => decrypt => prove => return
```

**NaCl Box encryption (X25519 + XSalsa20-Poly1305):**
- Client generates an ephemeral keypair per request
- Does X25519 DH with TEE's public key => shared secret
- Encrypts trade inputs with XSalsa20-Poly1305 (authenticated encryption)
- Server decrypts with its private key inside the enclave
- Authentication check ensures messages aren't tampered with

**Attestation (production):**
- GCP Confidential Space provides an OIDC token signed by Google
- Token proves: this container image hash is X, running on real SEV-SNP hardware
- Client verifies the token against Google's JWKS before sending encrypted inputs
- Ensures the TEE is running exactly the expected code

### 4. The Soroban Contract (`contracts/verifier/src/lib.rs`)

The contract uses the `ultrahonk_soroban_verifier` crate, which implements the full UltraHonk verification logic using Protocol 26 BN254 host functions.

**Verification flow:**

```
close(trader, commitment, nullifier, proof, publicInputs):
  |-> 1. Check nullifier not spent
  |-> 2. Check commitment belongs to trader
  |-> 3. Parse proof bytes (456 fields x 32 bytes = 14,592 bytes)
  |     - Sumcheck univariates
  |     - Gemini fold commitments
  |     - KZG quotient
  |     - Evaluations
  |
  |-> 4. Generate transcript challenges (Fiat-Shamir via Keccak256)
  |     - eta, beta, gamma, alpha_challenges
  |     - Sumcheck challenge
  |     - Gemini fold challenges
  |     - Shplonk z challenge
  |
  |-> 5. Verify Sumcheck (26 subrelations across 8 relation families)
  |     - UltraArithmetic (2 subrelations)
  |     - Permutation (2)
  |     - Lookup (2)
  |     - DeltaRangeConstraint (4)
  |     - Elliptic (4)
  |     - Auxiliary (7)
  |     - Poseidon2External (2)
  |     - Poseidon2Internal (3)
  |
  |-> 6. Verify Shplemini batch opening (Gemini + Shplonk + KZG)
  |     - Multi-scalar multiplication via g1_msm
  |     - Batch inversion of denominators
  |     - Single pairing check
  |
  |-> 7. If all pass: return collateral, mark nullifier spent
```

**Protocol 26 BN254 host functions used:**
- `g1_msm`: multi-scalar multiplication -- accumulates batch opening claims
- `g1_add`: point addition -- combines MSM results with constant terms
- `pairing_check`: verifies the final KZG pairing equation e(A, B) * e(C, D) = 1

### 5. The Verifier Crate (`crates/ultrahonk-soroban-verifier/`)

Vendored from [NethermindEth/rs-soroban-ultrahonk](https://github.com/NethermindEth/rs-soroban-ultrahonk), this is a pure-Rust implementation of the UltraHonk verifier for Soroban's BN254 host functions.

| Module | What it does |
|--------|-------------|
| `verifier.rs` | Top-level orchestration: Oink => Decider |
| `sumcheck.rs` | Verifies the multivariate sumcheck protocol |
| `relations.rs` | Evaluates 26 subrelations (8 families) |
| `shplemini.rs` | Gemini fold + Shplonk accumulator + KZG pairing check |
| `transcript.rs` | Fiat-Shamir transcript (Keccak256-based) |
| `types.rs` | VK, proof, wire indices, relation parameters |
| `field.rs` | BN254 scalar field arithmetic (wrapper over Bn254Fr) |
| `ec.rs` | BN254 curve operations (g1_msm, pairing_check) |
| `utils.rs` | Serialization/deserialization of VK and proof |
| `debug.rs` | Debug utilities, hex formatting |
| `hash.rs` | Keccak256 hash function |

**Why vendor instead of using git dependency?**
- Avoids version resolution issues between workspace dependency trees
- Ensures build reproducibility for the hackathon submission
- The crate is stable (matches Barretenberg v0.82.2 protocol)

---

## Data Flow: Complete Trade Lifecycle

### Opening a Position

```
1. User prepares trade: $10,000 LONG BTC at $50,000
                         amount=10_000_000, direction=0, price=50_000_000

2. User's browser computes commitment locally (<100ms):
   comm = Poseidon2([10_000_000, 0, 50_000_000, salt=0xdeadbeef])

3. User signs Stellar tx: open(commitment=comm, collateral=1_000_000)
   => Submitted to testnet
   => Contract stores: comm => {trader, 1_000_000}

4. Position is OPEN. No one knows the details.

   The chain sees:
     open(0x3a7f...deadbeef, 1_000_000)
   It does NOT see:
     10_000, LONG, $50,000
```

### Closing a Position

```
1. User wants to close. They need to prove knowledge of the trade details.

2. User's browser:
   a. Fetches TEE pubkey (GET /pubkey)
   b. Encrypts trade inputs (NaCl Box):
      { amount: "10000000", direction: "0", entry_price: "50000000",
        salt: "0xdeadbeef", secret: "0xc0ffee" }
   c. Sends encrypted box to POST /prove

3. TEE server (inside enclave):
   a. Decrypts with private key
   b. Runs Noir.execute() on inputs => returns commitment + nullifier values
   c. Runs UltraHonkBackend.generateProof() => cryptographic proof
   d. Returns { commitment, nullifier, proof, publicInputs }

4. User's browser:
   a. Signs Stellar tx: close(
        commitment=0x3a7f...deadbeef,
        nullifier=0x5c9e...c0ffee,
        proof=<14,592 bytes>,
        publicInputs=[0x3a7f...deadbeef, 0x5c9e...c0ffee]
      )

5. Stellar testnet (Soroban contract):
   a. Verifies nullifier not spent
   b. Verifies UltraHonk proof via BN254:
      - g1_msm: accumulate public inputs
      - g1_add: combine with VK constants
      - pairing_check: verify KZG opening
   c. Returns 1_000_000 collateral to trader
   d. Marks nullifier as spent

6. Position is CLOSED. Collateral returned.

   The chain sees:
     close(0x3a7f...deadbeef, 0x5c9e...c0ffee, <proof>, <inputs>)
     / Collateral returned: 1_000_000
   It does NOT see:
     10_000, LONG, $50,000, profit/loss
```

---

## Security Model

### What the ZK proof guarantees

| Property | How | Against |
|----------|-----|---------|
| **Soundness** | False proofs rejected by BN254 pairing check | Malicious prover creating fake positions |
| **Completeness** | Honest proofs always accepted | Legitimate trader can always close |
| **Zero knowledge** | Proof reveals nothing about private inputs | Chain observers, validators, MEV bots |
| **Binding** | Commitment pins trade details | Trader cannot claim different details at close |
| **Nullifier uniqueness** | Single-use nullifier prevents double-spend | Replay attacks |

### What the TEE guarantees

| Property | How | Against |
|----------|-----|---------|
| **Input privacy** | AMD SEV-SNP encrypted memory | Host operator, GCP, hypervisor |
| **Code integrity** | Attestation proves container image hash | Malicious code injection |
| **Key secrecy** | X25519 key generated inside enclave | Key extraction, memory dumps |

### Trust assumptions

| Assumption | Why it's reasonable |
|------------|-------------------|
| AMD SEV-SNP is secure | Hardware root of trust, publicly audited |
| GCP Confidential Space config is correct | Open-source attestation verification |
| Stellar Protocol 26 BN254 host fns are correct | Standardized, tested, used by many projects |
| bb.js UltraHonkBackend is correct | Barretenberg is widely used and audited |

---

## How to Run Everything

### Prerequisites

```bash
# Rust + Soroban CLI
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
cargo install --locked stellar-cli

# Noir
curl -L https://raw.githubusercontent.com/noir-lang/noirup/main/install | bash
noirup

# Node.js 20+
# Docker
```

### Step-by-step

```bash
# 1. Compile the Noir circuit
cd circuits/position
nargo compile
# Output: target/position_proof.json

# 2. Build the Soroban contract
cd contracts/verifier
cargo build --target wasm32-unknown-unknown --release
# Output: target/wasm32-unknown-unknown/release/stellar_verifier.wasm

# 3. Extract VK from compiled circuit for contract deploy
cd ../../scripts
node extract_vk.js
# Output: vk_bytes.hex -- the 1760-byte VK

# 4. Deploy contract to testnet with VK
stellar contract deploy \
  --wasm ../contracts/verifier/target/wasm32-unknown-unknown/release/stellar_verifier.wasm \
  --source <SECRET_KEY> \
  --network testnet
# Output: CONTRACT_ID

# 5. Start the TEE prover server (Docker)
cd ../examples/tee-zk-stellar
docker compose up -d
# Prover running at http://localhost:3000

# 6. Run the demo
cd client
npm install
npx tsx src/demo.ts
# Output: Full E2E trade lifecycle
```

---

## Project Map

```
cer-perp/
|-- circuits/position/              # Noir circuit: commitment + nullifier
|   |-- Nargo.toml
|   |-- src/main.nr                # Poseidon2 hash circuit
|
|-- contracts/verifier/             # Soroban UltraHonk verifier
|   |-- Cargo.toml
|   |-- src/lib.rs                 # open() / close() with BN254 VK
|
|-- crates/ultrahonk-soroban-verifier/  # Vendored verifier crate
|   |-- Cargo.toml
|   |-- src/
|       |-- verifier.rs            # Top-level verify()
|       |-- sumcheck.rs            # Sumcheck protocol
|       |-- relations.rs           # 26 subrelations
|       |-- shplemini.rs           # Batch opening + KZG
|       |-- transcript.rs          # Fiat-Shamir transcript
|       |-- ec.rs                  # BN254 curve ops
|       |-- field.rs               # Scalar field arithmetic
|       |-- types.rs               # VK, Proof, Wire indices
|       |-- utils.rs               # Parse bytes
|       |-- hash.rs                # Keccak256
|       |-- debug.rs               # Debug helpers
|
|-- examples/tee-zk-stellar/        # Runnable TEE + ZK example
|   |-- README.md                  # Example documentation
|   |-- docker-compose.yml         # Docker TEE setup
|   |-- tee-prover/               # TEE server (Fastify + bb.js)
|   |-- client/                   # Demo client
|   |-- circuit/                  # Copy of circuit for self-containment
|   |-- contract/                 # Contract reference
|   |-- scripts/                  # Deploy + test
|
|-- scripts/deploy.sh              # Deploy to testnet
|-- ARCHITECTURE.md                # Full architecture doc
|-- EXPLANATION.md                  # This file
|-- README.md                      # Project overview
```

---

## Why This Architecture

1. **No compromises on privacy**: every trade is mathematically private (ZK) and operationally private (TEE)
2. **Real ZK work on Stellar**: not a toy -- uses Protocol 26 BN254 host functions for actual cryptographic verification
3. **Production-quality components**: Barretenberg/bb.js for proving, Soroban for verification, GCP Confidential Space for TEE
4. **Modular design**: swap circuits, add DEX matching engine (RISC Zero), add oracles -- all without rebuilding the core
5. **Clear privacy model**: everyone knows exactly what they can and cannot see at every layer
6. **Demonstrable today**: runs on Stellar testnet, works in Docker, ready for production deployment

---

## References

- [UltraHonk Soroban Verifier](https://github.com/NethermindEth/rs-soroban-ultrahonk)
- [Stellar ZK Docs](https://developers.stellar.org/docs/build/apps/zk)
- [Stellar Privacy Docs](https://developers.stellar.org/docs/build/apps/privacy)
- [Noir Docs](https://noir-lang.org/docs/)
- [GCP Confidential Space](https://cloud.google.com/confidential-computing)
- [Barretenberg/bb.js](https://github.com/AztecProtocol/aztec-packages)
- [Stellar Protocol 26 BN254](https://docs.rs/soroban-sdk/latest/soroban_sdk/_migrating/v25_bn254/index.html)
- [Stellar Protocol 25 Poseidon](https://docs.rs/soroban-sdk/latest/soroban_sdk/_migrating/v25_poseidon/index.html)
