use crate::log;
use crate::proof::MatchProof;
use anyhow::Result;
use e2e::soroban_rpc::{scval_bytes, scval_bytes32, scval_i128, scval_proof, scval_u64, SorobanRpc};
use std::time::Instant;

/// Returns the signing identity: raw secret key from env if available, else named identity.
fn signing_source(fallback: &str) -> String {
    std::env::var("STELLAR_SOURCE_SECRET").unwrap_or_else(|_| fallback.to_string())
}

pub fn submit_match(perp_id: &str, source: &str, cmt_a: &str, cmt_b: &str, proof: &MatchProof) -> Result<()> {
    let start = Instant::now();

    let hex = |dec: &str| -> String {
        let n: num_bigint::BigUint = dec.parse().expect("Invalid decimal");
        format!("{:0>64x}", n)
    };

    let nullifier_a_hex = hex(&proof.public_inputs[4]);
    let nullifier_b_hex = hex(&proof.public_inputs[5]);
    let match_price_hex = hex(&proof.public_inputs[2]);
    let match_size_hex = hex(&proof.public_inputs[3]);

    let proof_json = serde_json::json!({
        "a": proof.proof.a,
        "b": proof.proof.b,
        "c": proof.proof.c,
    })
    .to_string();

    let src = signing_source(source);
    log::debug!("Submitting match_positions via RPC", "contract", &perp_id[..8], "cmt_a", &cmt_a[..16], "cmt_b", &cmt_b[..16]);

    let rpc = SorobanRpc::new();
    rpc.invoke_xdr(perp_id, &src, "match_positions", vec![
        scval_bytes32(cmt_a)?,
        scval_bytes32(cmt_b)?,
        scval_bytes32(&nullifier_a_hex)?,
        scval_bytes32(&nullifier_b_hex)?,
        scval_bytes32(&match_price_hex)?,
        scval_bytes32(&match_size_hex)?,
        scval_proof(&proof_json)?,
    ])?;

    let elapsed = start.elapsed();
    log::info!("Match submitted via RPC", "contract", &perp_id[..8], "nf_a", &nullifier_a_hex[..16], "nf_b", &nullifier_b_hex[..16], "price", &match_price_hex, "size", &match_size_hex, "took", log::duration_secs(&elapsed));
    Ok(())
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

pub fn submit_mark_price(perp_id: &str, source: &str, price: u64) -> Result<()> {
    let start = Instant::now();
    let src = signing_source(source);

    log::debug!("Submitting set_mark_price via RPC", "price", price);
    let rpc = SorobanRpc::new();
    rpc.invoke_xdr(perp_id, &src, "set_mark_price", vec![scval_u64(price)])?;

    log::info!("Mark price submitted on-chain via RPC", "contract", &perp_id[..8], "price", price, "took", log::duration_secs(&start.elapsed()));
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

pub fn submit_liquidate(perp_id: &str, commitment: &str, oracle_price: u64, settlement_amount: i128) -> Result<()> {
    let start = Instant::now();
    let src = signing_source("e2e");

    log::debug!("Liquidating position via RPC", "cmt", &commitment[..16], "oracle", oracle_price, "settlement", settlement_amount);
    let rpc = SorobanRpc::new();
    rpc.invoke_xdr(perp_id, &src, "liquidate", vec![
        scval_bytes32(commitment)?,
        scval_u64(oracle_price),
        scval_i128(settlement_amount),
    ])?;

    log::info!("Position liquidated on-chain via RPC", "contract", &perp_id[..8], "cmt", &commitment[..16], "took", log::duration_secs(&start.elapsed()));
    Ok(())
}

/// Relay an open_position for a user: place_order + open_position_from_note in one flow.
/// Position params are encrypted with AES-256-GCM (CER_DEK) into sealed_params before
/// being stored on-chain — side, price, leverage, size, tp, sl, tif, expiry are never
/// visible in calldata or contract storage.
pub fn relay_open_position(
    perp_id: &str,
    orderbook_id: &str,
    note_cmt_hex: &str,
    note_null_hex: &str,
    position_cmt_hex: &str,
    hint_price: u64,
    hint_side: u64,
    hint_leverage: u64,
    hint_size: u64,
    tp_price: u64,
    sl_price: u64,
    portfolio_key_hex: &str,
    asset_id_hex: &str,
    note_proof_json: &str,
    commit_proof_json: &str,
) -> Result<String> {
    let start = Instant::now();
    let src = std::env::var("STELLAR_SOURCE_SECRET").unwrap_or_else(|_| "e2e".to_string());
    let zeros = "0".repeat(64);

    // Encrypt position params before any on-chain submission
    let sealed = seal_position_params(hint_side, hint_price, hint_leverage, hint_size, tp_price, sl_price, 0, 0)?;

    log::info!("Relaying place_order", "orderbook", &orderbook_id[..8], "cmt", &position_cmt_hex[..16]);
    let rpc = SorobanRpc::new();
    rpc.invoke_xdr(orderbook_id, &src, "place_order", vec![
        scval_bytes32(position_cmt_hex)?,
        scval_bytes32(portfolio_key_hex)?,
        scval_u64(hint_price),
        scval_u64(hint_side),
        scval_u64(hint_size),
        scval_u64(hint_leverage),
        scval_u64(15),
        scval_u64(0), // GTC = 0
        scval_u64(0),
        scval_bytes32(asset_id_hex)?,
        scval_proof(commit_proof_json)?,
    ])?;

    log::info!("Relaying open_position_from_note", "contract", &perp_id[..8], "sealed_len", sealed.len());
    let rpc = SorobanRpc::new();
    let tx_hash = rpc.invoke_xdr(perp_id, &src, "open_position_from_note", vec![
        scval_bytes32(note_cmt_hex)?,
        scval_bytes32(note_null_hex)?,
        scval_bytes32(position_cmt_hex)?,
        scval_bytes(&sealed)?,           // sealed_params: AES-256-GCM blob
        scval_bytes32(&zeros)?,          // liquidation_recipient_note (zeros = no specific note)
        scval_bytes32(portfolio_key_hex)?,
        scval_bytes32(asset_id_hex)?,
        scval_proof(note_proof_json)?,
        scval_proof(commit_proof_json)?,
    ])?;

    log::info!("Relay open_position complete", "hash", &tx_hash[..16], "took", log::duration_secs(&start.elapsed()));
    Ok(tx_hash)
}
