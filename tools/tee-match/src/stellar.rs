use crate::log;
use crate::proof::MatchProof;
use anyhow::Result;
use e2e::soroban_rpc::{scval_address, scval_bytes, scval_bytes32, scval_i128, scval_proof, scval_u32, scval_u64, SorobanRpc};
use std::time::Instant;

/// Returns the signing identity: raw secret key from env if available, else named identity.
fn signing_source(fallback: &str) -> String {
    std::env::var("STELLAR_SOURCE_SECRET").unwrap_or_else(|_| fallback.to_string())
}

pub fn submit_cancel(
    orderbook_id: &str,
    _perp_id: &str,
    _owner: &str,
    commitment: &str,
    nullifier: &str,
    proof: &MatchProof,
    source: &str,
) -> Result<()> {
    let start = Instant::now();
    let proof_json = serde_json::json!({
        "a": proof.proof.a,
        "b": proof.proof.b,
        "c": proof.proof.c,
    })
    .to_string();

    let src = signing_source(source);
    log::debug!("Submitting cancel_order via RPC", "orderbook", &orderbook_id[..8], "cmt", &commitment[..16]);

    let rpc = SorobanRpc::new();
    rpc.invoke_xdr(orderbook_id, &src, "cancel_order", vec![
        scval_bytes32(commitment)?,
        scval_bytes32(nullifier)?,
        scval_proof(&proof_json)?,
    ])?;

    log::info!("Cancel order submitted on-chain via RPC", "orderbook", &orderbook_id[..8], "cmt", &commitment[..16], "nullifier", &nullifier[..16], "took", log::duration_secs(&start.elapsed()));
    Ok(())
}

/// Seal position params with AES-256-GCM (secure build) or plain packing (dev build).
/// Plaintext layout (64 bytes, all u64 big-endian):
///   side(8) | entry_price(8) | leverage(8) | size(8) | tp_price(8) | sl_price(8) | tif(8) | expiry(8)
/// Output (secure): 12-byte nonce || 80-byte ciphertext+tag = 92 bytes.
/// Output (dev):    64-byte plaintext (no encryption — dev mode only).
fn seal_position_params(
    side: u64, entry_price: u64, leverage: u64, size: u64,
    tp_price: u64, sl_price: u64, tif: u64, expiry_ledger: u64,
) -> Result<Vec<u8>> {
    let mut plaintext = [0u8; 64];
    plaintext[0..8].copy_from_slice(&side.to_be_bytes());
    plaintext[8..16].copy_from_slice(&entry_price.to_be_bytes());
    plaintext[16..24].copy_from_slice(&leverage.to_be_bytes());
    plaintext[24..32].copy_from_slice(&size.to_be_bytes());
    plaintext[32..40].copy_from_slice(&tp_price.to_be_bytes());
    plaintext[40..48].copy_from_slice(&sl_price.to_be_bytes());
    plaintext[48..56].copy_from_slice(&tif.to_be_bytes());
    plaintext[56..64].copy_from_slice(&expiry_ledger.to_be_bytes());

    #[cfg(feature = "secure")]
    {
        let dek_hex = std::env::var("CER_DEK").unwrap_or_else(|_| "0".repeat(64));
        let mut dek = [0u8; 32];
        hex::decode_to_slice(&dek_hex, &mut dek)
            .map_err(|e| anyhow::anyhow!("CER_DEK invalid hex: {e}"))?;
        let payload = crate::crypto::encrypt(&dek, &plaintext)?;
        let mut out = Vec::with_capacity(12 + payload.ciphertext.len());
        out.extend_from_slice(&payload.nonce);
        out.extend_from_slice(&payload.ciphertext);
        return Ok(out);
    }
    #[cfg(not(feature = "secure"))]
    Ok(plaintext.to_vec())
}

/// Unseal position params: reverse of seal_position_params.
/// Input: sealed blob from contract storage.
/// Output: (side, entry_price, leverage, size, tp_price, sl_price, tif, expiry_ledger).
pub fn unseal_position_params(sealed: &[u8]) -> Result<(u64, u64, u64, u64, u64, u64, u64, u64)> {
    #[cfg(feature = "secure")]
    {
        let dek_hex = std::env::var("CER_DEK").unwrap_or_else(|_| "0".repeat(64));
        let mut dek = [0u8; 32];
        hex::decode_to_slice(&dek_hex, &mut dek)
            .map_err(|e| anyhow::anyhow!("CER_DEK invalid hex: {e}"))?;
        if sealed.len() < 12 {
            anyhow::bail!("unseal: blob too short ({} bytes)", sealed.len());
        }
        let mut nonce = [0u8; 12];
        nonce.copy_from_slice(&sealed[..12]);
        let ct = sealed[12..].to_vec();
        let payload = crate::crypto::EncryptedPayload { nonce, ciphertext: ct };
        let plaintext = crate::crypto::decrypt(&dek, &payload)
            .map_err(|e| anyhow::anyhow!("unseal decrypt failed: {e}"))?;
        if plaintext.len() != 64 {
            anyhow::bail!("unseal: plaintext length mismatch (got {})", plaintext.len());
        }
        let read_u64 = |i: usize| -> u64 {
            let mut b = [0u8; 8];
            b.copy_from_slice(&plaintext[i..i+8]);
            u64::from_be_bytes(b)
        };
        Ok((read_u64(0), read_u64(8), read_u64(16), read_u64(24),
            read_u64(32), read_u64(40), read_u64(48), read_u64(56)))
    }
    #[cfg(not(feature = "secure"))]
    {
        if sealed.len() != 64 {
            anyhow::bail!("unseal: dev mode expects 64 bytes, got {}", sealed.len());
        }
        let read_u64 = |i: usize| -> u64 {
            let mut b = [0u8; 8];
            b.copy_from_slice(&sealed[i..i+8]);
            u64::from_be_bytes(b)
        };
        Ok((read_u64(0), read_u64(8), read_u64(16), read_u64(24),
            read_u64(32), read_u64(40), read_u64(48), read_u64(56)))
    }
}

/// Seal position params from stored order secrets (public wrapper for the HTTP relay handler).
pub fn seal_from_secrets(secrets: &crate::db::OrderSecrets) -> Result<Vec<u8>> {
    seal_position_params(
        secrets.side as u64,
        secrets.price as u64,
        secrets.leverage as u64,
        secrets.size as u64,
        0, 0, 0, 0,
    )
}

pub fn relay_open_position(
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
    store: &crate::db::SecretStore,
) -> Result<String> {
    let start = Instant::now();
    let src = std::env::var("STELLAR_SOURCE_SECRET").unwrap_or_else(|_| "e2e".to_string());
    let zeros = "0".repeat(64);

    log::info!("Relaying place_order", "orderbook", &orderbook_id[..8], "cmt", &position_cmt_hex[..16]);
    let rpc = SorobanRpc::new();
    rpc.invoke_xdr(orderbook_id, &src, "place_order", vec![
        scval_bytes32(position_cmt_hex)?,
        scval_bytes32(portfolio_key_hex)?,
        scval_bytes(&sealed_params)?,           // encrypted_hints
        scval_u64(15),                           // revealed: u64
        scval_u32(0),                            // tif: TimeInForce (#[repr(u32)]) — GTC=0
        scval_u64(0),                            // expiry_ledger: u64
        scval_bytes32(asset_id_hex)?,
        scval_proof(commit_proof_json)?,
    ])?;

    log::info!("Relaying open_position_from_note", "contract", &perp_id[..8], "sealed_len", sealed_params.len());
    let rpc = SorobanRpc::new();
    let tx_hash = rpc.invoke_xdr(perp_id, &src, "open_position_from_note", vec![
        scval_bytes32(note_cmt_hex)?,
        scval_bytes32(note_null_hex)?,
        scval_bytes32(position_cmt_hex)?,
        scval_bytes(&sealed_params)?,
        scval_bytes32(&zeros)?,                  // liquidation_recipient_note
        scval_bytes32(portfolio_key_hex)?,
        scval_bytes32(asset_id_hex)?,
        scval_bytes32(settlement_commitment)?,
        scval_proof(note_proof_json)?,
        scval_proof(commit_proof_json)?,
    ])?;

    // relay_open_position is the authoritative source for position size and collateral.
    // open_position_from_note locked the full note on-chain; fills only determine entry price.
    // We always overwrite collateral and remaining_size so that:
    //   - PnL is computed on the full order notional (correct leverage exposure)
    //   - Settlement returns the full deposited collateral ± PnL
    let (side, _hint_entry, leverage, order_size, _, _, _, _) = unseal_position_params(sealed_params)?;
    let recipient = store.get(position_cmt_hex).ok().flatten().and_then(|s| s.recipient.clone());
    let position_state = match store.get_position_state(position_cmt_hex)? {
        Some(mut s) => {
            s.collateral = collateral_amount;
            s.effective_collateral = collateral_amount;
            // Reset to full order notional so PnL is leveraged correctly.
            s.size = order_size;
            s.remaining_size = order_size;
            if s.recipient.is_none() {
                s.recipient = recipient;
            }
            s
        }
        None => {
            crate::db::PositionState {
                collateral: collateral_amount,
                matched_price: 0,
                funding_at_open: 0,
                effective_collateral: collateral_amount,
                entry_price: 0,
                leverage,
                side,
                partial_liq_done: false,
                asset_id: asset_id_hex.to_string(),
                size: order_size,
                last_funding_index: 0,
                protocol: false,
                remaining_size: order_size,
                asset_num: 0,
                open_time_ns: crate::engine::now_nanos(),
                tp_price: 0,
                sl_price: 0,
                recipient,
            }
        }
    };
    store.insert_position_state(position_cmt_hex, &position_state)?;

    log::info!("Relay open_position complete", "hash", &tx_hash[..16], "took", log::duration_secs(&start.elapsed()));
    Ok(tx_hash)
}

/// Relay a full cancel + withdraw cycle for a position:
/// 1. cancel_position_to_note  — marks position Cancelled, stores a refund note
/// 2. gen_note_proof           — ZK note spend proof for the refund note (~9s)
/// 3. withdraw_note            — redeems the note and sends tokens to `recipient_address`
pub fn relay_cancel_position(
    perp_id: &str,
    position_cmt_hex: &str,
    cancel_nullifier_hex: &str,
    cancel_proof_json: &str,
    recipient_address: &str,   // Stellar account that receives the tokens
    keys_dir: &std::path::Path,
    store: &crate::db::SecretStore,
) -> Result<String> {
    let start = Instant::now();
    let src = signing_source("e2e");

    // 1. Look up collateral from TEE DB
    let pos_state = store.get_position_state(position_cmt_hex)?
        .ok_or_else(|| anyhow::anyhow!("position state not found in TEE DB for {}", &position_cmt_hex[..16]))?;
    let collateral = pos_state.effective_collateral;

    log::info!("Relaying cancel_position_to_note", "cmt", &position_cmt_hex[..16], "collateral", collateral);

    // 2. Generate a fresh note for the refund (Poseidon2 commitment, no proof yet)
    let note_secret: u64 = rand::random();
    let (note_cmt_hex, note_null_hex) = crate::proof::compute_note_cmt_hex(collateral as u64, note_secret);

    // 3. Generate random amount-commitment blinding (SHA256-based, for cancel_position_to_note)
    let note_blinding: [u8; 32] = rand::random();
    let note_blinding_hex = hex::encode(note_blinding);

    // 4. cancel_position_to_note
    let rpc = SorobanRpc::new();
    rpc.invoke_xdr(perp_id, &src, "cancel_position_to_note", vec![
        scval_bytes32(position_cmt_hex)?,
        scval_bytes32(cancel_nullifier_hex)?,
        scval_bytes32(&note_cmt_hex)?,
        scval_i128(collateral),
        scval_bytes32(&note_blinding_hex)?,
        scval_proof(cancel_proof_json)?,
    ])?;
    log::info!("cancel_position_to_note confirmed", "cmt", &position_cmt_hex[..16]);

    // 5. Generate ZK note spend proof for the refund note (~9s)
    let note_proof = crate::proof::gen_note_proof(keys_dir, collateral as u64, note_secret)?;
    let note_proof_json = serde_json::json!({
        "a": note_proof.proof.proof.a,
        "b": note_proof.proof.proof.b,
        "c": note_proof.proof.proof.c,
    }).to_string();

    // 6. withdraw_note → transfers tokens directly to recipient
    log::info!("Relaying withdraw_note", "recipient", &recipient_address[..8], "amount", collateral);
    let rpc2 = SorobanRpc::new();
    let tx_hash = rpc2.invoke_xdr(perp_id, &src, "withdraw_note", vec![
        scval_bytes32(&note_cmt_hex)?,
        scval_bytes32(&note_null_hex)?,
        scval_address(recipient_address)?,
        scval_i128(collateral),
        scval_bytes32(&note_blinding_hex)?,
        scval_proof(&note_proof_json)?,
    ])?;

    log::info!("relay_cancel_position complete", "hash", &tx_hash[..16], "took", log::duration_secs(&start.elapsed()));
    Ok(tx_hash)
}

pub fn relay_settle_position(
    perp_id: &str,
    commitment: &str,
    status: u32,
    settlement_note: &str,
    settlement_amount: i128,
    settlement_blinding: &str,
    reward_amount: i128,
    ins_delta: i128,
    bad_debt: i128,
) -> Result<String> {
    let start = Instant::now();
    let src = signing_source("e2e");
    log::info!("Relaying settle_position", "cmt", &commitment[..16], "status", status);
    let rpc = SorobanRpc::new();
    let tx_hash = rpc.invoke_xdr(perp_id, &src, "settle_position", vec![
        scval_bytes32(commitment)?,
        scval_u32(status),
        scval_bytes32(settlement_note)?,
        scval_i128(settlement_amount),
        scval_bytes32(settlement_blinding)?,
        scval_i128(reward_amount),
        scval_i128(ins_delta),
        scval_i128(bad_debt),
    ])?;
    log::info!("Settle_position relayed", "hash", &tx_hash[..16], "took", log::duration_secs(&start.elapsed()));
    Ok(tx_hash)
}

pub fn relay_open_position_from_pool(
    perp_id: &str,
    orderbook_id: &str,
    pool_id: &str,
    pool_root: &str,
    pool_nullifier_hash: &str,
    position_cmt_hex: &str,
    sealed_params: &[u8],
    collateral_amount: i128,
    collateral_blinding: &str,
    settlement_commitment: &str,
    liquidation_recipient_note: &str,
    portfolio_key_hex: &str,
    asset_id_hex: &str,
    spend_proof_json: &str,
    commit_proof_json: &str,
    store: &crate::db::SecretStore,
) -> Result<String> {
    let start = Instant::now();
    let src = std::env::var("STELLAR_SOURCE_SECRET").unwrap_or_else(|_| "e2e".to_string());
    let zeros = "0".repeat(64);

    log::info!("Relaying place_order (pool path)", "orderbook", &orderbook_id[..8], "cmt", &position_cmt_hex[..16]);
    let rpc = SorobanRpc::new();
    rpc.invoke_xdr(orderbook_id, &src, "place_order", vec![
        scval_bytes32(position_cmt_hex)?,
        scval_bytes32(portfolio_key_hex)?,
        scval_bytes(sealed_params)?,
        scval_u64(15),   // revealed: u64
        scval_u32(0),    // tif: TimeInForce (#[repr(u32)]) — GTC=0
        scval_u64(0),    // expiry_ledger: u64
        scval_bytes32(asset_id_hex)?,
        scval_proof(commit_proof_json)?,
    ])?;

    log::info!("Relaying open_position_from_pool", "perp", &perp_id[..8], "nullifier", &pool_nullifier_hash[..16]);
    let rpc = SorobanRpc::new();
    let liq_note = if liquidation_recipient_note.is_empty() { &zeros } else { liquidation_recipient_note };
    let tx_hash = rpc.invoke_xdr(perp_id, &src, "open_position_from_pool", vec![
        scval_address(pool_id)?,
        scval_bytes32(pool_root)?,
        scval_bytes32(pool_nullifier_hash)?,
        scval_bytes32(position_cmt_hex)?,
        scval_bytes(sealed_params)?,
        scval_bytes32(settlement_commitment)?,
        scval_bytes32(liq_note)?,
        scval_bytes32(portfolio_key_hex)?,
        scval_bytes32(asset_id_hex)?,
        scval_proof(spend_proof_json)?,
        scval_proof(commit_proof_json)?,
    ])?;

    // Same rationale as relay_open_position: always set collateral and remaining_size
    // from the authoritative relay call, not from partial CLOB fills.
    let (side, _hint_entry, leverage, order_size, _, _, _, _) = unseal_position_params(sealed_params)?;
    let recipient = store.get(position_cmt_hex).ok().flatten().and_then(|s| s.recipient.clone());
    let position_state = match store.get_position_state(position_cmt_hex)? {
        Some(mut s) => {
            s.collateral = collateral_amount;
            s.effective_collateral = collateral_amount;
            s.size = order_size;
            s.remaining_size = order_size;
            if s.recipient.is_none() {
                s.recipient = recipient;
            }
            s
        }
        None => {
            crate::db::PositionState {
                collateral: collateral_amount,
                matched_price: 0,
                funding_at_open: 0,
                effective_collateral: collateral_amount,
                entry_price: 0,
                leverage,
                side,
                partial_liq_done: false,
                asset_id: asset_id_hex.to_string(),
                size: order_size,
                last_funding_index: 0,
                protocol: false,
                remaining_size: order_size,
                asset_num: 0,
                open_time_ns: crate::engine::now_nanos(),
                tp_price: 0,
                sl_price: 0,
                recipient,
            }
        }
    };
    store.insert_position_state(position_cmt_hex, &position_state)?;

    log::info!("Pool-based position opened", "hash", &tx_hash[..16], "took", log::duration_secs(&start.elapsed()));
    Ok(tx_hash)
}

pub fn relay_settle_partial(
    perp_id: &str,
    commitment: &str,
    new_settlement_commitment: &str,
    reward_note: &str,
    reward_amount: i128,
    reward_blinding: &str,
) -> Result<String> {
    let start = Instant::now();
    let src = signing_source("e2e");
    log::info!("Relaying settle_partial", "cmt", &commitment[..16], "reward", reward_amount);
    let rpc = SorobanRpc::new();
    let tx_hash = rpc.invoke_xdr(perp_id, &src, "settle_partial", vec![
        scval_bytes32(commitment)?,
        scval_bytes32(new_settlement_commitment)?,
        scval_bytes32(reward_note)?,
        scval_i128(reward_amount),
        scval_bytes32(reward_blinding)?,
    ])?;
    log::info!("Settle_partial relayed", "hash", &tx_hash[..16], "took", log::duration_secs(&start.elapsed()));
    Ok(tx_hash)
}

pub fn relay_withdraw_to_pool(
    perp_id: &str,
    pool_id: &str,
    note_commitment: &str,
    nullifier: &str,
    amount: i128,
    blinding: &str,
    new_pool_leaf: &str,
    new_pool_root: &str,
    remainder_note: &str,
    remainder_blinding: &str,
    note_spend_proof_json: &str,
    pool_insert_proof_json: &str,
) -> Result<String> {
    let start = Instant::now();
    let src = signing_source("e2e");
    log::info!("Relaying withdraw_to_pool", "note_cmt", &note_commitment[..16], "amount", amount);
    let rpc = SorobanRpc::new();
    let tx_hash = rpc.invoke_xdr(perp_id, &src, "withdraw_to_pool", vec![
        scval_address(pool_id)?,
        scval_bytes32(note_commitment)?,
        scval_bytes32(nullifier)?,
        scval_i128(amount),
        scval_bytes32(blinding)?,
        scval_bytes32(new_pool_leaf)?,
        scval_bytes32(new_pool_root)?,
        scval_bytes32(remainder_note)?,
        scval_bytes32(remainder_blinding)?,
        scval_proof(note_spend_proof_json)?,
        scval_proof(pool_insert_proof_json)?,
    ])?;
    log::info!("Withdraw_to_pool relayed", "hash", &tx_hash[..16], "took", log::duration_secs(&start.elapsed()));
    Ok(tx_hash)
}

pub fn relay_deposit_note(
    perp_id: &str,
    from: &str,
    note_commitment: &str,
    amount_commitment: &str,
    store: &crate::db::SecretStore,
    amount: i128,
    blinding: &str,
) -> Result<String> {
    let start = Instant::now();
    let src = signing_source("e2e");
    log::info!("Relaying deposit_note", "note_cmt", &note_commitment[..16]);

    let rpc = SorobanRpc::new();
    let tx_hash = rpc.invoke_xdr(perp_id, &src, "deposit_note", vec![
        scval_address(from)?,
        scval_bytes32(note_commitment)?,
        scval_i128(amount),
        scval_bytes32(amount_commitment)?,
    ])?;

    // Store note amount in TEE DB
    let mut blinding_bytes = [0u8; 32];
    hex::decode_to_slice(blinding, &mut blinding_bytes)?;
    let note_amount = crate::db::NoteAmount {
        amount,
        blinding: blinding_bytes,
        note_secret: 0,
    };
    store.insert_note_amount(note_commitment, &note_amount)?;

    log::info!("Deposit note relayed", "tx_hash", &tx_hash[..16], "took", log::duration_secs(&start.elapsed()));
    Ok(tx_hash)
}

pub fn relay_withdraw_note(
    perp_id: &str,
    note_commitment: &str,
    nullifier: &str,
    recipient: &str,
    amount: i128,
    blinding: &str,
    proof_json: &str,
) -> Result<String> {
    let start = Instant::now();
    let src = signing_source("e2e");
    log::info!("Relaying withdraw_note", "note_cmt", &note_commitment[..16]);

    let rpc = SorobanRpc::new();
    let tx_hash = rpc.invoke_xdr(perp_id, &src, "withdraw_note", vec![
        scval_bytes32(note_commitment)?,
        scval_bytes32(nullifier)?,
        scval_address(recipient)?,
        scval_i128(amount),
        scval_bytes32(blinding)?,
        scval_proof(proof_json)?,
    ])?;

    log::info!("Withdraw note relayed", "tx_hash", &tx_hash[..16], "took", log::duration_secs(&start.elapsed()));
    Ok(tx_hash)
}

pub struct SettlementNote {
    pub note_cmt: String,
    pub note_null: String,
    pub blinding_hex: String,
    pub note_secret: u64,
}

pub fn create_settlement_note(amount: i128) -> SettlementNote {
    let note_secret: u64 = rand::random();
    let (note_cmt, note_null) = crate::proof::compute_note_cmt_hex(amount as u64, note_secret);
    let blinding: [u8; 32] = rand::random();
    let blinding_hex = hex::encode(blinding);
    SettlementNote { note_cmt, note_null, blinding_hex, note_secret }
}
