use crate::poseidon2::{poseidon2_hash_t3, poseidon2_hash_t4};
use ark_bn254::Fr;
use ark_ff::{AdditiveGroup, Field};
use ark_r1cs_std::alloc::AllocVar;
use ark_r1cs_std::boolean::Boolean;
use ark_r1cs_std::convert::ToBitsGadget;
use ark_r1cs_std::eq::EqGadget;
use ark_r1cs_std::fields::fp::FpVar;
use ark_relations::gr1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};

fn enforce_cond_le(condition: FpVar<Fr>, a: FpVar<Fr>, b: FpVar<Fr>) -> Result<(), SynthesisError> {
    let diff = b - a;
    let cond_diff = condition * diff;
    let bits = cond_diff.to_bits_be()?;
    let num_bits = bits.len();
    for bit in bits.iter().take(num_bits - 64) {
        bit.enforce_equal(&Boolean::Constant(false))?;
    }
    Ok(())
}

fn enforce_le(a: FpVar<Fr>, b: FpVar<Fr>) -> Result<(), SynthesisError> {
    let diff = b - a;
    let bits = diff.to_bits_be()?;
    let num_bits = bits.len();
    for bit in bits.iter().take(num_bits - 64) {
        bit.enforce_equal(&Boolean::Constant(false))?;
    }
    Ok(())
}

pub struct OrderMatch {
    pub side_a: Fr,
    pub price_a: Fr,
    pub size_a: Fr,
    pub leverage_a: Fr,
    pub asset_a: Fr,
    pub is_market_a: Fr,
    pub nonce_a: Fr,
    pub secret_a: Fr,

    pub side_b: Fr,
    pub price_b: Fr,
    pub size_b: Fr,
    pub leverage_b: Fr,
    pub asset_b: Fr,
    pub is_market_b: Fr,
    pub nonce_b: Fr,
    pub secret_b: Fr,

    pub mp: Fr,
    pub ms: Fr,

    pub cmt_a: Fr,
    pub cmt_b: Fr,
    pub match_price: Fr,
    pub match_size: Fr,
    pub nullifier_a: Fr,
    pub nullifier_b: Fr,
}

impl ConstraintSynthesizer<Fr> for OrderMatch {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
        let pub_cmt_a = FpVar::new_input(cs.clone(), || Ok(self.cmt_a))?;
        let pub_cmt_b = FpVar::new_input(cs.clone(), || Ok(self.cmt_b))?;
        let pub_mp = FpVar::new_input(cs.clone(), || Ok(self.match_price))?;
        let pub_ms = FpVar::new_input(cs.clone(), || Ok(self.match_size))?;
        let pub_null_a = FpVar::new_input(cs.clone(), || Ok(self.nullifier_a))?;
        let pub_null_b = FpVar::new_input(cs.clone(), || Ok(self.nullifier_b))?;

        let p_side_a = FpVar::new_witness(cs.clone(), || Ok(self.side_a))?;
        let p_price_a = FpVar::new_witness(cs.clone(), || Ok(self.price_a))?;
        let p_size_a = FpVar::new_witness(cs.clone(), || Ok(self.size_a))?;
        let p_leverage_a = FpVar::new_witness(cs.clone(), || Ok(self.leverage_a))?;
        let p_asset_a = FpVar::new_witness(cs.clone(), || Ok(self.asset_a))?;
        let p_is_market_a = FpVar::new_witness(cs.clone(), || Ok(self.is_market_a))?;
        let p_nonce_a = FpVar::new_witness(cs.clone(), || Ok(self.nonce_a))?;
        let p_secret_a = FpVar::new_witness(cs.clone(), || Ok(self.secret_a))?;

        let p_side_b = FpVar::new_witness(cs.clone(), || Ok(self.side_b))?;
        let p_price_b = FpVar::new_witness(cs.clone(), || Ok(self.price_b))?;
        let p_size_b = FpVar::new_witness(cs.clone(), || Ok(self.size_b))?;
        let p_leverage_b = FpVar::new_witness(cs.clone(), || Ok(self.leverage_b))?;
        let p_asset_b = FpVar::new_witness(cs.clone(), || Ok(self.asset_b))?;
        let p_is_market_b = FpVar::new_witness(cs.clone(), || Ok(self.is_market_b))?;
        let p_nonce_b = FpVar::new_witness(cs.clone(), || Ok(self.nonce_b))?;
        let p_secret_b = FpVar::new_witness(cs.clone(), || Ok(self.secret_b))?;

        let p_mp = FpVar::new_witness(cs.clone(), || Ok(self.mp))?;
        let p_ms = FpVar::new_witness(cs.clone(), || Ok(self.ms))?;

        // Commitment A
        let ha1 = poseidon2_hash_t3(&p_side_a, &p_price_a, 1)?;
        let ha2 = poseidon2_hash_t3(&ha1, &p_size_a, 2)?;
        let ha3 = poseidon2_hash_t3(&ha2, &p_leverage_a, 3)?;
        let ha4 = poseidon2_hash_t3(&ha3, &p_asset_a, 4)?;
        let ha5 = poseidon2_hash_t3(&ha4, &p_is_market_a, 5)?;
        let ha6 = poseidon2_hash_t3(&ha5, &p_nonce_a, 6)?;
        let ha7 = poseidon2_hash_t3(&ha6, &p_secret_a, 7)?;
        pub_cmt_a.enforce_equal(&ha7)?;

        // Commitment B
        let hb1 = poseidon2_hash_t3(&p_side_b, &p_price_b, 1)?;
        let hb2 = poseidon2_hash_t3(&hb1, &p_size_b, 2)?;
        let hb3 = poseidon2_hash_t3(&hb2, &p_leverage_b, 3)?;
        let hb4 = poseidon2_hash_t3(&hb3, &p_asset_b, 4)?;
        let hb5 = poseidon2_hash_t3(&hb4, &p_is_market_b, 5)?;
        let hb6 = poseidon2_hash_t3(&hb5, &p_nonce_b, 6)?;
        let hb7 = poseidon2_hash_t3(&hb6, &p_secret_b, 7)?;
        pub_cmt_b.enforce_equal(&hb7)?;

        pub_mp.enforce_equal(&p_mp)?;
        pub_ms.enforce_equal(&p_ms)?;

        // Constraints
        p_asset_a.enforce_equal(&p_asset_b)?;

        let sum_sides = p_side_a.clone() + p_side_b.clone();
        sum_sides.enforce_equal(&FpVar::Constant(Fr::ONE))?;

        let market_product = p_is_market_a.clone() * p_is_market_b.clone();
        market_product.enforce_equal(&FpVar::Constant(Fr::ZERO))?;

        // Price constraints gated by limit flag
        let one = FpVar::Constant(Fr::ONE);
        let limit_a = one.clone() - p_is_market_a.clone();
        let is_bid_a = one.clone() - p_side_a.clone();
        let is_ask_a = p_side_a;
        let cond_bid_a = limit_a.clone() * is_bid_a;
        let cond_ask_a = limit_a * is_ask_a;

        let limit_b = one.clone() - p_is_market_b.clone();
        let is_bid_b = one - p_side_b.clone();
        let is_ask_b = p_side_b;
        let cond_bid_b = limit_b.clone() * is_bid_b;
        let cond_ask_b = limit_b * is_ask_b;

        enforce_cond_le(cond_bid_a, p_mp.clone(), p_price_a.clone())?;
        enforce_cond_le(cond_ask_a, p_price_a, p_mp.clone())?;
        enforce_cond_le(cond_bid_b, p_mp.clone(), p_price_b.clone())?;
        enforce_cond_le(cond_ask_b, p_price_b, p_mp.clone())?;

        // Size constraints: ms <= size_a AND ms <= size_b
        enforce_le(p_ms.clone(), p_size_a)?;
        enforce_le(p_ms.clone(), p_size_b)?;

        // Nullifiers
        let nf_a = poseidon2_hash_t4(&[ha7, p_mp.clone(), p_ms.clone()], 10)?;
        pub_null_a.enforce_equal(&nf_a)?;

        let nf_b = poseidon2_hash_t4(&[hb7, p_mp, p_ms], 10)?;
        pub_null_b.enforce_equal(&nf_b)?;

        Ok(())
    }
}
