# Phase 2: Blind Contract + TEE-Computed Settlement

## Architecture

```
Before (Phase 1):                     After (Phase 2):
─────────────────                     ─────────────────
Contract knows:                       Contract knows:
  - collateral (i128)                   - position_commitment (BytesN<32>)
  - matched_price (u64)                 - status / created_at / asset_id
  - funding_at_open (i128)              - sealed_params (encrypted blob)
  - effective_collateral (i128)         - note_commitment -> hash(amount, blind)
  - note amount (i128)                
  - match records                     
  - oracle price / twap               
  - funding state / rate              

TEE provides:                         TEE knows:
  - oracle_price + settlement_amount    - ALL plain values (collateral, match_price, etc.)
  - trigger_tp/sl/liquidate/close       - stored in its own SecretStore DB
                                       - computes all PnL, funding, rewards
                                       - submits net settlement commands
```

## Files Changed (in dependency order)

### 1. `contracts/types/src/lib.rs` — Update structs

**Remove:**
- `MatchRecord` (lines 142-151) — entire struct
- `OracleConfig` (lines 123-131) — entire struct
- `PriceSample` (lines 134-139) — entire struct
- `FundingState` (lines 154-160) — entire struct

**Remove from `PositionMeta`:**
- `collateral: i128`
- `matched_price: u64`
- `funding_at_open: i128`
- `effective_collateral: i128`
- `match_id: u64`

**Add to `PositionMeta`:**
- `settlement_commitment: BytesN<32>` — hash(collateral, matched_price, funding_at_open) that only TEE can open

Final `PositionMeta` will be:
```rust
#[contracttype]
#[derive(Clone)]
pub struct PositionMeta {
    pub status: PositionStatus,
    pub created_at: u64,
    pub partial_liq_done: bool,
    pub liquidation_recipient_note: BytesN<32>,
    pub asset_id: BytesN<32>,
    pub margin_mode: MarginMode,
    pub portfolio_key: BytesN<32>,
    pub sealed_params: Bytes,
    pub settlement_commitment: BytesN<32>,
}
```

**Remove from `DataKey` (in perp-engine):**
- `DataKey::OracleConfig`
- `DataKey::Match(u64)`
- `DataKey::NextMatchId`
- `DataKey::FundingState`
- `DataKey::MarkPrice`
- `DataKey::TwapSample(u64)`
- `DataKey::TwapHead`
- `DataKey::AssetOracle(BytesN<32>)`
- `DataKey::AssetTwapSample(BytesN<32>, u64)`
- `DataKey::AssetTwapHead(BytesN<32>)`

**Change `DataKey::Note`:**
- `Note(BytesN<32>)` stores `BytesN<32>` (hash of amount+blind) instead of `i128`

---

### 2. `contracts/perp-engine/src/lib.rs` — Major rewrite

#### Remove entire functions (delete):

| Function | Lines | Reason |
|----------|-------|--------|
| `match_positions` | 240-335 | Matching moves to TEE |
| `trigger_tp` | 479-519 | Replaced by `settle_position` |
| `trigger_sl` | 521-561 | Replaced by `settle_position` |
| `close_position_to_note` | 740-808 | Replaced by `settle_position` |
| `liquidate` | 812-909 | Replaced by `settle_position` |
| `settle_match` | 1294-1362 | Replaced by `settle_position` |
| `update_funding` | 1368-1416 | TEE handles funding |
| `set_price` | 1195-1243 | TEE provides prices |
| `set_asset_price` | 1068-1110 | TEE provides prices |
| `get_price` | 1245-1248 | Remove |
| `get_twap` | 1251-1255 | Remove |
| `get_oracle_config` | 1257-1259 | Remove |
| `set_oracle_admin` | 1164-1191 | Remove |
| `reset_asset_oracle` | 1132-1149 | Remove |
| `set_mark_price` | 1263-1279 | Remove |
| `get_mark_price` | 1281-1286 | Remove |
| `get_match_record` | 1288-1292 | Remove |
| `get_next_match_id` | 1422-1424 | Remove |
| `read_oracle_config` | 1447-1451 | Remove |
| `push_twap_sample` | 1454-1492 | Remove |
| `require_oracle_price` | 1494-1509 | Remove |
| `read_asset_oracle_config` | 1512-1515 | Remove |
| `push_asset_twap_sample` | 1517-1554 | Remove |
| `require_asset_oracle_price` | 1556-1574 | Remove |
| `read_funding_state` | 1576-1585 | Remove |
| `read_funding_cumulative` | 1587-1589 | Remove |
| `try_close_match` | 1591-1628 | Remove |
| `derive_mark_price` | 1630-1635 | Remove |
| `pay_liquidator_reward` | 1670-1675 | Remove (but keep insurance fund helpers) |
| `fund_insurance` | 913-918 | Keep |
| `insurance_balance` | 920-922 | Keep |
| `bad_debt` | 924-929 | Keep |

Total removed: ~600 lines.

#### Keep (unchanged):
- `initialize`, `set_tee_account`, `require_tee_auth`, `config`
- `register_asset`, `update_asset_config`, `get_asset_config`, `list_assets`, `get_asset_name`
- `get_position`, `is_spent`, `get_portfolio_group`, `get_note` (signature changes)
- `upgrade`
- `fund_insurance`, `insurance_balance`, `bad_debt`
- `add_to_portfolio`, `remove_from_portfolio`
- `add_margin_from_note` (signature changes)
- `cancel_position_to_note` (signature changes)

#### Change `deposit_note` (line 339):

**Before:**
```rust
pub fn deposit_note(env: Env, from: Address, note_commitment: BytesN<32>, amount: i128) {
    // stores: DataKey::Note(note_commitment) -> amount (i128)
    // event: (note_commitment, amount)
}
```

**After:**
```rust
pub fn deposit_note(env: Env, from: Address, note_commitment: BytesN<32>, amount_commitment: BytesN<32>) {
    // stores: DataKey::Note(note_commitment) -> amount_commitment (BytesN<32>)
    // NO plain amount in storage or event
    // event: (note_commitment,) — no amount
}
```

The contract transfers tokens from `from` to itself (same as before), but stores only the commitment hash. The TEE knows the real amount.

#### Change `withdraw_note` (line 364):

**Before:** reads `amount: i128` from note storage, verifies NoteSpend proof, transfers amount.

**After:** TEE-gated. The TEE provides the real amount:
```rust
pub fn withdraw_note(
    env: Env,
    note_commitment: BytesN<32>,
    nullifier: BytesN<32>,
    recipient: Address,
    amount: i128,          // TEE provides this
    blinding: BytesN<32>,  // TEE provides this so contract verifies hash
    proof: Groth16Proof,
) {
    Self::require_tee_auth(&env);  // NEW: TEE must authorize
    // 1. verify nullifier not spent
    // 2. read stored commitment: stored = DataKey::Note(note_commitment) -> BytesN<32>
    // 3. verify hash(amount, blinding) == stored
    // 4. verify NoteSpend proof
    // 5. transfer amount
    // 6. mark nullifier spent
    // event: (note_commitment, nullifier,) — no amount
}
```

This is a **trust shift**: users need TEE approval to withdraw. If unacceptable, we need a ZK circuit that proves `hash(amount, blind) == stored_commitment` without revealing amount. For now, use TEE gate.

#### Change `open_position_from_note` (line 567):

**Before:** reads `collateral` from note storage, stores it in PositionMeta.

**After:** TEE provides collateral + blinding, contract verifies commitment:
```rust
pub fn open_position_from_note(
    env: Env,
    note_commitment: BytesN<32>,
    note_nullifier: BytesN<32>,
    position_commitment: BytesN<32>,
    sealed_params: Bytes,
    liquidation_recipient_note: BytesN<32>,
    portfolio_key: BytesN<32>,
    asset_id: BytesN<32>,
    collateral_amount: i128,         // NEW: TEE provides
    collateral_blinding: BytesN<32>, // NEW: TEE provides
    settlement_commitment: BytesN<32>, // NEW: hash(collateral, 0, 0) for unmatched position
    note_proof: Groth16Proof,
    commit_proof: Groth16Proof,
) {
    // 1. verify note existence
    // 2. verify hash(collateral_amount, collateral_blinding) == stored note commitment
    // 3. verify proofs
    // 4. store PositionMeta WITHOUT collateral/matched_price/funding_at_open
    //    settlement_commitment replaces all of them
    // event: (position_commitment,) — no collateral
}
```

#### Change `add_margin_from_note` (line 412):

**Before:** reads `amount` from note storage, adds to `meta.collateral`.

**After:** TEE provides amount + blinding:
```rust
pub fn add_margin_from_note(
    env: Env,
    note_commitment: BytesN<32>,
    nullifier: BytesN<32>,
    position_commitment: BytesN<32>,
    amount: i128,             // NEW: TEE provides
    blinding: BytesN<32>,     // NEW: TEE provides
    new_settlement_commitment: BytesN<32>, // NEW: updated hash
    proof: Groth16Proof,
) {
    Self::require_tee_auth(&env); // NEW
    // 1. verify hash(amount, blinding) == stored note commitment
    // 2. verify NoteSpend proof
    // 3. update position's settlement_commitment
    // event: (position_commitment,) — no amounts
}
```

#### Change `cancel_position_to_note` (line 671):

**Before:** refunds `meta.collateral` to recipient note.

**After:** TEE provides refund amount + blinding:
```rust
pub fn cancel_position_to_note(
    env: Env,
    position_commitment: BytesN<32>,
    cancel_nullifier: BytesN<32>,
    recipient_note: BytesN<32>,
    refund_amount: i128,         // NEW: TEE provides
    refund_blinding: BytesN<32>, // NEW: TEE provides
    cancel_proof: Groth16Proof,
) {
    Self::require_tee_auth(&env); // NEW
    // 1. verify cancel proof
    // 2. store recipient_note with hash(refund_amount, refund_blinding)
    // 3. mark position Cancelled
    // event: (position_commitment, cancel_nullifier, recipient_note,) — no amounts
}
```

#### NEW: `settle_position` — replaces trigger_tp/sl/liquidate/close/settle_match:

```rust
pub fn settle_position(
    env: Env,
    commitment: BytesN<32>,
    status: PositionStatus,         // Closed or Liquidated
    settlement_note: BytesN<32>,     // recipient note for the owner
    settlement_amount: i128,        // amount to credit owner
    settlement_blinding: BytesN<32>, // blinding so contract can verify hash
    reward_amount: i128,            // to insurance fund (liquidator reward)
    ins_delta: i128,                // insurance fund change
    bad_debt: i128,                 // bad debt to accrue
) {
    Self::require_tee_auth(&env);
    // 1. read position, verify Matched status
    // 2. create settlement_note with hash(settlement_amount, settlement_blinding)
    // 3. update insurance fund
    // 4. accrue bad debt
    // 5. mark position status (Closed or Liquidated)
    // 6. remove from portfolio if cross
    // event: (commitment,) — minimal
}
```

#### Remove from `DataKey` enum:
- `OracleConfig`
- `Match(u64)`
- `NextMatchId`
- `FundingState`
- `MarkPrice`
- `TwapSample(u64)`
- `TwapHead`
- `AssetOracle(BytesN<32>)`
- `AssetTwapSample(BytesN<32>, u64)`
- `AssetTwapHead(BytesN<32>)`

Keep: `Config`, `Position(BytesN<32>)`, `Nullifier(BytesN<32>)`, `Balance(Address)`, `Note(BytesN<32>)`, `InsuranceFund`, `BadDebt`, `PortfolioGroup(BytesN<32>)`, `AssetConfig(BytesN<32>)`, `AssetName(BytesN<32>)`, `AssetList`, `TeeAccount`

#### Update `get_note` (line 473):

**Before:** `get_note(env, note_commitment) -> Option<i128>`
**After:** `get_note(env, note_commitment) -> Option<BytesN<32>>` — returns the commitment hash, not the plain amount.

#### Remove imports:
- Remove `FundingState, Groth16Error, Groth16Proof, MatchRecord, OracleConfig, PriceSample` from `types::` imports (some may still be needed — keep Groth16Proof)

#### Update `setup()` test helper (line 1881):
- Remove oracle-related setup
- Update `create_position` helper to include `settlement_commitment`

#### Update tests:
- Remove all oracle tests (test_oracle_*, line 1997-2103)
- Remove mark price tests (line 2133-2148)
- Remove settle_match test (line 2151-2203)
- Remove liquidate tests (line 2206-2269)
- Remove funding tests (line 2272-2413)
- Remove get_match_record test (line 2416-2420)
- Update remaining tests:
  - `test_add_margin_*` — TEE-gated now; add `require_tee_auth` mock
  - `test_withdraw_note_*` — TEE-gated now; add tee auth
  - `test_open_position_from_note_*` — new params
  - `test_cancel_position_to_note_*` — new params
  - `test_close_position_to_note_*` — remove (replaced by settle_position)
  - `test_trigger_tp_*` — remove (replaced by settle_position)
  - `test_trigger_sl_*` — remove (replaced by settle_position)
  - `test_partial_liquidation_*` — remove (replaced by settle_position)
  - `test_full_liquidation_*` — remove (replaced by settle_position)

---

### 3. `contracts/orderbook/src/lib.rs` — Remove match_positions dependency

Orderbook already doesn't have `match_positions` (only perp-engine does). No changes needed to orderbook.

Wait — actually the orderbook `place_order` handler in TEE (`handle_place`, `handle_market`) calls `do_match` which calls `stellar::submit_match` which calls the perp-engine's `match_positions`. Since `match_positions` is removed from the contract, we need to update the TEE matching flow.

But the orderbook CONTRACT itself doesn't change. The change is in the TEE serve.rs matching flow (handled in phase 2.7).

---

### 4. `tools/tee-match/src/db.rs` — Add position state and note amount stores

Add new types to `SecretStore`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionState {
    pub collateral: i128,
    pub matched_price: u64,
    pub funding_at_open: i128,
    pub effective_collateral: i128,
    pub entry_price: u64,
    pub leverage: u64,
    pub side: u64,  // 0=long, 1=short
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteAmount {
    pub amount: i128,
    pub blinding: [u8; 32],
}
```

Add methods to `SecretStore`:
- `insert_position_state(cmt_hex, &PositionState)` — store position plain values
- `get_position_state(cmt_hex) -> Option<PositionState>` — retrieve
- `insert_note_amount(note_cmt_hex, &NoteAmount)` — store note amount
- `get_note_amount(note_cmt_hex) -> Option<NoteAmount>` — retrieve

These use the same sled tree (`"secrets"`) with key prefixes:
- `pos_{cmt_hex}` for position state
- `note_{note_cmt_hex}` for note amounts

---

### 5. `tools/tee-match/src/stellar.rs` — Add settle_position relay, remove old relays

**Remove functions:**
- `submit_match` (lines 12-49) — match_positions removed from contract
- `submit_liquidate` (lines 201-215) — liquidate removed
- `relay_trigger_tp` (lines 94-106) — trigger_tp removed
- `relay_trigger_sl` (lines 108-120) — trigger_sl removed

**Add functions:**

```rust
/// Relay settle_position to the perp-engine contract.
/// This is the single generic settlement function replacing trigger_tp/sl/liquidate/close/settle_match.
pub fn relay_settle_position(
    perp_id: &str,
    commitment: &str,
    status: u32,                    // 2=Closed, 4=Liquidated
    settlement_note: &str,          // hex
    settlement_amount: i128,
    settlement_blinding: &str,      // hex 64 chars
    reward_amount: i128,
    ins_delta: i128,
    bad_debt: i128,
) -> Result<String> {
    let src = signing_source("e2e");
    let rpc = SorobanRpc::new();
    let tx_hash = rpc.invoke_xdr(perp_id, &src, "settle_position", vec![
        scval_bytes32(commitment)?,
        scval_u64(status),
        scval_bytes32(settlement_note)?,
        scval_i128(settlement_amount),
        scval_bytes32(settlement_blinding)?,
        scval_i128(reward_amount),
        scval_i128(ins_delta),
        scval_i128(bad_debt),
    ])?;
    Ok(tx_hash)
}
```

```rust
/// Relay deposit_note with amount commitment only.
pub fn relay_deposit_note(
    perp_id: &str,
    from: &str,
    note_commitment: &str,
    amount_commitment: &str,     // hash(amount, blind)
    amount: i128,                // plain amount (not submitted, stored in TEE DB)
    blinding: &str,              // hex blinding factor (stored in TEE DB)
) -> Result<String> {
    // 1. Store note_amount in TEE DB
    // 2. Submit deposit_note to contract (without plain amount)
    let src = signing_source("e2e");
    let rpc = SorobanRpc::new();
    let tx_hash = rpc.invoke_xdr(perp_id, &src, "deposit_note", vec![
        scval_address(from)?,
        scval_bytes32(note_commitment)?,
        scval_bytes32(amount_commitment)?,
    ])?;
    Ok(tx_hash)
}
```

```rust
/// Relay withdraw_note — TEE provides amount + blinding, contract verifies hash.
pub fn relay_withdraw_note(
    perp_id: &str,
    note_commitment: &str,
    nullifier: &str,
    recipient: &str,
    amount: i128,
    blinding: &str,
    proof: &str,
) -> Result<String> {
    let src = signing_source("e2e");
    let rpc = SorobanRpc::new();
    let tx_hash = rpc.invoke_xdr(perp_id, &src, "withdraw_note", vec![
        scval_bytes32(note_commitment)?,
        scval_bytes32(nullifier)?,
        scval_address(recipient)?,
        scval_i128(amount),
        scval_bytes32(blinding)?,
        scval_proof(proof)?,
    ])?;
    Ok(tx_hash)
}
```

```rust
/// Relay open_position_from_note with TEE-provided collateral and settlement_commitment.
pub fn relay_open_position_v2(
    perp_id: &str,
    orderbook_id: &str,
    note_cmt_hex: &str,
    note_null_hex: &str,
    position_cmt_hex: &str,
    sealed_params: &[u8],
    collateral_amount: i128,
    collateral_blinding: &str,
    settlement_commitment: &str,
    portfolio_key_hex: &str,
    asset_id_hex: &str,
    note_proof_json: &str,
    commit_proof_json: &str,
) -> Result<String> {
    let src = signing_source("e2e");
    // 1. Store position state in TEE DB
    // 2. place_order to orderbook (same as before)
    // 3. open_position_from_note to perp (with new params)
    // ...
}
```

**Update `relay_open_position` (lines 221-274):**
- Rename to `relay_open_position_v2`
- Add `collateral_amount`, `collateral_blinding`, `settlement_commitment` params
- Store position state in SecretStore after submission
- Remove `hint_price`, `hint_side`, `hint_leverage`, `hint_size` from params (they're inside sealed_params)

**Update `submit_cancel`:** No changes needed (orderbook cancel doesn't change).

**Update `submit_mark_price`:** Keep — mark price is still posted to contract for display. Actually no, we're removing `set_mark_price` from the contract. Remove this too.

---

### 6. `tools/tee-match/src/liquidator.rs` — Full pipeline

Replace the placeholder `check_and_liquidate` with real implementation:

```rust
pub fn check_and_liquidate(
    rpc: &SorobanRpc,
    perp_id: &str,
    commitment: &str,
    store: &SecretStore,
) -> Result<Option<String>> {
    // 1. Fetch position from chain: get_position(commitment)
    let pos = fetch_position(rpc, perp_id, commitment)?;
    if pos.status != Matched { return Ok(None); }

    // 2. Decrypt sealed_params → side, entry_price, leverage
    let sealed = pos.sealed_params.to_array();
    let params = stellar::unseal_position_params(&sealed)?;
    let (side, entry_price, leverage, _, _, _, _, _) = params;

    // 3. Look up position state from SecretStore
    let state = store.get_position_state(commitment)?
        .ok_or(anyhow::anyhow!("Position state not found in TEE DB"))?;

    // 4. Fetch oracle price from Pyth Hermes
    let oracle_price = fetch_oracle_price()?;

    // 5. Compute PnL and settlement
    let notional = state.collateral * leverage as i128;
    let pnl = if side == 0 { // long
        (oracle_price as i128 - entry_price as i128) * notional / entry_price as i128
    } else { // short
        (entry_price as i128 - oracle_price as i128) * notional / entry_price as i128
    };
    let settlement = state.collateral + pnl;

    // 6. Check if liquidation needed (settlement < 0 or settlement < mm)
    let mm = state.collateral * MAINTENANCE_MARGIN_BPS / 10_000;
    if settlement >= 0 && settlement >= mm { return Ok(None); }

    // 7. Compute rewards using asset config
    let asset_cfg = fetch_asset_config(rpc, perp_id, &pos.asset_id)?;
    let (reward, ins_delta, bad_debt) = compute_liquidation_rewards(
        state.effective_collateral, settlement, &asset_cfg
    );

    // 8. Determine status: Liquidated (or Closed if positive settlement)
    let status = if settlement >= 0 { 2u32 } else { 4u32 }; // Closed=2, Liquidated=4

    // 9. Call relay_settle_position
    let settlement_note = compute_settlement_note(settlement);
    let blinding = generate_blinding();
    let tx_hash = stellar::relay_settle_position(
        perp_id, commitment, status,
        &settlement_note, settlement.max(0), &blinding,
        reward, ins_delta, bad_debt,
    )?;

    // 10. Store note amount in TEE DB
    store.insert_note_amount(&settlement_note, &NoteAmount {
        amount: settlement.max(0),
        blinding,
    })?;

    Ok(Some(tx_hash))
}
```

Also add helper functions:
- `fetch_position(rpc, perp_id, commitment) -> PositionMeta` — calls `get_position` view function
- `fetch_asset_config(rpc, perp_id, asset_id) -> AssetConfig` — calls `get_asset_config`
- `compute_liquidation_rewards(effective_collateral, settlement, config) -> (reward, ins_delta, bad_debt)`
- `fetch_oracle_price() -> u64` — calls Pyth Hermes API or reads cached price
- `generate_blinding() -> [u8; 32]` — random bytes

Update `spawn()` function to use the new `check_and_liquidate` with full RPC client.

---

### 7. `tools/tee-match/src/serve.rs` — Update routes and handlers

**TCP handlers to update:**

- `handle_match` (line 697) — remove or change to use settle_position flow. Since matching now happens entirely in TEE with no on-chain `match_positions` call, this handler should instead:
  1. Verify both orders exist in SecretStore
  2. Compute match via `engine::find_match` (existing)
  3. Generate match proof (existing — needed? If we remove match_positions, we don't need the match proof anymore either!)
  4. For each matched pair, compute settlement for both positions
  5. Call `relay_settle_position` for each position
  
**Wait — this is a big simplification!** Since `match_positions` is removed, the TEE matching flow becomes:
  1. TEE matches two orders in its CLOB (already done)
  2. TEE computes settlement for each position directly
  3. TEE calls `settle_position` for each position (instead of `match_positions`)
  
No on-chain match record, no match proof, no nullifiers for matching. The positions go from Open directly to Closed/Liquidated when the TEE settles them.

**Simplify `do_match` (line 1291):**
```rust
fn do_match_and_settle(
    store: &SecretStore,
    keys: &PathBuf,
    cmt_a: &str,
    cmt_b: &str,
    perp: &str,
    source: &str,
    maker_side: engine::Side,
    maker_price: u64,
    maker_size: u64,
) -> Option<MatchResultData> {
    let a = store.get(cmt_a).ok()??;
    let b = store.get(cmt_b).ok()??;

    // 1. Find match params
    let params = engine::find_match(&a, &b)?;

    // 2. Look up/seal position state for both
    let state_a = store.get_position_state(cmt_a).ok()??;
    let state_b = store.get_position_state(cmt_b).ok()??;

    // 3. Compute settlement for both using oracle price
    let oracle_price = fetch_oracle_price();
    let settlement_a = compute_settlement(&state_a, oracle_price);
    let settlement_b = compute_settlement(&state_b, oracle_price);

    // 4. Call settle_position for each
    let note_a = compute_settlement_note(settlement_a);
    let note_b = compute_settlement_note(settlement_b);
    let blinding_a = generate_blinding();
    let blinding_b = generate_blinding();

    stellar::relay_settle_position(perp, cmt_a, 2, &note_a, settlement_a, &blinding_a, 0, 0, 0).ok()?;
    stellar::relay_settle_position(perp, cmt_b, 2, &note_b, settlement_b, &blinding_b, 0, 0, 0).ok()?;

    // 5. Store note amounts
    store.insert_note_amount(&note_a, &NoteAmount { amount: settlement_a, blinding: blinding_a }).ok()?;
    store.insert_note_amount(&note_b, &NoteAmount { amount: settlement_b, blinding: blinding_b }).ok()?;

    Some(MatchResultData { ... })
}
```

**TCP handlers to remove:**
- `handle_set_mark_price` (line 1204) — set_mark_price removed from contract

**TCP handlers to add:**
- `handle_settle` — manual settle_position trigger
- `handle_deposit_note` — relay deposit_note with commitment
- `handle_withdraw_note` — relay withdraw_note with TEE-provided amount

**HTTP handlers (`pub mod http`):**
- Remove `/trigger-tp` and `/trigger-sl` routes (lines 1816-1817)
- Add `/settle` route
- Remove `/set_mark_price` handler
- Update `/relay/open-position` handler to use v2 relay

**Secure HTTP handlers (`pub mod secure`):**
- Remove `/set_mark_price` route
- Remove `/trigger-tp` and `/trigger-sl` routes

---

### 8. `tools/e2e/src/soroban_rpc.rs` — Add scval_address if missing

The `relay_deposit_note` and `relay_withdraw_note` need `scval_address`. Check if it exists in `soroban_rpc.rs`. If not, add:
```rust
pub fn scval_address(addr: &str) -> ScVal {
    // Parse Stellar address (G... string) to ScVal::Address
}
```

---

## Implementation Order

### Step A: Update `types/src/lib.rs` (foundation)
Agent A edits `contracts/types/src/lib.rs`:
1. Remove `MatchRecord` struct
2. Remove `OracleConfig` struct
3. Remove `PriceSample` struct
4. Remove `FundingState` struct
5. Remove `collateral`, `matched_price`, `funding_at_open`, `effective_collateral`, `match_id` from `PositionMeta`
6. Add `settlement_commitment: BytesN<32>` to `PositionMeta`

### Step B: Rewrite `perp-engine/src/lib.rs`
Agent B edits `contracts/perp-engine/src/lib.rs`:
1. Remove `DataKey` variants (OracleConfig, Match, NextMatchId, FundingState, MarkPrice, Twap*, AssetOracle*, AssetTwap*)
2. Remove all listed functions (~600 lines)
3. Modify `deposit_note` to accept `amount_commitment: BytesN<32>` instead of `amount: i128`
4. Modify `withdraw_note` to be TEE-gated with `amount: i128, blinding: BytesN<32>` params
5. Modify `open_position_from_note` to accept `collateral_amount: i128, collateral_blinding: BytesN<32>, settlement_commitment: BytesN<32>` instead of reading collateral from note
6. Modify `add_margin_from_note` to be TEE-gated with `amount: i128, blinding: BytesN<32>, new_settlement_commitment: BytesN<32>`
7. Modify `cancel_position_to_note` to be TEE-gated with `refund_amount: i128, refund_blinding: BytesN<32>`
8. Add `settle_position` function
9. Modify `get_note` to return `Option<BytesN<32>>` instead of `Option<i128>`
10. Update tests

### Step C: Update `orderbook/src/lib.rs`
Agent C: No changes needed to the contract itself. Skip.

### Step D: Update `tee-match/src/db.rs`
Agent D edits `tools/tee-match/src/db.rs`:
1. Add `PositionState` struct
2. Add `NoteAmount` struct
3. Add `insert_position_state(&self, cmt_hex, &PositionState)` method
4. Add `get_position_state(&self, cmt_hex) -> Result<Option<PositionState>>` method
5. Add `insert_note_amount(&self, note_cmt_hex, &NoteAmount)` method
6. Add `get_note_amount(&self, note_cmt_hex) -> Result<Option<NoteAmount>>` method

### Step E: Update `tee-match/src/stellar.rs`
Agent E edits `tools/tee-match/src/stellar.rs`:
1. Remove `submit_match` function
2. Remove `submit_liquidate` function
3. Remove `relay_trigger_tp` function
4. Remove `relay_trigger_sl` function
5. Rework `relay_open_position` to v2 with new params + store position state
6. Add `relay_settle_position` function
7. Add `relay_deposit_note` function
8. Add `relay_withdraw_note` function

### Step F: Update `tee-match/src/liquidator.rs`
Agent F edits `tools/tee-match/src/liquidator.rs`:
1. Replace `check_and_liquidate` stub with full implementation
2. Update `spawn()` to use new implementation
3. Add helper functions: `fetch_position`, `fetch_asset_config`, `compute_liquidation_rewards`, `fetch_oracle_price`, `generate_blinding`

### Step G: Update `tee-match/src/serve.rs`
Agent G edits `tools/tee-match/src/serve.rs`:
1. Remove `handle_set_mark_price` function
2. Rework `do_match` → `do_match_and_settle` (no on-chain match, just settle_position)
3. Add `handle_settle` function
4. Add `handle_deposit_note` function (TCP)
5. Add `handle_withdraw_note` function (TCP)
6. Update HTTP routes: remove `/trigger-tp`, `/trigger-sl`, `/set_mark_price`; add `/settle`
7. Update `relay_open_position` HTTP handler

## Dependency Graph for Parallel Execution

```
types.rs ──► perp-engine.rs
  │
  ├──► orderbook.rs (no changes)
  │
  └──► TEE stellar.rs ──► TEE liquidator.rs
         │
         ├──► TEE serve.rs
         │
         └──► TEE db.rs (independent)
```

Steps that can run in parallel (once types.rs is done):
- B (perp-engine) + D (db.rs) + the read of existing serve.rs/liquidator.rs

Steps that depend on B + D + E:
- E (stellar.rs) depends on knowing new contract interface
- F (liquidator.rs) depends on stellar.rs + db.rs
- G (serve.rs) depends on stellar.rs + liquidator.rs + db.rs

## Testing Strategy

### Contract tests (Step B):
1. Remove all oracle/funding/match/liquidation tests (~25 tests removed)
2. Update `setup()` to not use oracle
3. Update `create_position` helper to include `settlement_commitment`
4. Update remaining tests:
   - `test_deposit_note_*` — new `amount_commitment` param
   - `test_withdraw_note_*` — TEE-gated, new params
   - `test_open_position_from_note_*` — new collateral params
   - `test_add_margin_from_note_*` — TEE-gated, new params
   - `test_cancel_position_to_note_*` — TEE-gated, new params
   - `test_settle_position_*` — new tests
   - `test_register_asset_*` — unchanged
5. Run: `cargo test -p perp-engine`

### TEE tests (Step D/E/F/G):
No Rust unit tests for TEE binary. Test via:
1. `cargo build -p tee-match` — must compile
2. Manual integration test with deployed contract

## Deployment Steps (after implementation)

1. `cargo build -p perp-engine --target wasm32-unknown-unknown --release`
2. `stellar contract deploy --wasm target/wasm32-unknown-unknown/release/perp_engine.wasm ...`
3. Update `VITE_PERP_ENGINE_ID` env var
4. `cargo build -p tee-match --release`
5. Restart TEE container
6. Call `set_tee_account` with new TEE public key
7. Test: deposit → open → match → settle flow
