# CER Perp: Threat Model & Trust Assumptions

> **Last updated:** July 2026. Covers circuits v2 (6 circuits), TEE v2 (HTTP + TCP), and frontend v2 (trade → close flow).

## 1. System Overview

```
┌──────────────┐     ┌──────────────────┐     ┌─────────────────┐
│   Browser    │◄───►│  TEE Match       │◄───►│  Soroban        │
│   (React SPA)│     │  Server          │     │  Contracts      │
│   Freighter  │     │  (GCP SEV-SNP)   │     │  (Stellar)      │
└──────────────┘     └──────────────────┘     └─────────────────┘
                            │                          │
                            ▼                          ▼
                      ┌──────────┐            ┌─────────────────┐
                      │  Sled DB │            │  Orderbook       │
                      │  (encrypted│           │  PerpEngine      │
                      │   at rest)│            │  CollateralToken │
                      └──────────┘            └─────────────────┘
```

- **Browser** signs all on-chain transactions (Freighter wallet). Submits `place_order`, `deposit_note`, `open_position_from_note`, `cancel_position_to_note`.
- **TEE server** runs the CLOB matching engine + Groth16 prover inside GCP Confidential Space (AMD SEV-SNP). Generates proofs on demand; stores order secrets keyed by commitment. Submits `match_positions` and `cancel_order` (orderbook) via the `e2e` identity. Exposes HTTP (port 9721) for the frontend and TCP (port 9720) for keepers.
- **Keepers** (separate VM) publish oracle prices, run the 32-level market maker, and monitor for liquidations. They communicate with the TEE over TCP and submit on-chain independently.

### 1.1 Circuits (6 Groth16 proofs, shared CRS)

| Circuit | R1CS constraints | Private witnesses | Public inputs |
| --- | --- | --- | --- |
| `OrderCommitment` | ~3 400 | side, price, size, leverage, asset, is_market, nonce, secret | commitment, portfolio_key |
| `OrderMatch` | ~8 200 | 2× full commitment witnesses | cmt_a, cmt_b, match_price, match_size, nullifier_a, nullifier_b |
| `OrderCancel` | ~900 | commitment, secret | nullifier |
| `NoteSpend` | ~800 | amount, secret | note_commitment, nullifier |
| `ShieldedInsert` | ~1 200 | leaf, merkle path | root_before, root_after, leaf |
| `ShieldedWithdraw` | ~1 100 | leaf, merkle path, secret | root, nullifier |

All six circuits share the same CRS (`alpha`, `beta`, `gamma`, `delta`). Each circuit has a distinct verification key (different `ic` vector).

### 1.2 On-chain contracts

| Contract | Key functions |
| --- | --- |
| **Orderbook** | `place_order`, `cancel_order`, `status`, `is_spent`, `expire_order` |
| **PerpEngine** | `deposit_note`, `withdraw_note`, `open_position_from_note`, `cancel_position_to_note`, `close_position_to_note`, `match_positions`, `liquidate`, `set_asset_price`, `set_mark_price` |
| **CollateralToken** | `mint`, `transfer`, `trust` (Stellar Asset Contract) |

### 1.3 Roles

| Role | Who | Authority |
| --- | --- | --- |
| Admin | `e2e` identity | `initialize`, `set_asset_price`, `register_asset` |
| Keeper | Separate VM | `set_asset_price`, `update_funding`, `liquidate` |
| Liquidator | Anyone | `liquidate` (no auth; anyone can call) |
| Trader | Individual user (Freighter) | Signs `place_order`, `deposit_note`, `open_position_from_note`, `cancel_position_to_note` |
| TEE | `e2e` identity | Submits `match_positions`, `cancel_order` (orderbook), posts mark price |
| Insurance Funder | Anyone | `fund_insurance` |

---

## 2. Trust Model

### 2.1 TEE Match Server (centralized)

The off-chain CLOB engine is the **primary trust anchor**:

- **Honest but curious** — Server faithfully executes FIFO matching, generates correct ZK proofs, and submits matches on-chain via the `e2e` identity.
- **Single point of failure** — If the TEE goes down, the CLOB stops. On-chain positions remain safe: users can close via `cancel_position_to_note` signed with their own wallet (no TEE dependency for close).
- **No user private keys** — The TEE has its own signing key (`e2e` identity). User transactions (`place_order`, `deposit_note`, `open_position_from_note`, `cancel_position_to_note`) are signed by the user's Freighter wallet. The TEE only submits `match_positions` and `cancel_order` (orderbook cleanup).
- **Order secrets in sled DB** — The TEE stores `OrderSecrets` (side, price, size, leverage, asset, nonce, secret) keyed by commitment for proof generation. This is the full order preimage — a compromised TEE host could read all open orders.

**Compromise scenarios:**

| Attack | Feasibility | Impact | Mitigation |
| --- | --- | --- | --- |
| Order queue reordering | High (server controls queue) | Front-running, skipped orders | ZK proof of inclusion / on-chain order queue (not yet implemented) |
| Match withholding | High | Liveness failure — orders never matched | Users submit match proofs independently (not yet implemented) |
| Fake fills inserted | Low (ZK proof fails on-chain) | Transaction rejected | Groth16 verification prevents invalid matches |
| Secret leakage from sled DB | Medium (filesystem or memory access) | All open orders revealed | sled DB encryption at rest; SEV-SNP memory encryption protects at runtime |
| TEE submits malicious mark price | Medium | Skewed funding rates, incorrect liquidations | TWAP deviation check (50% band); mark price is a public hint, not authoritative for settlement |

### 2.2 Proving System (Groth16 + BN254)

**Shared CRS risk:** All six circuits reuse the same `alpha`, `beta`, `gamma`, `delta`. The `ic` (input commitment) arrays differ per circuit, so a proof for one circuit cannot be replayed against another. However, the shared `delta` means circuits share the same simulation trapdoor — a vulnerability in one circuit's constraint system could theoretically be exploited to forge proofs for another.

| Attack | Impact | Severity |
| --- | --- | --- |
| CRS toxic waste compromised | Universal forgery | Critical (handled: `rand::thread_rng` on each run, fresh setup per deploy) |
| Weak entropy in `setup_all` | Forgeable proofs | Critical (handled: `OsRng` via `rand::thread_rng`) |
| Constraint system bug | Per-circuit forgery | High |
| Proof malleability | Nullifier bypass | Medium |
| VK / PK mismatch | All proofs rejected | High — causes `UnreachableCodeReached` on-chain. Mitigated by regenerating setup + rebuilding WASM + redeploying contracts together |

### 2.3 Oracle

| Attack | Impact | Mitigation |
| --- | --- | --- |
| Admin sets extreme price | Arbitrary liquidations, funding manipulation | Admin key security; TWAP deviation check (50% band) |
| Stale oracle | Liquidations blocked | Heartbeat enforcement; keeper polls Pyth every 30s |
| `set_mark_price` no auth | Anyone can post mark price | TEE is the expected caller; TWAP bounds limit impact |
| Pyth feed outage | Mark/index prices freeze | Frontend shows last known price; contracts still verify proofs |

### 2.4 Stellar / Soroban

| Risk | Detail |
| --- | --- |
| Reorg finality | Stellar uses classic consensus — reorgs are rare but possible. A match tx in a reorged ledger could be reversed. |
| Contract immutability | Contracts cannot be upgraded. Bugs require migration to a new deployment + updating all client `.env` configs. |
| Single-operation simulation | `simulateTransaction` rejects multi-op transactions. The frontend submits `place_order`, `deposit_note`, and `open_position_from_note` as three separate signed txs. |
| ScVal encoding mismatches | `#[repr(u32)]` enums (e.g., `TimeInForce`) serialize as `U32`, not `Map`. Encoding as a unit-variant Map causes `WasmVm(InvalidAction)` on-chain. |
| Soroban RPC rate limits | Public RPC endpoints (soroban-testnet.stellar.org) may throttle. Frontend uses exponential backoff; keepers use Alchemy RPC. |

---

## 3. Attack Vectors

### 3.1 Front-running / MEV

| Vector | Feasibility | Mitigation |
| --- | --- | --- |
| TEE reorders pending orders | High | Server is trusted; all operations logged |
| Validator front-runs contract call | Low (Stellar fee model ≠ MEV chain) | Stellar's fee-bid ordering limits extractable value |
| Hint fields reveal intent | Medium | `revealed` bitmask per field; user chooses what to reveal (default: 15 = all fields) |
| Frontend leaks intent via RPC | Medium | Transaction submitted to public Soroban RPC before confirmation; mempool observers see `hint_price`, `hint_side`, `hint_size` |

### 3.2 Liquidation Attacks

| Vector | Feasibility | Mitigation |
| --- | --- | --- |
| Oracle manipulation to trigger false liquidation | Medium | TWAP deviation check; admin-controlled oracle |
| Liquidator griefing (tiny positions) | Low | Gas cost on Stellar is real |
| Insurance fund draining via repeated partial liq | Low | Each position partial-liquidated at most once |
| User cannot close before liquidation | Medium | Frontend Close button needs TEE `cancel-proof` endpoint (GCP deployment dependency) |

**Liquidation math:**

```
Maintenance margin = effective_collateral × 500 / 10_000 = 5% of notional
Tier 1 partial:  settlement > 0 AND settlement < mm
                 → half collateral closed, 1% reward
Tier 2 full:     settlement ≤ 0 OR partial already done
                 → full close, 1.5% reward + 0.5% to insurance fund
```

### 3.3 Double-Spend / Replay

| Vector | Mitigation |
| --- | --- |
| Nullifier replay | `DataKey::Nullifier` set on first use; checked before any action |
| Note commitment replay | `DataKey::Note` set on `deposit_note`; checked on `open_position_from_note` |
| Match re-submission | `match_id` incremented on-chain; duplicate revert |
| Cancel reuse | Nullifier enforced across both `cancel_order` and `cancel_position_to_note` |
| Commitment reuse across txs | Orderbook checks `DataKey::Order(commitment)` before inserting; panics on duplicate |

### 3.4 Privacy

| Concern | Detail |
| --- | --- |
| Hint bitmask exposed on-chain | `revealed` field is public; bit 0=price, 1=side, 2=size, 3=leverage |
| Note commitment linkable | Poseidon2 commitment prevents amount recovery, but same user's notes may be linkable via timing / gas payment |
| TEE server sees all secrets | Server has full `OrderSecrets` in sled DB for proof generation |
| Match nullifiers link counterparties | Both nullifiers are public; observer learns two orders matched |
| LocalStorage leaks position data | `positionsStore` and `cerida-notes` store commitment hashes, symbols, leverage, and secrets in browser localStorage — accessible to any script with DOM access |

### 3.5 Frontend-specific

| Vector | Impact | Mitigation |
| --- | --- | --- |
| Vite proxy relays all `/tee` requests to TEE | TEE sees plaintext order params over HTTP (no TLS before LB) | TLS terminated at GCP LB; internal traffic is plaintext within VPC |
| `proofJsonToScVal` uses `Buffer.from(hex, 'hex')` | Incorrect polyfill produces invalid proofs → on-chain rejection | vite-plugin-node-polyfills provides Buffer; verified correct via E2E tests |
| `buildBundleTx` (multi-op) unsupported | Soroban RPC rejects simulation; frontend falls back to 3 separate txs | N/A — bundling code exists but is unused |
| Stale wallet balance after trade | UI shows pre-trade USDC balance until manual refresh | `refreshBalance()` called after successful position open |

---

## 4. Economic Security

### 4.1 Funding Rate

```
premium = oracle_price - mark_price
rate    = premium × 100 / mark_price  (basis points)
payment = rate × delta / FUNDING_INTERVAL
```

- FUNDING_INTERVAL = 5760 ledgers (~8 hours)
- MAX_FUNDING_RATE_BPS = 75 (±0.75% per interval, ~2.25% daily)
- Mark price posted by TEE (off-chain CLOB mid-price)
- Oracle price set by admin (external feed)

**Risk:** If `set_mark_price` is called with a manipulated value, funding rates are skewed. No current mitigation beyond trusting the TEE.

### 4.2 Insurance Fund

- Funded via voluntary deposits (`fund_insurance`)
- 0.5% of fully liquidated position accrues to fund
- Covers liquidator rewards when settlement is negative
- `bad_debt` accumulates if fund is exhausted — no automatic recapitalization

### 4.3 Bad Debt

When `settlement + insurance_draw < base_reward`, the shortfall is recorded as `bad_debt`. There is no mechanism to socialize or recover bad debt — it is a monotonic accumulator owned by no one.

---

## 5. Constants Summary

| Constant | Value | Purpose |
| --- | --- | --- |
| `FUNDING_INTERVAL` | 5760 ledgers (~8h) | Funding rate settlement period |
| `TWAP_WINDOW` | 8 samples | Oracle TWAP window |
| `MAX_FUNDING_RATE_BPS` | 75 (±0.75%) | Per-interval funding rate cap |
| `MAX_PRICE_DEVIATION_BPS` | 5000 (50%) | Max oracle deviation from TWAP |
| `MAINTENANCE_MARGIN_BPS` | 500 (5%) | Liquidation threshold |
| `PARTIAL_REWARD_BPS` | 100 (1%) | Tier 1 liquidator reward |
| `FULL_REWARD_BPS` | 150 (1.5%) | Tier 2 liquidator reward |
| `INS_FUND_BPS` | 50 (0.5%) | Insurance fund contribution |

---
