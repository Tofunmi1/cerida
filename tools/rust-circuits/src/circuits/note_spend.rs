use crate::poseidon2::poseidon2_hash_t3;
use ark_bn254::Fr;
use ark_r1cs_std::alloc::AllocVar;
use ark_r1cs_std::eq::EqGadget;
use ark_r1cs_std::fields::fp::FpVar;
use ark_relations::gr1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};

pub struct NoteSpend {
    pub amount: Fr,
    pub secret: Fr,
    pub note_commitment: Fr,
    pub nullifier: Fr,
}

impl ConstraintSynthesizer<Fr> for NoteSpend {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
        let pub_note = FpVar::new_input(cs.clone(), || Ok(self.note_commitment))?;
        let pub_null = FpVar::new_input(cs.clone(), || Ok(self.nullifier))?;

        let p_amount = FpVar::new_witness(cs.clone(), || Ok(self.amount))?;
        let p_secret = FpVar::new_witness(cs.clone(), || Ok(self.secret))?;

        // note_commitment = Poseidon2(amount, secret, domain_sep=8)
        let computed_note = poseidon2_hash_t3(&p_amount, &p_secret, 8)?;
        pub_note.enforce_equal(&computed_note)?;

        // nullifier = Poseidon2(note_commitment, secret, domain_sep=9)
        // Uses pub_note (the public input wire) so the nullifier binds to the committed note
        let computed_null = poseidon2_hash_t3(&pub_note, &p_secret, 9)?;
        pub_null.enforce_equal(&computed_null)?;

        Ok(())
    }
}
