# cer-perp: TEE + SP1 zkRollup Architecture

## Overview

A privacy-preserving perpetual futures DEX on Stellar that combines:
- **Circom Groth16** for per-order privacy (commitment proofs, placement proofs)
- **SP1 (Succinct)** for provable batch settlement (matching engine correctness)
- **TEE (AMD SEV-SNP / GCP Confidential Space)** for order confidentiality and fair ordering
- **Stellar Protocol 25/26** for on-chain verification via native BN254 operations

The key property: the TEE guarantees no one can see orders or frontrun them; the SP1 proof guarantees the matching engine ran correctly and can be verified on-chain by anyone. Neither requires trusting the operator.

---

## Trust Model

```
Without TEE:  SP1 proves correctness but anyone can manipulate which orders enter the batch
Without SP1:  TEE ensures fairness but settlement requires trusting TEE hardware output
With both:    TEE = fair input collection + order confidentiality
              SP1 = cryptographic proof the matching engine ran correctly
              Stellar = verifies SP1 Groth16 proof, no trust required
```

An operator **cannot frontrun** (orders are sealed in TEE before matching). An operator **cannot falsify a fill** (SP1 proof is required and math doesn't lie). An auditor can verify settlement without running the TEE.

---

## Latency

```
Order placement → fill notification (TEE CLOB):   10–150ms  (network latency to TEE server)
Fill notification → on-chain settled (SP1 batch): 60–120s   (SP1 proving, local CPU)
Fill notification → on-chain settled (Succinct Network): 30–60s (remote cluster)
Stellar TX confirmation:                           5–10s     (ledger close)

Batch window: collect fills for 30s, prove in parallel, settle in 1 TX
```

---

## Full System Diagram

```
┌──────────────────────────────────────────────────────────────────┐
│                          TRADERS                                 │
│                                                                  │
│  1. Generate order commitment locally:                           │
│     cmt = Poseidon2(side, price, size, leverage,                 │
│                     asset, is_market, nonce, secret)             │
│                                                                  │
│  2. Submit to Stellar (public, immutable):                       │
│     orderbook.place_order(cmt, groth16_commitment_proof)         │
│     perp_engine.open_position(cmt, groth16_commitment_proof)     │
│     perp_engine.deposit(who, amount)                             │
│                                                                  │
│  3. Submit to TEE (encrypted):                                   │
│     { cmd: "place", side, price, size, leverage,                 │
│       nonce, secret, commitment_hex }                            │
└─────────────────────────┬────────────────────────────────────────┘
                          │ TCP (encrypted order params)
                          ▼
┌──────────────────────────────────────────────────────────────────┐
│             TEE  (GCP Confidential Space, AMD SEV-SNP)           │
│             Remote Attestation: proves THIS binary is running    │
│                                                                  │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │  tee-match server  (tools/tee-match/)                      │  │
│  │                                                            │  │
│  │  serve.rs:  JSON-TCP server on :9720                       │  │
│  │  engine.rs: OrderBook { bids/asks: BTreeMap<u64,VecDeque>  │  │
│  │             orders: HashMap<id,Order>                      │  │
│  │             stop_orders, fills: Vec<Fill> }                │  │
│  │  db.rs:     SecretStore (sled KV)                          │  │
│  │             key: commitment_hex                            │  │
│  │             val: OrderSecrets { side, price, size,         │  │
│  │                  leverage, asset, nonce, secret }          │  │
│  │                                                            │  │
│  │  Flow per incoming order:                                  │  │
│  │    1. Receive { side, price, size, ... secret }            │  │
│  │    2. Validate: Poseidon2(fields) == commitment_hex        │  │
│  │    3. Store secrets in SecretStore (stays in enclave)      │  │
│  │    4. Place Order in CLOB (price-time FIFO)                │  │
│  │    5. Return fill notification immediately to trader       │  │
│  └────────────────────┬───────────────────────────────────────┘  │
│                       │ batch of N fills every 30s               │
│  ┌────────────────────▼───────────────────────────────────────┐  │
│  │  SP1 Host Prover  (tools/sp1-prover/host/)                 │  │
│  │                                                            │  │
│  │  For each fill (taker_id, maker_id, price, size):          │  │
│  │    - lookup OrderSecrets for taker and maker from db       │  │
│  │  Build BatchInput {                                        │  │
│  │    orders: Vec<OrderSecrets>,    // PRIVATE                │  │
│  │    commitments: Vec<[u8;32]>,    // PUBLIC (on-chain)      │  │
│  │    claimed_fills: Vec<Fill>,     // claimed by CLOB        │  │
│  │  }                                                         │  │
│  │  Run SP1 prover → Groth16 proof + vkey_hash + batch_digest │  │
│  └────────────────────┬───────────────────────────────────────┘  │
│                       │                                          │
└───────────────────────┼──────────────────────────────────────────┘
                        │ (sp1_groth16_proof, fills[N], nullifiers[N])
                        ▼
┌──────────────────────────────────────────────────────────────────┐
│                      STELLAR BLOCKCHAIN                          │
│                                                                  │
│  perp_engine.batch_settle(                                       │
│    proof: Groth16Proof,          // SP1 Groth16 output           │
│    fills: Vec<BatchFill>,        // (cmt_a, cmt_b, price, size)  │
│    nullifiers: Vec<(BytesN<32>,BytesN<32>)>                      │
│  )                                                               │
│  → verifies SP1 Groth16 proof using embedded SP1 program VK     │
│  → settles N positions in ONE Stellar TX                         │
│                                                                  │
│  Existing entry points (UNCHANGED):                              │
│    orderbook.place_order()     ← Circom Groth16 (commitment)     │
│    orderbook.cancel_order()    ← Circom Groth16 (cancel)         │
│    perp_engine.open_position() ← Circom Groth16 (commitment)     │
│    perp_engine.deposit()                                         │
│    perp_engine.close_position() ← Circom Groth16 (cancel)       │
│    perp_engine.liquidate()                                       │
│    perp_engine.update_funding()                                  │
│    perp_engine.settle_match()  ← oracle price settlement        │
└──────────────────────────────────────────────────────────────────┘
```

---

## Two-Layer Proof System

### Layer 1 — Privacy (per order, on placement)

**What it proves:** "I know private order fields (side, price, size, leverage, asset, is_market, nonce, secret) that hash via Poseidon2 chain to this public commitment."

**Circuit:** `circuits/src/order_commitment.circom`
```
Private inputs: side, price, size, leverage, asset, is_market, nonce, secret
Public output:  commitment (single BN254 field element)
Hash chain:     h1 = Poseidon2(side, price, domain=1)
                h2 = Poseidon2(h1, size, domain=2)
                h3 = Poseidon2(h2, leverage, domain=3)
                h4 = Poseidon2(h3, asset, domain=4)
                h5 = Poseidon2(h4, is_market, domain=5)
                h6 = Poseidon2(h5, nonce, domain=6)
                commitment = Poseidon2(h6, secret, domain=7)
```

**Proof system:** Circom 2.2.2 → Groth16 on BN254
**Trusted setup:** 14-bit Powers of Tau
**Verified by:** `contracts/verifier-groth16` using Stellar BN254 host functions
**Called from:** `orderbook.place_order()`, `perp_engine.open_position()`

Also exists: `circuits/src/order_cancel.circom`
```
Private inputs: commitment, secret
Public output:  nullifier = Poseidon2(commitment, secret, domain=3)
Used by: orderbook.cancel_order(), perp_engine.cancel_position(), perp_engine.close_position()
```

### Layer 2 — Correctness (per batch, on settlement)

**What it proves:** "The matching engine ran on these private orders (whose commitments are already on-chain) and produced exactly these fills. The nullifiers are correctly derived."

**SP1 Guest Program:** `tools/sp1-prover/guest/src/main.rs`
```
Private inputs: Vec<OrderSecrets>  { side, price, size, leverage, asset, nonce, secret }
Public inputs:  Vec<[u8;32]>       commitment hashes (already verified on-chain at place_order)
                Vec<Fill>          claimed fills { cmt_a, cmt_b, match_price, match_size }

Guest logic:
  1. For each order i:
       computed_cmt = poseidon2_chain(orders[i])
       assert computed_cmt == public_commitments[i]
  2. Run matching engine (matching-core crate, same code as tee-match/engine.rs):
       let mut book = OrderBook::new()
       for order in orders: book.place(order)
       let actual_fills = book.fills
  3. Assert claimed_fills == actual_fills (price, size, ordering)
  4. Derive nullifiers:
       nullifier_a = Poseidon2(cmt_a, match_price, match_size, domain=10)
       nullifier_b = Poseidon2(cmt_b, match_price, match_size, domain=10)

Public outputs (committed via sp1_zkvm::io::commit):
  BatchResult { fills: Vec<Fill>, nullifiers: Vec<(Nullifier, Nullifier)> }
```

**Proof system:** SP1 (Succinct) → STARK internally → Groth16 output on BN254
**Verified by:** `contracts/verifier-groth16` (SAME contract, different embedded VK)
**Called from:** `perp_engine.batch_settle()`

**SP1 Groth16 public inputs format:**
```
public_inputs[0] = vkey_hash       // hash of SP1 program binary (identifies which program ran)
public_inputs[1] = committed_digest // sha256(abi_encode(BatchResult))
```
The `batch_settle` contract hashes the submitted fills+nullifiers and checks against `committed_digest`.

---

## Repository Structure (Target State)

```
cer-perp/
├── circuits/src/
│   ├── order_commitment.circom   ← Layer 1: commitment proof
│   ├── order_cancel.circom       ← Layer 1: cancel/nullifier proof
│   ├── order_match.circom        ← Legacy: per-pair match (kept, not used in batch flow)
│   ├── comparators.circom
│   └── poseidon2/                ← Poseidon2 templates (width 2, 3, 4)
│
├── contracts/
│   ├── types/                    ← Shared: Groth16Error, OrderMeta, PositionMeta,
│   │                                       MatchRecord, FundingState, OracleConfig
│   ├── orderbook/
│   │   ├── build.rs              ← Embeds VK_COMMIT_IC, VK_CANCEL_IC from env vars
│   │   └── src/lib.rs            ← place_order, cancel_order, status, is_spent, order_meta
│   ├── perp-engine/
│   │   ├── build.rs              ← Embeds VK_COMMIT_IC, VK_CANCEL_IC, VK_MATCH_IC,
│   │   │                            VK_SP1_BATCH_IC (NEW: SP1 program VK)
│   │   └── src/lib.rs            ← All existing entry points UNCHANGED +
│   │                                batch_settle() NEW
│   ├── verifier-groth16/         ← Generic Groth16 verifier (used by both layers)
│   └── verifier/                 ← UltraHonk verifier (existing, separate)
│
├── crates/
│   ├── matching-core/            ← NEW: Pure Rust matching logic extracted from engine.rs
│   │   └── src/lib.rs            ← OrderBook, Order, Fill, Side, OrderType
│   │                                no_std compatible, no I/O, no logging
│   │                                imported by: sp1-prover guest + tee-match
│   └── ultrahonk-soroban-verifier/ ← Existing (unchanged)
│
├── tools/
│   ├── sp1-prover/               ← NEW: SP1 workspace
│   │   ├── Cargo.toml            ← workspace with guest + host members
│   │   ├── guest/
│   │   │   ├── Cargo.toml        ← [patch] sp1-zkvm, depends on matching-core
│   │   │   └── src/main.rs       ← SP1 guest program (see above)
│   │   └── host/
│   │       ├── Cargo.toml        ← depends on sp1-sdk, matching-core
│   │       └── src/lib.rs        ← prove_batch(orders, commitments, fills) -> Groth16Proof
│   │
│   ├── tee-match/src/
│   │   ├── engine.rs             ← UNCHANGED (matching logic stays, imported by matching-core)
│   │   ├── db.rs                 ← UNCHANGED (SecretStore with OrderSecrets)
│   │   ├── serve.rs              ← MODIFIED: after batch window, call sp1-prover host,
│   │   │                            submit batch_settle TX to Stellar
│   │   ├── proof.rs              ← UNCHANGED (still generates Layer 1 Circom proofs)
│   │   ├── stellar.rs            ← MODIFIED: add submit_batch_settle()
│   │   └── main.rs               ← UNCHANGED
│   │
│   └── e2e/src/
│       ├── benchmark.rs          ← MODIFIED: use batch_settle flow in step 6
│       ├── stellar.rs            ← MODIFIED: add batch_settle invocation
│       └── ... (rest unchanged)
```

---

## New Components to Build

### 1. `crates/matching-core/src/lib.rs`

Extract from `tools/tee-match/src/engine.rs`. Must be:
- Pure Rust, no `std::io`, no `sled`, no `log` calls
- `no_std` compatible (SP1 guest runs in a restricted environment)
- Expose: `OrderBook`, `Order`, `Fill`, `Side`, `OrderType`
- Same matching logic: BTreeMap FIFO, Limit/Market/IOC/FOK/StopLimit/StopMarket

```toml
# crates/matching-core/Cargo.toml
[package]
name = "matching-core"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1", features = ["derive"], default-features = false }
```

### 2. `tools/sp1-prover/guest/src/main.rs`

```rust
#![no_main]
sp1_zkvm::entrypoint!(main);

use matching_core::{OrderBook, Order, Side, OrderType};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct OrderSecrets {
    pub side: u64,
    pub price: u64,
    pub size: u64,
    pub leverage: u64,
    pub asset: u64,
    pub is_market: bool,
    pub nonce: u64,
    pub secret: u64,
}

#[derive(Serialize, Deserialize, PartialEq)]
pub struct Fill {
    pub cmt_a: [u8; 32],
    pub cmt_b: [u8; 32],
    pub match_price: u64,
    pub match_size: u64,
    pub nullifier_a: [u8; 32],
    pub nullifier_b: [u8; 32],
}

#[derive(Serialize, Deserialize)]
pub struct BatchInput {
    pub orders: Vec<OrderSecrets>,          // private: order details
    pub commitments: Vec<[u8; 32]>,         // public: commitment hashes (on-chain)
    pub taker_indices: Vec<usize>,          // which order index is taker per fill
    pub maker_indices: Vec<usize>,          // which order index is maker per fill
    pub claimed_prices: Vec<u64>,
    pub claimed_sizes: Vec<u64>,
}

#[derive(Serialize, Deserialize)]
pub struct BatchResult {
    pub fills: Vec<Fill>,
}

pub fn poseidon2_chain(s: &OrderSecrets) -> [u8; 32] {
    // Poseidon2 chain matching order_commitment.circom exactly
    // domain separations: 1,2,3,4,5,6,7
    // inputs: side, price, size, leverage, asset, is_market, nonce, secret
    // Returns 32-byte field element (big-endian)
    todo!("implement Poseidon2 in Rust matching the circom circuit constants")
}

pub fn poseidon2_nullifier(cmt: &[u8; 32], price: u64, size: u64) -> [u8; 32] {
    // Poseidon2(cmt, match_price, match_size, domain=10)
    // matching order_match.circom nullifier derivation
    todo!("implement Poseidon2 nullifier")
}

pub fn main() {
    let input: BatchInput = sp1_zkvm::io::read::<BatchInput>();

    // Step 1: verify each order's commitment
    for (i, (secrets, &cmt)) in input.orders.iter().zip(input.commitments.iter()).enumerate() {
        let computed = poseidon2_chain(secrets);
        assert_eq!(computed, cmt, "commitment mismatch at index {i}");
    }

    // Step 2: run matching engine
    let mut book = OrderBook::new();
    for (i, secrets) in input.orders.iter().enumerate() {
        let side = if secrets.side == 0 { Side::Bid } else { Side::Ask };
        let order_type = if secrets.is_market {
            OrderType::Market
        } else {
            OrderType::Limit
        };
        let order = Order {
            id: format!("{i}"),
            side,
            price: secrets.price,
            size: secrets.size,
            remaining: secrets.size,
            timestamp_ns: i as u128,
            order_type,
        };
        let _ = book.place(order);
    }

    // Step 3: assert claimed fills match actual fills and derive nullifiers
    let actual_fills = &book.fills;
    assert_eq!(actual_fills.len(), input.taker_indices.len(), "fill count mismatch");

    let mut result_fills: Vec<Fill> = Vec::new();
    for (i, fill) in actual_fills.iter().enumerate() {
        assert_eq!(fill.price, input.claimed_prices[i]);
        assert_eq!(fill.size, input.claimed_sizes[i]);

        let taker_cmt = input.commitments[input.taker_indices[i]];
        let maker_cmt = input.commitments[input.maker_indices[i]];

        let (cmt_a, cmt_b) = if fill.taker_side == matching_core::Side::Bid {
            (taker_cmt, maker_cmt)
        } else {
            (maker_cmt, taker_cmt)
        };

        let nullifier_a = poseidon2_nullifier(&cmt_a, fill.price, fill.size);
        let nullifier_b = poseidon2_nullifier(&cmt_b, fill.price, fill.size);

        result_fills.push(Fill { cmt_a, cmt_b, match_price: fill.price, match_size: fill.size, nullifier_a, nullifier_b });
    }

    sp1_zkvm::io::commit(&BatchResult { fills: result_fills });
}
```

### 3. `tools/sp1-prover/host/src/lib.rs`

```rust
use sp1_sdk::{ProverClient, SP1Stdin, SP1ProofWithPublicValues};
use anyhow::Result;

// ELF binary of the guest program (embedded at compile time)
const GUEST_ELF: &[u8] = include_bytes!("../../guest/elf/riscv32im-succinct-zkvm-elf");

pub struct BatchProof {
    pub groth16_proof_bytes: Vec<u8>,  // raw Groth16 proof
    pub vkey_hash: [u8; 32],           // SP1 program VK hash
    pub committed_digest: [u8; 32],    // sha256(abi_encode(BatchResult))
}

pub fn prove_batch(input: &super::BatchInput) -> Result<BatchProof> {
    let client = ProverClient::new();
    let (pk, vk) = client.setup(GUEST_ELF);

    let mut stdin = SP1Stdin::new();
    stdin.write(input);

    // Use Groth16 output for Stellar compatibility
    let proof = client.prove(&pk, stdin).groth16().run()?;
    proof.verify(&vk)?;

    Ok(BatchProof {
        groth16_proof_bytes: proof.bytes(),
        vkey_hash: vk.bytes32(),
        committed_digest: proof.public_values.as_slice()[32..64].try_into().unwrap(),
    })
}

// For demo/testing: mock prover (instant, no real proof)
pub fn prove_batch_mock(input: &super::BatchInput) -> Result<BatchProof> {
    std::env::set_var("SP1_PROVER", "mock");
    prove_batch(input)
}
```

### 4. `contracts/perp-engine/src/lib.rs` — New Entry Point

```rust
// Add to perp-engine alongside existing entry points

#[derive(Clone)]
pub struct BatchFill {
    pub cmt_a: BytesN<32>,
    pub cmt_b: BytesN<32>,
    pub match_price: BytesN<32>,
    pub match_size: BytesN<32>,
    pub nullifier_a: BytesN<32>,
    pub nullifier_b: BytesN<32>,
}

pub fn batch_settle(
    env: Env,
    proof: Groth16Proof,          // SP1 Groth16 output
    vkey_hash: BytesN<32>,        // SP1 program identifier
    fills: Vec<BatchFill>,        // fills to settle
) -> u32 {
    // 1. Compute committed_digest = sha256(abi_encode(fills))
    //    to reconstruct what the SP1 guest committed
    let digest = compute_batch_digest(&env, &fills);

    // 2. Build public inputs for SP1 Groth16 verification
    //    SP1 always commits [vkey_hash, committed_digest] as the two public inputs
    let mut pi: Vec<Bn254Fr> = Vec::new(&env);
    pi.push_back(Bn254Fr::from_bytes(vkey_hash.clone()));
    pi.push_back(Bn254Fr::from_bytes(digest));

    // 3. Verify SP1 Groth16 proof using SP1 program's VK
    //    (embedded in build.rs as VK_SP1_BATCH_IC, different from Circom VKs)
    let sp1_vk = load_sp1_vk(&env);
    match verify_groth16(&env, &sp1_vk, &proof, &pi) {
        Ok(true) => {}
        _ => panic!("invalid SP1 batch proof"),
    }

    // 4. Assert vkey_hash matches the expected SP1 program
    //    (prevents using a different SP1 program with a valid proof)
    assert_eq!(vkey_hash, EXPECTED_SP1_VKEY_HASH);

    // 5. Settle all fills
    let count = fills.len() as u32;
    for fill in fills.iter() {
        settle_pair(
            &env,
            &fill.cmt_a,
            &fill.cmt_b,
            &fill.nullifier_a,
            &fill.nullifier_b,
            &fill.match_price,
            &fill.match_size,
        );
    }

    env.events().publish(
        (soroban_sdk::symbol_short!("batch"),),
        (count,),
    );

    count
}

fn settle_pair(
    env: &Env,
    cmt_a: &BytesN<32>,
    cmt_b: &BytesN<32>,
    nullifier_a: &BytesN<32>,
    nullifier_b: &BytesN<32>,
    match_price: &BytesN<32>,
    match_size: &BytesN<32>,
) {
    // Check nullifiers not spent
    let nk_a = DataKey::Nullifier(nullifier_a.clone());
    let nk_b = DataKey::Nullifier(nullifier_b.clone());
    if env.storage().persistent().has(&nk_a) { panic!("nullifier A already spent"); }
    if env.storage().persistent().has(&nk_b) { panic!("nullifier B already spent"); }

    // Load positions
    let pk_a = DataKey::Position(cmt_a.clone());
    let pk_b = DataKey::Position(cmt_b.clone());
    let mut meta_a: PositionMeta = env.storage().persistent().get(&pk_a)
        .unwrap_or_else(|| panic!("position A not found"));
    let mut meta_b: PositionMeta = env.storage().persistent().get(&pk_b)
        .unwrap_or_else(|| panic!("position B not found"));

    if meta_a.status != PositionStatus::Open || meta_b.status != PositionStatus::Open {
        panic!("both positions must be open");
    }

    let exec_price = field_to_u64(match_price);
    let exec_size = field_to_u64(match_size);
    let match_id = Self::next_match_id(env);
    let now = env.ledger().sequence() as u64;

    // Record match
    let record = MatchRecord {
        cmt_a: cmt_a.clone(),
        cmt_b: cmt_b.clone(),
        match_price: exec_price,
        match_size: exec_size,
        matched_at: now,
        closed: false,
    };
    env.storage().persistent().set(&DataKey::Match(match_id), &record);

    // Update positions
    meta_a.matched_price = exec_price;
    meta_a.status = PositionStatus::Matched;
    meta_a.match_id = match_id;
    meta_a.funding_at_open = Self::read_funding_cumulative(env);
    meta_b.matched_price = exec_price;
    meta_b.status = PositionStatus::Matched;
    meta_b.match_id = match_id;
    meta_b.funding_at_open = Self::read_funding_cumulative(env);

    env.storage().persistent().set(&pk_a, &meta_a);
    env.storage().persistent().set(&pk_b, &meta_b);
    env.storage().persistent().set(&nk_a, &true);
    env.storage().persistent().set(&nk_b, &true);
    for key in [&pk_a, &pk_b, &nk_a, &nk_b] {
        env.storage().persistent().extend_ttl(key, 17280, 17280);
    }
}
```

### 5. `contracts/perp-engine/build.rs` — Add SP1 VK

```rust
// Add alongside existing VK_COMMIT_IC, VK_CANCEL_IC, VK_MATCH_IC embedding:
// SP1 program VK is extracted after building the SP1 guest with --groth16
// and running: cargo prove build && cargo prove vk

let sp1_vk_path = std::env::var("VK_SP1_BATCH_JSON")
    .expect("VK_SP1_BATCH_JSON must be set to path of SP1 program VK JSON");
// ... embed same way as Circom VKs
```

### 6. Modified `tools/tee-match/src/serve.rs` — Batch Settlement Loop

```rust
// Add to the serve loop alongside existing match handling:

const BATCH_INTERVAL_SECS: u64 = 30;
const MIN_BATCH_SIZE: usize = 5;

// Background thread: every BATCH_INTERVAL_SECS, if enough fills accumulated:
std::thread::spawn(move || {
    loop {
        std::thread::sleep(Duration::from_secs(BATCH_INTERVAL_SECS));
        
        let pending_fills: Vec<PendingFill> = {
            let mut guard = pending_fills.lock().unwrap();
            if guard.len() < MIN_BATCH_SIZE { continue; }
            std::mem::take(&mut *guard)
        };

        // Build SP1 input from fills + secrets from db
        let batch_input = build_batch_input(&db, &pending_fills);
        
        // Prove (blocking, runs inside TEE)
        let proof = sp1_prover::prove_batch(&batch_input)
            .expect("SP1 proving failed");

        // Submit one Stellar TX for N fills
        stellar::submit_batch_settle(&proof, &pending_fills)
            .expect("batch_settle TX failed");
    }
});
```

---

## Build Pipeline

```
Step 1: Compile Circom circuits
  circom circuits/src/order_commitment.circom → order_commitment.r1cs + order_commitment_js/
  circom circuits/src/order_cancel.circom    → order_cancel.r1cs + order_cancel_js/
  (order_match.circom kept for legacy flow)

Step 2: Groth16 trusted setup (snarkjs)
  snarkjs groth16 setup order_commitment.r1cs ptau14.ptau order_commitment.zkey
  snarkjs zkey export verificationkey order_commitment.zkey circuits/keys/order_commitment_vk.json
  (same for order_cancel)

Step 3: Build SP1 guest
  cd tools/sp1-prover && cargo prove build
  # Produces: guest/elf/riscv32im-succinct-zkvm-elf

Step 4: Extract SP1 Groth16 VK
  cargo prove vk --elf guest/elf/riscv32im-succinct-zkvm-elf
  # Produces: circuits/keys/sp1_batch_vk.json

Step 5: Build Stellar contracts (embed all VKs)
  VK_COMMIT_JSON=circuits/keys/order_commitment_vk.json \
  VK_CANCEL_JSON=circuits/keys/order_cancel_vk.json \
  VK_SP1_BATCH_JSON=circuits/keys/sp1_batch_vk.json \
  cargo build --target wasm32v1-none --release -p orderbook -p perp-engine -p verifier-groth16

Step 6: Build tools
  cargo build --release --manifest-path tools/tee-match/Cargo.toml
  cargo build --release --manifest-path tools/e2e/Cargo.toml
```

---

## Poseidon2 in Rust (Critical Implementation Detail)

The SP1 guest must re-implement `order_commitment.circom`'s Poseidon2 chain in pure Rust. The circom constants come from `circuits/src/poseidon2/poseidon2_const.circom`. These are fixed BN254 field constants.

Key parameters:
- Field: BN254 scalar field (254-bit prime)
- Width: 2 (for commitment chain) and 3 (for nullifier)
- Rounds: 8 full + 22 partial = 30 total (Poseidon2 standard for BN254 t=2)
- Round constants: from `poseidon2_const.circom` (must match exactly)
- MDS matrix: Poseidon2 internal matrix (circom-specific)

The Rust implementation must produce **bit-identical** output to the circom circuit for the same inputs. Test by running both and comparing outputs before deploying.

Suggested approach: use `zkhash` crate (Rust Poseidon2 implementation) and verify it matches the circom output on known vectors.

---

## SP1 Groth16 on Stellar: Verification Compatibility

SP1 Groth16 output uses BN254 curve (same as Circom). Stellar Protocol 25/26 exposes:
- `bn254_pairing` — pairing check e(A,B) = e(C,D)
- `bn254_g1_msm` — multi-scalar multiplication on G1
- `bn254_g2_msm` — multi-scalar multiplication on G2

Groth16 verification requires exactly these operations. The existing `contracts/verifier-groth16` already implements this. To verify SP1 proofs, deploy the same contract with the SP1 program's VK embedded (instead of the Circom VK).

SP1 Groth16 proof format: standard Groth16 `(A: G1, B: G2, C: G1)` — identical format to Circom Groth16. The only difference is the VK and the 2 fixed public inputs (`vkey_hash`, `committed_digest`).

---

## Proving Speed

```
SP1 STARK generation:
  10 orders / 5 fills:   ~1–2M cycles  →  ~0.2–0.5s
  100 orders / 50 fills: ~10–20M cycles → ~2–5s
  1000 orders / 500 fills: ~100M cycles → ~20–40s

Groth16 wrapping (fixed cost regardless of batch size):
  Local CPU (8 cores):      60–120s
  Succinct Prover Network:  30–60s

Total per batch:
  Local, any batch size:   65–125s
  Succinct Network:        35–70s

Batching strategy:
  Collect fills for 30s → prove → settle ONE Stellar TX
  Effective throughput: 500 fills per 2-minute proving window
```

---

## Data Flow Summary

```
[Trader]
  1. Compute commitment locally (Poseidon2 hash of order fields)
  2. Submit place_order(cmt, circom_proof) to Stellar orderbook
  3. Submit open_position(cmt, circom_proof) + deposit() to Stellar perp-engine
  4. Submit { side, price, size, ..., secret } to TEE tee-match server

[TEE tee-match]
  5. Validate: recompute commitment from fields, check matches submitted cmt
  6. Store secrets in SecretStore (sled KV, stays in enclave)
  7. Place order in CLOB (BTreeMap, FIFO)
  8. If fill: notify trader immediately (~10ms)
  9. Every 30s: if >= 5 fills accumulated:
       a. Build BatchInput from fills + secrets
       b. Run SP1 host prover → Groth16 proof (~90s)
       c. Call perp_engine.batch_settle(proof, fills) via Stellar CLI

[Stellar perp-engine.batch_settle]
  10. Verify SP1 Groth16 proof via BN254 pairings
  11. For each fill: update PositionMeta, record MatchRecord, mark nullifiers spent
  12. Emit batch event
  13. Done — N positions settled in 1 TX
```

---

## What Is NOT Changed

- `circuits/src/order_commitment.circom` — unchanged
- `circuits/src/order_cancel.circom` — unchanged  
- `contracts/orderbook/src/lib.rs` — unchanged
- `contracts/verifier-groth16/` — unchanged
- `contracts/types/` — unchanged
- `tools/tee-match/src/engine.rs` — unchanged (logic copied to matching-core)
- `tools/tee-match/src/proof.rs` — unchanged (Circom Layer 1 proofs)
- `tools/tee-match/src/db.rs` — unchanged
- `tools/e2e/src/stellar.rs` — mostly unchanged
- All existing `perp-engine` entry points — unchanged, kept working

---

## Key Invariants

1. `batch_settle` MUST check `vkey_hash == EXPECTED_SP1_VKEY_HASH`. This prevents an attacker from substituting a different SP1 program with a valid Groth16 proof.

2. The SP1 guest's Poseidon2 implementation MUST be bit-identical to `order_commitment.circom`. Validate with test vectors before deploying.

3. Nullifier uniqueness: `batch_settle` checks each nullifier against persistent storage before settling, same as `match_positions`. A fill cannot be settled twice even across batches.

4. The TEE's attestation certificate (from GCP Confidential Space) covers the `tee-match` binary including the SP1 host prover code. This proves the batch inputs were collected fairly, complementing the SP1 correctness proof.

5. Batch ordering: if the SP1 prover fails (e.g. timeout), fills are re-queued into the next batch window. The CLOB state in the TEE is the authoritative source; on-chain settlement is eventually consistent.
