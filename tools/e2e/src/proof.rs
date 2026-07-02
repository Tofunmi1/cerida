use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result};
use ark_bn254::{Bn254, Fr, Fq, Fq2, G1Affine, G2Affine};
use ark_ff::{AdditiveGroup, Field, PrimeField};
use ark_groth16::{Groth16, ProvingKey, prepare_verifying_key};
use rust_circuits::{
    ProofOutput, load_pk, compute_note_commitment, compute_note_nullifier, compute_nullifier,
    fr_to_biguint, prove_cancel_with_pk,
    compute_leaf_hash, compute_pool_nullifier_hash, compute_merkle_path, compute_new_root,
    compute_root_from_leaves, compute_empty_root, prove_shielded_insert, prove_shielded_withdraw,
    circuits::shielded_insert::TREE_DEPTH,
};

pub type RawProof = ProofOutput;

fn pk_path(keys_dir: &Path, name: &str) -> std::path::PathBuf {
    keys_dir.join(format!("{}.pk.bin", name))
}

/// Verify the proof locally using the VK from the loaded proving key.
fn verify_proof_raw(pk: &ProvingKey<Bn254>, proof: &ProofOutput, public: &[Fr]) {
    let pvk = prepare_verifying_key(&pk.vk);
    let proof_ark = ark_groth16::Proof {
        a: parse_g1(&proof.proof.a).into(),
        b: parse_g2(&proof.proof.b).into(),
        c: parse_g1(&proof.proof.c).into(),
    };
    let result = Groth16::<Bn254>::verify_proof(&pvk, &proof_ark, public).unwrap();
    assert!(result, "LOCAL VERIFICATION FAILED — generated proof does not verify against exported VK!");
}

pub fn gen_commitment(
    keys_dir: &Path,
    side: u64,
    price: u64,
    size: u64,
    leverage: u64,
    asset: u64,
    is_market: u64,
    nonce: u64,
    secret: u64,
    use_cross: bool,
) -> Result<RawProof> {
    let pk = load_pk(&pk_path(keys_dir, "order_commitment"))
        .with_context(|| format!("Failed to load commitment proving key from {}", keys_dir.display()))?;
    let is_market_fr = if is_market != 0 { Fr::ONE } else { Fr::ZERO };
    let out = rust_circuits::prove_commitment_with_pk(
        &pk,
        Fr::from(side), Fr::from(price), Fr::from(size),
        Fr::from(leverage), Fr::from(asset), is_market_fr,
        Fr::from(nonce), Fr::from(secret), use_cross,
    )?;
    // Verify proof locally before trusting on-chain
    let cmt = Fr::from_str(&out.public_inputs[0]).unwrap();
    verify_proof_raw(&pk, &out, &[cmt]);
    Ok(out)
}

/// Generate a NoteSpend proof for deposit_note→withdraw_note flows.
/// Returns (note_commitment_hex, nullifier_hex, RawProof).
pub fn gen_note_spend(keys_dir: &Path, amount: u64, secret: u64) -> Result<(String, String, RawProof)> {
    let pk = load_pk(&pk_path(keys_dir, "note_spend"))
        .with_context(|| format!("Failed to load note_spend.pk.bin from {}", keys_dir.display()))?;
    let amount_fr = Fr::from(amount);
    let secret_fr = Fr::from(secret);
    let note_cmt = compute_note_commitment(amount_fr, secret_fr);
    let nullifier = compute_note_nullifier(note_cmt, secret_fr);
    let out = rust_circuits::prove_note_spend_with_pk(&pk, amount_fr, secret_fr)?;
    // Verify locally before submitting
    let pvk = prepare_verifying_key(&pk.vk);
    let proof_ark = ark_groth16::Proof {
        a: parse_g1(&out.proof.a).into(),
        b: parse_g2(&out.proof.b).into(),
        c: parse_g1(&out.proof.c).into(),
    };
    let result = Groth16::<Bn254>::verify_proof(&pvk, &proof_ark, &[note_cmt, nullifier]).unwrap();
    assert!(result, "NoteSpend proof failed local verification");
    let cmt_hex = format!("{:0>64x}", fr_to_biguint(&note_cmt));
    let null_hex = format!("{:0>64x}", fr_to_biguint(&nullifier));
    Ok((cmt_hex, null_hex, out))
}

fn parse_g1(hex: &str) -> G1Affine {
    let x = Fq::from_be_bytes_mod_order(&hex::decode(&hex[..64]).unwrap());
    let y = Fq::from_be_bytes_mod_order(&hex::decode(&hex[64..]).unwrap());
    G1Affine::new(x, y)
}

fn parse_g2(hex: &str) -> G2Affine {
    let x_c1 = Fq::from_be_bytes_mod_order(&hex::decode(&hex[..64]).unwrap());
    let x_c0 = Fq::from_be_bytes_mod_order(&hex::decode(&hex[64..128]).unwrap());
    let y_c1 = Fq::from_be_bytes_mod_order(&hex::decode(&hex[128..192]).unwrap());
    let y_c0 = Fq::from_be_bytes_mod_order(&hex::decode(&hex[192..]).unwrap());
    G2Affine::new(Fq2::new(x_c0, x_c1), Fq2::new(y_c0, y_c1))
}

/// Returned by gen_pool_insert — everything the caller needs for pool.deposit.
pub struct PoolInsertResult {
    pub commitment_hex: String,
    pub old_root_hex: String,
    pub new_root_hex: String,
    pub leaf_index: u64,
    pub proof: RawProof,
}

/// Returned by gen_pool_withdraw — everything the caller needs for pool.withdraw.
pub struct PoolWithdrawResult {
    pub nullifier_hash_hex: String,
    pub root_hex: String,
    pub recipient_hex: String,
    pub proof: RawProof,
}

/// Generate a ShieldedInsert proof for pool.deposit.
/// `current_leaves` = hex-encoded leaf hashes already in the tree (empty for first deposit).
/// `recipient_pk` is only used to derive the note commitment — pool uses secret+nullifier.
pub fn gen_pool_insert(
    keys_dir: &Path,
    secret: u64,
    nullifier: u64,
    current_leaves: &[Fr],
) -> Result<PoolInsertResult> {
    let pk = load_pk(&pk_path(keys_dir, "shielded_insert"))
        .with_context(|| "Failed to load shielded_insert.pk.bin")?;

    let secret_fr = Fr::from(secret);
    let nullifier_fr = Fr::from(nullifier);
    let commitment = compute_leaf_hash(secret_fr, nullifier_fr);

    let leaf_index = current_leaves.len() as u64;
    let old_root = if current_leaves.is_empty() {
        compute_empty_root()
    } else {
        compute_root_from_leaves(current_leaves)
    };

    let path = compute_merkle_path(current_leaves, leaf_index as usize);
    let new_root = compute_new_root(commitment, leaf_index, &path);

    let out = prove_shielded_insert(&pk, old_root, new_root, commitment, leaf_index, path)?;

    // Local verification
    let pvk = ark_groth16::prepare_verifying_key(&pk.vk);
    let proof_ark = ark_groth16::Proof {
        a: parse_g1(&out.proof.a).into(),
        b: parse_g2(&out.proof.b).into(),
        c: parse_g1(&out.proof.c).into(),
    };
    let pi = [old_root, new_root, commitment, Fr::from(leaf_index)];
    assert!(
        Groth16::<Bn254>::verify_proof(&pvk, &proof_ark, &pi).unwrap(),
        "ShieldedInsert proof failed local verification"
    );

    Ok(PoolInsertResult {
        commitment_hex: format!("{:0>64x}", fr_to_biguint(&commitment)),
        old_root_hex: format!("{:0>64x}", fr_to_biguint(&old_root)),
        new_root_hex: format!("{:0>64x}", fr_to_biguint(&new_root)),
        leaf_index,
        proof: out,
    })
}

/// Generate a ShieldedWithdraw proof for pool.withdraw.
/// `all_leaves` = all leaf hashes currently in the tree (commitment must be among them).
/// `recipient_hex` = 32-byte hex that binds the proof to a recipient (sha256 of pubkey or similar).
pub fn gen_pool_withdraw(
    keys_dir: &Path,
    secret: u64,
    nullifier: u64,
    all_leaves: &[Fr],
    recipient_hex: &str,
) -> Result<PoolWithdrawResult> {
    let pk = load_pk(&pk_path(keys_dir, "shielded_withdraw"))
        .with_context(|| "Failed to load shielded_withdraw.pk.bin")?;

    let secret_fr = Fr::from(secret);
    let nullifier_fr = Fr::from(nullifier);
    let commitment = compute_leaf_hash(secret_fr, nullifier_fr);
    let nullifier_hash = compute_pool_nullifier_hash(nullifier_fr);

    let leaf_index = all_leaves
        .iter()
        .position(|&l| l == commitment)
        .with_context(|| "commitment not found in leaf set")?;

    let root = compute_root_from_leaves(all_leaves);
    let path = compute_merkle_path(all_leaves, leaf_index);
    let mut path_indices = [false; TREE_DEPTH];
    for i in 0..TREE_DEPTH {
        path_indices[i] = ((leaf_index >> i) & 1) == 1;
    }

    let recipient_bytes = hex::decode(recipient_hex.trim_start_matches("0x"))
        .with_context(|| "invalid recipient hex")?;
    let recipient_fr = Fr::from_be_bytes_mod_order(&recipient_bytes);

    let out = prove_shielded_withdraw(
        &pk, root, nullifier_hash, recipient_fr,
        secret_fr, nullifier_fr, path, path_indices,
    )?;

    // Local verification
    let pvk = ark_groth16::prepare_verifying_key(&pk.vk);
    let proof_ark = ark_groth16::Proof {
        a: parse_g1(&out.proof.a).into(),
        b: parse_g2(&out.proof.b).into(),
        c: parse_g1(&out.proof.c).into(),
    };
    assert!(
        Groth16::<Bn254>::verify_proof(&pvk, &proof_ark, &[root, nullifier_hash, recipient_fr]).unwrap(),
        "ShieldedWithdraw proof failed local verification"
    );

    Ok(PoolWithdrawResult {
        nullifier_hash_hex: format!("{:0>64x}", fr_to_biguint(&nullifier_hash)),
        root_hex: format!("{:0>64x}", fr_to_biguint(&root)),
        recipient_hex: recipient_hex.trim_start_matches("0x").to_string(),
        proof: out,
    })
}

/// Generate a NoteSpend proof using amount=0 as sentinel for a settlement note.
/// The note_commitment = Poseidon2(0, secret, 8) is used as the key in storage.
/// Returns (note_commitment_hex, nullifier_hex, RawProof).
pub fn gen_settlement_note_spend(keys_dir: &Path, secret: u64) -> Result<(String, String, RawProof)> {
    gen_note_spend(keys_dir, 0, secret)
}

/// Generate an OrderCancel proof for a position commitment.
/// Returns (nullifier_hex, RawProof).
pub fn gen_cancel(keys_dir: &Path, commitment_hex: &str, secret: u64) -> Result<(String, RawProof)> {
    use std::str::FromStr;
    let pk = load_pk(&pk_path(keys_dir, "order_cancel"))
        .with_context(|| format!("Failed to load order_cancel.pk.bin from {}", keys_dir.display()))?;
    let cmt_bytes = hex::decode(commitment_hex).context("invalid commitment hex")?;
    let cmt_fr = Fr::from_be_bytes_mod_order(&cmt_bytes);
    let secret_fr = Fr::from(secret);
    let nullifier = rust_circuits::compute_nullifier(cmt_fr, secret_fr);
    let out = rust_circuits::prove_cancel_with_pk(&pk, cmt_fr, secret_fr)?;
    let pvk = prepare_verifying_key(&pk.vk);
    let proof_ark = ark_groth16::Proof {
        a: parse_g1(&out.proof.a).into(),
        b: parse_g2(&out.proof.b).into(),
        c: parse_g1(&out.proof.c).into(),
    };
    let verified = Groth16::<Bn254>::verify_proof(&pvk, &proof_ark, &[nullifier]).unwrap();
    assert!(verified, "cancel proof failed local verification");
    let null_hex = format!("{:0>64x}", fr_to_biguint(&nullifier));
    Ok((null_hex, out))
}

pub fn gen_match(
    keys_dir: &Path,
    side_a: u64, price_a: u64, size_a: u64, leverage_a: u64,
    asset_a: u64, is_market_a: u64, nonce_a: u64, secret_a: u64,
    side_b: u64, price_b: u64, size_b: u64, leverage_b: u64,
    asset_b: u64, is_market_b: u64, nonce_b: u64, secret_b: u64,
    mp: u64, ms: u64,
) -> Result<RawProof> {
    let pk = load_pk(&pk_path(keys_dir, "order_match"))
        .with_context(|| format!("Failed to load match proving key from {}", keys_dir.display()))?;
    let is_market_a_fr = if is_market_a != 0 { Fr::ONE } else { Fr::ZERO };
    let is_market_b_fr = if is_market_b != 0 { Fr::ONE } else { Fr::ZERO };
    let out = rust_circuits::prove_match_with_pk(
        &pk,
        Fr::from(side_a), Fr::from(price_a), Fr::from(size_a), Fr::from(leverage_a),
        Fr::from(asset_a), is_market_a_fr, Fr::from(nonce_a), Fr::from(secret_a),
        Fr::from(side_b), Fr::from(price_b), Fr::from(size_b), Fr::from(leverage_b),
        Fr::from(asset_b), is_market_b_fr, Fr::from(nonce_b), Fr::from(secret_b),
        Fr::from(mp), Fr::from(ms),
    )?;
    // Verify proof locally before trusting on-chain
    let cmt_a = Fr::from_str(&out.public_inputs[0]).unwrap();
    let cmt_b = Fr::from_str(&out.public_inputs[1]).unwrap();
    let mp_fr = Fr::from_str(&out.public_inputs[2]).unwrap();
    let ms_fr = Fr::from_str(&out.public_inputs[3]).unwrap();
    let nf_a = Fr::from_str(&out.public_inputs[4]).unwrap();
    let nf_b = Fr::from_str(&out.public_inputs[5]).unwrap();
    verify_proof_raw(&pk, &out, &[cmt_a, cmt_b, mp_fr, ms_fr, nf_a, nf_b]);
    Ok(out)
}
