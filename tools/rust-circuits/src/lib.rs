pub mod circuits;
pub mod poseidon2;

#[cfg(test)]
mod tests {
    use crate::circuits::cancel::OrderCancel;
    use crate::circuits::commitment::OrderCommitment;
    use crate::circuits::match_circuit::OrderMatch;
    use crate::poseidon2::{poseidon2_hash_t3, poseidon2_hash_t4};
    use ark_bn254::{Bn254, Fr};
    use ark_ff::{AdditiveGroup, Field};
    use ark_groth16::{prepare_verifying_key, Groth16};
    use ark_r1cs_std::fields::fp::FpVar;
    use ark_r1cs_std::GR1CSVar;
    use ark_relations::gr1cs::{ConstraintSynthesizer, ConstraintSystem, SynthesisError};

    fn compute_commitment(
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

    fn compute_nullifier(cmt: Fr, secret: Fr) -> Fr {
        let pc = FpVar::Constant(cmt);
        let ps = FpVar::Constant(secret);
        let nf = poseidon2_hash_t3(&pc, &ps, 3).unwrap();
        nf.value().unwrap()
    }

    fn compute_match_nullifier(cmt: Fr, mp: Fr, ms: Fr) -> Fr {
        let pc = FpVar::Constant(cmt);
        let pm = FpVar::Constant(mp);
        let ps = FpVar::Constant(ms);
        let nf = poseidon2_hash_t4(&[pc, pm, ps], 10).unwrap();
        nf.value().unwrap()
    }

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
        let cmt = compute_commitment(
            fields[0], fields[1], fields[2], fields[3],
            fields[4], fields[5], fields[6], fields[7],
        );

        let cs = ConstraintSystem::new_ref();
        let circuit = OrderCommitment {
            side: fields[0], price: fields[1], size: fields[2],
            leverage: fields[3], asset: fields[4], is_market: fields[5],
            nonce: fields[6], secret: fields[7], commitment: cmt,
        };
        circuit.generate_constraints(cs.clone()).unwrap();
        assert!(cs.is_satisfied().unwrap());
    }

    #[test]
    fn test_commitment_groth16() -> Result<(), SynthesisError> {
        let rng = &mut ark_std::test_rng();
        let fields = make_order_fields(0, 100, 10, 1, 5, 0, 42, 123456);
        let cmt = compute_commitment(
            fields[0], fields[1], fields[2], fields[3],
            fields[4], fields[5], fields[6], fields[7],
        );

        let setup_circuit = OrderCommitment {
            side: Fr::ZERO, price: Fr::ZERO, size: Fr::ZERO,
            leverage: Fr::ZERO, asset: Fr::ZERO, is_market: Fr::ZERO,
            nonce: Fr::ZERO, secret: Fr::ZERO, commitment: Fr::ZERO,
        };
        let pk = Groth16::<Bn254>::generate_random_parameters_with_reduction(setup_circuit, rng)?;
        let vk = pk.vk.clone();

        let prove_circuit = OrderCommitment {
            side: fields[0], price: fields[1], size: fields[2],
            leverage: fields[3], asset: fields[4], is_market: fields[5],
            nonce: fields[6], secret: fields[7], commitment: cmt,
        };
        let proof = Groth16::<Bn254>::create_random_proof_with_reduction(prove_circuit, &pk, rng)?;

        let pvk = prepare_verifying_key(&vk);
        assert!(Groth16::<Bn254>::verify_proof(&pvk, &proof, &[cmt])?);
        Ok(())
    }

    #[test]
    fn test_cancel_cs_satisfied() {
        let cmt = compute_commitment(
            Fr::from(0), Fr::from(100), Fr::from(10), Fr::from(1),
            Fr::from(5), Fr::from(0), Fr::from(42), Fr::from(123456),
        );
        let null = compute_nullifier(cmt, Fr::from(123456));

        let cs = ConstraintSystem::new_ref();
        let circuit = OrderCancel {
            commitment: cmt, secret: Fr::from(123456), nullifier: null,
        };
        circuit.generate_constraints(cs.clone()).unwrap();
        assert!(cs.is_satisfied().unwrap());
    }

    #[test]
    fn test_cancel_groth16() -> Result<(), SynthesisError> {
        let rng = &mut ark_std::test_rng();
        let cmt = compute_commitment(
            Fr::from(0), Fr::from(100), Fr::from(10), Fr::from(1),
            Fr::from(5), Fr::from(0), Fr::from(42), Fr::from(123456),
        );
        let null = compute_nullifier(cmt, Fr::from(123456));

        let setup_circuit = OrderCancel {
            commitment: Fr::ZERO, secret: Fr::ZERO, nullifier: Fr::ZERO,
        };
        let pk = Groth16::<Bn254>::generate_random_parameters_with_reduction(setup_circuit, rng)?;
        let vk = pk.vk.clone();

        let prove_circuit = OrderCancel {
            commitment: cmt, secret: Fr::from(123456), nullifier: null,
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
            side_a: a[0], price_a: a[1], size_a: a[2], leverage_a: a[3],
            asset_a: a[4], is_market_a: a[5], nonce_a: a[6], secret_a: a[7],
            side_b: b[0], price_b: b[1], size_b: b[2], leverage_b: b[3],
            asset_b: b[4], is_market_b: b[5], nonce_b: b[6], secret_b: b[7],
            mp, ms,
            cmt_a, cmt_b, match_price: mp, match_size: ms, nullifier_a: null_a, nullifier_b: null_b,
        };
        let public = [cmt_a, cmt_b, mp, ms, null_a, null_b];
        (circuit, public)
    }

    #[test]
    fn test_match_cs_satisfied() {
        let (circuit, _) = make_valid_match_circuit();
        let cs = ConstraintSystem::new_ref();
        circuit.generate_constraints(cs.clone()).unwrap();
        assert!(cs.is_satisfied().unwrap());
    }

    #[test]
    fn test_match_groth16() -> Result<(), SynthesisError> {
        let rng = &mut ark_std::test_rng();

        let (prove_circuit, public) = make_valid_match_circuit();

        // Setup with a dummy version
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
        let a = make_order_fields(0, 100, 10, 1, 5, 0, 42, 123456);
        let b = make_order_fields(1, 100, 15, 2, 99, 0, 7, 789012);
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
            mp, ms,
            cmt_a, cmt_b, match_price: mp, match_size: ms,
            nullifier_a: null_a, nullifier_b: null_b,
        };
        let cs = ConstraintSystem::new_ref();
        circuit.generate_constraints(cs.clone()).unwrap();
        assert!(!cs.is_satisfied().unwrap());
    }

    #[test]
    fn test_match_invalid_side() {
        let a = make_order_fields(0, 100, 10, 1, 5, 0, 42, 123456);
        let b = make_order_fields(0, 100, 15, 2, 5, 0, 7, 789012);
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
            mp, ms,
            cmt_a, cmt_b, match_price: mp, match_size: ms,
            nullifier_a: null_a, nullifier_b: null_b,
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
            side_a: a[0], price_a: a[1], size_a: a[2], leverage_a: a[3],
            asset_a: a[4], is_market_a: a[5], nonce_a: a[6], secret_a: a[7],
            side_b: b[0], price_b: b[1], size_b: b[2], leverage_b: b[3],
            asset_b: b[4], is_market_b: b[5], nonce_b: b[6], secret_b: b[7],
            mp, ms,
            cmt_a, cmt_b, match_price: mp, match_size: ms,
            nullifier_a: null_a, nullifier_b: null_b,
        };
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

        let pk = Groth16::<Bn254>::generate_random_parameters_with_reduction(dummy, rng).unwrap();
        let vk = pk.vk.clone();
        let proof = Groth16::<Bn254>::create_random_proof_with_reduction(circuit, &pk, rng).unwrap();
        let pvk = prepare_verifying_key(&vk);
        let public = [cmt_a, cmt_b, mp, ms, null_a, null_b];
        assert!(Groth16::<Bn254>::verify_proof(&pvk, &proof, &public).unwrap());
    }

    #[test]
    fn test_match_invalid_price_bid_too_low() {
        let a = make_order_fields(0, 90, 10, 1, 5, 0, 42, 123456);
        let b = make_order_fields(1, 100, 15, 2, 5, 0, 7, 789012);
        let mp = Fr::from(95u64);
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
            mp, ms,
            cmt_a, cmt_b, match_price: mp, match_size: ms,
            nullifier_a: null_a, nullifier_b: null_b,
        };
        let cs = ConstraintSystem::new_ref();
        circuit.generate_constraints(cs.clone()).unwrap();
        assert!(!cs.is_satisfied().unwrap());
    }

    #[test]
    fn test_match_invalid_size_too_big() {
        let a = make_order_fields(0, 100, 10, 1, 5, 0, 42, 123456);
        let b = make_order_fields(1, 100, 15, 2, 5, 0, 7, 789012);
        let mp = Fr::from(100u64);
        let ms = Fr::from(12u64);

        let cmt_a = compute_commitment(a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7]);
        let cmt_b = compute_commitment(b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]);
        let null_a = compute_match_nullifier(cmt_a, mp, ms);
        let null_b = compute_match_nullifier(cmt_b, mp, ms);

        let circuit = OrderMatch {
            side_a: a[0], price_a: a[1], size_a: a[2], leverage_a: a[3],
            asset_a: a[4], is_market_a: a[5], nonce_a: a[6], secret_a: a[7],
            side_b: b[0], price_b: b[1], size_b: b[2], leverage_b: b[3],
            asset_b: b[4], is_market_b: b[5], nonce_b: b[6], secret_b: b[7],
            mp, ms,
            cmt_a, cmt_b, match_price: mp, match_size: ms,
            nullifier_a: null_a, nullifier_b: null_b,
        };
        let cs = ConstraintSystem::new_ref();
        circuit.generate_constraints(cs.clone()).unwrap();
        assert!(!cs.is_satisfied().unwrap());
    }
}
