#![no_std]

extern crate alloc;

pub use tiny_types::{Groth16Error, Groth16Proof, VerificationKeyBytes};
use soroban_sdk::{
    BytesN, Env, Vec, contract, contractimpl,
    crypto::bn254::{Bn254Fr, Bn254G1Affine as G1Affine, Bn254G2Affine as G2Affine},
    vec,
};

include!(concat!(env!("OUT_DIR"), "/vk.rs"));

#[derive(Clone)]
pub struct VerificationKey {
    pub alpha: G1Affine,
    pub beta: G2Affine,
    pub gamma: G2Affine,
    pub delta: G2Affine,
    pub ic: Vec<G1Affine>,
}

fn embedded_vk(env: &Env) -> VerificationKey {
    let mut ic_vec: Vec<G1Affine> = Vec::new(env);
    for bytes in VK_IC.iter() {
        ic_vec.push_back(G1Affine::from_bytes(BytesN::from_array(env, bytes)));
    }
    VerificationKey {
        alpha: G1Affine::from_bytes(BytesN::from_array(env, &VK_ALPHA_G1)),
        beta: G2Affine::from_bytes(BytesN::from_array(env, &VK_BETA_G2)),
        gamma: G2Affine::from_bytes(BytesN::from_array(env, &VK_GAMMA_G2)),
        delta: G2Affine::from_bytes(BytesN::from_array(env, &VK_DELTA_G2)),
        ic: ic_vec,
    }
}

#[contract]
pub struct TinyVerifier;

#[contractimpl]
impl TinyVerifier {
    pub fn verify(
        env: Env,
        proof: Groth16Proof,
        public_inputs: Vec<Bn254Fr>,
    ) -> Result<bool, Groth16Error> {
        let vk = embedded_vk(&env);
        let bn = env.crypto().bn254();

        if public_inputs.len().checked_add(1) != Some(vk.ic.len()) {
            return Err(Groth16Error::MalformedPublicInputs);
        }

        let mut vk_x = vk.ic.get(0).ok_or(Groth16Error::MalformedPublicInputs)?;

        for i in 0..public_inputs.len() {
            let s = public_inputs
                .get(i)
                .ok_or(Groth16Error::MalformedPublicInputs)?;
            let ic_idx = i
                .checked_add(1)
                .ok_or(Groth16Error::MalformedPublicInputs)?;
            let v = vk
                .ic
                .get(ic_idx)
                .ok_or(Groth16Error::MalformedPublicInputs)?;
            let prod = bn.g1_mul(&v, &s);
            vk_x = bn.g1_add(&vk_x, &prod);
        }

        let neg_a = -proof.a;

        let g1_points = vec![&env, neg_a, vk.alpha.clone(), vk_x, proof.c];
        let g2_points = vec![
            &env,
            proof.b,
            vk.beta.clone(),
            vk.gamma.clone(),
            vk.delta.clone(),
        ];
        if bn.pairing_check(g1_points, g2_points) {
            Ok(true)
        } else {
            Err(Groth16Error::InvalidProof)
        }
    }
}
