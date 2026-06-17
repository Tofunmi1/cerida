use crate::ec::{g1_add, g1_msm, pairing_check, G1Point, G2Point};
use crate::field::Bn254Fr;
use crate::types::{G1Commitment, VerificationKey};
use soroban_sdk::Env;

pub struct Shplemini;

impl Shplemini {
    pub fn verify(
        env: &Env,
        vk: &VerificationKey,
        batch_opening_commitments: &[G1Commitment],
        batch_evaluations: &[Bn254Fr],
        gemini_fold_comms: &[G1Commitment],
        gemini_initial_shifted: &G1Commitment,
        shplonk_q: &G1Commitment,
        kzg_quotient: &G1Commitment,
        gemini_challenges: &[Bn254Fr],
        shplonk_challenge: Bn254Fr,
        rho: Bn254Fr,
    ) -> bool {
        let num_commitments = batch_opening_commitments.len();
        let num_evaluations = batch_evaluations.len();

        if num_commitments == 0 {
            return true;
        }

        let mut accumulator = G1Point::zero();
        let mut rho_pow = Bn254Fr::one();

        for i in 0..num_commitments {
            let comm = &batch_opening_commitments[i];
            let comm_point = G1Point {
                x: comm.x,
                y: comm.y,
            };
            let comm_neg = comm_point.negate();
            let eval = batch_evaluations[i];
            let eval_point = G1Point {
                x: Bn254Fr::zero(),
                y: Bn254Fr::zero(),
            };

            let mut scaled = g1_msm(
                env,
                &[comm_neg, G1Point {
                    x: Bn254Fr::from_bytes_be(&Bn254Fr::zero().to_bytes_be()),
                    y: Bn254Fr::from_bytes_be(&Bn254Fr::zero().to_bytes_be()),
                }],
                &[rho_pow, rho_pow * eval],
            );
            accumulator = g1_add(env, &accumulator, &scaled);
            rho_pow = rho_pow * rho;
        }

        let gemini_initial = G1Point {
            x: gemini_fold_comms[0].x,
            y: gemini_fold_comms[0].y,
        };
        let mut combined = g1_add(env, &accumulator, &gemini_initial);

        let shplonk_q_point = G1Point {
            x: shplonk_q.x,
            y: shplonk_q.y,
        };
        let kzg_q_point = G1Point {
            x: kzg_quotient.x,
            y: kzg_quotient.y,
        };

        let pairing_lhs = g1_add(env, &combined, &shplonk_q_point);
        let pairing_rhs = g1_add(env, &kzg_q_point, &G1Point::zero());

        let pairing_result = pairing_check(
            env,
            &[pairing_lhs, G1Point {
                x: Bn254Fr::from_bytes_be(&Bn254Fr::zero().to_bytes_be()),
                y: Bn254Fr::from_bytes_be(&Bn254Fr::zero().to_bytes_be()),
            }],
            &[
                G2Point {
                    x: [Bn254Fr::zero(), Bn254Fr::zero()],
                    y: [Bn254Fr::zero(), Bn254Fr::zero()],
                },
                G2Point {
                    x: [Bn254Fr::zero(), Bn254Fr::zero()],
                    y: [Bn254Fr::zero(), Bn254Fr::zero()],
                },
            ],
        );

        pairing_result
    }
}
