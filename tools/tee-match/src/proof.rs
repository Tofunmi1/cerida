use crate::db::OrderSecrets;
use crate::log;
use anyhow::Result;
use ark_bn254::Fr;
use ark_ff::{AdditiveGroup, Field};
use rust_circuits::{prove_commitment as rc_prove_cmt, prove_match as rc_prove_match, ProofOutput};

pub type MatchProof = ProofOutput;

pub fn gen_commitment_proof(_keys_dir: &std::path::Path, secrets: &OrderSecrets) -> Result<MatchProof> {
    let is_market = if secrets.is_market { Fr::ONE } else { Fr::ZERO };
    let out = rc_prove_cmt(
        Fr::from(secrets.side), Fr::from(secrets.price), Fr::from(secrets.size),
        Fr::from(secrets.leverage), Fr::from(secrets.asset), is_market,
        Fr::from(secrets.nonce), Fr::from(secrets.secret),
    )?;
    log::debug!("Commitment proof generated via native Rust circuits",
        "side", secrets.side,
        "price", secrets.price,
        "size", secrets.size);
    Ok(out)
}

pub fn gen_match_proof(
    _keys_dir: &std::path::Path,
    a: &OrderSecrets,
    b: &OrderSecrets,
    mp: u64,
    ms: u64,
) -> Result<MatchProof> {
    let is_market_a = if a.is_market { Fr::ONE } else { Fr::ZERO };
    let is_market_b = if b.is_market { Fr::ONE } else { Fr::ZERO };
    let out = rc_prove_match(
        Fr::from(a.side), Fr::from(a.price), Fr::from(a.size), Fr::from(a.leverage),
        Fr::from(a.asset), is_market_a, Fr::from(a.nonce), Fr::from(a.secret),
        Fr::from(b.side), Fr::from(b.price), Fr::from(b.size), Fr::from(b.leverage),
        Fr::from(b.asset), is_market_b, Fr::from(b.nonce), Fr::from(b.secret),
        Fr::from(mp), Fr::from(ms),
    )?;
    log::debug!("Match proof generated via native Rust circuits",
        "side_a", a.side, "price_a", a.price,
        "side_b", b.side, "price_b", b.price,
        "match_price", mp, "match_size", ms);
    Ok(out)
}
