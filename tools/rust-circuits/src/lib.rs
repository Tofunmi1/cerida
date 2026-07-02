#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::assign_op_pattern,
    clippy::needless_range_loop,
    clippy::redundant_closure
)]

pub mod circuits;
pub mod poseidon2;

use ark_bn254::{Bn254, Fq, Fr, G1Affine, G2Affine};
use ark_ff::{AdditiveGroup, BigInteger, Field, PrimeField, UniformRand};
use ark_groth16::{Groth16, ProvingKey, VerifyingKey};
use ark_r1cs_std::fields::fp::FpVar;
use ark_r1cs_std::GR1CSVar;
use ark_relations::gr1cs::{ConstraintSynthesizer, SynthesisError};
use ark_serialize::CanonicalDeserialize;
use circuits::cancel::OrderCancel;
use circuits::commitment::OrderCommitment;
use circuits::match_circuit::OrderMatch;
use circuits::note_spend::NoteSpend;
use circuits::shielded_insert::{ShieldedInsert, TREE_DEPTH};
use circuits::shielded_withdraw::ShieldedWithdraw;
use num_bigint::BigUint;
use poseidon2::{poseidon2_hash_t3, poseidon2_hash_t4};

#[derive(serde::Serialize)]
pub struct ProofHex {
    pub a: String,
    pub b: String,
    pub c: String,
}

#[derive(serde::Serialize)]
pub struct ProofOutput {
    pub proof: ProofHex,
    pub public_inputs: Vec<String>,
}

pub fn g1_to_hex(g1: &G1Affine) -> String {
    let x_be = g1.x.into_bigint().to_bytes_be();
    let y_be = g1.y.into_bigint().to_bytes_be();
    format!("{}{}", hex::encode(x_be), hex::encode(y_be))
}

pub fn g2_to_hex(g2: &G2Affine) -> String {
    let c0_be = g2.x.c0.into_bigint().to_bytes_be();
    let c1_be = g2.x.c1.into_bigint().to_bytes_be();
    let d0_be = g2.y.c0.into_bigint().to_bytes_be();
    let d1_be = g2.y.c1.into_bigint().to_bytes_be();
    format!(
        "{}{}{}{}",
        hex::encode(c1_be),
        hex::encode(c0_be),
        hex::encode(d1_be),
        hex::encode(d0_be),
    )
}

pub fn fr_to_biguint(f: &Fr) -> BigUint {
    BigUint::from_bytes_be(&f.into_bigint().to_bytes_be())
}

fn fq_to_biguint(f: &Fq) -> BigUint {
    BigUint::from_bytes_be(&f.into_bigint().to_bytes_be())
}

fn g1_to_json(g1: &G1Affine) -> serde_json::Value {
    serde_json::json!([
        fq_to_biguint(&g1.x).to_string(),
        fq_to_biguint(&g1.y).to_string(),
        "1"
    ])
}

fn g2_to_json(g2: &G2Affine) -> serde_json::Value {
    serde_json::json!([
        [
            fq_to_biguint(&g2.x.c0).to_string(),
            fq_to_biguint(&g2.x.c1).to_string(),
        ],
        [
            fq_to_biguint(&g2.y.c0).to_string(),
            fq_to_biguint(&g2.y.c1).to_string(),
        ],
        ["1", "0"]
    ])
}

pub fn vk_to_json(vk: &VerifyingKey<Bn254>) -> serde_json::Value {
    let ic: Vec<serde_json::Value> = vk.gamma_abc_g1.iter().map(|g1| g1_to_json(g1)).collect();
    serde_json::json!({
        "protocol": "groth16",
        "curve": "bn128",
        "nPublic": ic.len() - 1,
        "vk_alpha_1": g1_to_json(&vk.alpha_g1),
        "vk_beta_2": g2_to_json(&vk.beta_g2),
        "vk_gamma_2": g2_to_json(&vk.gamma_g2),
        "vk_delta_2": g2_to_json(&vk.delta_g2),
        "vk_alphabeta_12": serde_json::Value::Null,
        "IC": ic,
    })
}

/// Full setup: generates a single shared CRS (alpha/beta/gamma/delta) and produces
/// proving/verifying keys for all three circuits. All VKs will share the same
/// alpha_g1, beta_g2, gamma_g2, delta_g2 — this is required by the Soroban contracts.
fn gen_with_crs(
    circuit: impl ConstraintSynthesizer<Fr>,
    alpha: Fr,
    beta: Fr,
    gamma: Fr,
    delta: Fr,
    g1_gen: ark_bn254::G1Projective,
    g2_gen: ark_bn254::G2Projective,
    rng: &mut impl rand::Rng,
) -> Result<(ProvingKey<Bn254>, VerifyingKey<Bn254>), SynthesisError> {
    let pk = Groth16::<Bn254>::generate_parameters_with_qap(
        circuit, alpha, beta, gamma, delta, g1_gen, g2_gen, rng,
    )?;
    let vk = pk.vk.clone();
    Ok((pk, vk))
}

pub fn setup_all(
    rng: &mut impl rand::Rng,
) -> Result<[(ProvingKey<Bn254>, VerifyingKey<Bn254>); 4], SynthesisError> {
    let alpha = Fr::rand(rng);
    let beta = Fr::rand(rng);
    let gamma = Fr::ONE;
    let delta = Fr::rand(rng);
    let g1_gen = ark_bn254::G1Projective::rand(rng);
    let g2_gen = ark_bn254::G2Projective::rand(rng);

    let commit = gen_with_crs(
        OrderCommitment {
            side: Fr::ZERO,
            price: Fr::ZERO,
            size: Fr::ZERO,
            leverage: Fr::ZERO,
            asset: Fr::ZERO,
            is_market: Fr::ZERO,
            nonce: Fr::ZERO,
            secret: Fr::ZERO,
            commitment: Fr::ZERO,
            use_cross: Fr::ZERO,
            portfolio_key: Fr::ZERO,
        },
        alpha,
        beta,
        gamma,
        delta,
        g1_gen,
        g2_gen,
        rng,
    )?;

    let cancel = gen_with_crs(
        OrderCancel {
            commitment: Fr::ZERO,
            secret: Fr::ZERO,
            nullifier: Fr::ZERO,
        },
        alpha,
        beta,
        gamma,
        delta,
        g1_gen,
        g2_gen,
        rng,
    )?;

    let r#match = gen_with_crs(
        OrderMatch {
            side_a: Fr::ZERO,
            price_a: Fr::ZERO,
            size_a: Fr::ZERO,
            leverage_a: Fr::ZERO,
            asset_a: Fr::ZERO,
            is_market_a: Fr::ZERO,
            nonce_a: Fr::ZERO,
            secret_a: Fr::ZERO,
            side_b: Fr::ONE,
            price_b: Fr::ZERO,
            size_b: Fr::ZERO,
            leverage_b: Fr::ZERO,
            asset_b: Fr::ZERO,
            is_market_b: Fr::ZERO,
            nonce_b: Fr::ZERO,
            secret_b: Fr::ZERO,
            mp: Fr::ZERO,
            ms: Fr::ZERO,
            cmt_a: Fr::ZERO,
            cmt_b: Fr::ZERO,
            match_price: Fr::ZERO,
            match_size: Fr::ZERO,
            nullifier_a: Fr::ZERO,
            nullifier_b: Fr::ZERO,
        },
        alpha,
        beta,
        gamma,
        delta,
        g1_gen,
        g2_gen,
        rng,
    )?;

    let note_spend = gen_with_crs(
        NoteSpend {
            amount: Fr::ZERO,
            secret: Fr::ZERO,
            note_commitment: Fr::ZERO,
            nullifier: Fr::ZERO,
        },
        alpha,
        beta,
        gamma,
        delta,
        g1_gen,
        g2_gen,
        rng,
    )?;

    Ok([commit, cancel, r#match, note_spend])
}

/// Setup for the two shielded-pool circuits (independent trusted setup from the perp circuits).
pub fn setup_pool(
    rng: &mut impl rand::Rng,
) -> Result<[(ProvingKey<Bn254>, VerifyingKey<Bn254>); 2], SynthesisError> {
    let alpha = Fr::rand(rng);
    let beta = Fr::rand(rng);
    let gamma = Fr::ONE;
    let delta = Fr::rand(rng);
    let g1_gen = ark_bn254::G1Projective::rand(rng);
    let g2_gen = ark_bn254::G2Projective::rand(rng);

    let insert = gen_with_crs(
        ShieldedInsert::dummy(),
        alpha,
        beta,
        gamma,
        delta,
        g1_gen,
        g2_gen,
        rng,
    )?;
    let withdraw = gen_with_crs(
        ShieldedWithdraw::dummy(),
        alpha,
        beta,
        gamma,
        delta,
        g1_gen,
        g2_gen,
        rng,
    )?;

    Ok([insert, withdraw])
}

/// Zero subtree hashes for use as Merkle path siblings.
/// zeros[i] = hash of an all-zero subtree at depth i (the sibling you'd use at level i in a path).
/// zeros[0] = Fr::ZERO  (empty leaf)
/// zeros[i] = Poseidon2(zeros[i-1], zeros[i-1], 32 + (i-1))
pub fn compute_pool_zeros() -> [Fr; TREE_DEPTH] {
    let mut zeros = [Fr::ZERO; TREE_DEPTH];
    for i in 1..TREE_DEPTH {
        let left = FpVar::Constant(zeros[i - 1]);
        let right = FpVar::Constant(zeros[i - 1]);
        zeros[i] = poseidon2_hash_t3(&left, &right, 32 + (i - 1) as u64)
            .unwrap()
            .value()
            .unwrap();
    }
    zeros
}

/// Empty Merkle tree root (all leaves are Fr::ZERO).
/// Iteratively hashes the zero subtrees bottom-up without allocating the full leaf set.
pub fn compute_empty_root() -> Fr {
    // zeros[0] = Fr::ZERO (empty leaf)
    // zeros[i] = Poseidon(zeros[i-1], zeros[i-1], 32 + (i-1))  for i in 1..=TREE_DEPTH
    let mut current = Fr::ZERO;
    for level in 0..TREE_DEPTH {
        let lv = FpVar::Constant(current);
        let rv = FpVar::Constant(current);
        current = poseidon2_hash_t3(&lv, &rv, 32 + level as u64)
            .unwrap()
            .value()
            .unwrap();
    }
    current
}

/// Compute the Merkle root after inserting `commitment` at `leaf_index`,
/// given the current sibling path `path_elements`.
pub fn compute_new_root(commitment: Fr, leaf_index: u64, path_elements: &[Fr; TREE_DEPTH]) -> Fr {
    let mut current = commitment;
    for (i, sibling) in path_elements.iter().enumerate() {
        let bit = (leaf_index >> i) & 1;
        let (left, right) = if bit == 0 {
            (current, *sibling)
        } else {
            (*sibling, current)
        };
        let lv = FpVar::Constant(left);
        let rv = FpVar::Constant(right);
        current = poseidon2_hash_t3(&lv, &rv, 32 + i as u64)
            .unwrap()
            .value()
            .unwrap();
    }
    current
}

/// Compute the Merkle path (siblings) for a leaf at `leaf_index`.
/// Uses a compact layer: only the subtree containing actual leaves + zeros for empty halves.
pub fn compute_merkle_path(leaves: &[Fr], leaf_index: usize) -> [Fr; TREE_DEPTH] {
    let zeros = compute_pool_zeros();

    // Build a compact layer: leaves padded to next power-of-two (at least 2)
    let n = leaves.len().next_power_of_two().clamp(2, 1 << TREE_DEPTH);
    let mut layer: Vec<Fr> = (0..n)
        .map(|i| {
            if i < leaves.len() {
                leaves[i]
            } else {
                Fr::ZERO
            }
        })
        .collect();

    let mut path = [Fr::ZERO; TREE_DEPTH];
    let mut idx = leaf_index;

    for level in 0..TREE_DEPTH {
        let sibling_idx = idx ^ 1;
        // If sibling is within our compact layer, use it; otherwise it's a zero subtree
        path[level] = layer.get(sibling_idx).copied().unwrap_or(zeros[level]);

        // Collapse layer one level up
        let parent_len = layer.len().div_ceil(2);
        let mut parent = Vec::with_capacity(parent_len);
        for j in 0..parent_len {
            let l = layer.get(2 * j).copied().unwrap_or(zeros[level]);
            let r = layer.get(2 * j + 1).copied().unwrap_or(zeros[level]);
            let lv = FpVar::Constant(l);
            let rv = FpVar::Constant(r);
            parent.push(
                poseidon2_hash_t3(&lv, &rv, 32 + level as u64)
                    .unwrap()
                    .value()
                    .unwrap(),
            );
        }
        layer = parent;
        idx >>= 1;
    }
    path
}

/// Build the Merkle root from a (possibly partial) leaf set.
/// Empty slots are Fr::ZERO. Uses zeros[] to prune empty subtrees efficiently.
pub fn compute_root_from_leaves(leaves: &[Fr]) -> Fr {
    let zeros = compute_pool_zeros();
    // Work with only as many leaves as needed (next power-of-two or at least 2)
    let n = leaves.len().next_power_of_two().clamp(2, 1 << TREE_DEPTH);
    let mut layer: Vec<Fr> = (0..n)
        .map(|i| {
            if i < leaves.len() {
                leaves[i]
            } else {
                Fr::ZERO
            }
        })
        .collect();

    for level in 0..TREE_DEPTH {
        if layer.len() <= 1 {
            // rest of the path uses empty subtree hashes
            let mut current = layer[0];
            for l in level..TREE_DEPTH {
                let sib = if l < TREE_DEPTH { zeros[l] } else { Fr::ZERO };
                let lv = FpVar::Constant(current);
                let rv = FpVar::Constant(sib);
                current = poseidon2_hash_t3(&lv, &rv, 32 + l as u64)
                    .unwrap()
                    .value()
                    .unwrap();
            }
            return current;
        }
        let parent_len = layer.len().div_ceil(2);
        let mut parent = Vec::with_capacity(parent_len);
        for j in 0..parent_len {
            let l = layer.get(2 * j).copied().unwrap_or(zeros[level]);
            let r = layer.get(2 * j + 1).copied().unwrap_or(zeros[level]);
            let lv = FpVar::Constant(l);
            let rv = FpVar::Constant(r);
            parent.push(
                poseidon2_hash_t3(&lv, &rv, 32 + level as u64)
                    .unwrap()
                    .value()
                    .unwrap(),
            );
        }
        layer = parent;
    }
    layer[0]
}

pub fn compute_leaf_hash(secret: Fr, nullifier: Fr) -> Fr {
    let ps = FpVar::Constant(secret);
    let pn = FpVar::Constant(nullifier);
    poseidon2_hash_t3(&ps, &pn, 30).unwrap().value().unwrap()
}

pub fn compute_pool_nullifier_hash(nullifier: Fr) -> Fr {
    let pn = FpVar::Constant(nullifier);
    let zero = FpVar::Constant(Fr::ZERO);
    poseidon2_hash_t3(&pn, &zero, 31).unwrap().value().unwrap()
}

pub fn prove_shielded_insert(
    pk: &ProvingKey<Bn254>,
    old_root: Fr,
    new_root: Fr,
    commitment: Fr,
    leaf_index: u64,
    path_elements: [Fr; TREE_DEPTH],
) -> Result<ProofOutput, SynthesisError> {
    let mut rng = rand::thread_rng();
    let circuit = ShieldedInsert {
        old_root,
        new_root,
        commitment,
        leaf_index,
        path_elements,
    };
    prove_with_pk(
        pk,
        circuit,
        vec![old_root, new_root, commitment, Fr::from(leaf_index)],
        &mut rng,
    )
}

pub fn prove_shielded_withdraw(
    pk: &ProvingKey<Bn254>,
    root: Fr,
    nullifier_hash: Fr,
    recipient: Fr,
    secret: Fr,
    nullifier: Fr,
    path_elements: [Fr; TREE_DEPTH],
    path_indices: [bool; TREE_DEPTH],
) -> Result<ProofOutput, SynthesisError> {
    let mut rng = rand::thread_rng();
    let circuit = ShieldedWithdraw {
        root,
        nullifier_hash,
        recipient,
        secret,
        nullifier,
        path_elements,
        path_indices,
    };
    prove_with_pk(pk, circuit, vec![root, nullifier_hash, recipient], &mut rng)
}

pub fn load_pk(path: impl AsRef<std::path::Path>) -> std::io::Result<ProvingKey<Bn254>> {
    let bytes = std::fs::read(path)?;
    let pk = ProvingKey::deserialize_compressed(bytes.as_slice())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    Ok(pk)
}

fn prove_raw(
    setup: impl ConstraintSynthesizer<Fr>,
    circuit: impl ConstraintSynthesizer<Fr>,
    public_inputs: Vec<Fr>,
    rng: &mut impl rand::Rng,
) -> Result<ProofOutput, SynthesisError> {
    let pk = Groth16::<Bn254>::generate_random_parameters_with_reduction(setup, rng)?;
    let proof = Groth16::<Bn254>::create_random_proof_with_reduction(circuit, &pk, rng)?;
    Ok(ProofOutput {
        proof: ProofHex {
            a: g1_to_hex(&proof.a),
            b: g2_to_hex(&proof.b),
            c: g1_to_hex(&proof.c),
        },
        public_inputs: public_inputs
            .iter()
            .map(|f| f.into_bigint().to_string())
            .collect(),
    })
}

fn prove_with_pk(
    pk: &ProvingKey<Bn254>,
    circuit: impl ConstraintSynthesizer<Fr>,
    public_inputs: Vec<Fr>,
    rng: &mut impl rand::Rng,
) -> Result<ProofOutput, SynthesisError> {
    let proof = Groth16::<Bn254>::create_random_proof_with_reduction(circuit, pk, rng)?;
    Ok(ProofOutput {
        proof: ProofHex {
            a: g1_to_hex(&proof.a),
            b: g2_to_hex(&proof.b),
            c: g1_to_hex(&proof.c),
        },
        public_inputs: public_inputs
            .iter()
            .map(|f| f.into_bigint().to_string())
            .collect(),
    })
}

pub fn compute_commitment(
    side: Fr,
    price: Fr,
    size: Fr,
    leverage: Fr,
    asset: Fr,
    is_market: Fr,
    nonce: Fr,
    secret: Fr,
) -> Fr {
    let ps = FpVar::Constant(side);
    let pp = FpVar::Constant(price);
    let pz = FpVar::Constant(size);
    let pl = FpVar::Constant(leverage);
    let pa = FpVar::Constant(asset);
    let pm = FpVar::Constant(is_market);
    let pn = FpVar::Constant(nonce);
    let ps2 = FpVar::Constant(secret);

    let h1 = poseidon2_hash_t3(&ps, &pp, 1).unwrap();
    let h2 = poseidon2_hash_t3(&h1, &pz, 2).unwrap();
    let h3 = poseidon2_hash_t3(&h2, &pl, 3).unwrap();
    let h4 = poseidon2_hash_t3(&h3, &pa, 4).unwrap();
    let h5 = poseidon2_hash_t3(&h4, &pm, 5).unwrap();
    let h6 = poseidon2_hash_t3(&h5, &pn, 6).unwrap();
    let h7 = poseidon2_hash_t3(&h6, &ps2, 7).unwrap();
    h7.value().unwrap()
}

pub fn compute_portfolio_key(secret: Fr) -> Fr {
    let ps = FpVar::Constant(secret);
    let zero = FpVar::Constant(Fr::from(0u64));
    poseidon2_hash_t3(&ps, &zero, 20).unwrap().value().unwrap()
}

pub fn compute_nullifier(cmt: Fr, secret: Fr) -> Fr {
    let pc = FpVar::Constant(cmt);
    let ps = FpVar::Constant(secret);
    poseidon2_hash_t3(&pc, &ps, 3).unwrap().value().unwrap()
}

pub fn compute_match_nullifier(cmt: Fr, mp: Fr, ms: Fr) -> Fr {
    let pc = FpVar::Constant(cmt);
    let pm = FpVar::Constant(mp);
    let ps = FpVar::Constant(ms);
    poseidon2_hash_t4(&[pc, pm, ps], 10)
        .unwrap()
        .value()
        .unwrap()
}

pub fn prove_commitment(
    side: Fr,
    price: Fr,
    size: Fr,
    leverage: Fr,
    asset: Fr,
    is_market: Fr,
    nonce: Fr,
    secret: Fr,
    use_cross: bool,
) -> Result<ProofOutput, SynthesisError> {
    let cmt = compute_commitment(side, price, size, leverage, asset, is_market, nonce, secret);
    let cross_fr = if use_cross { Fr::from(1u64) } else { Fr::ZERO };
    let pk = if use_cross {
        compute_portfolio_key(secret)
    } else {
        Fr::ZERO
    };
    let mut rng = rand::thread_rng();
    let setup = OrderCommitment {
        side: Fr::ZERO,
        price: Fr::ZERO,
        size: Fr::ZERO,
        leverage: Fr::ZERO,
        asset: Fr::ZERO,
        is_market: Fr::ZERO,
        nonce: Fr::ZERO,
        secret: Fr::ZERO,
        commitment: Fr::ZERO,
        use_cross: Fr::ZERO,
        portfolio_key: Fr::ZERO,
    };
    let circuit = OrderCommitment {
        side,
        price,
        size,
        leverage,
        asset,
        is_market,
        nonce,
        secret,
        commitment: cmt,
        use_cross: cross_fr,
        portfolio_key: pk,
    };
    prove_raw(setup, circuit, vec![cmt, pk], &mut rng)
}

pub fn prove_cancel(commitment: Fr, secret: Fr) -> Result<ProofOutput, SynthesisError> {
    let nullifier = compute_nullifier(commitment, secret);
    let mut rng = rand::thread_rng();
    let setup = OrderCancel {
        commitment: Fr::ZERO,
        secret: Fr::ZERO,
        nullifier: Fr::ZERO,
    };
    let circuit = OrderCancel {
        commitment,
        secret,
        nullifier,
    };
    prove_raw(setup, circuit, vec![nullifier], &mut rng)
}

pub fn prove_commitment_with_pk(
    pk: &ProvingKey<Bn254>,
    side: Fr,
    price: Fr,
    size: Fr,
    leverage: Fr,
    asset: Fr,
    is_market: Fr,
    nonce: Fr,
    secret: Fr,
    use_cross: bool,
) -> Result<ProofOutput, SynthesisError> {
    let cmt = compute_commitment(side, price, size, leverage, asset, is_market, nonce, secret);
    let cross_fr = if use_cross { Fr::from(1u64) } else { Fr::ZERO };
    let portfolio_key = if use_cross {
        compute_portfolio_key(secret)
    } else {
        Fr::ZERO
    };
    let mut rng = rand::thread_rng();
    let circuit = OrderCommitment {
        side,
        price,
        size,
        leverage,
        asset,
        is_market,
        nonce,
        secret,
        commitment: cmt,
        use_cross: cross_fr,
        portfolio_key,
    };
    prove_with_pk(pk, circuit, vec![cmt, portfolio_key], &mut rng)
}

pub fn prove_match(
    a_side: Fr,
    a_price: Fr,
    a_size: Fr,
    a_lev: Fr,
    a_asset: Fr,
    a_market: Fr,
    a_nonce: Fr,
    a_secret: Fr,
    b_side: Fr,
    b_price: Fr,
    b_size: Fr,
    b_lev: Fr,
    b_asset: Fr,
    b_market: Fr,
    b_nonce: Fr,
    b_secret: Fr,
    mp: Fr,
    ms: Fr,
) -> Result<ProofOutput, SynthesisError> {
    let cmt_a = compute_commitment(
        a_side, a_price, a_size, a_lev, a_asset, a_market, a_nonce, a_secret,
    );
    let cmt_b = compute_commitment(
        b_side, b_price, b_size, b_lev, b_asset, b_market, b_nonce, b_secret,
    );
    let null_a = compute_match_nullifier(cmt_a, mp, ms);
    let null_b = compute_match_nullifier(cmt_b, mp, ms);

    let mut rng = rand::thread_rng();
    let setup = OrderMatch {
        side_a: Fr::ZERO,
        price_a: Fr::ZERO,
        size_a: Fr::ZERO,
        leverage_a: Fr::ZERO,
        asset_a: Fr::ZERO,
        is_market_a: Fr::ZERO,
        nonce_a: Fr::ZERO,
        secret_a: Fr::ZERO,
        side_b: Fr::ONE,
        price_b: Fr::ZERO,
        size_b: Fr::ZERO,
        leverage_b: Fr::ZERO,
        asset_b: Fr::ZERO,
        is_market_b: Fr::ZERO,
        nonce_b: Fr::ZERO,
        secret_b: Fr::ZERO,
        mp: Fr::ZERO,
        ms: Fr::ZERO,
        cmt_a: Fr::ZERO,
        cmt_b: Fr::ZERO,
        match_price: Fr::ZERO,
        match_size: Fr::ZERO,
        nullifier_a: Fr::ZERO,
        nullifier_b: Fr::ZERO,
    };
    let circuit = OrderMatch {
        side_a: a_side,
        price_a: a_price,
        size_a: a_size,
        leverage_a: a_lev,
        asset_a: a_asset,
        is_market_a: a_market,
        nonce_a: a_nonce,
        secret_a: a_secret,
        side_b: b_side,
        price_b: b_price,
        size_b: b_size,
        leverage_b: b_lev,
        asset_b: b_asset,
        is_market_b: b_market,
        nonce_b: b_nonce,
        secret_b: b_secret,
        mp,
        ms,
        cmt_a,
        cmt_b,
        match_price: mp,
        match_size: ms,
        nullifier_a: null_a,
        nullifier_b: null_b,
    };
    let public = vec![cmt_a, cmt_b, mp, ms, null_a, null_b];
    prove_raw(setup, circuit, public, &mut rng)
}

pub fn compute_note_commitment(amount: Fr, secret: Fr) -> Fr {
    let pa = FpVar::Constant(amount);
    let ps = FpVar::Constant(secret);
    poseidon2_hash_t3(&pa, &ps, 8).unwrap().value().unwrap()
}

pub fn compute_note_nullifier(note_commitment: Fr, secret: Fr) -> Fr {
    let pc = FpVar::Constant(note_commitment);
    let ps = FpVar::Constant(secret);
    poseidon2_hash_t3(&pc, &ps, 9).unwrap().value().unwrap()
}

pub fn prove_note_spend(amount: Fr, secret: Fr) -> Result<ProofOutput, SynthesisError> {
    let note_cmt = compute_note_commitment(amount, secret);
    let nullifier = compute_note_nullifier(note_cmt, secret);
    let mut rng = rand::thread_rng();
    let setup = NoteSpend {
        amount: Fr::ZERO,
        secret: Fr::ZERO,
        note_commitment: Fr::ZERO,
        nullifier: Fr::ZERO,
    };
    let circuit = NoteSpend {
        amount,
        secret,
        note_commitment: note_cmt,
        nullifier,
    };
    prove_raw(setup, circuit, vec![note_cmt, nullifier], &mut rng)
}

pub fn prove_note_spend_with_pk(
    pk: &ProvingKey<Bn254>,
    amount: Fr,
    secret: Fr,
) -> Result<ProofOutput, SynthesisError> {
    let note_cmt = compute_note_commitment(amount, secret);
    let nullifier = compute_note_nullifier(note_cmt, secret);
    let mut rng = rand::thread_rng();
    let circuit = NoteSpend {
        amount,
        secret,
        note_commitment: note_cmt,
        nullifier,
    };
    prove_with_pk(pk, circuit, vec![note_cmt, nullifier], &mut rng)
}

pub fn prove_cancel_with_pk(
    pk: &ProvingKey<Bn254>,
    commitment: Fr,
    secret: Fr,
) -> Result<ProofOutput, SynthesisError> {
    let nullifier = compute_nullifier(commitment, secret);
    let mut rng = rand::thread_rng();
    let circuit = OrderCancel {
        commitment,
        secret,
        nullifier,
    };
    prove_with_pk(pk, circuit, vec![nullifier], &mut rng)
}

pub fn prove_match_with_pk(
    pk: &ProvingKey<Bn254>,
    a_side: Fr,
    a_price: Fr,
    a_size: Fr,
    a_lev: Fr,
    a_asset: Fr,
    a_market: Fr,
    a_nonce: Fr,
    a_secret: Fr,
    b_side: Fr,
    b_price: Fr,
    b_size: Fr,
    b_lev: Fr,
    b_asset: Fr,
    b_market: Fr,
    b_nonce: Fr,
    b_secret: Fr,
    mp: Fr,
    ms: Fr,
) -> Result<ProofOutput, SynthesisError> {
    let cmt_a = compute_commitment(
        a_side, a_price, a_size, a_lev, a_asset, a_market, a_nonce, a_secret,
    );
    let cmt_b = compute_commitment(
        b_side, b_price, b_size, b_lev, b_asset, b_market, b_nonce, b_secret,
    );
    let null_a = compute_match_nullifier(cmt_a, mp, ms);
    let null_b = compute_match_nullifier(cmt_b, mp, ms);

    let mut rng = rand::thread_rng();
    let circuit = OrderMatch {
        side_a: a_side,
        price_a: a_price,
        size_a: a_size,
        leverage_a: a_lev,
        asset_a: a_asset,
        is_market_a: a_market,
        nonce_a: a_nonce,
        secret_a: a_secret,
        side_b: b_side,
        price_b: b_price,
        size_b: b_size,
        leverage_b: b_lev,
        asset_b: b_asset,
        is_market_b: b_market,
        nonce_b: b_nonce,
        secret_b: b_secret,
        mp,
        ms,
        cmt_a,
        cmt_b,
        match_price: mp,
        match_size: ms,
        nullifier_a: null_a,
        nullifier_b: null_b,
    };
    let public = vec![cmt_a, cmt_b, mp, ms, null_a, null_b];
    prove_with_pk(pk, circuit, public, &mut rng)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_ff::Field;
    use ark_groth16::prepare_verifying_key;

    fn make_order_fields(
        side: u64,
        price: u64,
        size: u64,
        leverage: u64,
        asset: u64,
        is_market: u64,
        nonce: u64,
        secret: u64,
    ) -> [Fr; 8] {
        [
            Fr::from(side),
            Fr::from(price),
            Fr::from(size),
            Fr::from(leverage),
            Fr::from(asset),
            Fr::from(is_market),
            Fr::from(nonce),
            Fr::from(secret),
        ]
    }

    #[test]
    fn test_commitment_cs_satisfied() {
        let fields = make_order_fields(0, 100, 10, 1, 5, 0, 42, 123456);
        let cmt = compute_commitment(
            fields[0], fields[1], fields[2], fields[3], fields[4], fields[5], fields[6], fields[7],
        );
        use ark_relations::gr1cs::ConstraintSystem;
        let cs = ConstraintSystem::new_ref();
        let circuit = OrderCommitment {
            side: fields[0],
            price: fields[1],
            size: fields[2],
            leverage: fields[3],
            asset: fields[4],
            is_market: fields[5],
            nonce: fields[6],
            secret: fields[7],
            commitment: cmt,
            use_cross: Fr::ZERO,
            portfolio_key: Fr::ZERO,
        };
        circuit.generate_constraints(cs.clone()).unwrap();
        assert!(cs.is_satisfied().unwrap());
    }

    #[test]
    fn test_commitment_groth16() -> Result<(), SynthesisError> {
        let rng = &mut ark_std::test_rng();
        let fields = make_order_fields(0, 100, 10, 1, 5, 0, 42, 123456);
        let cmt = compute_commitment(
            fields[0], fields[1], fields[2], fields[3], fields[4], fields[5], fields[6], fields[7],
        );
        let setup_circuit = OrderCommitment {
            side: Fr::ZERO,
            price: Fr::ZERO,
            size: Fr::ZERO,
            leverage: Fr::ZERO,
            asset: Fr::ZERO,
            is_market: Fr::ZERO,
            nonce: Fr::ZERO,
            secret: Fr::ZERO,
            commitment: Fr::ZERO,
            use_cross: Fr::ZERO,
            portfolio_key: Fr::ZERO,
        };
        let pk = Groth16::<Bn254>::generate_random_parameters_with_reduction(setup_circuit, rng)?;
        let vk = pk.vk.clone();
        let prove_circuit = OrderCommitment {
            side: fields[0],
            price: fields[1],
            size: fields[2],
            leverage: fields[3],
            asset: fields[4],
            is_market: fields[5],
            nonce: fields[6],
            secret: fields[7],
            commitment: cmt,
            use_cross: Fr::ZERO,
            portfolio_key: Fr::ZERO,
        };
        let proof = Groth16::<Bn254>::create_random_proof_with_reduction(prove_circuit, &pk, rng)?;
        let pvk = prepare_verifying_key(&vk);
        assert!(Groth16::<Bn254>::verify_proof(
            &pvk,
            &proof,
            &[cmt, Fr::ZERO]
        )?);
        Ok(())
    }

    #[test]
    fn test_cancel_cs_satisfied() {
        use ark_relations::gr1cs::ConstraintSystem;
        let cmt = compute_commitment(
            Fr::from(0),
            Fr::from(100),
            Fr::from(10),
            Fr::from(1),
            Fr::from(5),
            Fr::from(0),
            Fr::from(42),
            Fr::from(123456),
        );
        let null = compute_nullifier(cmt, Fr::from(123456));
        let cs = ConstraintSystem::new_ref();
        let circuit = OrderCancel {
            commitment: cmt,
            secret: Fr::from(123456),
            nullifier: null,
        };
        circuit.generate_constraints(cs.clone()).unwrap();
        assert!(cs.is_satisfied().unwrap());
    }

    #[test]
    fn test_cancel_groth16() -> Result<(), SynthesisError> {
        let rng = &mut ark_std::test_rng();
        let cmt = compute_commitment(
            Fr::from(0),
            Fr::from(100),
            Fr::from(10),
            Fr::from(1),
            Fr::from(5),
            Fr::from(0),
            Fr::from(42),
            Fr::from(123456),
        );
        let null = compute_nullifier(cmt, Fr::from(123456));
        let setup_circuit = OrderCancel {
            commitment: Fr::ZERO,
            secret: Fr::ZERO,
            nullifier: Fr::ZERO,
        };
        let pk = Groth16::<Bn254>::generate_random_parameters_with_reduction(setup_circuit, rng)?;
        let vk = pk.vk.clone();
        let prove_circuit = OrderCancel {
            commitment: cmt,
            secret: Fr::from(123456),
            nullifier: null,
        };
        let proof = Groth16::<Bn254>::create_random_proof_with_reduction(prove_circuit, &pk, rng)?;
        let pvk = prepare_verifying_key(&vk);
        assert!(Groth16::<Bn254>::verify_proof(&pvk, &proof, &[null])?);
        Ok(())
    }

    fn make_valid_match_circuit() -> (OrderMatch, [Fr; 6]) {
        let a = make_order_fields(0, 100, 10, 1, 5, 0, 42, 123456);
        let b = make_order_fields(1, 100, 15, 2, 5, 0, 7, 789012);
        let mp = Fr::from(100u64);
        let ms = Fr::from(8u64);
        let cmt_a = compute_commitment(a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7]);
        let cmt_b = compute_commitment(b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]);
        let null_a = compute_match_nullifier(cmt_a, mp, ms);
        let null_b = compute_match_nullifier(cmt_b, mp, ms);
        let circuit = OrderMatch {
            side_a: a[0],
            price_a: a[1],
            size_a: a[2],
            leverage_a: a[3],
            asset_a: a[4],
            is_market_a: a[5],
            nonce_a: a[6],
            secret_a: a[7],
            side_b: b[0],
            price_b: b[1],
            size_b: b[2],
            leverage_b: b[3],
            asset_b: b[4],
            is_market_b: b[5],
            nonce_b: b[6],
            secret_b: b[7],
            mp,
            ms,
            cmt_a,
            cmt_b,
            match_price: mp,
            match_size: ms,
            nullifier_a: null_a,
            nullifier_b: null_b,
        };
        let public = [cmt_a, cmt_b, mp, ms, null_a, null_b];
        (circuit, public)
    }

    #[test]
    fn test_match_cs_satisfied() {
        use ark_relations::gr1cs::ConstraintSystem;
        let (circuit, _) = make_valid_match_circuit();
        let cs = ConstraintSystem::new_ref();
        circuit.generate_constraints(cs.clone()).unwrap();
        assert!(cs.is_satisfied().unwrap());
    }

    #[test]
    fn test_match_groth16() -> Result<(), SynthesisError> {
        let rng = &mut ark_std::test_rng();
        let (prove_circuit, public) = make_valid_match_circuit();
        let dummy = OrderMatch {
            side_a: Fr::ZERO,
            price_a: Fr::ZERO,
            size_a: Fr::ZERO,
            leverage_a: Fr::ZERO,
            asset_a: Fr::ZERO,
            is_market_a: Fr::ZERO,
            nonce_a: Fr::ZERO,
            secret_a: Fr::ZERO,
            side_b: Fr::ONE,
            price_b: Fr::ZERO,
            size_b: Fr::ZERO,
            leverage_b: Fr::ZERO,
            asset_b: Fr::ZERO,
            is_market_b: Fr::ZERO,
            nonce_b: Fr::ZERO,
            secret_b: Fr::ZERO,
            mp: Fr::ZERO,
            ms: Fr::ZERO,
            cmt_a: Fr::ZERO,
            cmt_b: Fr::ZERO,
            match_price: Fr::ZERO,
            match_size: Fr::ZERO,
            nullifier_a: Fr::ZERO,
            nullifier_b: Fr::ZERO,
        };
        let pk = Groth16::<Bn254>::generate_random_parameters_with_reduction(dummy, rng)?;
        let vk = pk.vk.clone();
        let proof = Groth16::<Bn254>::create_random_proof_with_reduction(prove_circuit, &pk, rng)?;
        let pvk = prepare_verifying_key(&vk);
        assert!(Groth16::<Bn254>::verify_proof(&pvk, &proof, &public)?);
        Ok(())
    }

    #[test]
    fn test_match_invalid_asset() {
        use ark_relations::gr1cs::ConstraintSystem;
        let a = make_order_fields(0, 100, 10, 1, 5, 0, 42, 123456);
        let b = make_order_fields(1, 100, 15, 2, 99, 0, 7, 789012);
        let mp = Fr::from(100u64);
        let ms = Fr::from(8u64);
        let cmt_a = compute_commitment(a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7]);
        let cmt_b = compute_commitment(b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]);
        let null_a = compute_match_nullifier(cmt_a, mp, ms);
        let null_b = compute_match_nullifier(cmt_b, mp, ms);
        let circuit = OrderMatch {
            side_a: a[0],
            price_a: a[1],
            size_a: a[2],
            leverage_a: a[3],
            asset_a: a[4],
            is_market_a: a[5],
            nonce_a: a[6],
            secret_a: a[7],
            side_b: b[0],
            price_b: b[1],
            size_b: b[2],
            leverage_b: b[3],
            asset_b: b[4],
            is_market_b: b[5],
            nonce_b: b[6],
            secret_b: b[7],
            mp,
            ms,
            cmt_a,
            cmt_b,
            match_price: mp,
            match_size: ms,
            nullifier_a: null_a,
            nullifier_b: null_b,
        };
        let cs = ConstraintSystem::new_ref();
        circuit.generate_constraints(cs.clone()).unwrap();
        assert!(!cs.is_satisfied().unwrap());
    }

    #[test]
    fn test_match_invalid_side() {
        use ark_relations::gr1cs::ConstraintSystem;
        let a = make_order_fields(0, 100, 10, 1, 5, 0, 42, 123456);
        let b = make_order_fields(0, 100, 15, 2, 5, 0, 7, 789012);
        let mp = Fr::from(100u64);
        let ms = Fr::from(8u64);
        let cmt_a = compute_commitment(a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7]);
        let cmt_b = compute_commitment(b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]);
        let null_a = compute_match_nullifier(cmt_a, mp, ms);
        let null_b = compute_match_nullifier(cmt_b, mp, ms);
        let circuit = OrderMatch {
            side_a: a[0],
            price_a: a[1],
            size_a: a[2],
            leverage_a: a[3],
            asset_a: a[4],
            is_market_a: a[5],
            nonce_a: a[6],
            secret_a: a[7],
            side_b: b[0],
            price_b: b[1],
            size_b: b[2],
            leverage_b: b[3],
            asset_b: b[4],
            is_market_b: b[5],
            nonce_b: b[6],
            secret_b: b[7],
            mp,
            ms,
            cmt_a,
            cmt_b,
            match_price: mp,
            match_size: ms,
            nullifier_a: null_a,
            nullifier_b: null_b,
        };
        let cs = ConstraintSystem::new_ref();
        circuit.generate_constraints(cs.clone()).unwrap();
        assert!(!cs.is_satisfied().unwrap());
    }

    #[test]
    fn test_match_market_limit() {
        let rng = &mut ark_std::test_rng();
        let a = make_order_fields(0, 0, 10, 1, 5, 1, 42, 123456);
        let b = make_order_fields(1, 100, 15, 2, 5, 0, 7, 789012);
        let mp = Fr::from(100u64);
        let ms = Fr::from(8u64);
        let cmt_a = compute_commitment(a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7]);
        let cmt_b = compute_commitment(b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]);
        let null_a = compute_match_nullifier(cmt_a, mp, ms);
        let null_b = compute_match_nullifier(cmt_b, mp, ms);
        let circuit = OrderMatch {
            side_a: a[0],
            price_a: a[1],
            size_a: a[2],
            leverage_a: a[3],
            asset_a: a[4],
            is_market_a: a[5],
            nonce_a: a[6],
            secret_a: a[7],
            side_b: b[0],
            price_b: b[1],
            size_b: b[2],
            leverage_b: b[3],
            asset_b: b[4],
            is_market_b: b[5],
            nonce_b: b[6],
            secret_b: b[7],
            mp,
            ms,
            cmt_a,
            cmt_b,
            match_price: mp,
            match_size: ms,
            nullifier_a: null_a,
            nullifier_b: null_b,
        };
        let dummy = OrderMatch {
            side_a: Fr::ZERO,
            price_a: Fr::ZERO,
            size_a: Fr::ZERO,
            leverage_a: Fr::ZERO,
            asset_a: Fr::ZERO,
            is_market_a: Fr::ZERO,
            nonce_a: Fr::ZERO,
            secret_a: Fr::ZERO,
            side_b: Fr::ONE,
            price_b: Fr::ZERO,
            size_b: Fr::ZERO,
            leverage_b: Fr::ZERO,
            asset_b: Fr::ZERO,
            is_market_b: Fr::ZERO,
            nonce_b: Fr::ZERO,
            secret_b: Fr::ZERO,
            mp: Fr::ZERO,
            ms: Fr::ZERO,
            cmt_a: Fr::ZERO,
            cmt_b: Fr::ZERO,
            match_price: Fr::ZERO,
            match_size: Fr::ZERO,
            nullifier_a: Fr::ZERO,
            nullifier_b: Fr::ZERO,
        };
        let pk = Groth16::<Bn254>::generate_random_parameters_with_reduction(dummy, rng).unwrap();
        let vk = pk.vk.clone();
        let proof =
            Groth16::<Bn254>::create_random_proof_with_reduction(circuit, &pk, rng).unwrap();
        let pvk = prepare_verifying_key(&vk);
        let public = [cmt_a, cmt_b, mp, ms, null_a, null_b];
        assert!(Groth16::<Bn254>::verify_proof(&pvk, &proof, &public).unwrap());
    }

    #[test]
    fn test_match_invalid_price_bid_too_low() {
        use ark_relations::gr1cs::ConstraintSystem;
        let a = make_order_fields(0, 90, 10, 1, 5, 0, 42, 123456);
        let b = make_order_fields(1, 100, 15, 2, 5, 0, 7, 789012);
        let mp = Fr::from(95u64);
        let ms = Fr::from(8u64);
        let cmt_a = compute_commitment(a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7]);
        let cmt_b = compute_commitment(b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]);
        let null_a = compute_match_nullifier(cmt_a, mp, ms);
        let null_b = compute_match_nullifier(cmt_b, mp, ms);
        let circuit = OrderMatch {
            side_a: a[0],
            price_a: a[1],
            size_a: a[2],
            leverage_a: a[3],
            asset_a: a[4],
            is_market_a: a[5],
            nonce_a: a[6],
            secret_a: a[7],
            side_b: b[0],
            price_b: b[1],
            size_b: b[2],
            leverage_b: b[3],
            asset_b: b[4],
            is_market_b: b[5],
            nonce_b: b[6],
            secret_b: b[7],
            mp,
            ms,
            cmt_a,
            cmt_b,
            match_price: mp,
            match_size: ms,
            nullifier_a: null_a,
            nullifier_b: null_b,
        };
        let cs = ConstraintSystem::new_ref();
        circuit.generate_constraints(cs.clone()).unwrap();
        assert!(!cs.is_satisfied().unwrap());
    }

    #[test]
    fn test_note_spend_cs_satisfied() {
        use ark_relations::gr1cs::ConstraintSystem;
        let amount = Fr::from(1_000_000u64);
        let secret = Fr::from(999888777u64);
        let note_cmt = compute_note_commitment(amount, secret);
        let nullifier = compute_note_nullifier(note_cmt, secret);
        let cs = ConstraintSystem::new_ref();
        let circuit = circuits::note_spend::NoteSpend {
            amount,
            secret,
            note_commitment: note_cmt,
            nullifier,
        };
        circuit.generate_constraints(cs.clone()).unwrap();
        assert!(cs.is_satisfied().unwrap());
    }

    #[test]
    fn test_note_spend_groth16() -> Result<(), SynthesisError> {
        let rng = &mut ark_std::test_rng();
        let amount = Fr::from(500_000u64);
        let secret = Fr::from(42424242u64);
        let note_cmt = compute_note_commitment(amount, secret);
        let nullifier = compute_note_nullifier(note_cmt, secret);
        let setup_circuit = circuits::note_spend::NoteSpend {
            amount: Fr::ZERO,
            secret: Fr::ZERO,
            note_commitment: Fr::ZERO,
            nullifier: Fr::ZERO,
        };
        let pk = Groth16::<Bn254>::generate_random_parameters_with_reduction(setup_circuit, rng)?;
        let vk = pk.vk.clone();
        let prove_circuit = circuits::note_spend::NoteSpend {
            amount,
            secret,
            note_commitment: note_cmt,
            nullifier,
        };
        let proof = Groth16::<Bn254>::create_random_proof_with_reduction(prove_circuit, &pk, rng)?;
        let pvk = prepare_verifying_key(&vk);
        assert!(Groth16::<Bn254>::verify_proof(
            &pvk,
            &proof,
            &[note_cmt, nullifier]
        )?);
        Ok(())
    }

    #[test]
    fn test_note_spend_wrong_nullifier_unsatisfied() {
        use ark_relations::gr1cs::ConstraintSystem;
        let amount = Fr::from(1000u64);
        let secret = Fr::from(12345u64);
        let note_cmt = compute_note_commitment(amount, secret);
        let wrong_null = Fr::from(999u64);
        let cs = ConstraintSystem::new_ref();
        let circuit = circuits::note_spend::NoteSpend {
            amount,
            secret,
            note_commitment: note_cmt,
            nullifier: wrong_null,
        };
        circuit.generate_constraints(cs.clone()).unwrap();
        assert!(!cs.is_satisfied().unwrap());
    }

    #[test]
    fn test_match_invalid_size_too_big() {
        use ark_relations::gr1cs::ConstraintSystem;
        let a = make_order_fields(0, 100, 10, 1, 5, 0, 42, 123456);
        let b = make_order_fields(1, 100, 15, 2, 5, 0, 7, 789012);
        let mp = Fr::from(100u64);
        let ms = Fr::from(12u64);
        let cmt_a = compute_commitment(a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7]);
        let cmt_b = compute_commitment(b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]);
        let null_a = compute_match_nullifier(cmt_a, mp, ms);
        let null_b = compute_match_nullifier(cmt_b, mp, ms);
        let circuit = OrderMatch {
            side_a: a[0],
            price_a: a[1],
            size_a: a[2],
            leverage_a: a[3],
            asset_a: a[4],
            is_market_a: a[5],
            nonce_a: a[6],
            secret_a: a[7],
            side_b: b[0],
            price_b: b[1],
            size_b: b[2],
            leverage_b: b[3],
            asset_b: b[4],
            is_market_b: b[5],
            nonce_b: b[6],
            secret_b: b[7],
            mp,
            ms,
            cmt_a,
            cmt_b,
            match_price: mp,
            match_size: ms,
            nullifier_a: null_a,
            nullifier_b: null_b,
        };
        let cs = ConstraintSystem::new_ref();
        circuit.generate_constraints(cs.clone()).unwrap();
        assert!(!cs.is_satisfied().unwrap());
    }

    // ── Shielded pool tests ──────────────────────────────────────────────────

    #[test]
    fn test_shielded_insert_empty_tree() {
        use ark_relations::gr1cs::ConstraintSystem;

        let secret = Fr::from(42u64);
        let nullifier = Fr::from(99u64);
        let commitment = compute_leaf_hash(secret, nullifier);

        let old_root = compute_empty_root();
        let path = compute_merkle_path(&[], 0);
        let new_root = compute_new_root(commitment, 0, &path);

        let circuit = circuits::shielded_insert::ShieldedInsert {
            old_root,
            new_root,
            commitment,
            leaf_index: 0,
            path_elements: path,
        };
        let cs = ConstraintSystem::new_ref();
        circuit.generate_constraints(cs.clone()).unwrap();
        assert!(cs.is_satisfied().unwrap());
    }

    #[test]
    fn test_shielded_insert_wrong_new_root_unsatisfied() {
        use ark_relations::gr1cs::ConstraintSystem;

        let commitment = compute_leaf_hash(Fr::from(1u64), Fr::from(2u64));
        let old_root = compute_empty_root();
        let path = compute_merkle_path(&[], 0);
        let real_new_root = compute_new_root(commitment, 0, &path);
        let wrong_new_root = real_new_root + Fr::from(1u64);

        let circuit = circuits::shielded_insert::ShieldedInsert {
            old_root,
            new_root: wrong_new_root,
            commitment,
            leaf_index: 0,
            path_elements: path,
        };
        let cs = ConstraintSystem::new_ref();
        circuit.generate_constraints(cs.clone()).unwrap();
        assert!(!cs.is_satisfied().unwrap());
    }

    #[test]
    fn test_shielded_withdraw_after_insert() {
        use ark_relations::gr1cs::ConstraintSystem;

        let secret = Fr::from(1234u64);
        let nullifier = Fr::from(5678u64);
        let commitment = compute_leaf_hash(secret, nullifier);
        let nullifier_hash = compute_pool_nullifier_hash(nullifier);
        let recipient = Fr::from(0xdeadbeefu64);

        let leaves = vec![commitment];
        let leaf_index = 0usize;
        let root = compute_root_from_leaves(&leaves);
        let path = compute_merkle_path(&leaves, leaf_index);
        let mut path_indices = [false; TREE_DEPTH];
        for i in 0..TREE_DEPTH {
            path_indices[i] = ((leaf_index >> i) & 1) == 1;
        }

        let circuit = circuits::shielded_withdraw::ShieldedWithdraw {
            root,
            nullifier_hash,
            recipient,
            secret,
            nullifier,
            path_elements: path,
            path_indices,
        };
        let cs = ConstraintSystem::new_ref();
        circuit.generate_constraints(cs.clone()).unwrap();
        assert!(cs.is_satisfied().unwrap());
    }

    #[test]
    fn test_shielded_withdraw_wrong_nullifier_unsatisfied() {
        use ark_relations::gr1cs::ConstraintSystem;

        let secret = Fr::from(1u64);
        let nullifier = Fr::from(2u64);
        let commitment = compute_leaf_hash(secret, nullifier);
        let wrong_null_hash = Fr::from(999u64); // not Poseidon(nullifier, 0, 31)

        let leaves = vec![commitment];
        let root = compute_root_from_leaves(&leaves);
        let path = compute_merkle_path(&leaves, 0);

        let circuit = circuits::shielded_withdraw::ShieldedWithdraw {
            root,
            nullifier_hash: wrong_null_hash,
            recipient: Fr::from(1u64),
            secret,
            nullifier,
            path_elements: path,
            path_indices: [false; TREE_DEPTH],
        };
        let cs = ConstraintSystem::new_ref();
        circuit.generate_constraints(cs.clone()).unwrap();
        assert!(!cs.is_satisfied().unwrap());
    }

    #[test]
    fn test_shielded_insert_groth16() -> Result<(), SynthesisError> {
        let rng = &mut ark_std::test_rng();
        let commitment = compute_leaf_hash(Fr::from(7u64), Fr::from(13u64));
        let old_root = compute_empty_root();
        let path = compute_merkle_path(&[], 0);
        let new_root = compute_new_root(commitment, 0, &path);

        let dummy = circuits::shielded_insert::ShieldedInsert::dummy();
        let pk = Groth16::<Bn254>::generate_random_parameters_with_reduction(dummy, rng)?;
        let vk = pk.vk.clone();

        let circuit = circuits::shielded_insert::ShieldedInsert {
            old_root,
            new_root,
            commitment,
            leaf_index: 0,
            path_elements: path,
        };
        let proof = Groth16::<Bn254>::create_random_proof_with_reduction(circuit, &pk, rng)?;
        let pvk = ark_groth16::prepare_verifying_key(&vk);
        let public = [old_root, new_root, commitment, Fr::from(0u64)];
        assert!(Groth16::<Bn254>::verify_proof(&pvk, &proof, &public)?);
        Ok(())
    }

    #[test]
    fn test_shielded_withdraw_groth16() -> Result<(), SynthesisError> {
        let rng = &mut ark_std::test_rng();
        let secret = Fr::from(333u64);
        let nullifier = Fr::from(444u64);
        let commitment = compute_leaf_hash(secret, nullifier);
        let nullifier_hash = compute_pool_nullifier_hash(nullifier);
        let recipient = Fr::from(0xabcdu64);

        let leaves = vec![commitment];
        let root = compute_root_from_leaves(&leaves);
        let path = compute_merkle_path(&leaves, 0);

        let dummy = circuits::shielded_withdraw::ShieldedWithdraw::dummy();
        let pk = Groth16::<Bn254>::generate_random_parameters_with_reduction(dummy, rng)?;
        let vk = pk.vk.clone();

        let circuit = circuits::shielded_withdraw::ShieldedWithdraw {
            root,
            nullifier_hash,
            recipient,
            secret,
            nullifier,
            path_elements: path,
            path_indices: [false; TREE_DEPTH],
        };
        let proof = Groth16::<Bn254>::create_random_proof_with_reduction(circuit, &pk, rng)?;
        let pvk = ark_groth16::prepare_verifying_key(&vk);
        assert!(Groth16::<Bn254>::verify_proof(
            &pvk,
            &proof,
            &[root, nullifier_hash, recipient]
        )?);
        Ok(())
    }
}
