pub mod circuits;
pub mod poseidon2;

use ark_bn254::{Bn254, Fr, G1Affine, G2Affine};
use ark_ff::{AdditiveGroup, BigInteger, Field, PrimeField};
use ark_groth16::Groth16;
use ark_relations::gr1cs::{ConstraintSynthesizer, SynthesisError};
use ark_r1cs_std::fields::fp::FpVar;
use ark_r1cs_std::GR1CSVar;
use circuits::cancel::OrderCancel;
use circuits::commitment::OrderCommitment;
use circuits::match_circuit::OrderMatch;
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

pub fn compute_commitment(
    side: Fr, price: Fr, size: Fr, leverage: Fr,
    asset: Fr, is_market: Fr, nonce: Fr, secret: Fr,
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

pub fn compute_nullifier(cmt: Fr, secret: Fr) -> Fr {
    let pc = FpVar::Constant(cmt);
    let ps = FpVar::Constant(secret);
    poseidon2_hash_t3(&pc, &ps, 3).unwrap().value().unwrap()
}

pub fn compute_match_nullifier(cmt: Fr, mp: Fr, ms: Fr) -> Fr {
    let pc = FpVar::Constant(cmt);
    let pm = FpVar::Constant(mp);
    let ps = FpVar::Constant(ms);
    poseidon2_hash_t4(&[pc, pm, ps], 10).unwrap().value().unwrap()
}

pub fn prove_commitment(
    side: Fr, price: Fr, size: Fr, leverage: Fr,
    asset: Fr, is_market: Fr, nonce: Fr, secret: Fr,
) -> Result<ProofOutput, SynthesisError> {
    let cmt = compute_commitment(side, price, size, leverage, asset, is_market, nonce, secret);
    let mut rng = rand::thread_rng();
    let setup = OrderCommitment {
        side: Fr::ZERO, price: Fr::ZERO, size: Fr::ZERO,
        leverage: Fr::ZERO, asset: Fr::ZERO, is_market: Fr::ZERO,
        nonce: Fr::ZERO, secret: Fr::ZERO, commitment: Fr::ZERO,
    };
    let circuit = OrderCommitment { side, price, size, leverage, asset, is_market, nonce, secret, commitment: cmt };
    prove_raw(setup, circuit, vec![cmt], &mut rng)
}

pub fn prove_cancel(
    commitment: Fr, secret: Fr,
) -> Result<ProofOutput, SynthesisError> {
    let nullifier = compute_nullifier(commitment, secret);
    let mut rng = rand::thread_rng();
    let setup = OrderCancel { commitment: Fr::ZERO, secret: Fr::ZERO, nullifier: Fr::ZERO };
    let circuit = OrderCancel { commitment, secret, nullifier };
    prove_raw(setup, circuit, vec![nullifier], &mut rng)
}

pub fn prove_match(
    a_side: Fr, a_price: Fr, a_size: Fr, a_lev: Fr,
    a_asset: Fr, a_market: Fr, a_nonce: Fr, a_secret: Fr,
    b_side: Fr, b_price: Fr, b_size: Fr, b_lev: Fr,
    b_asset: Fr, b_market: Fr, b_nonce: Fr, b_secret: Fr,
    mp: Fr, ms: Fr,
) -> Result<ProofOutput, SynthesisError> {
    let cmt_a = compute_commitment(a_side, a_price, a_size, a_lev, a_asset, a_market, a_nonce, a_secret);
    let cmt_b = compute_commitment(b_side, b_price, b_size, b_lev, b_asset, b_market, b_nonce, b_secret);
    let null_a = compute_match_nullifier(cmt_a, mp, ms);
    let null_b = compute_match_nullifier(cmt_b, mp, ms);

    let mut rng = rand::thread_rng();
    let setup = OrderMatch {
        side_a: Fr::ZERO, price_a: Fr::ZERO, size_a: Fr::ZERO,
        leverage_a: Fr::ZERO, asset_a: Fr::ZERO, is_market_a: Fr::ZERO,
        nonce_a: Fr::ZERO, secret_a: Fr::ZERO,
        side_b: Fr::ONE, price_b: Fr::ZERO, size_b: Fr::ZERO,
        leverage_b: Fr::ZERO, asset_b: Fr::ZERO, is_market_b: Fr::ZERO,
        nonce_b: Fr::ZERO, secret_b: Fr::ZERO,
        mp: Fr::ZERO, ms: Fr::ZERO,
        cmt_a: Fr::ZERO, cmt_b: Fr::ZERO,
        match_price: Fr::ZERO, match_size: Fr::ZERO,
        nullifier_a: Fr::ZERO, nullifier_b: Fr::ZERO,
    };
    let circuit = OrderMatch {
        side_a: a_side, price_a: a_price, size_a: a_size, leverage_a: a_lev,
        asset_a: a_asset, is_market_a: a_market, nonce_a: a_nonce, secret_a: a_secret,
        side_b: b_side, price_b: b_price, size_b: b_size, leverage_b: b_lev,
        asset_b: b_asset, is_market_b: b_market, nonce_b: b_nonce, secret_b: b_secret,
        mp, ms, cmt_a, cmt_b, match_price: mp, match_size: ms,
        nullifier_a: null_a, nullifier_b: null_b,
    };
    let public = vec![cmt_a, cmt_b, mp, ms, null_a, null_b];
    prove_raw(setup, circuit, public, &mut rng)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_ff::Field;
    use ark_groth16::prepare_verifying_key;

    fn make_order_fields(
        side: u64, price: u64, size: u64, leverage: u64, asset: u64,
        is_market: u64, nonce: u64, secret: u64,
    ) -> [Fr; 8] {
        [
            Fr::from(side), Fr::from(price), Fr::from(size), Fr::from(leverage),
            Fr::from(asset), Fr::from(is_market), Fr::from(nonce), Fr::from(secret),
        ]
    }

    #[test]
    fn test_commitment_cs_satisfied() {
        let fields = make_order_fields(0, 100, 10, 1, 5, 0, 42, 123456);
        let cmt = compute_commitment(fields[0], fields[1], fields[2], fields[3], fields[4], fields[5], fields[6], fields[7]);
        use ark_relations::gr1cs::ConstraintSystem;
        let cs = ConstraintSystem::new_ref();
        let circuit = OrderCommitment { side: fields[0], price: fields[1], size: fields[2], leverage: fields[3], asset: fields[4], is_market: fields[5], nonce: fields[6], secret: fields[7], commitment: cmt };
        circuit.generate_constraints(cs.clone()).unwrap();
        assert!(cs.is_satisfied().unwrap());
    }

    #[test]
    fn test_commitment_groth16() -> Result<(), SynthesisError> {
        let rng = &mut ark_std::test_rng();
        let fields = make_order_fields(0, 100, 10, 1, 5, 0, 42, 123456);
        let cmt = compute_commitment(fields[0], fields[1], fields[2], fields[3], fields[4], fields[5], fields[6], fields[7]);
        let setup_circuit = OrderCommitment { side: Fr::ZERO, price: Fr::ZERO, size: Fr::ZERO, leverage: Fr::ZERO, asset: Fr::ZERO, is_market: Fr::ZERO, nonce: Fr::ZERO, secret: Fr::ZERO, commitment: Fr::ZERO };
        let pk = Groth16::<Bn254>::generate_random_parameters_with_reduction(setup_circuit, rng)?;
        let vk = pk.vk.clone();
        let prove_circuit = OrderCommitment { side: fields[0], price: fields[1], size: fields[2], leverage: fields[3], asset: fields[4], is_market: fields[5], nonce: fields[6], secret: fields[7], commitment: cmt };
        let proof = Groth16::<Bn254>::create_random_proof_with_reduction(prove_circuit, &pk, rng)?;
        let pvk = prepare_verifying_key(&vk);
        assert!(Groth16::<Bn254>::verify_proof(&pvk, &proof, &[cmt])?);
        Ok(())
    }

    #[test]
    fn test_cancel_cs_satisfied() {
        use ark_relations::gr1cs::ConstraintSystem;
        let cmt = compute_commitment(Fr::from(0), Fr::from(100), Fr::from(10), Fr::from(1), Fr::from(5), Fr::from(0), Fr::from(42), Fr::from(123456));
        let null = compute_nullifier(cmt, Fr::from(123456));
        let cs = ConstraintSystem::new_ref();
        let circuit = OrderCancel { commitment: cmt, secret: Fr::from(123456), nullifier: null };
        circuit.generate_constraints(cs.clone()).unwrap();
        assert!(cs.is_satisfied().unwrap());
    }

    #[test]
    fn test_cancel_groth16() -> Result<(), SynthesisError> {
        let rng = &mut ark_std::test_rng();
        let cmt = compute_commitment(Fr::from(0), Fr::from(100), Fr::from(10), Fr::from(1), Fr::from(5), Fr::from(0), Fr::from(42), Fr::from(123456));
        let null = compute_nullifier(cmt, Fr::from(123456));
        let setup_circuit = OrderCancel { commitment: Fr::ZERO, secret: Fr::ZERO, nullifier: Fr::ZERO };
        let pk = Groth16::<Bn254>::generate_random_parameters_with_reduction(setup_circuit, rng)?;
        let vk = pk.vk.clone();
        let prove_circuit = OrderCancel { commitment: cmt, secret: Fr::from(123456), nullifier: null };
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
            side_a: a[0], price_a: a[1], size_a: a[2], leverage_a: a[3],
            asset_a: a[4], is_market_a: a[5], nonce_a: a[6], secret_a: a[7],
            side_b: b[0], price_b: b[1], size_b: b[2], leverage_b: b[3],
            asset_b: b[4], is_market_b: b[5], nonce_b: b[6], secret_b: b[7],
            mp, ms, cmt_a, cmt_b, match_price: mp, match_size: ms,
            nullifier_a: null_a, nullifier_b: null_b,
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
            side_a: Fr::ZERO, price_a: Fr::ZERO, size_a: Fr::ZERO,
            leverage_a: Fr::ZERO, asset_a: Fr::ZERO, is_market_a: Fr::ZERO,
            nonce_a: Fr::ZERO, secret_a: Fr::ZERO,
            side_b: Fr::ONE, price_b: Fr::ZERO, size_b: Fr::ZERO,
            leverage_b: Fr::ZERO, asset_b: Fr::ZERO, is_market_b: Fr::ZERO,
            nonce_b: Fr::ZERO, secret_b: Fr::ZERO,
            mp: Fr::ZERO, ms: Fr::ZERO,
            cmt_a: Fr::ZERO, cmt_b: Fr::ZERO,
            match_price: Fr::ZERO, match_size: Fr::ZERO,
            nullifier_a: Fr::ZERO, nullifier_b: Fr::ZERO,
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
        let circuit = OrderMatch { side_a: a[0], price_a: a[1], size_a: a[2], leverage_a: a[3], asset_a: a[4], is_market_a: a[5], nonce_a: a[6], secret_a: a[7], side_b: b[0], price_b: b[1], size_b: b[2], leverage_b: b[3], asset_b: b[4], is_market_b: b[5], nonce_b: b[6], secret_b: b[7], mp, ms, cmt_a, cmt_b, match_price: mp, match_size: ms, nullifier_a: null_a, nullifier_b: null_b };
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
        let circuit = OrderMatch { side_a: a[0], price_a: a[1], size_a: a[2], leverage_a: a[3], asset_a: a[4], is_market_a: a[5], nonce_a: a[6], secret_a: a[7], side_b: b[0], price_b: b[1], size_b: b[2], leverage_b: b[3], asset_b: b[4], is_market_b: b[5], nonce_b: b[6], secret_b: b[7], mp, ms, cmt_a, cmt_b, match_price: mp, match_size: ms, nullifier_a: null_a, nullifier_b: null_b };
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
        let circuit = OrderMatch { side_a: a[0], price_a: a[1], size_a: a[2], leverage_a: a[3], asset_a: a[4], is_market_a: a[5], nonce_a: a[6], secret_a: a[7], side_b: b[0], price_b: b[1], size_b: b[2], leverage_b: b[3], asset_b: b[4], is_market_b: b[5], nonce_b: b[6], secret_b: b[7], mp, ms, cmt_a, cmt_b, match_price: mp, match_size: ms, nullifier_a: null_a, nullifier_b: null_b };
        let dummy = OrderMatch { side_a: Fr::ZERO, price_a: Fr::ZERO, size_a: Fr::ZERO, leverage_a: Fr::ZERO, asset_a: Fr::ZERO, is_market_a: Fr::ZERO, nonce_a: Fr::ZERO, secret_a: Fr::ZERO, side_b: Fr::ONE, price_b: Fr::ZERO, size_b: Fr::ZERO, leverage_b: Fr::ZERO, asset_b: Fr::ZERO, is_market_b: Fr::ZERO, nonce_b: Fr::ZERO, secret_b: Fr::ZERO, mp: Fr::ZERO, ms: Fr::ZERO, cmt_a: Fr::ZERO, cmt_b: Fr::ZERO, match_price: Fr::ZERO, match_size: Fr::ZERO, nullifier_a: Fr::ZERO, nullifier_b: Fr::ZERO };
        let pk = Groth16::<Bn254>::generate_random_parameters_with_reduction(dummy, rng).unwrap();
        let vk = pk.vk.clone();
        let proof = Groth16::<Bn254>::create_random_proof_with_reduction(circuit, &pk, rng).unwrap();
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
        let circuit = OrderMatch { side_a: a[0], price_a: a[1], size_a: a[2], leverage_a: a[3], asset_a: a[4], is_market_a: a[5], nonce_a: a[6], secret_a: a[7], side_b: b[0], price_b: b[1], size_b: b[2], leverage_b: b[3], asset_b: b[4], is_market_b: b[5], nonce_b: b[6], secret_b: b[7], mp, ms, cmt_a, cmt_b, match_price: mp, match_size: ms, nullifier_a: null_a, nullifier_b: null_b };
        let cs = ConstraintSystem::new_ref();
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
        let circuit = OrderMatch { side_a: a[0], price_a: a[1], size_a: a[2], leverage_a: a[3], asset_a: a[4], is_market_a: a[5], nonce_a: a[6], secret_a: a[7], side_b: b[0], price_b: b[1], size_b: b[2], leverage_b: b[3], asset_b: b[4], is_market_b: b[5], nonce_b: b[6], secret_b: b[7], mp, ms, cmt_a, cmt_b, match_price: mp, match_size: ms, nullifier_a: null_a, nullifier_b: null_b };
        let cs = ConstraintSystem::new_ref();
        circuit.generate_constraints(cs.clone()).unwrap();
        assert!(!cs.is_satisfied().unwrap());
    }
}
