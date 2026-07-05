# Privacy Fix Plan

## Current Leaks

### 1. `PositionMeta` struct (on-chain storage) — `contracts/perp-engine/src/lib.rs:154`
Exposed fields:
- `collateral`, `entry_price`, `matched_price`, `side`, `leverage`
- `hint_size`, `tif`, `expiry_ledger`, `tp_price`, `sl_price`
- `funding_at_open`
- `liquidation_recipient_note`
- `portfolio_key`
- `asset_id`, `margin_mode`, `effective_collateral`

→ Anyone reading contract storage sees full position details.

### 2. `open_position_from_note` calldata (line 634)
All params are plain: `hint_price`, `hint_side`, `hint_leverage`, `hint_size`, `tif`, `expiry_ledger`, `tp_price`, `sl_price`, `liquidation_recipient_note`, `portfolio_key`, `asset_id`.

→ Visible on Stellar Expert and any archive node.

### 3. `close_position_to_note` calldata (line 861)
- `close_nullifier`, `recipient_note`, `oracle_price`, `settlement_amount`

→ Recipient note + settlement links the position to a specific payout.

### 4. `trigger_tp` / `trigger_sl` events (lines 559, 623)
- Emits `position_commitment, oracle_price, settlement_amount`

→ Links the position commitment to settlement value.

### 5. `liquidate` event (line 1068)
- Emits `position_commitment, oracle_price, settlement_amount, reward`

→ Reveals which position was underwater and by how much.

### 6. `settle_match` event (line 1603)
- Emits `match_id, oracle_price, settlement_a, settlement_b`

→ Settlement amounts reveal PnL of both matched positions.

### 7. `match_positions` calldata (line 229)
- `match_price` and `match_size` passed as field elements (but the order books `hint_price`/`hint_size` are already public)

→ Match execution price is public.

### 8. `place_order` calldata (orderbook contract)
- Side, price range hint, size, leverage are all visible.

---

## Phase 1: Strip Plain Fields from Calldata + Storage

### Step 1.1 — Collapse `PositionMeta` storage fields

Replace all plain position fields with a single `sealed_params: Bytes` blob and a `matched_price` (needed for match execution).

**Before:**
```rust
pub struct PositionMeta {
    pub collateral: i128,
    pub entry_price: u64,
    pub matched_price: u64,
    pub side: u64,
    pub leverage: u64,
    pub status: PositionStatus,
    pub created_at: u64,
    pub match_id: u64,
    pub funding_at_open: i128,
    pub hint_size: u64,
    pub tif: TimeInForce,
    pub expiry_ledger: u64,
    pub tp_price: u64,
    pub sl_price: u64,
    pub effective_collateral: i128,
    pub partial_liq_done: bool,
    pub liquidation_recipient_note: BytesN<32>,
    pub asset_id: BytesN<32>,
    pub margin_mode: MarginMode,
    pub portfolio_key: BytesN<32>,
}
```

**After:**
```rust
pub struct PositionMeta {
    pub collateral: i128,
    pub matched_price: u64,
    pub status: PositionStatus,
    pub created_at: u64,
    pub match_id: u64,
    pub funding_at_open: i128,
    pub effective_collateral: i128,
    pub partial_liq_done: bool,
    pub liquidation_recipient_note: BytesN<32>,
    pub asset_id: BytesN<32>,
    pub margin_mode: MarginMode,
    pub portfolio_key: BytesN<32>,
    pub sealed_params: Bytes,     // AES-256-GCM encrypted blob — only TEE decrypts
}
```

Removed: `entry_price`, `side`, `leverage`, `hint_size`, `tif`, `expiry_ledger`, `tp_price`, `sl_price`.

### Step 1.2 — Change `open_position_from_note` signature

**Before:** 17 params, all plain.
```rust
pub fn open_position_from_note(
    env: Env,
    note_commitment: BytesN<32>,
    note_nullifier: BytesN<32>,
    position_commitment: BytesN<32>,
    hint_price: u64,
    hint_side: u64,
    hint_leverage: u64,
    hint_size: u64,
    tif: TimeInForce,
    expiry_ledger: u64,
    tp_price: u64,
    sl_price: u64,
    liquidation_recipient_note: BytesN<32>,
    portfolio_key: BytesN<32>,
    asset_id: BytesN<32>,
    note_proof: Groth16Proof,
    commit_proof: Groth16Proof,
)
```

**After:** 8 params, sensitive fields replaced by `sealed_params`.
```rust
pub fn open_position_from_note(
    env: Env,
    note_commitment: BytesN<32>,
    note_nullifier: BytesN<32>,
    position_commitment: BytesN<32>,
    sealed_params: Bytes,          // ← TEE-encrypted blob
    liquidation_recipient_note: BytesN<32>,
    portfolio_key: BytesN<32>,
    asset_id: BytesN<32>,
    note_proof: Groth16Proof,
    commit_proof: Groth16Proof,
)
```

Validation logic that reads plain fields (side check, leverage check, TP/SL cross-check, TIF validation) must move into the TEE. The contract only validates what it needs (e.g., asset is registered, note exists, proof verifies).

### Step 1.3 — Change `close_position_to_note` signature

**Before:** takes `oracle_price` and `settlement_amount` from TEE (good — already partially done).

Maintain the TEE-gated approach. The settlement note credit and nullifier spending stay on-chain, but the PnL computation moves off-chain to the TEE.

*(Likely already correct in current `x` branch.)*

### Step 1.4 — Change `trigger_tp` / `trigger_sl` signatures

**Before:** takes `oracle_price` and `settlement_amount` from TEE (good).

The contract should NOT check `tp_price` / `sl_price` thresholds — that logic moves to the TEE. The contract just checks position status, verifies TEE auth, and credits the note.

Remove on-chain condition check:
```rust
if tp_price == 0 { panic!("no TP price set"); }
if (is_long && oracle_price < tp_price) || ... { panic!("TP not triggered"); }
```

### Step 1.5 — Change `liquidate` signature

**Before:** takes `oracle_price` and `settlement_amount` from TEE (good).

Remove on-chain solvency check:
```rust
// Remove: compute_settlement_with_funding + health check
// Remove: cross-margin portfolio health check
// All of this moves to TEE
```

The contract just verifies TEE auth, marks position closed, and credits the reward + settlement note.

### Step 1.6 — Change `settle_match` signature

**Before:** takes `oracle_price`, `settlement_a`, `settlement_b` from TEE (good).

Remove on-chain settlement computation:
```rust
// Remove: compute_settlement_with_funding calls
// Remove: funding delta computation
// TEE provides final settlement amounts
```

### Step 1.7 — Obfuscate events

| Event | Current | Proposed |
|-------|---------|----------|
| `match_positions` | `(match_id, cmt_a, cmt_b, price, size)` | `(match_id)` only |
| `settle_match` | `(match_id, oracle_price, settlement_a, settlement_b)` | Remove entirely or keep only `(match_id)` |
| `trigger_tp` | `(position_cmt, oracle_price, settlement)` | Remove or emit only `(commitment)` |
| `trigger_sl` | Same as TP | Same |
| `liquidate` | `(position_cmt, oracle_price, settlement, reward)` | Remove or emit only `(commitment)` |
| `open_position_from_note` | `(note_cmt, note_null, pos_cmt, collateral, created_at)` | `(position_commitment, collateral)` — strip note linkage |
| `close_position_to_note` | `(pos_cmt, close_null, recipient, settlement, oracle)` | `(position_commitment)` only |

---

## Phase 2: TEE-Side Changes

### Step 2.1 — Decrypt `sealed_params` in TEE before contracts read them

The TEE already has `seal_position_params()` in `tools/tee-match/src/stellar.rs`. Add a matching `unseal_position_params()` that decrypts the blob and returns `(side, entry_price, leverage, size, tp_price, sl_price, tif, expiry_ledger)`.

For `liquidate`, `trigger_tp`, `trigger_sl`:
1. TEE fetches the position from chain (includes `sealed_params` blob)
2. TEE decrypts `sealed_params` inside the enclave
3. TEE fetches oracle price
4. TEE computes settlement using the decrypted params
5. TEE calls the contract with `(commitment, oracle_price, settlement_amount)`

### Step 2.2 — Fix `submit_liquidate` to actually decrypt and compute

Currently in `tools/tee-match/src/liquidator.rs`, `oracle_price` and `settlement_amount` are hardcoded to 0. Implement:
1. Fetch position storage → get `sealed_params`
2. Decrypt → get actual position params
3. Fetch oracle price (Pyth Hermes or pull from the oracle contract)
4. Compute PnL inside the TEE
5. Pass `(commitment, oracle_price, settlement_amount)` to contract

### Step 2.3 — Plumb TP/SL triggers through TEE

Currently `trigger_tp` / `trigger_sl` are TEE-gated but no one calls them from the TEE. Add a TEE endpoint that:
1. Fetches position → decrypts `sealed_params`
2. Checks if TP/SL condition is met (oracle vs threshold)
3. Computes settlement
4. Calls contract

---

## Phase 3: Order Book Privacy

### Step 3.1 — Remove plain text hints from `place_order`

Currently `place_order` receives `hint_price`, `hint_side`, `hint_size`, `hint_leverage`. Replace with a single `encrypted_hints: Bytes` blob. The orderbook contract stores the encrypted blob and only the TEE decrypts it during matching.

Requires changes to `contracts/orderbook/src/lib.rs`:
- `place_order` should take `encrypted_hints: Bytes` instead of plain params
- Remove plain fields from order storage
- `match_positions` already runs in the TEE, which can decrypt both sides

### Step 3.2 — Match execution privacy

`match_positions` currently takes plain `match_price` and `match_size` as field elements. Move match price computation into the TEE:
- TEE decrypts both orders
- TEE computes match price (midpoint, or oracle-based)
- TEE provides `match_price` and `match_size` to the contract

---

## Summary of Changes Needed

| File | Change |
|------|--------|
| `contracts/perp-engine/src/lib.rs` | Collapse `PositionMeta`, change all function signatures, obfuscate events |
| `tools/tee-match/src/stellar.rs` | Add `unseal_position_params()`, fix all relay functions to decrypt + compute |
| `tools/tee-match/src/liquidator.rs` | Implement real liquidation logic (decrypt → oracle → compute → submit) |
| `tools/tee-match/src/serve.rs` | Add TP/SL trigger endpoints |
| `contracts/orderbook/src/lib.rs` | Encrypted order hints |
| `tools/e2e/src/soroban_rpc.rs` | Add `scval_bytes()` if missing |
