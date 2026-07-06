# CER-PERP Agent Handoff

---

## Infrastructure

| Resource | Value |
|---|---|
| GCP Project | `cer-perp-tee-2` (346182764109) |
| TEE VM | `tee-match-2`, zone `us-central1-a` |
| Keepers VM | `keepers-vm-2`, zone `us-central1-b` |
| Contract | `CCT476K37KCWZFXWMXXPUKH2FWJESOJVLMSGS2DKCFZFYSZII42XY4VW` (Stellar testnet) |
| TEE signing key | `SCPJ6HPSZJIALU42CPOQVYRHXTQZ3BNYD3DYYVBL7WMTADWKLBYZ7KE7` (SOURCE = RELAYER) |
| TEE pubkey | `GAZ7LYN2ROIKRVKK4BIL5S4PVRED2YD6YNB4BA5LYB4TSQGN4BZKHTTP` |
| RPC | `https://stellar-testnet.g.alchemy.com/v2/FqjaGAy9IMENhdv2i_3UUVDPZnNClYNq` |
| Vercel project | `cerida` (`prj_88pdAswbfwaAY2h5jvhAvaloAABQ`), production branch `x` |
| Live URL | `ceridapp.xyz` |
| Git branch | Always commit to `x` — never `main` |

**Commit rules:**
- Never add `Co-Authored-By` lines
- Never include VM IP addresses in commit message text

---

## System Architecture

```
User Browser (ceridapp.xyz)
    │
    │ HTTPS — /tee/* proxied to TEE via Vercel edge rewrite
    ▼
Vercel (React Router SPA, branch x auto-deploys)
    │
    │ HTTP port 9721
    ▼
TEE Server (tee-match-2)
    ├── TCP port 9720 ─── Keepers market maker (fast-init, place, cancel)
    ├── HTTP port 9721 ── Frontend relay endpoints
    ├── SQLite DB (/var/lib/tee-keys/tee-db) — secrets, notes, CLOB state
    ├── ZK keys (/var/lib/tee-keys/*.pk.bin) — proving keys (Groth16)
    └── Liquidator thread — scans positions, submits settle_position
    │
    │ Soroban RPC (Alchemy testnet)
    ▼
Stellar Testnet — PerpEngine contract
    │
    ▲
Keepers VM (keepers-vm-2)
    ├── Market maker thread — 32 bid + 32 ask levels × 7 markets = 448 quotes
    └── (Oracle keeper — NOT YET IMPLEMENTED, see Incomplete section)
```

---

## How to Deploy

### Frontend
```bash
git add <files>
git commit -m "message"
git push origin x
```
Vercel GitHub integration auto-deploys to production. **Never use `vercel --prod`** — CLI deploys fail with "No Output Directory named 'client' found" due to a build-cache bug (SPA mode deletes the server build).

### TEE Server (tee-match-2)

The Cloud Build SSH hot-swap step **often fails silently** in Cloud Build: the tee-match-2 VM has a read-only `/root` filesystem, so `sudo docker login` cannot write credentials to `/root/.docker`, the pull fails, and the old container is restarted. Because the script does not use `set -e`, the Cloud Build step can still report success even though the new image was never deployed. Always verify the running image digest after a deployment.

```bash
# Step 1 — trigger build (image is pushed; SSH step may silently fail, that's expected)
gcloud builds submit --config=cloudbuild-tee-2.yaml --project=cer-perp-tee-2 --async .

# Step 2 — manually hot-swap after build finishes.
# Use DOCKER_CONFIG=/tmp/docker-cfg because /root is read-only on tee-match-2.
gcloud compute ssh tee-match-2 --project=cer-perp-tee-2 --zone=us-central1-a --tunnel-through-iap --quiet --command="
  mkdir -p /tmp/docker-cfg
  export DOCKER_CONFIG=/tmp/docker-cfg
  IMAGE=us-central1-docker.pkg.dev/cer-perp-tee-2/tee-match-repo/tee-match:latest
  TOKEN=\$(curl -sf 'http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token' -H 'Metadata-Flavor: Google' | python3 -c 'import sys,json; print(json.load(sys.stdin)[\"access_token\"])')
  echo \$TOKEN | sudo DOCKER_CONFIG=/tmp/docker-cfg docker login -u oauth2accesstoken --password-stdin us-central1-docker.pkg.dev
  sudo DOCKER_CONFIG=/tmp/docker-cfg docker pull \$IMAGE
  sudo docker stop tee-match 2>/dev/null || true
  sudo docker rm tee-match 2>/dev/null || true
  RELAYER=\$(curl -sf http://metadata.google.internal/computeMetadata/v1/instance/attributes/tee-env-STELLAR_RELAYER_SECRET -H Metadata-Flavor:Google)
  SOURCE=\$(curl -sf http://metadata.google.internal/computeMetadata/v1/instance/attributes/tee-env-STELLAR_SOURCE_SECRET -H Metadata-Flavor:Google)
  sudo docker run -d --name tee-match --restart=always --network=host \
    -e STELLAR_RELAYER_SECRET=\$RELAYER \
    -e STELLAR_SOURCE_SECRET=\$SOURCE \
    -v /var/lib/tee-keys:/keys \
    \$IMAGE
"

# Step 3 — verify the running image matches the registry digest
REG_DIGEST=\$(gcloud artifacts docker tags list us-central1-docker.pkg.dev/cer-perp-tee-2/tee-match-repo/tee-match --format='value(version.basename())' --limit=1)
RUN_DIGEST=\$(gcloud compute ssh tee-match-2 --project=cer-perp-tee-2 --zone=us-central1-a --tunnel-through-iap --quiet --command="IMAGE=\$(sudo docker inspect --format='{{.Image}}' tee-match); sudo docker image inspect --format='{{range .RepoDigests}}{{println .}}{{end}}' \$IMAGE" | awk -F'@' '{print \$2}')
echo "Registry: \$REG_DIGEST"
echo "Running:  \$RUN_DIGEST"
[ "\$REG_DIGEST" = "\$RUN_DIGEST" ] && echo 'tee-match is up to date' || echo 'WARNING: tee-match is NOT running the latest image'
```

### Keepers (keepers-vm-2)
```bash
# Step 1 — trigger build
gcloud builds submit --config=cloudbuild-keepers-2.yaml --project=cer-perp-tee-2 --async .

# Step 2 — hot-swap
gcloud compute ssh keepers-vm-2 --project=cer-perp-tee-2 --zone=us-central1-b --tunnel-through-iap --quiet --command="
  IMAGE=us-central1-docker.pkg.dev/cer-perp-tee-2/tee-match-repo/keepers:latest
  TOKEN=\$(curl -sf 'http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token' -H 'Metadata-Flavor: Google' | python3 -c 'import sys,json; print(json.load(sys.stdin)[\"access_token\"])')
  echo \$TOKEN | sudo docker login -u oauth2accesstoken --password-stdin us-central1-docker.pkg.dev
  sudo docker pull \$IMAGE
  sudo docker stop keepers 2>/dev/null || true
  sudo docker rm keepers 2>/dev/null || true
  ORACLE_SECRET=\$(curl -sf http://metadata.google.internal/computeMetadata/v1/instance/attributes/keeper-env-ORACLE_SECRET -H Metadata-Flavor:Google)
  PERP_ID=\$(curl -sf http://metadata.google.internal/computeMetadata/v1/instance/attributes/keeper-env-PERP_ID -H Metadata-Flavor:Google)
  TEE_ADDR=\$(curl -sf http://metadata.google.internal/computeMetadata/v1/instance/attributes/keeper-env-TEE_ADDR -H Metadata-Flavor:Google)
  sudo docker run -d --name keepers --restart=always \
    -e ORACLE_SECRET=\$ORACLE_SECRET \
    -e SOROBAN_RPC_URL=https://stellar-testnet.g.alchemy.com/v2/FqjaGAy9IMENhdv2i_3UUVDPZnNClYNq \
    \$IMAGE --perp-id \$PERP_ID --tee-addr \$TEE_ADDR
"
```

### Contract Upgrade
```bash
# Build WASM
cd contracts && cargo build --target wasm32v1-none --release -p perp-engine

# Upgrade
STELLAR_SOURCE_SECRET=SCPJ6HPSZJIALU42CPOQVYRHXTQZ3BNYD3DYYVBL7WMTADWKLBYZ7KE7 \
SOROBAN_RPC_URL=https://stellar-testnet.g.alchemy.com/v2/FqjaGAy9IMENhdv2i_3UUVDPZnNClYNq \
./tools/e2e/target/debug/e2e upgrade --perp-id CCT476K37KCWZFXWMXXPUKH2FWJESOJVLMSGS2DKCFZFYSZII42XY4VW

# After any upgrade, reset the TEE account or settle_position breaks:
STELLAR_SOURCE_SECRET=SCPJ6... SOROBAN_RPC_URL=... \
./tools/e2e/target/debug/e2e set-tee-account --perp-id CCT476K37KCWZFXWMXXPUKH2FWJESOJVLMSGS2DKCFZFYSZII42XY4VW
```

### Check Logs
```bash
# TEE
gcloud compute ssh tee-match-2 --project=cer-perp-tee-2 --zone=us-central1-a \
  --tunnel-through-iap --quiet --command="sudo docker logs tee-match --tail=100 2>&1"

# Keepers
gcloud compute ssh keepers-vm-2 --project=cer-perp-tee-2 --zone=us-central1-b \
  --tunnel-through-iap --quiet --command="sudo docker logs keepers --tail=100 2>&1"
```

---

## Key File Map

| What | File |
|---|---|
| Contract (Rust, Soroban) | `contracts/perp-engine/src/lib.rs` |
| TEE server entry, TCP loop | `tools/tee-match/src/serve.rs` |
| TEE HTTP endpoints (Axum) | `tools/tee-match/src/serve.rs` — `pub mod http` at line ~1647 |
| TEE → Stellar relay calls | `tools/tee-match/src/stellar.rs` |
| TEE liquidator thread | `tools/tee-match/src/liquidator.rs` |
| TEE ZK proof generation | `tools/tee-match/src/proof.rs` |
| TEE SQLite DB layer | `tools/tee-match/src/db.rs` |
| CLOB engine | `tools/tee-match/src/engine.rs` |
| e2e CLI (admin tool) | `tools/e2e/src/main.rs` |
| ZK circuits (arkworks groth16) | `tools/rust-circuits/` |
| Market maker | `keepers/src/market_maker.rs` |
| Oracle price fetch (Pyth) | `keepers/src/oracle.rs` |
| Frontend trading panel | `app/app/components/trade/trading-panel.tsx` |
| Positions / Orders panel | `app/app/components/trade/positions-panel.tsx` |
| TEE HTTP client | `app/app/lib/tee-client.ts` |
| Contract TX builders | `app/app/lib/contracts.ts` |
| Position local store | `app/app/lib/positions-store.ts` |

---

## Full TEE HTTP API

The TEE HTTP server (port 9721) is the frontend's interface. All endpoints are under `/tee/` via the Vercel edge rewrite.

### Proof / Commitment Endpoints (called before placing orders)

| Endpoint | Method | Purpose | Time |
|---|---|---|---|
| `/init` | POST | Generate commitment + Groth16 proof (slow path) | ~9s |
| `/fast-init` | POST | Commitment hash only, no proof (used by MM) | <1ms |
| `/commit-proof` | POST | Generate Groth16 commitment proof for existing cmt | ~9s |
| `/cancel-proof` | POST | Generate Groth16 cancel proof + nullifier | ~9s |
| `/note-proof` | POST | Generate Groth16 note-spend proof (for withdraw) | ~9s |
| `/note-cmt` | POST | Fast note commitment hash, no proof | <1ms |

### CLOB / Order Management (TCP path, used internally by keepers)

| Endpoint | Command | Purpose |
|---|---|---|
| TCP `place` | `{"cmd":"place","cmt":"...","order_type":"limit","price":N,"size":N}` | Place limit or market order on CLOB |
| TCP `cancel` | `{"cmd":"cancel","cmt":"..."}` | Remove from CLOB |
| TCP `market` | `{"cmd":"market","cmt":"..."}` | Market order on CLOB |
| TCP `get_market` | `{"cmd":"get_market","asset":N}` | 32-level depth snapshot |

### Relay Endpoints (HTTP, frontend calls these)

| Endpoint | Method | Purpose |
|---|---|---|
| `POST /relay/open-position` | Market order → queue for batch relay → `open_position_from_note` on-chain |
| `POST /relay/place-limit` | Limit order → store in `limit_relay_store` → CLOB insert; relay on fill |
| `POST /relay/open-position-pool` | Pool-based position open (pool-spend path) |
| `POST /relay/cancel-position` | Cancel + ZK note proof + `withdraw_note` (returns tokens to user) |
| `POST /relay/deposit-note` | Queue pre-signed deposit XDR for batch submission |
| `POST /relay/withdraw-to-pool` | Note spend → pool insert |
| `GET /get-market?asset=N` | Order book depth (no auth) |
| `GET /note-amount?cmt=<hex>` | Look up note amount in TEE DB (for settlement claim UI) |
| `GET /relay/position-tx?cmt=<hex>` | Poll for on-chain TX hash after queued relay |

### Batch Relay System

The TEE queues relays in two in-memory `Vec`s:
- `relay_queue` — market order positions
- `deposit_queue` — user deposit TXs

Every **10 seconds** (`RELAY_BATCH_SECS = 10`), both queues are drained, **shuffled** (Fisher-Yates), and submitted to Stellar in random order. This breaks timing correlation between user HTTP requests and on-chain TXs — an observer cannot link a deposit to a position open.

Limit orders use a separate `limit_relay_store` (HashMap): when a limit order fills, the maker's and/or taker's relay params are moved from `limit_relay_store` → `relay_queue` and hit the next batch window.

---

## Position Lifecycle — Complete Flow

### Market Order (user opens a long/short at market price)

```
Frontend                     TEE                           Stellar
──────────────────────────────────────────────────────────────────
1. tee.fastInit(...)  ──►  compute commitment hex (fast)
                           store secrets in SQLite
                      ◄──  { commitment }

2. tee.commitProof(cmt) ──► gen_commitment_proof (~9s)
                        ◄──  { proof }

3. build deposit_note TX (contracts.ts → buildDepositAndPlaceTx)
   user signs TX with wallet

4. submitAndWait(signedXdr) ────────────────────────────────────► deposit_note on-chain
                                                                   (USDC moved to contract)

5. tee.noteProof(amount, secret) ──► gen_note_proof (~9s)
                                 ◄──  { note_cmt, note_null, proof }

6. tee.relayOpenPosition({...}) ──► push to relay_queue
                                ◄──  { ok: true, queued: true }
                                     (returns immediately)

   [10s batch window passes]

   TEE spawns relay_queue flush:
   - relay_open_position:
     a) invoke place_order on orderbook contract ──────────────► place_order confirmed
     b) invoke open_position_from_note on perp ───────────────► open_position confirmed
     c) store PositionState in SQLite
     d) store position_tx (hash) in SQLite

7. Frontend polls: tee.pollPositionTx(cmt) ──► GET /relay/position-tx?cmt=...
   Returns tx_hash once relay fires.
```

### Limit Order (user places limit, waits for fill)

```
1. Same init + commitProof as market order above.

2. tee.noteProof(...) for note spending.

3. tee.relayLimitOrder({...})
   ──► TEE stores PendingRelay in limit_relay_store[cmt]
   ──► TEE inserts order into CLOB as engine::OrderType::Limit

4. CLOB matches when counter-order arrives:
   - MM maker fills: taker PendingRelay → relay_queue (MM has no PendingRelay)
   - User maker fills: both maker + taker PendingRelay → relay_queue

5. Batch window fires → both positions opened on-chain simultaneously.
   (User's position stays "pending" in positionsStore until tx_hash appears)
```

### Position Close (user closes open position)

```
1. tee.cancelProof(cmt)  ──►  gen_cancel_proof (~9s)
                         ◄──   { proof, nullifier }

2. tee.relayCancel({perp, position_cmt, cancel_nullifier, cancel_proof, recipient})
   ──► TEE (blocking, not queued):
       a. look up PositionState in SQLite → get collateral
       b. generate fresh note: (note_cmt, note_null) = compute_note_cmt_hex(collateral, secret)
       c. invoke cancel_position_to_note on perp engine ──────► marks position Cancelled
       d. gen_note_proof(collateral, secret) (~9s)
       e. invoke withdraw_note on perp engine ─────────────────► USDC sent to recipient
   ◄──  { tx_hash }
```

### Liquidation (liquidator thread, runs in TEE process)

The liquidator thread (`tools/tee-match/src/liquidator.rs`) scans every entry in SQLite with prefix `pos_` every N seconds (configured via `--liquidator-interval`):

```
For each position:
  1. look up PositionState in TEE DB
  2. fetch oracle price from Pyth Hermes (https://hermes.pyth.network/v2/updates/price/latest)
  3. compute PnL = notional * (oracle_price - entry_price) / entry_price  [long]
                  or notional * (entry_price - oracle_price) / entry_price [short]
  4. settlement = effective_collateral + pnl
  5. maintenance_margin = effective_collateral * 5% (MAINTENANCE_MARGIN_BPS = 500)
  
  if settlement >= maintenance_margin → solvent, skip
  
  if partially_liq_done == false AND settlement > 0:
    → Partial liquidation (Tier 1):
      reward = half_collateral * 1%
      relay_settle_partial(perp, cmt, new_settlement_commitment, reward_note, reward, blinding)
      update PositionState: effective_collateral -= half, partial_liq_done = true
      store reward NoteAmount in SQLite
  else:
    → Full liquidation (Tier 2):
      base_reward = effective_collateral * 1.5%
      ins_fee = effective_collateral * 0.5%
      relay_settle_position(perp, cmt, status=4, settlement_note, to_note, ...)
      zero out PositionState in SQLite
      store settlement NoteAmount in SQLite
```

**Key bug fixed**: `require_tee_auth` in the contract checks the stored TEE account. The TEE signing key (`STELLAR_SOURCE_SECRET`) must equal the registered pubkey. Run `set-tee-account` if auth fails.

---

## TEE SQLite DB Structure

All persistence lives in `/var/lib/tee-keys/tee-db` (sled key-value store, not raw SQLite despite the name).

| Key prefix | Value type | What it stores |
|---|---|---|
| `sec_<cmt>` | `OrderSecrets` | side, price, size, leverage, asset, nonce, secret, is_market |
| `pos_<cmt>` | `PositionState` | collateral, entry_price, leverage, side, effective_collateral, partial_liq_done, asset_id |
| `note_<cmt>` | `NoteAmount` | amount (i128), blinding ([u8;32]) |
| `tx_<cmt>` | String | on-chain TX hash after relay fires |
| `book_<asset>` | Serialized OrderBook | CLOB state, persisted on every write |
| `fill_<id>` | FillRecord | audit trail: maker, taker, price, size, status |

`PositionState` is stored when:
- `relay_open_position` or `relay_open_position_from_pool` succeeds
- Partial liquidation updates effective_collateral
- Full liquidation zeroes it out

`NoteAmount` is stored when:
- `relay_cancel_position` generates a refund note
- `relay_settle_position` generates a settlement note
- `relay_settle_partial` stores a liquidator reward note
- `relay_deposit_note` stores the deposited note

---

## ZK Proof Types

All circuits are in `tools/rust-circuits/` using **arkworks groth16**.

| Proof type | Generated by | Time | Used for |
|---|---|---|---|
| `OrderCommitment` | `proof::gen_commitment_proof` | ~9s | Placing orders (verifies commitment = Poseidon2(side, price, size, leverage, asset, nonce, secret)) |
| `OrderCancel` | `proof::gen_cancel_proof` | ~9s | Cancelling positions (`cancel_position_to_note`) |
| `NoteSpend` | `proof::gen_note_proof` | ~9s | Withdrawing notes (`withdraw_note`), opening positions from notes |
| Fast commitment hash | `proof::compute_commitment_hex` | <1ms | MM pool pre-generation (no ZK proof, just Poseidon2 hash) |
| Fast note commitment | `proof::compute_note_cmt_hex` | <1ms | Quick note creation without proof |

The proving keys (`.pk.bin` files) live in `/var/lib/tee-keys/` on the TEE VM. If they're missing the container starts but all proof calls fail.

---

## Sealed Position Params

When a position is opened, the TEE **encrypts** the order parameters (side, entry_price, leverage, size, tp_price, sl_price, tif, expiry_ledger) into a `BytesN<92>` blob stored on-chain:

- **Dev build** (no `secure` feature): 64-byte plaintext (8 × u64 big-endian)  
- **Secure build** (`secure` feature): 12-byte nonce + 80-byte AES-256-GCM ciphertext  

The DEK is from env var `CER_DEK` (set by GCP Confidential Space launcher after KMS unwrap). This means only the same TEE instance can unseal position params for settlement computation. If `CER_DEK` changes, all existing sealed positions become unreadable to the new TEE — migration procedure doesn't exist yet.

---

## Contract Function Reference

**Contract**: `CCT476K37KCWZFXWMXXPUKH2FWJESOJVLMSGS2DKCFZFYSZII42XY4VW`  
**TEE account (stored in contract)**: `GAZ7LYN2ROIKRVKK4BIL5S4PVRED2YD6YNB4BA5LYB4TSQGN4BZKHTTP`  
**Admin key**: same key  

| Function | Called by | Auth required |
|---|---|---|
| `open_position_from_note(note_cmt, note_null, pos_cmt, sealed_params, liq_note, portfolio_key, asset_id, settlement_cmt, note_proof, commit_proof)` | TEE relay | ZK note-spend + order-commitment proofs |
| `open_position_from_pool(pool_id, pool_root, pool_nullifier, pos_cmt, sealed_params, settlement_cmt, liq_note, portfolio_key, asset_id, spend_proof, commit_proof)` | TEE relay | ZK pool-spend + commit proofs |
| `cancel_position_to_note(pos_cmt, cancel_null, recipient_note, refund_amount, refund_blinding, cancel_proof)` | TEE relay | ZK cancel proof |
| `settle_position(pos_cmt, status, settlement_note, settlement_amount, blinding, reward, ins_delta, bad_debt)` | TEE liquidator | `require_tee_auth` |
| `settle_partial(pos_cmt, new_settlement_cmt, reward_note, reward_amount, reward_blinding)` | TEE liquidator | `require_tee_auth` |
| `deposit_note(from, note_cmt, amount, amount_commitment)` | User (signed TX) | `from.require_auth()` |
| `withdraw_note(note_cmt, note_null, recipient, amount, blinding, note_proof)` | TEE relay | ZK note-spend proof |
| `withdraw_to_pool(pool_id, note_cmt, nullifier, amount, blinding, new_leaf, new_root, remainder_note, remainder_blinding, note_proof, insert_proof)` | TEE relay | ZK note-spend + pool-insert proofs |
| `push_oracle_price(asset_id, price, timestamp)` | Oracle keeper | admin auth |
| `get_position(pos_cmt)` | Anyone (read) | none |
| `get_note(note_cmt)` | Anyone (read) | none |
| `set_tee_account(admin, new_account)` | Admin | `admin.require_auth()` |
| `upgrade(new_wasm_hash)` | Admin | admin auth |

**`require_tee_auth` works because:** `settle_position` and `settle_partial` call `addr.require_auth()` where `addr` is the stored TEE account. Since the TX source IS that account (the TEE signs with `STELLAR_SOURCE_SECRET`), invoker auth passes automatically without an explicit auth entry.

---

## Frontend Data Flow

### Key invariants in the frontend

**`positionsStore`** (`localStorage` key `cerp_positions_v2`): Every opened position is stored locally with:
```typescript
{
  commitment: string  // 64-char hex — TEE-generated, unique per order
  wallet: string      // Stellar pubkey
  symbol: string      // e.g. "BTC-PERP"
  side: 0 | 1        // 0 = long, 1 = short
  leverage: number
  openedAt: number
  entryPrice: number  // 0 until the TX is confirmed on-chain
  collateral: number  // in display USDC units
  size: number        // = collateral * leverage
  orderType?: 'market' | 'limit'
  limitPrice?: number // only for limit orders
}
```

**`getPosition(cmt)`** in `contracts.ts`: Simulates `get_position` on the Soroban RPC. Returns:
- `PositionMeta` — if on-chain entry found (status, createdAt, partialLiqDone, assetId)
- `POSITION_NOT_FOUND` (`'not_found'`) — if contract panics (position doesn't exist)
- `null` — if the RPC call itself threw an exception

**Critical filter logic** in `positions-panel.tsx`:
```typescript
// Pending limit orders: not yet on-chain → meta is POSITION_NOT_FOUND
const pendingOrders = positions.filter(
  (p) => p.stored.orderType === 'limit' && (p.meta === null || p.meta === POSITION_NOT_FOUND)
)
// Active positions: on-chain and open/matched
const active = positions.filter(
  (p) => p.meta !== POSITION_NOT_FOUND && p.meta !== null && Number(p.meta.status) < 2
)
```

**Position status values** (`PositionStatus` enum in contract):
- `0` = Open
- `1` = Matched  
- `2` = Closed
- `4` = Liquidated  
(Status `3` doesn't exist; the gap is intentional.)

### Environment Variables (Vercel / `.env`)

```
VITE_TEE_URL                — TEE HTTP base (dev: empty, prod: /tee via Vercel rewrite)
VITE_PERP_ENGINE_ID         — CCT476K37KCWZFXWMXXPUKH2FWJESOJVLMSGS2DKCFZFYSZII42XY4VW
VITE_ORDERBOOK_ID           — orderbook contract address
VITE_SHIELDED_POOL_ID       — pool contract address
VITE_COLLATERAL_TOKEN_ID    — USDC SAC address
VITE_SOROBAN_RPC_URL        — Alchemy RPC
VITE_NETWORK_PASSPHRASE     — Test SDF Network ; September 2015
VITE_MINTER_SECRET          — issuer secret for test USDC minting
```

---

## Market Maker Configuration

**7 markets**, **32 levels** per side, **448 active quotes** total.

Spread formula:
- Crypto: level `i` → `(5 + 3*i)` bps from mid
- RWA: level `i` → `(10 + 5*i)` bps from mid

Size formula: `base_size * 1.08^(level-1)` (geometric growth outward)

Refresh trigger: price moves more than **0.5%** from `mid_at_gen` → cancel all, regenerate pool at new mid.

TTL: quotes older than **300s** (5 min) are cancelled on next tick.

Tick interval: **60s** (configurable via `--mm-interval-secs`).

Pool buffer: **2× LEVELS** (64 slots) pre-generated per market so there's always inventory.

**Pyth feed IDs:**

| Market | asset_id | Pyth ID (no 0x) | Base fallback |
|---|---|---|---|
| BTC-PERP | 0 | `e62df6c8b4a85fe1a67db44dc12de5db330f7ac66b72dc658afedf0f4a415b43` | $61,000 |
| XRP-PERP | 1 | `ec5d399846a9209f3fe5881d70aae9268c94339ff9817e8d18ff19fa05eea1c8` | $1.12 |
| XLM-PERP | 2 | `b7a8eba68a997cd0210c2e1e4ee811ad2d174b3611c22d9ebf16f4cb7e9ba850` | $0.11 |
| SPACEX-PERP | 3 | *(none)* | $350 |
| TSLA-PERP | 4 | `16dad506d7db8da01c87581c87ca897a012a153557d4d578c3b9c9e1bc0632f1` | $390 |
| OIL-PERP | 5 | `925ca92ff005ae943c158e3563f59698ce7e75c5a8c8dd43303a0a154887b3e6` | $70 |
| GOLD-PERP | 6 | `765d2ba906dbc32ca17cc11f5310a89e9ee1f6420508c63861f2f8ba4ee34bb2` | $4,179 |

Pyth prices are fetched from `https://hermes.pyth.network/v2/updates/price/latest?ids[]=<id>` on every MM tick and normalized to the contract's 7-decimal scale (1e7 = $1.00).

---

## What Works

| Feature | Status |
|---|---|
| Market orders (end-to-end) | Working |
| Limit orders + CLOB matching | Working |
| Orders tab showing pending limits | Working |
| Close / cancel position | Working |
| Market maker (live Pyth prices) | Working — 448 quotes |
| Full liquidation | Working — TEE auth fixed |
| Partial liquidation | Working |
| Settlement after match | Working |
| Order book live price display | Working |
| ZK proofs (all three types) | Working |
| Batch relay privacy (10s shuffle) | Working |

---

## What's Incomplete

### 1. Settlement Withdrawal UI (**highest priority — users can't recover funds**)

After a position is settled (closed by match or liquidated), a `NoteAmount` is stored in the TEE DB keyed by the `settlement_note` hex. The user has **no way to claim it** from the frontend.

**What needs to be built:**

**TEE side** — add endpoint to `serve.rs` `http` module:
```rust
// POST /relay/withdraw-settlement
// Body: { "perp": "...", "position_cmt": "..." }
// 1. look up settlement note from TEE DB via position_cmt
//    (need to store settlement_note hex → position_cmt mapping when relay fires)
// 2. look up NoteAmount {amount, blinding} from DB
// 3. gen_note_proof(keys, amount, secret) — but wait: we don't have the note_secret
//    because settlement notes are generated with rand::random() at settle time, not stored
//    → PROBLEM: need to store note_secret in NoteAmount (add a field)
// OR: use relay_withdraw_note with the already-stored blinding (blinding alone is not enough)
// Correct fix: when storing NoteAmount at settlement, also store the note_secret that was
// used to create the note so it can be proven later.
```

This requires a DB schema change: `NoteAmount` needs a `secret: u64` field. Today it only stores `amount` and `blinding`.

**Current NoteAmount struct** (`tools/tee-match/src/db.rs`):
```rust
pub struct NoteAmount {
    pub amount: i128,
    pub blinding: [u8; 32],
}
```

**Needed**:
```rust
pub struct NoteAmount {
    pub amount: i128,
    pub blinding: [u8; 32],
    pub note_secret: u64,   // to regenerate the ZK proof at claim time
}
```

**Store note_cmt → position_cmt mapping** so the UI can look up "what settlement note does this position have" via the commitment.

**Frontend** — add "Claim" button in `positions-panel.tsx` for `status === 2` or `status === 4`:
```typescript
// In the Positions tab row, when meta.status >= 2:
<button onClick={() => handleClaim(cmt)}>Claim</button>

// handleClaim calls: POST /tee/relay/withdraw-settlement { perp, position_cmt }
// TEE does: look up settlement note → gen_note_proof → relay_withdraw_note → tokens to wallet
```

Add to `tee-client.ts`:
```typescript
async relayWithdrawSettlement(params: { perp: string; position_cmt: string; recipient: string }): Promise<{ tx_hash: string }>
```

---

### 2. Limit Order Cancel (TEE-side CLOB removal)

The Orders tab Cancel button only removes the order from `positionsStore` (local). The order stays in the TEE's CLOB until it fills or TTLs (5 min).

If the user cancels locally but the order fills before TTL, a position opens that the user didn't intend.

**Fix**: Cancel button should call the TEE and remove from CLOB:
- Add `POST /relay/cancel-limit` endpoint to TEE's `http` module
- Handler removes cmt from `limit_relay_store`, calls `book.cancel(cmt)` on the CLOB, persists book
- No on-chain TX needed — limit orders aren't on-chain yet when pending

**Frontend** (`positions-panel.tsx` Orders tab):
```typescript
// Replace current onClick handler:
onClick={() => { positionsStore.remove(cmt); setPositions(...) }}
// With:
onClick={() => handleCancelLimit(cmt)}

// Handler:
async function handleCancelLimit(cmt: string) {
  await fetch(`${TEE_URL}/relay/cancel-limit`, {
    method: 'POST',
    body: JSON.stringify({ position_cmt: cmt }),
  })
  positionsStore.remove(cmt)
  setPositions(p => p.filter(x => x.stored.commitment !== cmt))
}
```

---

### 3. Oracle Keeper (on-chain TWAP, required for settlement)

`keepers/src/main.rs` only spawns the market maker thread. The oracle keeper that pushes Pyth prices to the contract is not implemented. The contract has `push_oracle_price(asset_id, price, timestamp)` and `get_asset_twap(asset_id)`. Without on-chain TWAP, the liquidator uses TEE-local Pyth prices (which it already fetches) but the contract cannot verify settlement amounts independently.

**Add to `keepers/src/main.rs`**:
```rust
// Oracle keeper thread
let oracle_perp = cli.perp_id.clone();
thread::spawn(move || {
    loop {
        for market in MARKETS {
            if market.pyth_id.is_empty() { continue; }
            if let Ok(prices) = oracle::fetch(&[market.pyth_id]) {
                if let Some(p) = prices.get(market.pyth_id) {
                    let asset_id_hex = format!("{:0>64x}", market.asset_id);
                    let _ = push_oracle_price(&oracle_perp, &asset_id_hex, p.scaled);
                }
            }
        }
        thread::sleep(Duration::from_secs(30));
    }
});
```

`push_oracle_price` needs to be implemented in `keepers/` as a Soroban RPC call using the existing `e2e::soroban_rpc::SorobanRpc`.

---

### 4. Trade History Tab

Currently shows "Trade history coming soon." Already have data — just render it.

**File**: `app/app/components/trade/positions-panel.tsx`, the `tab === 'Trades'` block at line 301.

Closed positions are in `positionsStore` (all positions) but `active` filters them to `status < 2`. Filter the other way:

```typescript
const closedPositions = positions.filter(
  (p) => p.meta !== POSITION_NOT_FOUND && p.meta !== null && Number(p.meta.status) >= 2
)
```

Render them in a table with entry price, close status (Closed/Liquidated), size, collateral.

---

### 5. Position Entry Price Missing

Open positions show `—` for entry price because `stored.entryPrice` is always `0`. The entry price is the fill price from the relay response.

For **market orders**: the relay is queued and returns immediately (`{ ok: true, queued: true }`), so fill price isn't available at open time. Poll `GET /relay/position-tx?cmt=...` until the TX appears, then call `getPosition` which has the on-chain settlement_commitment — but entry price isn't stored on-chain.

**Best fix**: Have the TEE return the entry price when the position is actually opened. The relay batch fires and stores `position_tx`. Add an endpoint `GET /relay/position-meta?cmt=...` that returns `{ tx_hash, entry_price, filled_size }` from the TEE DB `PositionState` (which stores `matched_price`).

---

### 6. Deposit UI (no USDC deposit screen)

Users can't fund positions via the UI. The smart contract `deposit_note` function is implemented and `buildDepositNoteTx` exists in `contracts.ts`. There's just no deposit screen.

**What needs to be built:**
1. UI: "Deposit" button → input amount → approve USDC → call `tee.noteCmt(amount, secret)` → `buildDepositNoteTx(...)` → user signs → `submitAndWait(signedXdr)` → `tee.relayDepositNote(signedXdr)` (queues for batch)
2. The note's `amount` and `secret` must be persisted locally (localStorage) keyed by `note_cmt` so the user can spend it later to open a position

---

### 7. Cloud Build Auto Hot-Swap (IAP auth fails)

After any `gcloud builds submit`, the SSH step in the YAML always fails with `[4033: 'not authorized']` when run by the Cloud Build service account, even with `roles/iap.tunnelResourceAccessor` granted.

**Best long-term fix:** Install a cron or Watchtower-like service on each VM:
```bash
# /etc/cron.d/tee-autoupdate
*/5 * * * * root /usr/local/bin/check-and-update-tee.sh
```

Where the script polls the Artifact Registry digest and restarts if it changed. This eliminates the IAP SSH dependency from Cloud Build entirely.

---

## Common Bugs and How to Fix Them

### `settle_position` or `settle_partial` TX fails (auth error)

**Cause**: The contract's stored TEE account doesn't match `STELLAR_SOURCE_SECRET`.  
**Check**: Look at TEE logs for "auth" in the error message.  
**Fix**: Build the e2e tool and run:
```bash
STELLAR_SOURCE_SECRET=SCPJ6... SOROBAN_RPC_URL=... \
./tools/e2e/target/debug/e2e set-tee-account --perp-id CCT476K37...
```
This updates the on-chain TEE account to match the current signing key.

### Market maker pool drains / `active=0`

**Symptom**: Order book shows 0 quotes.  
**Check**: `sudo docker logs keepers --tail=200` — look for `fast-init` or `place` errors.  
**Cause A**: TEE unreachable — keepers can't connect to port 9720.  
**Cause B**: TEE is overloaded with proof generation — fast-init queue backed up.  
**Fix**: Restart keepers after confirming TEE is up. The pool regenerates on next tick.

### Order book prices far from market

**Symptom**: Spread is $1,000+ away from actual market.  
**Cause**: Pyth fetch failing — MM falls back to `base_price` (hardcoded).  
**Check**: Keepers logs for Hermes API errors (`hermes.pyth.network`).  
**Fix**: If Pyth is temporarily down, prices recover on next tick (60s). If persistent, the Hermes URL may have changed — update `keepers/src/oracle.rs`.

### Frontend shows positions but Orders tab empty

**Symptom**: Limit orders placed but not showing in Orders tab.  
**Fix already applied**: `positions-panel.tsx` line 129 checks both `=== null` and `=== POSITION_NOT_FOUND`. If this regresses, confirm the `POSITION_NOT_FOUND` import is at the top of the file.

### TEE container not starting

**Check**:
```bash
sudo docker logs tee-match 2>&1 | head -50
```
**Cause A**: ZK proving keys missing — `/var/lib/tee-keys/` lacks `*.pk.bin` files. The container starts but all `init` calls fail.  
**Cause B**: SQLite DB locked — previous container didn't stop cleanly.  
**Fix B**: `sudo fuser -k /var/lib/tee-keys/tee-db` then restart container.

### Vercel deploy fails

**Symptom**: "No Output Directory named 'client' found"  
**Cause**: Someone used `vercel --prod` or `vercel deploy` CLI.  
**Fix**: Only deploy via `git push origin x`. Never use Vercel CLI for this project.

### Contract simulation fails with "host function failure"

**Symptom**: `getPosition` returns `POSITION_NOT_FOUND` for a position you know exists.  
**Cause A**: Position was never on-chain (batch relay hasn't fired yet). Wait up to 10s.  
**Cause B**: Wrong contract ID in `VITE_PERP_ENGINE_ID`.  
**Cause C**: Commitment hex has leading zeros stripped — always use 64-char hex, zero-padded.

---

## Important Invariants — Never Break These

1. **Git branch**: All commits go to `x`. Never push to `main`.
2. **`STELLAR_SOURCE_SECRET` = `STELLAR_RELAYER_SECRET`** on the production VM. Same key.
3. **`require_tee_auth`** works via invoker auth — the TX source IS the TEE account. If `STELLAR_SOURCE_SECRET` ever changes, run `set-tee-account` before any settlement/liquidation attempt.
4. **ZK proof generation takes ~9s per proof**. Market orders require two proofs (commitment + note-spend) so the UI takes ~18s from init to relay queued. This is expected.
5. **Batch relay queue** introduces up to 10s delay between HTTP response and on-chain TX. `pollPositionTx` polls until the TX appears.
6. **`POSITION_NOT_FOUND`** is the string `'not_found'`, not `null`. These are two distinct states. Never conflate them.
7. **Pending limit orders** are never on-chain until they fill. They exist only in the TEE's `limit_relay_store` and the user's `localStorage`.
8. **TEE DB volume** (`/var/lib/tee-keys/`) must be mounted with `-v /var/lib/tee-keys:/keys` on every container restart. Without it, all position secrets are lost and positions become unrecoverable.
9. **Sealed position params** use `CER_DEK` for encryption in the `secure` build. If the DEK changes, existing sealed positions can't be decrypted — no migration path exists yet.
10. **`note_amount_commitment`** on-chain = `SHA256(amount_le_16bytes || blinding_32bytes)`. The blinding must be kept secret (TEE-only); the amount is revealed only to the TEE when spending.
11. **Commit rule**: No Co-Authored-By lines. No VM IP addresses in commit message text.
