# Private RWA Perpetual DEX — Architecture

## System Overview

A privacy-preserving perpetual futures DEX for tokenized RWAs on Stellar, using a **dual-ZK stack** (Noir/UltraHonk + RISC Zero) wrapped in a **TEE** (GCP Confidential Space).

## Architectural Philosophy

- **Zero-knowledge proofs** for on-chain validity (cryptographic guarantees)
- **Trusted Execution Environment** for off-chain privacy (protects inputs during proof generation)
- **Minimal trust**: client encrypts to TEE pubkey, TEE attests its code, Stellar verifies proofs
- **Modular verifiers**: separate UltraHonk and Groth16 verifier contracts, shared Perp Engine state

## ZK Stack Decision

| Layer | Technology | Purpose | Proof Type | Verifier |
|-------|-----------|---------|------------|----------|
| Privacy | Noir (NoirLang) | Position commitments, nullifiers, range proofs | UltraHonk | `indextree/ultrahonk_soroban_contract` |
| Execution | RISC Zero (zkVM) | Order matching, liquidation, P&L | Groth16 (BN254) | `NethermindEth/stellar-risc0-verifier` |

**Why both?** Noir is ideal for simple constraint systems (hashing, membership, ranges) with fast proving. RISC Zero handles complex program logic (matching engine, risk checks) by proving Rust execution. Together they cover the full DEX surface.

**Why not just one?** Pure Noir would require encoding a DEX matching engine in circuit constraints — painful. Pure RISC Zero would be overkill for simple hash commitments. Dual-ZK is the right split.

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              User (Browser)                                  │
│                                                                              │
│  ┌────────────────────┐  ┌───────────────┐  ┌───────────────────────────┐  │
│  │ Freighter Wallet    │  │ Trading UI    │  │ Encryptor (NaCl Box)      │  │
│  │ (sign Stellar txs)  │  │ (order form)  │  │ encrypts private inputs   │  │
│  └─────────┬──────────┘  └───────┬───────┘  │ with TEE public key       │  │
│            │                     │           └─────────────┬─────────────┘  │
└────────────┼─────────────────────┼─────────────────────────┼────────────────┘
             │                     │                         │
             ▼                     ▼                         ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                      TEE Prover Server (GCP SEV-SNP)                        │
│                                                                             │
│  ┌────────────────────────────────────────────────────────────────────────┐ │
│  │                          AMD SEV-SNP Enclave                           │ │
│  │                                                                        │ │
│  │  ┌──────────────────────────────────────────────────────────────────┐  │ │
│  │  │                    Prover Orchestrator                           │  │ │
│  │  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │  │ │
│  │  │  │ Decryptor    │  │ Attestation  │  │ Session Manager      │  │  │ │
│  │  │  │(NaCl Box)    │  │(OIDC token)  │  │(nonce, request cache)│  │  │ │
│  │  │  └──────┬───────┘  └──────────────┘  └──────────────────────┘  │  │ │
│  │  │         │                                                       │  │ │
│  │  │  ┌──────▼────────────────────────────────────────────────────┐  │  │ │
│  │  │  │              Request Router                                │  │  │ │
│  │  │  │  /prove/position  →  Noir/bb.js (UltraHonk)               │  │  │ │
│  │  │  │  /prove/order     →  RISC Zero (Groth16)                  │  │  │ │
│  │  │  │  /prove/liquidate →  RISC Zero (Groth16)                  │  │  │ │
│  │  │  │  /prove/balance   →  Noir/bb.js (UltraHonk)               │  │  │ │
│  │  │  └────────────────────────────────────────────────────────────┘  │  │ │
│  │  └──────────────────────────────────────────────────────────────────┘  │ │
│  └────────────────────────────────────────────────────────────────────────┘ │
└────────────────────────────────┬────────────────────────────────────────────┘
                                 │
                  ┌──────────────┴──────────────┐
                  │                              │
                  ▼                              ▼
┌─────────────────────────────┐  ┌─────────────────────────────┐
│   UltraHonk Proof           │  │  Groth16 Proof              │
│   (BN254 pairings)          │  │  (BN254 pairings)           │
└─────────────┬───────────────┘  └──────────────┬──────────────┘
              │                                  │
              ▼                                  ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                          Stellar / Soroban                                   │
│                                                                             │
│  ┌──────────────────────┐  ┌──────────────────────┐                        │
│  │  UltraHonk Verifier  │  │  RISC Zero Verifier  │                         │
│  │  (Poseidon2+BN254)   │  │  (Groth16/BN254)     │                        │
│  │  VK set at deploy    │  │  EmergencyStop       │                        │
│  └──────────┬───────────┘  └───────────┬──────────┘                        │
│             │                          │                                    │
│  ┌──────────▼──────────────────────────▼─────────────────────────────────┐ │
│  │                          Perp Engine                                   │ │
│  │                                                                         │ │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌────────────┐ │ │
│  │  │ Position Mgr │  │ Collateral   │  │ Oracle Feed  │  │ Settlement │ │ │
│  │  │ (ZK state)   │  │ Manager      │  │ (price)      │  │ Engine     │ │ │
│  │  └──────────────┘  └──────────────┘  └──────────────┘  └────────────┘ │ │
│  └─────────────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Data Flow: Full Trade Lifecycle

### Phase 1: Opening a Position

```
User                            TEE Server                     Stellar
 │                                │                             │
 │  1. Compute commitment         │                             │
 │     (Noir.js witness gen,      │                             │
 │      <100ms, local)            │                             │
 │     comm = Poseidon2(          │                             │
 │       amount, direction,       │                             │
 │       price, salt)             │                             │
 │                                │                             │
 │  2. Sign Stellar tx:           │                             │
 │     open(commitment)           │                             │
 │────────────────────────────────────────────────────────────▶│
 │                                │                             │
 │                                │                             │  3. Store
 │                                │                             │     commitment
 │                                │                             │
 │  4. Request TEE pubkey         │                             │
 │──────────────────────────────▶│                             │
 │                                │                             │
 │  5. Encrypt trade inputs       │                             │
 │     with TEE pubkey            │                             │
 │     (NaCl Box)                 │                             │
 │──────────────────────────────▶│                             │
 │                                │                             │
 │                                │  6. Decrypt inside enclave  │
 │                                │  7. Generate UltraHonk      │
 │                                │     proof:                   │
 │                                │     - commitment matches    │
 │                                │     - valid direction       │
 │                                │     - sufficient margin     │
 │                                │     (~5-15s)                │
 │                                │                             │
 │  8. Receive proof              │                             │
 │◀──────────────────────────────│                             │
 │                                │                             │
 │  (wait days/weeks before       │                             │
 │   closing)                     │                             │
```

### Phase 2: Closing a Position

```
User                            TEE Server                     Stellar
 │                                │                             │
 │  1. Generate nullifier         │                             │
 │     nullifier = Poseidon2(     │                             │
 │       commitment, secret)      │                             │
 │                                │                             │
 │  2. Sign Stellar tx:           │                             │
 │     close(commitment,          │                             │
 │           nullifier)           │                             │
 │────────────────────────────────────────────────────────────▶│
 │                                │                             │
 │                                │                             │  3. Verify proof
 │                                │                             │     via BN254
 │                                │                             │     host fns
 │                                │                             │
 │                                │                             │  4. Check nullifier
 │                                │                             │     not spent
 │                                │                             │
 │                                │                             │  5. Return collateral
 │                                │                             │  6. Mark nullifier
 │                                │                             │     as spent
```

### Phase 3: Full DEX Trade (RISC Zero — Future)

```
User                            TEE Server                     Stellar
 │                                │                             │
 │  1. Encrypt order details      │                             │
 │──────────────────────────────▶│                             │
 │                                │                             │
 │                                │  2. RISC Zero guest:        │
 │                                │     - Read order batch      │
 │                                │     - Match orders          │
 │                                │     - Compute new positions │
 │                                │     - Check liquidations    │
 │                                │     - Compute P&L           │
 │                                │     - Commit new state root │
 │                                │                             │
 │                                │  3. Generate Groth16 proof  │
 │                                │     (~1-5min)               │
 │                                │                             │
 │  4. Receive proof + results    │                             │
 │◀──────────────────────────────│                             │
 │                                │                             │
 │  5. Submit to Stellar:         │                             │
 │     - UltraHonk proof          │                             │
 │       (position validity)      │                             │
 │     - Groth16 proof            │                             │
 │       (execution correctness)  │                             │
 │────────────────────────────────────────────────────────────▶│
 │                                │                             │  6. Both verifiers
 │                                │                             │     pass
 │                                │                             │  7. State updated
```

## TEE Architecture

### GCP Confidential Space Deployment

```
┌──────────────────────────────────────────────────────────────┐
│                    GCP Confidential Space                      │
│                                                               │
│  workload.operator.google.com/confidential-space              │
│                                                               │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │  Container: prover-server (Fastify + bb.js RISC0)        │  │
│  │                                                          │  │
│  │  ┌────────────────────┐  ┌───────────────────────────┐  │  │
│  │  │  Private Key       │  │  Attestation Token        │  │  │
│  │  │  (X25519,          │  │  (OIDC, signed by AMD     │  │  │
│  │  │   generated on     │  │   SEV-SNP hardware,       │  │  │
│  │  │   first boot,      │  │   includes container      │  │  │
│  │  │   never leaves)    │  │   image hash)             │  │  │
│  │  └────────────────────┘  └───────────────────────────┘  │  │
│  │                                                          │  │
│  │  Memory: encrypted by AMD SEV-SNP hardware               │  │
│  │  - Private inputs decrypted only in CPU registers        │  │
│  │  - Hypervisor / GCP cannot read enclave memory           │  │
│  │  - Attestation proves exact container image running      │  │
│  └──────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────┘

Client Verification Flow:
  User ──▶ GET /attestation ──▶ Returns OIDC token + pubkey
  User ──▶ Verify token signature against AMD root CA
  User ──▶ Check token contains expected container image hash
  User ──▶ If valid: encrypt inputs with pubkey and send
```

### Local Dev Mode (No TEE)

```
  User ──▶ GET /pk ──▶ Returns X25519 pubkey (no attestation)
  User ──▶ Encrypt inputs with pubkey
  User ──▶ POST /prove ──▶ Server decrypts, generates proof
  Server ──▶ Returns proof (inputs visible to server operator)
```

## Smart Contract Design

### UltraHonk Verifier (`contracts/verifier/`)

Based on `indextree/ultrahonk_soroban_contract`.

```
contract: UltraHonkVerifier
  - init(vk: VerificationKey)    // VK set once at deploy, immutable
  - verify(proof: bytes, public_inputs: Field[]) → bool
    // Uses Protocol 26 BN254 host functions:
    //   g1_msm → g1_add → pairing_check

Poseidon2 commitment verified inside the UltraHonk proof itself.
The contract does NOT re-hash on-chain; the proof proves correct hashing.
```

### RISC Zero Groth16 Verifier (`contracts/risc0-verifier/`)

Based on `NethermindEth/stellar-risc0-verifier`.

```
contract: Groth16Verifier
  - init(verifying_key: bytes)
  - verify(proof: Groth16Proof, public_inputs: bytes) → bool
  
contract: VerifierRouter
  - verify(proof_type: u32, proof: bytes) → bool
    // Routes to correct verifier version
  - emergency_stop()              // Timelock-gated
  - upgrade_verifier(new: address) // Governance
```

### Perp Engine (`contracts/perp-engine/`)

```
contract: PerpEngine
  // --- Admin ---
  - init(ultrahonk_verifier: address, groth16_verifier: address)
  
  // --- UltraHonk-gated ---
  - open_position(trader: Address, commitment: Field, collateral: i128)
    // Stores commitment + collateral
    // No ZK proof needed at open (just commitment)
    
  - close_position(
      trader: Address,
      commitment: Field,
      nullifier: Field,
      proof: bytes,
      public_inputs: Field[]
    ) → i128
    // Verifies UltraHonk proof via verifier contract
    // Checks nullifier not spent
    // Returns collateral to trader
    // Marks nullifier as spent
    
  // --- Groth16-gated (future) ---
  - execute_batch(
      proof: Groth16Proof,
      public_inputs: bytes
    )
    // Verifies RISC Zero Groth16 proof
    // Batch settles multiple trades
    
  // --- Views ---
  - collateral_of(commitment: Field) → i128
  - is_spent(nullifier: Field) → bool
  - position_of(commitment: Field) → Position
```

## Circuit Design

### Position Circuit (`circuits/position/`) — Noir

```noir
// Private inputs
struct PositionInput {
    amount: Field,        // position size
    direction: Field,     // 0=LONG, 1=SHORT
    entry_price: Field,   // entry price in USD cents
    salt: Field,          // randomness for commitment
    secret: Field,        // secret for nullifier derivation
    collateral: Field,    // deposited collateral
}

// Public inputs
struct PositionOutput {
    commitment: Field,    // Poseidon2([amount, direction, entry_price, salt])
    nullifier: Field,     // Poseidon2([commitment, secret])
    min_collateral: Field, // minimum required (from oracle/or contract)
}

// Proves:
// 1. commitment == Poseidon2([amount, direction, entry_price, salt])
// 2. nullifier == Poseidon2([commitment, secret])
// 3. direction == 0 OR direction == 1
// 4. collateral >= min_collateral
```

This is our working `tiny/circuit/` circuit, ready to go.

### DEX Guest (`circuits/dex-guest/`) — RISC Zero (future)

```rust
// RISC Zero zkVM guest: runs inside the prover
// Proves correct DEX execution without revealing positions

fn main() {
    // Read encrypted state from host
    let ctx: ExecutionContext = env::read();
    
    // Match orders
    let result = execute_batch(
        &ctx.orders,
        &ctx.positions,
        &ctx.price_feed,
    );
    
    // Commit to new state root + P&L
    let journal = ExecutionJournal {
        prev_state_root: ctx.state_root,
        new_state_root: compute_root(&result.positions),
        matches_hash: poseidon2(&result.matches),
        pnl_commitment: poseidon2(&result.pnl),
        liquidation_hashes: poseidon2(&result.liquidations),
    };
    
    env::commit(&journal);
}
```

## Project Structure

```
├── contracts/
│   ├── verifier/               # UltraHonk verifier (fork indextree)
│   ├── risc0-verifier/         # Groth16 verifier (fork Nethermind)
│   └── perp-engine/            # DEX logic: positions, collateral, matching
│
├── circuits/
│   ├── position/               # Noir: commitment/nullifier/range (WORKING)
│   ├── trade/                  # Noir: trade execution proof (future)
│   ├── balance/                # Noir: balance proof (future)
│   ├── dex-guest/              # RISC Zero: DEX execution (future)
│   └── settlement/             # Noir: settlement proof (future)
│
├── app/
│   ├── prover-server/          # TEE prover (Fastify + bb.js + RISC0)
│   │   ├── src/
│   │   │   ├── index.ts        # Entry: Fastify server
│   │   │   ├── enclave.ts      # TEE boundary: decrypt, attest
│   │   │   ├── orchestrator.ts # /prove/* routing
│   │   │   ├── provers/
│   │   │   │   ├── ultrahonk.ts  # bb.js UltraHonk
│   │   │   │   └── risczero.ts   # RISC Zero prover
│   │   │   ├── crypto.ts       # NaCl Box encrypt/decrypt
│   │   │   ├── attestation.ts  # OIDC attestation verification
│   │   │   └── types.ts        # Shared types
│   │   ├── Dockerfile
│   │   ├── package.json
│   │   └── tsconfig.json
│   │
│   └── trader-ui/              # Frontend (React/Next.js)
│       ├── src/
│       │   ├── App.tsx         # Main trading interface
│       │   ├── wallet.ts       # Freighter integration
│       │   ├── encryptor.ts    # NaCl Box encryption
│       │   ├── prover-client.ts # /prove/* client
│       │   └── stellar.ts      # Stellar RPC client
│       └── package.json
│
├── tiny/                       # Prototype sandbox (WORKING)
│   ├── circuit/
│   ├── contracts/
│   ├── server/
│   └── client/
│
├── tests/
│   ├── integration/            # E2E tests
│   └── fixtures/               # Test vectors
│
├── scripts/
│   ├── deploy.sh               # Deploy to testnet
│   └── tee-start.sh            # Start TEE container
│
├── docs/
│
├── ARCHITECTURE.md             # This file
├── RESEARCH.md                  # Research notes
├── SESSION_SUMMARY.md          # Build log
└── README.md                   # Project overview
```

## Privacy Model

| Component | Sees | Does Not See |
|-----------|------|-------------|
| **Stellar chain** | commitment hash, nullifier, UltraHonk proof bytes | amount, direction, price, salt, secret, trader identity in proof |
| **TEE enclave** | decrypted trade inputs (inside SEV-SNP memory) | private key material (stays in TEE), user's Stellar secret key |
| **TEE operator (GCP)** | encrypted ciphertexts (NaCl Box), attestation logs | decrypted trade inputs, private keys |
| **User** | their own positions and trades | other users' positions and trades |
| **Regulator** | selective disclosure proofs (future) | non-disclosed trades |

## Security Properties

1. **Double-spend protection**: Nullifier prevents reusing the same commitment
2. **Collateral safety**: UltraHonk proof includes range check proving collateral >= minimum
3. **Replay prevention**: Each close uses a unique nullifier, committed on-chain
4. **Input privacy**: NaCl Box encryption to TEE pubkey, decrypted only inside SEV-SNP
5. **Code integrity**: TEE attestation proves exact container image hash
6. **Verifier immutability**: VK set once at deploy, cannot be changed
7. **Emergency stop (RISC Zero verifier)**: Timelock-gated pause for upgrades

## Hackathon Path (5 days)

```
Day 1-2: UltraHonk integration
  - Fork indextree/ultrahonk_soroban_contract → contracts/verifier/
  - Get `just e2e` running with our position circuit
  - Deploy UltraHonk verifier to testnet
  - Wire up tiny/server to generate real UltraHonk proofs

Day 3-4: Full E2E flow + TEE pattern
  - Prover server with encrypted request/response (NaCl Box)
  - Client-side: compute commitment, encrypt, call server, submit to Stellar
  - Close flow: verify UltraHonk proof on-chain, return collateral
  - Docker container for prover server (TEE-ready)

Day 5: Polish + demo
  - Record 2-3 min demo video
  - Push to GitHub
  - Write submission
  - Optional: deploy RISC Zero verifier stub
```

## Key References

- [UltraHonk Soroban Verifier](https://github.com/indextree/ultrahonk_soroban_contract)
- [RISC Zero Stellar Verifier](https://github.com/NethermindEth/stellar-risc0-verifier)
- [Stellar ZK Docs](https://developers.stellar.org/docs/build/apps/zk)
- [Stellar Privacy Docs](https://developers.stellar.org/docs/build/apps/privacy)
- [GCP Confidential Space](https://cloud.google.com/confidential-computing)
- [Noir Docs](https://noir-lang.org/docs/)
- [RISC Zero Docs](https://dev.risczero.com/)
- [Soroban SDK BN254](https://docs.rs/soroban-sdk/latest/soroban_sdk/_migrating/v25_bn254/index.html)
- [Soroban SDK Poseidon](https://docs.rs/soroban-sdk/latest/soroban_sdk/_migrating/v25_poseidon/index.html)
