use crate::log;
use crate::proof::MatchProof;
use anyhow::Result;
use e2e::soroban_rpc::{scval_bytes32, scval_proof, scval_tif, scval_u64, SorobanRpc};
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

pub fn submit_liquidate(perp_id: &str, commitment: &str) -> Result<()> {
    let start = Instant::now();
    let src = signing_source("e2e");

    log::debug!("Liquidating position via RPC", "cmt", &commitment[..16]);
    let rpc = SorobanRpc::new();
    rpc.invoke_xdr(perp_id, &src, "liquidate", vec![scval_bytes32(commitment)?])?;

    log::info!("Position liquidated on-chain via RPC", "contract", &perp_id[..8], "cmt", &commitment[..16], "took", log::duration_secs(&start.elapsed()));
    Ok(())
}

/// Relay an open_position for a user: place_order + open_position_from_note in one flow.
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
        scval_tif("GTC")?,
        scval_u64(0),
        scval_bytes32(asset_id_hex)?,
        scval_proof(commit_proof_json)?,
    ])?;

    log::info!("Relaying open_position_from_note", "contract", &perp_id[..8], "side", hint_side);
    let rpc = SorobanRpc::new();
    rpc.invoke_xdr(perp_id, &src, "open_position_from_note", vec![
        scval_bytes32(note_cmt_hex)?,
        scval_bytes32(note_null_hex)?,
        scval_bytes32(position_cmt_hex)?,
        scval_u64(hint_price),
        scval_u64(hint_side),
        scval_u64(hint_leverage),
        scval_u64(hint_size),
        scval_tif("GTC")?,
        scval_u64(0),
        scval_u64(tp_price),
        scval_u64(sl_price),
        scval_bytes32(&zeros)?,
        scval_bytes32(portfolio_key_hex)?,
        scval_bytes32(asset_id_hex)?,
        scval_proof(note_proof_json)?,
        scval_proof(commit_proof_json)?,
    ])?;

    log::info!("Relay open_position complete", "took", log::duration_secs(&start.elapsed()));
    Ok(position_cmt_hex.to_string())
}
