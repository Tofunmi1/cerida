use crate::poseidon2::poseidon2_hash_t3;
use ark_bn254::Fr;
use ark_ff::{AdditiveGroup, Zero};
use ark_r1cs_std::alloc::AllocVar;
use ark_r1cs_std::eq::EqGadget;
use ark_r1cs_std::fields::fp::FpVar;
use ark_relations::gr1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};

use super::shielded_insert::TREE_DEPTH;

/// Proves knowledge of a secret that produced a leaf in the Merkle tree,
/// without revealing which leaf or its index.
///
/// Public inputs (in order): root, nullifier_hash, recipient
/// Private witnesses: secret, nullifier, path_elements[TREE_DEPTH], path_indices[TREE_DEPTH]
///
/// Constraints:
///   1. leaf = Poseidon2(secret, nullifier, 30) is in the tree rooted at `root`
///   2. nullifier_hash = Poseidon2(nullifier, 0, 31) — ties the proof to a spendable note
///   3. recipient is a public input that binds the proof to a specific recipient address
///      (prevents front-running: swapping recipient makes the proof invalid)
pub struct ShieldedWithdraw {
    // Public
    pub root: Fr,
    pub nullifier_hash: Fr,
    pub recipient: Fr,

    // Private
    pub secret: Fr,
    pub nullifier: Fr,
    pub path_elements: [Fr; TREE_DEPTH],
    pub path_indices: [bool; TREE_DEPTH],
}

impl ShieldedWithdraw {
    pub fn dummy() -> Self {
        Self {
            root: Fr::ZERO,
            nullifier_hash: Fr::ZERO,
            recipient: Fr::ZERO,
            secret: Fr::ZERO,
            nullifier: Fr::ZERO,
            path_elements: [Fr::ZERO; TREE_DEPTH],
            path_indices: [false; TREE_DEPTH],
        }
    }
}

impl ConstraintSynthesizer<Fr> for ShieldedWithdraw {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
        // ── Public inputs ──────────────────────────────────────────────────
        let pub_root = FpVar::new_input(cs.clone(), || Ok(self.root))?;
        let pub_null_hash = FpVar::new_input(cs.clone(), || Ok(self.nullifier_hash))?;
        // recipient is declared as public so the proof is bound to a specific address;
        // the circuit adds no further constraints on it.
        let _pub_recipient = FpVar::new_input(cs.clone(), || Ok(self.recipient))?;

        // ── Private witnesses ──────────────────────────────────────────────
        let p_secret = FpVar::new_witness(cs.clone(), || Ok(self.secret))?;
        let p_nullifier = FpVar::new_witness(cs.clone(), || Ok(self.nullifier))?;

        let path: Vec<FpVar<Fr>> = self
            .path_elements
            .iter()
            .map(|&e| FpVar::new_witness(cs.clone(), || Ok(e)))
            .collect::<Result<_, _>>()?;

        let indices: Vec<FpVar<Fr>> = self
            .path_indices
            .iter()
            .map(|&b| {
                FpVar::new_witness(cs.clone(), || Ok(if b { Fr::from(1u64) } else { Fr::ZERO }))
            })
            .collect::<Result<_, _>>()?;

        // ── Constants ──────────────────────────────────────────────────────
        let one = FpVar::Constant(Fr::from(1u64));
        let zero_const = FpVar::Constant(Fr::zero());

        // ── Boolean constraints on path_indices ────────────────────────────
        for b in &indices {
            (b * (b - &one)).enforce_equal(&zero_const)?;
        }

        // ── Nullifier hash constraint ──────────────────────────────────────
        // nullifier_hash = Poseidon2(nullifier, 0, 31)
        let computed_null_hash = poseidon2_hash_t3(&p_nullifier, &zero_const, 31)?;
        pub_null_hash.enforce_equal(&computed_null_hash)?;

        // ── Leaf hash ─────────────────────────────────────────────────────
        // leaf = Poseidon2(secret, nullifier, 30)
        let leaf = poseidon2_hash_t3(&p_secret, &p_nullifier, 30)?;

        // ── Merkle path to root ────────────────────────────────────────────
        let mut current = leaf;
        for (i, (bit, sibling)) in indices.iter().zip(path.iter()).enumerate() {
            // bit=0: current is left child; bit=1: current is right child
            let left = (&one - bit) * &current + bit * sibling;
            let right = bit * &current + (&one - bit) * sibling;
            current = poseidon2_hash_t3(&left, &right, 32 + i as u64)?;
        }
        pub_root.enforce_equal(&current)?;

        Ok(())
    }
}
