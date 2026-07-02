use crate::poseidon2::poseidon2_hash_t3;
use ark_bn254::Fr;
use ark_ff::Zero;
use ark_r1cs_std::alloc::AllocVar;
use ark_r1cs_std::eq::EqGadget;
use ark_r1cs_std::fields::fp::FpVar;
use ark_relations::gr1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};

pub struct OrderCommitment {
    pub side: Fr,
    pub price: Fr,
    pub size: Fr,
    pub leverage: Fr,
    pub asset: Fr,
    pub is_market: Fr,
    pub nonce: Fr,
    pub secret: Fr,
    pub commitment: Fr,
    /// 0 = isolated margin, 1 = cross-margin. Private witness; boolean-constrained.
    pub use_cross: Fr,
    /// Public: Poseidon2(secret, 0, domain=20) when use_cross=1, else 0.
    /// Zero sentinel = isolated. Non-zero = cross-margin group tag.
    pub portfolio_key: Fr,
}

impl ConstraintSynthesizer<Fr> for OrderCommitment {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
        // Public inputs: commitment, portfolio_key
        let pub_cmt = FpVar::new_input(cs.clone(), || Ok(self.commitment))?;
        let pub_pk = FpVar::new_input(cs.clone(), || Ok(self.portfolio_key))?;

        // Private witnesses
        let p_side = FpVar::new_witness(cs.clone(), || Ok(self.side))?;
        let p_price = FpVar::new_witness(cs.clone(), || Ok(self.price))?;
        let p_size = FpVar::new_witness(cs.clone(), || Ok(self.size))?;
        let p_leverage = FpVar::new_witness(cs.clone(), || Ok(self.leverage))?;
        let p_asset = FpVar::new_witness(cs.clone(), || Ok(self.asset))?;
        let p_is_market = FpVar::new_witness(cs.clone(), || Ok(self.is_market))?;
        let p_nonce = FpVar::new_witness(cs.clone(), || Ok(self.nonce))?;
        let p_secret = FpVar::new_witness(cs.clone(), || Ok(self.secret))?;
        let p_use_cross = FpVar::new_witness(cs.clone(), || Ok(self.use_cross))?;

        // Commitment hash chain (unchanged)
        let h1 = poseidon2_hash_t3(&p_side, &p_price, 1)?;
        let h2 = poseidon2_hash_t3(&h1, &p_size, 2)?;
        let h3 = poseidon2_hash_t3(&h2, &p_leverage, 3)?;
        let h4 = poseidon2_hash_t3(&h3, &p_asset, 4)?;
        let h5 = poseidon2_hash_t3(&h4, &p_is_market, 5)?;
        let h6 = poseidon2_hash_t3(&h5, &p_nonce, 6)?;
        let h7 = poseidon2_hash_t3(&h6, &p_secret, 7)?;
        pub_cmt.enforce_equal(&h7)?;

        // Boolean constraint: use_cross * (use_cross - 1) == 0
        let one = FpVar::Constant(Fr::from(1u64));
        (&p_use_cross * (&p_use_cross - &one)).enforce_equal(&FpVar::Constant(Fr::zero()))?;

        // portfolio_key = use_cross * Poseidon2(secret, 0, domain=20)
        let zero = FpVar::Constant(Fr::zero());
        let pk_hash = poseidon2_hash_t3(&p_secret, &zero, 20)?;
        let pk_gated = &p_use_cross * &pk_hash;
        pub_pk.enforce_equal(&pk_gated)?;

        Ok(())
    }
}
