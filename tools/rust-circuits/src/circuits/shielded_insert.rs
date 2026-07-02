use crate::poseidon2::poseidon2_hash_t3;
use ark_bn254::Fr;
use ark_ff::{AdditiveGroup, Zero};
use ark_r1cs_std::alloc::AllocVar;
use ark_r1cs_std::eq::EqGadget;
use ark_r1cs_std::fields::fp::FpVar;
use ark_relations::gr1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};

pub const TREE_DEPTH: usize = 20;

/// Proves that a commitment was correctly inserted into an append-only Merkle tree.
///
/// Public inputs (in order): old_root, new_root, commitment, leaf_index
/// Private witnesses: path_elements[TREE_DEPTH] (the siblings along the insertion path)
///
/// The circuit verifies:
///   1. The old tree has an empty leaf (Fr::ZERO) at leaf_index, producing old_root
///   2. Replacing that leaf with commitment produces new_root
///   3. The same path_elements serve both checks (only the leaf value differs)
pub struct ShieldedInsert {
    // Public
    pub old_root: Fr,
    pub new_root: Fr,
    pub commitment: Fr,
    pub leaf_index: u64,

    // Private
    pub path_elements: [Fr; TREE_DEPTH],
}

impl ShieldedInsert {
    pub fn dummy() -> Self {
        Self {
            old_root: Fr::ZERO,
            new_root: Fr::ZERO,
            commitment: Fr::ZERO,
            leaf_index: 0,
            path_elements: [Fr::ZERO; TREE_DEPTH],
        }
    }
}

fn select(
    bit: &FpVar<Fr>,
    current: &FpVar<Fr>,
    sibling: &FpVar<Fr>,
    one: &FpVar<Fr>,
) -> Result<(FpVar<Fr>, FpVar<Fr>), SynthesisError> {
    // bit=0 -> current is left child: left=current, right=sibling
    // bit=1 -> current is right child: left=sibling, right=current
    let left = (one - bit) * current + bit * sibling;
    let right = bit * current + (one - bit) * sibling;
    Ok((left, right))
}

fn merkle_root_from_leaf(
    leaf: &FpVar<Fr>,
    bits: &[FpVar<Fr>],
    path_elements: &[FpVar<Fr>],
    one: &FpVar<Fr>,
) -> Result<FpVar<Fr>, SynthesisError> {
    let mut current = leaf.clone();
    for (i, (bit, sibling)) in bits.iter().zip(path_elements.iter()).enumerate() {
        let (left, right) = select(bit, &current, sibling, one)?;
        current = poseidon2_hash_t3(&left, &right, 32 + i as u64)?;
    }
    Ok(current)
}

impl ConstraintSynthesizer<Fr> for ShieldedInsert {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
        // ── Public inputs ──────────────────────────────────────────────────
        let pub_old_root = FpVar::new_input(cs.clone(), || Ok(self.old_root))?;
        let pub_new_root = FpVar::new_input(cs.clone(), || Ok(self.new_root))?;
        let pub_commitment = FpVar::new_input(cs.clone(), || Ok(self.commitment))?;
        let pub_leaf_index = FpVar::new_input(cs.clone(), || Ok(Fr::from(self.leaf_index)))?;

        // ── Constants ──────────────────────────────────────────────────────
        let one = FpVar::Constant(Fr::from(1u64));
        let zero_const = FpVar::Constant(Fr::zero());

        // ── Bit decompose leaf_index ───────────────────────────────────────
        // Private boolean witnesses constrained to reconstruct leaf_index
        let bits: Vec<FpVar<Fr>> = (0..TREE_DEPTH)
            .map(|i| {
                let bit = (self.leaf_index >> i) & 1;
                FpVar::new_witness(cs.clone(), || Ok(Fr::from(bit)))
            })
            .collect::<Result<_, _>>()?;

        // Boolean constraints: bit * (bit - 1) == 0
        for b in &bits {
            (b * (b - &one)).enforce_equal(&zero_const)?;
        }

        // Reconstruction: Σ bits[i] * 2^i == leaf_index
        let mut reconstructed = zero_const.clone();
        let mut pow2 = Fr::from(1u64);
        for b in &bits {
            reconstructed = reconstructed + b * FpVar::Constant(pow2);
            pow2 = pow2 + pow2;
        }
        pub_leaf_index.enforce_equal(&reconstructed)?;

        // ── Private path elements ──────────────────────────────────────────
        let path: Vec<FpVar<Fr>> = self
            .path_elements
            .iter()
            .map(|&e| FpVar::new_witness(cs.clone(), || Ok(e)))
            .collect::<Result<_, _>>()?;

        // ── Old root check (empty leaf = Fr::ZERO) ─────────────────────────
        let empty_leaf = zero_const.clone();
        let computed_old = merkle_root_from_leaf(&empty_leaf, &bits, &path, &one)?;
        pub_old_root.enforce_equal(&computed_old)?;

        // ── New root check (leaf = commitment) ─────────────────────────────
        let computed_new = merkle_root_from_leaf(&pub_commitment, &bits, &path, &one)?;
        pub_new_root.enforce_equal(&computed_new)?;

        Ok(())
    }
}
