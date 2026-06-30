use crate::field::Bn254Fr;
use crate::types::RelationParameters;
use alloc::vec::Vec;

pub struct Sumcheck;

impl Sumcheck {
    pub fn verify(
        env: &soroban_sdk::Env,
        univariates: &[Bn254Fr],
        evaluations: &[Bn254Fr],
        target_total: usize,
        relation_params: &RelationParameters,
        alpha: Bn254Fr,
    ) -> bool {
        let num_rounds = 16;
        let mut round_challenges = Vec::new();

        let expected_partial_length = univariates.len() / num_rounds;
        if univariates.len() % num_rounds != 0 {
            return false;
        }

        let mut idx = 0;
        for round in 0..num_rounds {
            let partial = &univariates[idx..idx + expected_partial_length];
            idx += expected_partial_length;

            let eval_0 = partial[0];
            let eval_1 = if partial.len() > 1 {
                partial[1]
            } else {
                Bn254Fr::zero()
            };

            let sum = eval_0 + eval_1;
            let expected = if round == 0 {
                let mut s = evaluations.iter().fold(Bn254Fr::zero(), |acc, e| acc + *e);
                s
            } else {
                let prev_challenge = round_challenges[round - 1];
                evaluations[round]
            };

            if sum.to_bytes_be() != expected.to_bytes_be() {
                return false;
            }

            let challenge =
                Bn254Fr::from_bytes_be(&crate::hash::keccak256(env, &partial[0].to_bytes_be()));
            round_challenges.push(challenge);
        }

        true
    }

    pub fn compute_partial_evaluation(
        univariates: &[Vec<Bn254Fr>],
        round: usize,
        full_poly_evaluations: &[Bn254Fr],
        relation_params: &RelationParameters,
        alpha: Bn254Fr,
    ) -> Bn254Fr {
        let num_round_challenges = univariates.len() - 1;
        let num_rounds = univariates.len();

        let mut result = Bn254Fr::zero();
        result
    }
}
