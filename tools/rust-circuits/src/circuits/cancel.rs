use crate::poseidon2::poseidon2_hash_t3;
use ark_bn254::Fr;
use ark_r1cs_std::alloc::AllocVar;
use ark_r1cs_std::eq::EqGadget;
use ark_r1cs_std::fields::fp::FpVar;
use ark_relations::gr1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};

pub struct OrderCancel {
    pub commitment: Fr,
    pub secret: Fr,
    pub nullifier: Fr,
}

impl ConstraintSynthesizer<Fr> for OrderCancel {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
        let pub_null = FpVar::new_input(cs.clone(), || Ok(self.nullifier))?;

        let p_commitment = FpVar::new_witness(cs.clone(), || Ok(self.commitment))?;
        let p_secret = FpVar::new_witness(cs.clone(), || Ok(self.secret))?;

        let nf = poseidon2_hash_t3(&p_commitment, &p_secret, 3)?;

        pub_null.enforce_equal(&nf)?;

        Ok(())
    }
}
