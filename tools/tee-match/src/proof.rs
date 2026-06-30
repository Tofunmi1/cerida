use crate::db::OrderSecrets;
use crate::log;
use anyhow::{Context, Result};
use ark_bn254::Fr;
use ark_ff::{AdditiveGroup, Field};
use rust_circuits::{load_pk, prove_cancel_with_pk, prove_commitment_with_pk, prove_match_with_pk, ProofOutput};
use std::path::{Path, PathBuf};

pub type MatchProof = ProofOutput;

fn pk_path(keys_dir: &Path, name: &str) -> PathBuf {
    keys_dir.join(format!("{}.pk.bin", name))
}

pub fn gen_commitment_proof(keys_dir: &Path, secrets: &OrderSecrets) -> Result<MatchProof> {
    let pk = load_pk(&pk_path(keys_dir, "order_commitment"))
        .with_context(|| format!("Failed to load commitment pk from {}", keys_dir.display()))?;
    let is_market = if secrets.is_market { Fr::ONE } else { Fr::ZERO };
    let out = prove_commitment_with_pk(
        &pk,
        Fr::from(secrets.side), Fr::from(secrets.price), Fr::from(secrets.size),
        Fr::from(secrets.leverage), Fr::from(secrets.asset), is_market,
        Fr::from(secrets.nonce), Fr::from(secrets.secret),
    )?;
    log::debug!("Commitment proof generated via native Rust circuits (pk)",
        "side", secrets.side,
        "price", secrets.price,
        "size", secrets.size);
    Ok(out)
}

pub fn gen_cancel_proof(keys_dir: &Path, secrets: &OrderSecrets) -> Result<MatchProof> {
    let pk = load_pk(&pk_path(keys_dir, "order_cancel"))
        .with_context(|| format!("Failed to load cancel pk from {}", keys_dir.display()))?;
    let cmt = rust_circuits::compute_commitment(
        Fr::from(secrets.side), Fr::from(secrets.price), Fr::from(secrets.size),
        Fr::from(secrets.leverage), Fr::from(secrets.asset),
        if secrets.is_market { Fr::ONE } else { Fr::ZERO },
        Fr::from(secrets.nonce), Fr::from(secrets.secret),
    );
    let out = prove_cancel_with_pk(&pk, cmt, Fr::from(secrets.secret))?;
    log::debug!("Cancel proof generated via native Rust circuits (pk)",
        "side", secrets.side,
        "price", secrets.price);
    Ok(out)
}

pub fn gen_match_proof(
    keys_dir: &Path,
    a: &OrderSecrets,
    b: &OrderSecrets,
    mp: u64,
    ms: u64,
) -> Result<MatchProof> {
    let pk = load_pk(&pk_path(keys_dir, "order_match"))
        .with_context(|| format!("Failed to load match pk from {}", keys_dir.display()))?;
    let is_market_a = if a.is_market { Fr::ONE } else { Fr::ZERO };
    let is_market_b = if b.is_market { Fr::ONE } else { Fr::ZERO };
    let out = prove_match_with_pk(
        &pk,
        Fr::from(a.side), Fr::from(a.price), Fr::from(a.size), Fr::from(a.leverage),
        Fr::from(a.asset), is_market_a, Fr::from(a.nonce), Fr::from(a.secret),
        Fr::from(b.side), Fr::from(b.price), Fr::from(b.size), Fr::from(b.leverage),
        Fr::from(b.asset), is_market_b, Fr::from(b.nonce), Fr::from(b.secret),
        Fr::from(mp), Fr::from(ms),
    )?;
    log::debug!("Match proof generated via native Rust circuits (pk)",
        "side_a", a.side, "price_a", a.price,
        "side_b", b.side, "price_b", b.price,
        "match_price", mp, "match_size", ms);
    Ok(out)
}
