# GCP Confidential Space TEE Hardening

## Architecture

```
┌─────────────────────────────────────────────────────┐
│  Client (User browser / CLI)                        │
│  ┌──────────────────────────────────────────────┐   │
│  │ 1. Connect TLS to TEE Server                 │   │
│  │ 2. Request attestation token with            │   │
│  │    nonce = SHA256(TLS EKM)                   │   │
│  │ 3. Verify OIDC token:                        │   │
│  │    a. RS256 signature (Google JWKS)          │   │
│  │    b. hwmodel == GCP_AMD_SEV_SNP             │   │
│  │    c. dbgstat == disabled-since-boot         │   │
│  │    d. secboot == true                        │   │
│  │    e. image_digest == expected hash          │   │
│  │    f. eat_nonce matches TLS EKM              │   │
│  │ 4. Encrypt order params with session key     │   │
│  │ 5. Send encrypted order to TEE               │   │
│  └──────────────────────────────────────────────┘   │
└──────────────────────┬──────────────────────────────┘
                       │ TLS + channel binding
                       ▼
┌─────────────────────────────────────────────────────┐
│  GCP Confidential Space                              │
│  ┌──────────────────────────────────────────────┐   │
│  │  TEE Server (container)                      │   │
│  │                                              │   │
│  │  On boot:                                    │   │
│  │  1. Get attestation from GCA                 │   │
│  │  2. Unwrap DB key from Cloud KMS             │   │
│  │     (KMS access gated by WIP attestation     │   │
│  │      policy matching image_digest)           │   │
│  │  3. Start TLS listener                       │   │
│  │                                              │   │
│  │  On request:                                 │   │
│  │  4. Decrypt order inside enclave memory      │   │
│  │  5. Match, generate proof                    │   │
│  │  6. Submit to Stellar                        │   │
│  │  7. Encrypted sled DB persistence            │   │
│  └──────────────────────────────────────────────┘   │
│                                                      │
│  vTPM ─── evidence ──→ Google Cloud Attestation      │
│                          │ returns OIDC JWT          │
│                          │ signed by Google           │
└─────────────────────────────────────────────────────┘
```

## Attestation Flow (detail)

```
Client                          TEE Server                     GCA
  │                                  │                          │
  │── TLS handshake ────────────────→│                          │
  │←─────────────────────── TLS ─────│                          │
  │── GET /attestation?nonce=EKM ──→│                          │
  │                                  │── evidence + nonce ────→│
  │                                  │←── OIDC JWT ────────────│
  │←────────── OIDC JWT ─────────────│                          │
  │                                  │                          │
  │ Verify:                          │                          │
  │  1. Google JWKS signature        │                          │
  │  2. hwmodel = GCP_AMD_SEV_SNP   │                          │
  │  3. dbgstat = disabled-since-boot│                          │
  │  4. secboot = true               │                          │
  │  5. image_digest = expected      │                          │
  │  6. eat_nonce == EKM             │                          │
  │                                  │                          │
  │── encrypt(order, session_key) ──→│                          │
  │                                  │ decrypt inside enclave   │
  │                                  │ match, prove, submit     │
```

## Key Management

```
Cloud KMS CMK (symmetric AES-256)
  │
  ├── Access controlled by WIP attestation policy:
  │     assertion.submods.container.image_digest == <hash>
  │     AND assertion.hwmodel == "GCP_AMD_SEV_SNP"
  │     AND assertion.dbgstat == "disabled-since-boot"
  │
  └── Wraps DB Encryption Key (DEK) — AES-256-GCM
        │
        └── Encrypts all sled tree values:
              - "secrets" tree (OrderSecrets)
              - "book" tree (serialized OrderBook)
              - "fills" tree (FillEntry)
```

## Flow: End to End (User Places Order)

1. User generates note proof locally (client-side WASM prover — future)
2. User connects TLS to TEE Server
3. User requests attestation token with nonce = SHA256(TLS EKM)
4. TEE Server proxies to GCA with evidence + nonce, returns OIDC JWT
5. User verifies attestation token
6. User encrypts order side/price/size/leverage/secret with TLS session key
7. TEE Server decrypts inside enclave, generates commitment proof
8. TEE Server stores encrypted OrderSecrets in encrypted sled DB
9. TEE Server returns commitment hash to user
10. User submits commitment proof to orderbook contract
11. TEE Server matches, generates match proof, submits on-chain

## Contract Changes Summary

### Remove Address from On-Chain State

| File | Change |
|---|---|
| `types/src/lib.rs:80` | `OrderMeta.owner: Address` → remove |
| `orderbook/src/lib.rs:93` | `place_order(owner, ...)` → remove `owner` param, no `require_auth()` |
| `orderbook/src/lib.rs:164` | `cancel_order(owner, ...)` → remove `owner` param, no `require_auth()` |
| `perp-engine/src/lib.rs:145` | `PositionMeta.owner: Address` → remove |
| `perp-engine/src/lib.rs:164` | `PositionMeta.from_note: bool` → remove |
| `perp-engine/src/lib.rs:384` | `open_position(owner, ...)` → remove `owner` param, no `require_auth()` |
| `perp-engine/src/lib.rs:496` | `cancel_position(owner, ...)` → remove, folded into `cancel_position_to_note` |
| `perp-engine/src/lib.rs:688` | `close_position(owner, ...)` → remove, folded into `close_position_to_note` |
| `perp-engine/src/lib.rs:1119` | `open_position_from_note(..., liquidation_recipient)` → remove `liquidation_recipient` |
| `perp-engine/src/lib.rs:1267` | Remove `from_note` guard from `cancel_position_to_note` |
| `perp-engine/src/lib.rs:1342` | Remove `from_note` guard from `close_position_to_note` |
| `perp-engine/src/liquidate` | Replace `meta.owner` paths with note-only paths |
| All events | Strip `owner`/`Address` from event payloads |

## Code Changes Needed

### Phase 1: Contract Privacy Refactor (Remove Address Leaks)

**File: `contracts/types/src/lib.rs`**
- Remove `owner: Address` from `OrderMeta`

**File: `contracts/orderbook/src/lib.rs`**
- `place_order`: Remove `owner` param; only verify commitment proof
- `cancel_order`: Remove `owner` param; only verify cancel proof + nullifier
- Events: Remove `owner` from `place` and `cancel` events
- Remove `owner` from all storage (`OrderMeta.owner` gone)

**File: `contracts/perp-engine/src/lib.rs`**
- `PositionMeta`: Remove `owner`, `from_note`
- `open_position`: Remove `owner` param, `require_auth()`, `owner` from event
- `cancel_position`: Remove entirely (use `cancel_position_to_note` only)
- `close_position`: Remove entirely (use `close_position_to_note` only)
- `open_position_from_note`: Remove `liquidation_recipient: Address` param; proceeds always go to `liquidation_recipient_note`
- `cancel_position_to_note`: Remove `from_note` guard
- `close_position_to_note`: Remove `from_note` guard
- `liquidate`: Replace `meta.owner` with note-only path; remove vault fallback
- `pay_liquidator_reward`: Take pinned collateral instead of `owner` — reward taken from protocol funds
- `pay_liquidation_proceeds`: Remove `owner` param, always use note path
- `settle_pair`: Remove `owner` params; settlement always goes through notes
- Remove `credit_user_collateral` / `debit_user_collateral` (no more public balance)
- All events: Remove `Address` payloads; use only commitments/nullifiers

### Phase 2: TEE Server — GCP Confidential Space Integration

**New file: `tools/tee-match/src/attestation.rs`**
- `request_token(audience, nonces)` — HTTP call to local Confidential Space API
- `verify_token(token, expected_nonce, expected_digest)` — RS256 validation, claim checks
- Endpoint handler `GET /attestation` — returns token bound to TLS EKM

**New file: `tools/tee-match/src/crypto.rs`**
- `derive_session_key(tls_ekm)` — HKDF-SHA256 from TLS exported keying material
- `encrypt_with_session_key(key, plaintext)` — AES-256-GCM
- `decrypt_with_session_key(key, ciphertext)` — AES-256-GCM
- `generate_dek()` — random 32-byte DEK
- `encrypt_dek_with_kms(dek, kms_key_id)` — wrap DEK with Cloud KMS
- `decrypt_dek_with_kms(wrapped_dek)` — unwrap DEK via Cloud KMS

**New file: `tools/tee-match/src/kms.rs`**
- `gcp_kms_client(attestation_token)` — authenticated KMS client via workload identity
- `unwrap_key(ciphertext_dek, attestation)` — unwrap with CMK
- `wrap_key(plaintext_dek, attestation)` — wrap with CMK

**Rewrite: `tools/tee-match/src/serve.rs`**
- Replace TCP `TcpListener` with TLS (warp/axum + rustls)
- Routes:
  - `GET /attestation?nonce=<base64>` — proxy to GCA, return OIDC JWT
  - `POST /init` — receive encrypted order, decrypt, generate commitment
  - `POST /cancel` — same, encrypted
  - `POST /match` — existing match logic (TEE-initiated, not client-facing)
  - `GET /market?asset=N` — public market data (no encryption needed)
- Decrypt all incoming order parameters inside request handlers
- Zeroize plaintext after proof generation

**Modify: `tools/tee-match/src/db.rs`**
- All `insert`/`get` operations wrap values with AEAD encrypt/decrypt
- On startup: unwrap DEK from KMS, hold in memory
- On shutdown: zeroize DEK

**Modify: `tools/tee-match/Cargo.toml`**
- Add: `tokio`, `axum`, `rustls`, `pem`, `rcgen`, `reqwest`, `ring`, `jsonwebtoken`, `base64`, `aes-gcm`, `hkdf`, `zeroize`

### Phase 3: Client SDK

**New file: `tools/e2e/src/tls_client.rs`** (or `tools/sdk/`)
- TLS connection to TEE server with certificate validation
- `request_attestation(connection, nonce)` — fetch and verify OIDC token
- `verify_attestation_token(token)` — check Google signature, hwmodel, dbgstat, secboot, image_digest, nonce
- `encrypt_order(order_params, session_key)` — AEAD encrypt
- `decrypt_response(ciphertext, session_key)` — AEAD decrypt

**Modify: `tools/e2e/src/client.rs`**
- Use TLS client instead of TCP
- Add attestation verification step before any order submission
- Encrypt order parameters before sending to `/init`

### Phase 4: Infrastructure

**New file: `Dockerfile.tee-match`**
- Multi-stage Rust build
- Drop root, non-privileged user
- No shell, no package manager
- COPY only the compiled binary + proving keys

**New file: `deploy/confidential-space.yaml`**
- Container image reference
- Attestation service: `google_cloud_attestation`
- KMS key reference
- WIP configuration

**New file: `deploy/setup.sh`**
- Create Cloud KMS CMK
- Create workload identity pool + provider (GCA as OIDC)
- Bind KMS decrypt permission to WIP with attestation policy
- Build and push container to Artifact Registry

### Phase 5: CI/CD

**Modify: `.github/workflows/ci.yml`**
- Add Docker build step
- Push to Artifact Registry on merge to main
- Smoke test against testnet

## Security Properties

After these changes:

| Threat | Mitigation |
|---|---|
| Operator steals order secrets | Encrypted at rest (AEAD) + in-memory only inside SEV-SNP enclave |
| Malicious operator replaces binary | Attestation token includes image_digest; client verifies it |
| Cloud provider/hypervisor reads memory | AMD SEV-SNP memory encryption |
| Replay attack | Nonce bound to TLS session via EKM |
| Man-in-the-middle | TLS + attestation channel binding |
| On-chain privacy leak | No Address stored on-chain; only commitments/nullifiers |
| Liquidator steals funds | Liquidation proceeds always go to pre-committed note |
| Operator matches orders unfairly | Match circuit enforces crossing prices, opposite sides, same asset — publicly verifiable |

## Circuits & ZK Flow (No Changes Needed)

The four existing circuits are unchanged:

| Circuit | Public Inputs | Private Witnesses | Purpose |
|---|---|---|---|
| Commitment | `commitment, portfolio_key` | 9 fields + secret | Prove order params hash to commitment |
| Match | `cmt_a, cmt_b, match_price, match_size, nullifier_a, nullifier_b` | Both orders' 17 fields | Prove orders cross correctly |
| Cancel | `nullifier` | commitment + secret | Prove right to cancel |
| NoteSpend | `note_commitment, nullifier` | amount + secret | Prove ownership of shielded note |

**Why no changes needed:**
- The circuits already prove correctness without leaking order secrets to the chain
- The privacy gain comes from the encryption/attestation wrapper, not circuit redesign
- Match price/size remain public (needed for on-chain settlement) — this is the "dark pool" model where fills are public but the order book is private

**ZK Flow (GCP Confidential Space):**
```
1. Client connects TLS → verifies attestation (hwmodel, dbgstat, image_digest, nonce)
2. Client encrypts order params with session key → sends to TEE
3. TEE decrypts inside enclave → generates Groth16 commitment proof
4. TEE submits commitment proof + commitment to orderbook contract
5. TEE decrypts both orders at match time → generates Groth16 match proof
6. TEE submits match proof to perp-engine contract
7. TEE encrypts OrderSecrets → stores in encrypted sled DB
8. On restart: TEE unwraps DB key from KMS → decrypts persistence
```

**Proving keys** live inside the enclave — guaranteed by container image attestation. The TEE is the sole prover; users verify the attestation then trust the proofs (which are independently verifiable on-chain).

## Comparison with Opal Exchange

[Opal Exchange](https://docs.opaldex.com) uses a similar stack (TEE enclave + ZK proofs + client-side encryption) but differs in matching model:

| Feature | Opal Exchange | CER Perp (Ours) |
|---|---|---|
| Matching | Frequent batch auctions, uniform clearing price | Continuous CLOB, price-time priority |
| Order submission | Encrypted client-side → ciphertext to TEE | Encrypted client-side → ciphertext to TEE |
| Proofs | Post-trade batch-level ZK proofs | Per-match Groth16 proofs |
| Trust model | TEE enclave + ZK (+ future MPC) | GCP Confidential Space + ZK |
| Settlement | On-chain or state commitments | On-chain Soroban contracts |

Opal uses "sealed order flow" where orders are encrypted on the client and only decrypted inside the enclave at batch close — identical to our planned encryption flow. Their batch-level approach (one proof per batch) is more gas-efficient but delays execution.

## Migration Path

1. Deploy new contracts (no `owner`, note-only paths)
2. Deploy TEE server in GCP Confidential Space
3. Users deposit to notes (not public balance)
4. Old `open_position`/`close_position`/`cancel_position` → remove from SDK
5. Remove public balance functions (`deposit`/`withdraw` with Address — keep only `deposit_note`/`withdraw_note`)

## Open Questions

1. **Cross-collateral vault**: The current vault (`CollateralVault`) uses `Address`. Need a note-compatible vault or internal accounting
2. **Liquidator reward**: Currently taken from `owner`'s locked collateral. Without `owner`, reward must come from protocol/insurance fund
3. **Oracle**: `set_price` uses `admin.require_auth()` — no change needed (oracle admin is a trusted role)
4. **Match settlement (settle_match)**: Currently uses `owner` for settlement — needs note path
5. **Pre-existing positions**: Old positions with `owner` set — hard fork or migration contract needed
