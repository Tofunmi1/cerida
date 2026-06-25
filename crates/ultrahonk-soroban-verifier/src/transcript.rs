use alloc::vec::Vec;
use crate::field::Bn254Fr;
use crate::hash::keccak256;
use soroban_sdk::Env;

pub struct Transcript {
    env: Env,
    state: Vec<u8>,
}

impl Transcript {
    pub fn new(env: &Env) -> Self {
        Transcript {
            env: env.clone(),
            state: Vec::new(),
        }
    }

    pub fn append(&mut self, data: &[u8]) {
        self.state.extend_from_slice(data);
    }

    pub fn append_fr(&mut self, fr: &Bn254Fr) {
        self.state.extend_from_slice(&fr.to_bytes_be());
    }

    pub fn append_g1_commitment(&mut self, comm: &crate::types::G1Commitment) {
        self.state.extend_from_slice(&comm.x.to_bytes_be());
        self.state.extend_from_slice(&comm.y.to_bytes_be());
    }

    pub fn get_challenge(&mut self) -> Bn254Fr {
        let hash = keccak256(&self.env, &self.state);
        self.state.extend_from_slice(&hash);
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&hash);
        Bn254Fr::from_bytes_be(&bytes)
    }

    pub fn get_challenges(&mut self, n: usize) -> Vec<Bn254Fr> {
        (0..n).map(|_| self.get_challenge()).collect()
    }

    pub fn get_challenge_field(&mut self) -> Bn254Fr {
        self.get_challenge()
    }

    pub fn generate_sumcheck_challenges(&mut self, num_rounds: usize) -> Vec<Bn254Fr> {
        let mut challenges = Vec::with_capacity(num_rounds);
        for _ in 0..num_rounds {
            challenges.push(self.get_challenge());
        }
        challenges
    }

    pub fn generate_gemini_fold_challenges(&mut self, num_rounds: usize) -> Vec<Bn254Fr> {
        let mut challenges = Vec::with_capacity(num_rounds);
        for _ in 0..num_rounds {
            challenges.push(self.get_challenge());
        }
        challenges
    }

    pub fn generate_shplonk_challenge(&mut self) -> Bn254Fr {
        self.get_challenge()
    }
}
