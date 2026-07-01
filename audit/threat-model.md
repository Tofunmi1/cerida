# CER Perp: Threat Model & Trust Assumptions

## 1. System Overview

```
┌──────────────┐     ┌──────────────────┐     ┌─────────────────┐
│   User CLI   │◄───►│  TEE Match       │◄───►│  Soroban        │
│   (e2e tool) │     │  Server          │     │  Contracts      │
│   Browser    │     │  (off-chain)     │     │  (Stellar)      │
└──────────────┘     └──────────────────┘     └─────────────────┘
                           │                          │
                           ▼                          ▼
                     ┌──────────┐            ┌─────────────────┐
                     │  Sled DB │            │  Orderbook       │
                     │  (local) │            │  PerpEngine      │
                     └──────────┘            │  OracleConfig    │
                                             └─────────────────┘
```

### 1.1 Circuits (4 Groth16 proofs, shared CRS)

| Circuit | Private Witnesses | Public Inputs | Domain |
|---|---|---|---|
| `OrderCommitment` | side, price, size, leverage, asset, is_market, nonce, secret | commitment = Poseidon2(...) | orderbook |
| `OrderMatch` | 2× (side, price, size, leverage, asset, is_market, nonce, secret) | cmt_a, cmt_b, match_price, match_size, nullifier_a, nullifier_b | perp-engine |
| `OrderCancel` | commitment, secret | nullifier | orderbook + perp-engine |
| `NoteSpend` | amount, secret | note_commitment, nullifier | perp-engine |

All four circuits share the same CRS (`alpha`, `beta`, `gamma`, `delta`). Each circuit has a distinct verification key (different `ic` vector length and content).

### 1.2 On-chain contracts

- **Orderbook** — `place_order`, `cancel_order`, `status`, `is_spent`
- **PerpEngine** — `deposit`/`deposit_note`, `withdraw_note`, `open_position`/`open_position_from_note`, `cancel_position`/`cancel_position_to_note`, `close_position`/`close_position_to_note`, `match_positions`, `liquidate`, `update_funding`, `set_price`, `set_mark_price`, `fund_insurance`, `set_tp_sl`, `trigger_tp`, `trigger_sl`

### 1.3 Roles

| Role | Who | Authority |
|---|---|---|
| Admin | `e2e` (Stellar identity) | `set_price`, `initialize` |
| Keeper | Anyone | `update_funding` (requires caller auth) |
| Liquidator | Anyone | `liquidate` (requires caller auth) |
| Order Owner | Individual user | `open_position`, `cancel_order`, `close_position` (require owner auth) |
| TEE Server | `e2e` identity | Posts mark price, submits matches, submits cancels |
| Insurance Funder | Anyone | `fund_insurance` (requires caller auth) |

---

## 2. Trust Model

### 2.1 TEE Match Server (centralized)

The off-chain CLOB engine is the **primary trust anchor**:

- **Honest but curious** — Server faithfully executes FIFO matching, generates correct ZK proofs, and submits them on-chain.
- **Single point of failure** — If the TEE goes down, the CLOB stops functioning. On-chain positions remain safe (users can close via `close_position` directly).
- **No private key material** — The server has no user secrets. It stores order secrets in sled DB for proof generation, accessible via `SecretStore`.

**Compromise scenarios:**
| Attack | Feasibility | Impact | Mitigation |
|---|---|---|---|
| Order queue reordering | High (server controls queue) | Front-running, skipped orders | ZK proof of inclusion / on-chain order queue (not yet implemented) |
| Match withholding | High | Liveness failure | Users submit match proofs independently (not yet implemented) |
| Fake fills inserted | Low (ZK proof fails on-chain) | Transaction rejected | Groth16 verification prevents invalid matches |
| Secret leakage from sled DB | Medium (filesystem access) | All orders revealed | sled DB encryption (not yet implemented) |

### 2.2 Proving System (Groth16 + BN254)

**Shared CRS risk:** All four circuits reuse the same `alpha`, `beta`, `gamma`, `delta`. The `ic` (input commitment) arrays differ per circuit, so a proof for one circuit cannot be replayed against another. However, the shared `delta` means the two circuits share the same simulation trapdoor — a vulnerability in one circuit's constraint system could theoretically be exploited to forge proofs for another.

| Attack | Impact | Severity |
|---|---|---|
| CRS toxic waste compromised | Universal forgery | Critical (handled: `rand::thread_rng` on each run) |
| Weak entropy in `setup_all` | Forgeable proofs | Critical (handled: `OsRng` via `rand::thread_rng`) |
| Constraint system bug | Per-circuit forgery | High |
| Proof malleability | Nullifier bypass | Medium |

### 2.3 Oracle

| Attack | Impact | Mitigation |
|---|---|---|
| Admin sets extreme price | Arbitrary liquidations, funding manipulation | Admin key security; TWAP deviation check (50% band) |
| Stale oracle | Liquidations blocked | Heartbeat enforcement |
| `set_mark_price` spam | Funding rate manipulation | No auth on setter (TEE is expected caller) |

### 2.4 Stellar / Soroban

| Risk | Detail |
|---|---|
| Reorg finality | Stellar uses classic consensus, so reorgs are rare but possible. A match tx in a forked ledger could be reversed. |
| Contract upgradeability | Contracts are immutable once deployed. Bugs cannot be patched without migration. |
| XDR processing false positives | Soroban CLI v22.2.0 reports "xdr processing error" on successful submits. Both `submit_match` and `submit_cancel` treat this as success. |

---

## 3. Attack Vectors

### 3.1 Front-running / MEV

| Vector | Feasibility | Mitigation |
|---|---|---|
| TEE reorders pending orders | High | Server is trusted; logged |
| Validator front-runs contract call | Low (Stellar fee model ≠ MEV chain) | Stellar's fee-bid ordering limits extractable value |
| Hint fields reveal intent | Medium | Bitmask per field; user chooses what to reveal |

### 3.2 Liquidation Attacks

| Vector | Feasibility | Mitigation |
|---|---|---|
| Oracle manipulation to trigger false liquidation | Medium | TWAP deviation check; admin-controlled oracle |
| Liquidator griefing (tiny positions) | Low | Gas cost on Stellar is real |
| Insurance fund draining via repeated partial liq | Low | Each position partial-liquidated at most once |

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
|---|---|
| Nullifier replay | `DataKey::Nullifier` set on first use (`true`); checked before any action |
| Note commitment replay | `DataKey::Note` set on `deposit_note`; checked on `open_position_from_note` |
| Match re-submission | `match_id` incremented on-chain; duplicate revert |
| Cancel reuse | Nullifier enforced across both `cancel_order` and `cancel_position` |

### 3.4 Privacy

| Concern | Detail |
|---|---|
| Hint bitmask exposed on-chain | `revealed` field is public; bit 0=price, 1=side, 2=size, 3=leverage |
| Note commitment linkable | Poseidon2 commitment prevents amount recovery, but same user's notes may be linkable via timing / gas payment |
| TEE server sees all secrets | Server has full `OrderSecrets` in sled DB for proof generation |
| Match nullifiers link counterparties | Both nullifiers are public; observer learns two orders matched |

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
|---|---|---|
| `FUNDING_INTERVAL` | 5760 ledgers (~8h) | Funding rate settlement period |
| `TWAP_WINDOW` | 8 samples | Oracle TWAP window |
| `MAX_FUNDING_RATE_BPS` | 75 (±0.75%) | Per-interval funding rate cap |
| `MAX_PRICE_DEVIATION_BPS` | 5000 (50%) | Max oracle deviation from TWAP |
| `MAINTENANCE_MARGIN_BPS` | 500 (5%) | Liquidation threshold |
| `PARTIAL_REWARD_BPS` | 100 (1%) | Tier 1 liquidator reward |
| `FULL_REWARD_BPS` | 150 (1.5%) | Tier 2 liquidator reward |
| `INS_FUND_BPS` | 50 (0.5%) | Insurance fund contribution |

---

## 6. Open Issues (not yet addressed)

1. **On-chain order queue** — TEE is the sole order book; no on-chain commitment queue exists, so users cannot independently verify their order's position in the FIFO queue.
2. **TEE key rotation** — The server uses a static Stellar identity; no mechanism to rotate or threshold-share the signing key.
3. **Decentralized proving** — Only the TEE can generate match proofs; there is no mechanism for users to submit matches directly.
4. **Bad debt recovery** — No mechanism to socialize or auction bad debt to recapitalize the insurance fund.
5. **Circuit audit** — The R1CS circuits have not been formally verified. Constraint correctness relies on code review.
6. **Shared CRS ceremony** — The CRS is generated in-process with `OsRng`; there is no multi-party ceremony or public transcript.
7. **Multi-asset collateral** — Currently single-asset (native XLM). Cross-margin and portfolio margin are future work.
