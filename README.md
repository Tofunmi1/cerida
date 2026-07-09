<p align="center">
  <img src="cerida.png" width="96" alt="Cerida" />
</p>

<h1 align="center">Cerida</h1>

<p align="center"><em>Zero-knowledge perpetual futures for real-world assets, on Stellar.</em></p>

<p align="center">
  Trade BTC, XRP, XLM, SpaceX, Tesla, Oil, and Gold with full position privacy  collateral, size, direction, and P&L never touch the public chain in plaintext.
</p>

<p align="center">
  <a href="https://stellar.org">
    <img src="https://img.shields.io/badge/Stellar-Soroban-4e44ce?style=flat-square&logo=stellar" />
  </a>
  <img src="https://img.shields.io/badge/ZK-Groth16%20%2F%20BN254-7c3aed?style=flat-square" />
  <img src="https://img.shields.io/badge/TEE-GCP%20Confidential%20Space-1d4ed8?style=flat-square&logo=google-cloud" />
  <img src="https://img.shields.io/badge/Rust-2024-orange?style=flat-square&logo=rust" />
</p>

---


---

## Contents

- [What is Cerida?](#what-is-cerida)
- [How It Works](#how-it-works)
  - [Deposit](#1-deposit-shielded-notes)
  - [Place an Order](#2-place-an-order--ordercommitment-proof)
  - [Match](#3-match--ordermatch-proof)
  - [Open Position](#4-open-position--note-spend)
  - [Close / Withdraw](#5-close--withdraw)
- [Trading Features](#trading-features)
  - [Order types](#order-types)
  - [Take Profit / Stop Loss](#take-profit--stop-loss-tpsl)
  - [Leverage](#leverage)
  - [Isolated vs Cross](#isolated-vs-cross-margin)
  - [Liquidation](#liquidation)
- [ZK Circuits](#zk-circuits)
  - [R1CS primer](#what-r1cs-means-here)
  - [Circuit overview](#circuit-overview)
  - [Gadget library](#r1cs-gadget-library)
  - [OrderMatch walkthrough](#ordermatch-constraint-walkthrough)
  - [Cross-margin](#cross-margin-extension)
- [TEE: Matching Engine](#tee-the-matching-engine)
- [Live Markets](#live-markets)
- [Keeper Infrastructure](#keeper-infrastructure)
- [Hackathon Milestones](#hackathon-milestones)
- [Architecture](#architecture-at-a-glance)
- [Running Locally](#running-locally)
- [Contracts](#contracts)
- [Stack](#stack)

---

---

## How It Works

### 1. Deposit (shielded notes)

A user deposits USDC into the perp engine. The deposit creates a **shielded note**: a Poseidon2 commitment to `(amount, secret)` stored on-chain. Only the depositor knows the preimage.

```
note_commitment = Poseidon2(amount, secret, domain=8)
```

Funds in the shielded pool cannot be linked to any position or withdrawal without the secret.

---

### 2. Place an Order OrderCommitment proof

To place an order, the client sends encrypted order parameters to the TEE:

```
{ side, price, size, leverage, asset, is_market, nonce, secret }
```

The TEE generates a **Groth16 proof** inside the enclave (inputs never leave the SEV-SNP boundary) and returns a commitment:

```
h1 = Poseidon2(side,  price,     domain=1)
h2 = Poseidon2(h1,   size,      domain=2)
h3 = Poseidon2(h2,   leverage,  domain=3)
h4 = Poseidon2(h3,   asset,     domain=4)
h5 = Poseidon2(h4,   is_market, domain=5)
h6 = Poseidon2(h5,   nonce,     domain=6)
commitment = Poseidon2(h6, secret, domain=7)
```

The commitment is submitted on-chain alongside the proof. Stellar verifies the Groth16 proof via BN254 MSM and pairing host functions, then registers the order in the CLOB.

---

### 3. Match OrderMatch proof

When two orders cross in the TEE's CLOB engine, the enclave generates an **OrderMatch proof** that proves in zero-knowledge:

- Both order commitments are valid (the Poseidon2 hash chain holds for each)
- Orders are on opposite sides `side_a + side_b = 1`
- Same underlying asset `asset_a = asset_b`
- Not both market orders simultaneously
- Match price is within each limit's declared bounds
- Match size ≤ both order sizes
- Nullifiers are correctly derived: `nullifier = Poseidon2(commitment, match_price, match_size, domain=10)`

Public outputs: `(cmt_a, cmt_b, match_price, match_size, nullifier_a, nullifier_b)` no private order details exposed.

---

### 4. Open Position note spend

Opening a matched position requires two proofs in sequence:

1. **NoteSpend** proves the trader knows the secret for a shielded note worth ≥ required collateral
2. **OrderCommitment** binds the open position to the previously matched order

The note is spent (nullifier published on-chain), collateral is locked in the perp engine, and the position commitment is stored.

---

### 5. Close / Withdraw

Closing a position or withdrawing from the shielded pool requires a NoteSpend proof proving knowledge of the secret that committed the note. The nullifier is checked for uniqueness (replay protection), collateral is returned, and the nullifier is marked spent forever.

### 6. What Stellar Enforces On-Chain

The Stellar contract anchors every position and withdrawal through **ZK-proof verification, commitment anchoring, and nullifier integrity**:

- **ZK proof verification**: Stellar runs BN254 MSM and pairing host functions (Protocol 26) to verify every Groth16 proof before accepting any state transition. No proof, no change.
- **Commitment anchoring**: Position openings and note deposits are stored as Poseidon2 hashes — the contract enforces that the commitment chain is consistent and the deposited collateral matches the amount commitment.
- **Nullifier uniqueness**: Every note spend and position close publishes a nullifier. The contract rejects replays — each nullifier is marked spent irrevocably.
- **Authorized settlement**: The perp engine authorizes exactly one Stellar account (the TEE) to `settle_position` and `settle_partial`. The TEE is the only entity that knows the private inputs required to open a settlement commitment, making it the sole valid settlement proposer.
- **Collateral integrity**: Tokens are locked in the contract on position open and only released on a valid `settle_position` call signed by the authorized TEE account. The contract enforces that released amounts never exceed committed collateral.

No oracle prices or funding rates are verified on-chain — those computations run inside the TEE using live Pyth data, and the settled amounts are committed to the contract as opaque hashes.

---

## Trading Features

### Order types

The CLOB engine inside the TEE supports six order types:

| Type | Behaviour |
| --- | --- |
| **Market** | Fills immediately at best available price. No price constraint in the ZK proof. |
| **Limit** | Rests in the book until the market price crosses the declared limit. |
| **Stop-Limit** | Dormant until `mark_price` crosses the stop trigger, then activates as a limit order. |
| **Stop-Market** | Dormant until `mark_price` crosses the stop trigger, then activates as a market order. |
| **IOC** (Immediate-or-Cancel) | Fills whatever is available right now, cancels the rest. Never rests. |
| **FOK** (Fill-or-Kill) | Fills the entire size immediately or rejects entirely. |

Stop orders sit in a separate stop book. The engine scans and triggers them on every mark price update:

- **Bid stops** trigger when `mark_price ≥ stop_price` (used for stop-loss on shorts, buy-stop breakouts)
- **Ask stops** trigger when `mark_price ≤ stop_price` (used for stop-loss on longs, sell-stop breakouts)

When triggered, a Stop-Limit promotes to a resting limit order; a Stop-Market promotes to a market order and fills immediately.

### Take Profit / Stop Loss (TP/SL)

TP and SL are implemented as a pair of stop orders placed alongside the opening commitment:

- **Take Profit** Stop-Limit on the opposite side at your target price (`stop_type = stop_limit, stop_price = tp_price`)
- **Stop Loss** Stop-Market on the opposite side at your risk limit (`stop_type = stop_market, stop_price = sl_price`)

Both reference the same `position_commitment`. When either triggers and fills, the matching proof nullifies the position commitment so the other is automatically invalidated on settlement.

### Leverage

Up to **50× leverage** on crypto markets, **10–20× on RWA markets**, enforced on-chain in the perp engine:

```
notional = collateral × leverage
liquidation_price (long)  = entry × (1 − 0.92 / leverage)
liquidation_price (short) = entry × (1 + 0.92 / leverage)
```

The contract validates `leverage ≤ asset.max_leverage` and rejects the transaction if exceeded.

### Isolated vs Cross Margin

Configured via a single boolean witness `use_cross` in the `OrderCommitment` circuit.

- **Isolated** each position has its own collateral bucket. A loss cannot spill to other positions.
- **Cross** positions share a portfolio collateral pool, identified by a `portfolio_key = Poseidon2(secret, 0, domain=20)`. The key links positions without revealing the trader's identity or secret.

Margin mode is proved in zero-knowledge the on-chain state only sees the `portfolio_key` hash.

### Liquidation

Any address can call `liquidate(position_commitment)`. The contract:

1. Reads the position's `collateral`, `leverage`, `entry_price`, and current `mark_price` from oracle
2. Computes unrealised PnL: `pnl = (mark − entry) / entry × collateral × leverage × direction`
3. If `collateral + pnl < maintenance_margin`, the position is under-collateralised
4. Liquidator receives a fee; remaining collateral (if any) is returned to the pool
5. Position commitment is marked `Liquidated` nullifier cannot be spent again

The keeper liquidator scans a watchlist and submits liquidation calls automatically.

---

## ZK Circuits

All circuits are written in pure Rust using [arkworks](https://arkworks.rs) BN254 + Groth16. No Circom. No NoirLang. No transpilation.

Every constraint is a native **R1CS** constraint over the BN254 scalar field (`Fr`, 254 bits), authored directly with `ark-r1cs-std` and `ark-relations`. The circuits live in `tools/rust-circuits/src/circuits/` and are compiled into the TEE binary — the same code that defines the constraints also generates witnesses inside the enclave.

### What R1CS means here

An R1CS instance is a system of equations of the form:

```
(A · w) ∘ (B · w) = (C · w)
```

where `w` is the witness vector (private + public inputs) and `A`, `B`, `C` are sparse matrices. The Groth16 prover produces a constant-size proof `π = (A, B, C) ∈ G1×G2×G1` that satisfies the R1CS without revealing `w`.

In arkworks, each gadget call (hash round, range check, boolean gate) allocates one or more rows in these matrices. We author constraints using `ConstraintSystemRef<Fr>` — no intermediate AST, no DSL, no compilation step. The constraint system is built in-process, proven in-process, and the proof is the only thing that leaves the enclave.

The hash primitive throughout is **Poseidon2** (width-3 sponge, domain separator injected per round), implemented as a native R1CS gadget (`Poseidon2Var`) over `FpVar<Fr>`. This matches the Soroban Protocol 26 BN254 host function exactly, so on-chain verification reduces to a single host call.

### Circuit overview

| Circuit | R1CS constraints (approx) | Public inputs | Proves |
| --- | --- | --- | --- |
| `OrderCommitment` | ~3 400 | `commitment`, `portfolio_key` | 7-step Poseidon2 chain over `(side, price, size, leverage, asset, is_market, nonce, secret)` equals commitment; optional cross-margin `portfolio_key` |
| `NoteSpend` | ~800 | `note_commitment`, `nullifier` | `note_commitment = Poseidon2(amount, secret, 8)` and `nullifier = Poseidon2(note_commitment, secret, 9)` |
| `OrderMatch` | ~8 200 | `cmt_a`, `cmt_b`, `match_price`, `match_size`, `nullifier_a`, `nullifier_b` | Both commitment chains, opposite sides, same asset, conditional price bounds, size range checks, nullifier derivation |
| `OrderCancel` | ~900 | `commitment`, `cancel_nullifier` | Secret holder derives valid cancel nullifier: `cancel_nullifier = Poseidon2(commitment, secret, 11)` |
| `ShieldedInsert` | ~1 200 | `root_before`, `root_after`, `leaf` | Poseidon2 Merkle path from leaf to new root is correctly computed |
| `ShieldedWithdraw` | ~1 100 | `root`, `nullifier` | Membership proof (leaf ∈ tree) and correct nullifier derivation |

### R1CS gadget library

Each reusable primitive is a self-contained gadget that allocates a fixed number of constraints:

**`Poseidon2Var`** — the workhorse. Width-3 sponge with Matmul + S-box rounds. Each absorption + permutation costs ~120 R1CS rows. The commitment circuit runs 7 sequential absorptions (one per hash in the chain), totalling ~840 rows just for the hash.

**`enforce_cond_le`** — conditional ≤ check used for price bounds in `OrderMatch`. Decomposes both operands into 64-bit limbs (`FpVar::to_bits_le`), then gates the comparison with a boolean selector (`is_limit`). Costs ~390 constraints per check (two sides of the book = ~780 rows).

**`enforce_bool`** — asserts a witness is 0 or 1. Used for `side`, `is_market`, `use_cross`. Costs 1 constraint: `w × (w − 1) = 0`.

**`enforce_eq_if`** — conditional equality: `selector × (a − b) = 0`. Used to assert `asset_a = asset_b` only when both commitments are for the same market. 1 constraint.

**`range_check_u64`** — unpacks a field element into 64 bits and constrains each bit. Costs 64 constraints. Used on `match_size`, `collateral`, and `amount` to prevent overflow exploits.

### OrderMatch: constraint walkthrough

`OrderMatch` is the largest circuit (~8 200 constraints). It combines:

1. **Two full commitment chains** — re-derives `cmt_a` and `cmt_b` from private witnesses `(side_a, price_a, size_a, leverage_a, asset_a, is_market_a, nonce_a, secret_a)` and their `_b` counterparts. ~1 680 constraints.
2. **Side check** — `side_a + side_b = 1` (opposite sides). 1 constraint.
3. **Asset check** — `asset_a = asset_b`. 1 constraint.
4. **Market order exclusion** — `is_market_a × is_market_b = 0` (can't match two market orders). 1 constraint.
5. **Price bounds** — `enforce_cond_le` applied twice (once per side), gated by `is_limit`. ~780 constraints.
6. **Size range checks** — `range_check_u64` on `match_size`, `size_a`, `size_b` plus two ≤ comparisons. ~400 constraints.
7. **Two nullifier derivations** — `Poseidon2(cmt_x, match_price, match_size, domain=10)` for each side. ~240 constraints.
8. **Boolean enforcement** on `side_a/b`, `is_market_a/b`. 4 constraints.

Everything else is wiring (linear combinations) — zero additional multiplication gates.

### Cross-margin extension

`OrderCommitment` supports isolated and cross-margin accounts through a single boolean witness `use_cross`:

```
portfolio_key = use_cross × Poseidon2(secret, 0, domain=20)
```

Implemented as `enforce_eq_if(use_cross, portfolio_key, poseidon2_var)` — 1 gate plus the Poseidon2 cost (~120 rows). A zero `portfolio_key` signals isolated margin; non-zero groups positions into a shared pool. The secret is never revealed; the chain only sees the `portfolio_key` hash.

---

## TEE: The Matching Engine

`tee-match` is a Rust binary compiled into a Docker image and deployed to [GCP Confidential Space](https://cloud.google.com/confidential-computing) with AMD SEV-SNP attestation.

```
┌─────────────────────────────────────────────────────────────┐
│               GCP Confidential Space (SEV-SNP)              │
│                                                             │
│  ┌──────────────────────────────────────────────────────┐   │
│  │               tee-match (Rust binary)                │   │
│  │                                                      │   │
│  │   TCP :9720   encrypted JSON-lines (orders/proofs)  │   │
│  │   HTTP :9721  public REST (depth, mark price)       │   │
│  │                                                      │   │
│  │   • CLOB engine (price-time priority, RwLock + WAL)  │   │
│  │   • Groth16 prover (arkworks, in-process)            │   │
│  │   • Poseidon2 commitment hash (same circuit code)    │   │
│  │   • Stellar tx construction + submission             │   │
│  │   • KMS-backed signing key                           │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                             │
│  The host sees: encrypted memory, nothing else.             │
└─────────────────────────────────────────────────────────────┘
```

The enclave holds the Groth16 proving keys for all circuits. It decrypts order inputs, runs the CLOB, and generates proofs entirely inside the SEV-SNP boundary. The output is a proof blob and a Stellar transaction, which the client submits.

### Public HTTP endpoints (port 9721)

| Endpoint | Returns |
| --- | --- |
| `GET /get-market?asset=N` | 32-level bid/ask depth (prices in 7-decimal scale) |
| `GET /mark-price?asset=N` | Current oracle mark price |
| `POST /place-order` | Accepts order inputs, returns Groth16 proof + commitment |
| `POST /prove-note-cmt` | Returns Poseidon2 note commitment |
| `POST /prove-note-spend` | Returns NoteSpend Groth16 proof |

---

## Live Markets

Seven perpetual markets run on testnet, priced via [Pyth Hermes](https://hermes.pyth.network) real-time WebSocket feeds:

| Market      | Asset          | Category |
| ----------- | -------------- | -------- |
| BTC-PERP    | Bitcoin        | Crypto   |
| XRP-PERP    | XRP / XRPL     | Crypto   |
| XLM-PERP    | Stellar (XLM)  | Crypto   |
| SPACEX-PERP | SpaceX equity  | RWA      |
| TSLA-PERP   | Tesla          | RWA      |
| OIL-PERP    | WTI Crude Oil  | RWA      |
| GOLD-PERP   | Gold (XAU/USD) | RWA      |

Charts use **Pyth Benchmarks** for historical OHLCV candles and **Pyth Hermes WebSocket** for live 1-minute candle updates. The chart shows the **index price** the Pyth oracle feed which is the standard for oracle-priced perp venues (GMX, dYdX, Hyperliquid all do this).

---

## Keeper Infrastructure

A keeper binary handles all automated on-chain activity:

**Oracle keeper** fetches Pyth prices every 30 seconds, submits `set_asset_price` on-chain for each market via the `stellar` CLI + Soroban RPC.

**Market maker** 32 bid levels + 32 ask levels per market. Size grows geometrically: `base_size × 1.08^level`. Spread formula per category:

- Crypto markets: `(5 + 3×level)` bps
- RWA markets: `(10 + 5×level)` bps

Re-quotes when mid moves >0.5% or quotes are stale (>5 min TTL). Commitment proofs are pre-generated in a pool to avoid proof latency during re-balancing.

**Liquidator** scans a watchlist of matched positions, calls `liquidate()` on any commitment that falls below the maintenance margin threshold.

---

## Hackathon Milestones

**ZK circuits from scratch** Designed and implemented 6 Groth16 circuits in pure Rust with arkworks. No Circom. No NoirLang. R1CS constraints hand-written over BN254. Implemented Poseidon2 width-3 sponge natively in `ark-r1cs-std`. Proved on testnet: verifier accepts valid witnesses, panics on invalid ones.

**Soroban contracts + TEE wired end-to-end** Deployed `perp-engine`, `orderbook`, and `collateral-token` to Stellar testnet. Built `tee-match` a Rust CLOB engine + Groth16 prover inside a GCP Confidential Space Docker image. Wired the full flow: shielded deposit → commitment proof → order placement → TEE match → on-chain settlement. Real proofs. Real transactions. Verified on-chain.

**Keeper infrastructure** Integrated Pyth Network (Hermes REST + WebSocket) for all 7 markets. Built the 32-level algorithmic market maker with geometric size scaling and per-category spread. Oracle keeper submits live prices every 30 seconds. Liquidator watches matched positions automatically.

**Frontend on testnet** Full trading UI: Freighter wallet, live 32-level orderbook (TEE depth polling with seeded fallback), Pyth-powered candlestick charts (historical + live 1-min WebSocket ticks), funding rate, shielded deposit/withdraw, real transaction history from Stellar Horizon. End-to-end trade live on testnet with real ZK proofs.

---

## Architecture at a Glance

```
Browser (Freighter Wallet)
      │
      │  encrypted order inputs → TEE pubkey
      ▼
TEE: tee-match  (GCP Confidential Space / AMD SEV-SNP)
      │
      │  1. decrypt inside enclave
      │  2. CLOB matching engine
      │  3. Groth16 proof generation (arkworks BN254)
      │  4. Stellar transaction construction
      │
      ▼
Stellar Testnet  (Soroban Protocol 26)
      │
      │  verify_groth16(proof, public_inputs, vk)
      │    → BN254 multi-scalar multiplication
      │    → BN254 pairing check
      │
      ├── perp-engine       positions, collateral, oracle prices
      ├── orderbook         order commitments, nullifiers
      └── collateral-token  USDC (mint / burn / transfer)
```

---

## Running Locally

**Prerequisites**: Rust 1.78+, Node 20+, Docker, `stellar` CLI, Freighter browser wallet.

```bash
# Clone
git clone https://github.com/your-org/cer-perp && cd cer-perp

# Frontend
cd app && npm install && npm run dev

# TEE server (local  no attestation)
docker build -t tee-match tools/tee-match
docker run -p 9720:9720 -p 9721:9721 tee-match

# Keepers (needs deployed contract ID)
cd keepers && cargo run -- \
  --perp-id <PERP_ENGINE_CONTRACT_ID> \
  --tee-addr 127.0.0.1:9720 \
  --no-liquidator
```

**Prove a commitment without the TEE**:

```bash
cd tools/rust-circuits
cargo run -- prove-commitment \
  --side 0 --price 6100000000000 --size 100000 \
  --leverage 10 --asset 0 --nonce 42 --secret 999
```

---

## Contracts

Contract IDs are set via environment variables in `app/.env`:

```
VITE_PERP_ENGINE_ID=...
VITE_ORDERBOOK_ID=...
VITE_COLLATERAL_TOKEN_ID=...
VITE_TEE_HTTP=http://35.255.76.255:9721
```

---

## Stack

| Component       | Technology                                         |
| --------------- | -------------------------------------------------- |
| Smart contracts | Rust / Soroban SDK                                 |
| ZK proof system | arkworks (BN254, Groth16, ark-r1cs-std)            |
| Hash function   | Poseidon2 (width-3, BN254 scalar field)            |
| TEE             | GCP Confidential Space, AMD SEV-SNP                |
| Matching engine | Custom CLOB (Rust, price-time priority, WAL)       |
| Oracle          | Pyth Network (Hermes REST + WebSocket)             |
| Wallet          | Freighter (Stellar)                                |
| Frontend        | React, Remix, lightweight-charts, TailwindCSS      |
| Keepers         | Rust (oracle + 32-level market maker + liquidator) |
