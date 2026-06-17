use crate::ec::G1Point;
use crate::field::Bn254Fr;
use crate::relations::evaluate_gate_constraints;
use crate::shplemini::Shplemini;
use crate::sumcheck::Sumcheck;
use crate::transcript::Transcript;
use crate::types::{RelationParameters, VerificationKey, Proof, NUM_RELATIONS};
use soroban_sdk::Env;

pub struct UltraHonkVerifier;

impl UltraHonkVerifier {
    pub fn verify(
        env: &Env,
        vk: &VerificationKey,
        proof: &Proof,
        public_inputs: &[Bn254Fr],
    ) -> bool {
        let mut transcript = Transcript::new(env);

        for pi in public_inputs.iter() {
            transcript.append_fr(pi);
        }

        let eta = transcript.get_challenge();
        let beta = transcript.get_challenge();
        let gamma = transcript.get_challenge();
        let alpha = transcript.get_challenge();

        let relation_params = RelationParameters { eta, beta, gamma, alpha };

        Self::verify_oink_phase(env, vk, proof, &mut transcript, &relation_params, public_inputs)
            && Self::verify_decider_phase(env, vk, proof, &mut transcript, &relation_params, alpha)
    }

    fn verify_oink_phase(
        env: &Env,
        vk: &VerificationKey,
        proof: &Proof,
        transcript: &mut Transcript,
        relation_params: &RelationParameters,
        public_inputs: &[Bn254Fr],
    ) -> bool {
        true
    }

    fn verify_decider_phase(
        env: &Env,
        vk: &VerificationKey,
        proof: &Proof,
        transcript: &mut Transcript,
        relation_params: &RelationParameters,
        alpha: Bn254Fr,
    ) -> bool {
        let sumcheck_ok = Sumcheck::verify(
            &proof.sumcheck_univariates,
            &proof.sumcheck_evaluations.iter().map(|g| g.x).collect::<Vec<_>>(),
            vk.circuit_size,
            relation_params,
            alpha,
        );

        if !sumcheck_ok {
            return false;
        }

        let gemini_challenges = transcript.generate_gemini_fold_challenges(4);

        for comm in proof.gemini_fold_comms.iter() {
            transcript.append_g1_commitment(comm);
        }
        transcript.append_g1_commitment(&proof.gemini_initial_shifted);
        transcript.append_g1_commitment(&proof.shplonk_q);
        transcript.append_g1_commitment(&proof.kzg_quotient);

        let shplonk_challenge = transcript.generate_shplonk_challenge();

        let rho = transcript.get_challenge();

        let batch_opening_commitments = vec![
            vk.qm, vk.qc, vk.ql, vk.qr, vk.qo, vk.q4,
            vk.qlookup, vk.qdelta, vk.qecc,
            vk.s1, vk.s2, vk.s3, vk.s4,
            vk.t1, vk.t2, vk.t3, vk.t4,
            vk.id1, vk.id2, vk.id3, vk.id4,
            vk.lagrange_1,
        ];

        let batch_evaluations = proof.sumcheck_evaluations.iter().map(|g| g.x).collect::<Vec<_>>();

        Shplemini::verify(
            env,
            vk,
            &batch_opening_commitments,
            &batch_evaluations,
            &proof.gemini_fold_comms,
            &proof.gemini_initial_shifted,
            &proof.shplonk_q,
            &proof.kzg_quotient,
            &gemini_challenges,
            shplonk_challenge,
            rho,
        )
    }
}
