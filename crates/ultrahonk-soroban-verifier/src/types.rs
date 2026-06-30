use crate::field::Bn254Fr;
use alloc::vec::Vec;

#[derive(Clone, Copy, Debug)]
pub struct G1Commitment {
    pub x: Bn254Fr,
    pub y: Bn254Fr,
}

#[derive(Clone, Debug)]
pub struct VerificationKey {
    pub circuit_size: usize,
    pub num_public_inputs: usize,
    pub pub_inputs_offset: usize,
    pub qm: G1Commitment,
    pub qc: G1Commitment,
    pub ql: G1Commitment,
    pub qr: G1Commitment,
    pub qo: G1Commitment,
    pub q4: G1Commitment,
    pub qlookup: G1Commitment,
    pub qdelta: G1Commitment,
    pub qecc: G1Commitment,
    pub s1: G1Commitment,
    pub s2: G1Commitment,
    pub s3: G1Commitment,
    pub s4: G1Commitment,
    pub t1: G1Commitment,
    pub t2: G1Commitment,
    pub t3: G1Commitment,
    pub t4: G1Commitment,
    pub id1: G1Commitment,
    pub id2: G1Commitment,
    pub id3: G1Commitment,
    pub id4: G1Commitment,
    pub lagrange_1: G1Commitment,
}

#[derive(Clone, Debug)]
pub struct Proof {
    pub sumcheck_univariates: Vec<Bn254Fr>,
    pub sumcheck_evaluations: Vec<G1Commitment>,
    pub gemini_fold_comms: Vec<G1Commitment>,
    pub gemini_initial_shifted: G1Commitment,
    pub shplonk_q: G1Commitment,
    pub kzg_quotient: G1Commitment,
    pub batch_opening_commitment: Option<G1Commitment>,
}

#[derive(Clone, Copy, Debug)]
pub struct RelationParameters {
    pub eta: Bn254Fr,
    pub beta: Bn254Fr,
    pub gamma: Bn254Fr,
    pub alpha: Bn254Fr,
}

#[derive(Clone, Copy, Debug)]
pub struct WireIndices {
    pub w_l: usize,
    pub w_r: usize,
    pub w_o: usize,
    pub w_4: usize,
}

pub const GATE_COUNT: usize = 3;
pub const TOTAL_CIRCUIT_SIZE: usize = 65536;
pub const LOG_N: usize = 16;
pub const NUM_RELATIONS: usize = 26;
pub const NUM_SUBRELATIONS: [usize; 8] = [2, 2, 2, 4, 4, 7, 2, 3];
pub const BATCHED_RELATION_PARTIAL_LENGTH: usize = 5;
pub const PROOF_BYTES: usize = 14592;
pub const VK_BYTES: usize = 1760;
