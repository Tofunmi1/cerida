use anyhow::{Context, Result};
use ark_bn254::{Bn254, G1Affine, G2Affine};
use ark_circom::{CircomBuilder, CircomConfig, CircomReduction, read_zkey};
use ark_ff::{BigInteger, PrimeField};
use ark_groth16::Groth16;
use serde::Serialize;
use rand::thread_rng;
use std::path::Path;

pub struct GeneratedProof {
    pub proof_hex_a: String,
    pub proof_hex_b: String,
    pub proof_hex_c: String,
    pub commitment: String,
    pub nullifier: String,
}

#[derive(Serialize)]
pub struct CliProofOutput {
    pub proof: CliProof,
    pub public_inputs: Vec<String>,
    pub commitment: String,
    pub nullifier: String,
}

#[derive(Serialize)]
pub struct CliProof {
    pub a: String,
    pub b: String,
    pub c: String,
}

fn g1_to_hex(g1: &G1Affine) -> String {
    let x_be = g1.x.into_bigint().to_bytes_be();
    let y_be = g1.y.into_bigint().to_bytes_be();
    format!("{}{}", hex::encode(&x_be), hex::encode(&y_be))
}

fn g2_to_hex(g2: &G2Affine) -> String {
    let c0_be = g2.x.c0.into_bigint().to_bytes_be();
    let c1_be = g2.x.c1.into_bigint().to_bytes_be();
    let d0_be = g2.y.c0.into_bigint().to_bytes_be();
    let d1_be = g2.y.c1.into_bigint().to_bytes_be();
    format!(
        "{}{}{}{}",
        hex::encode(&c1_be),
        hex::encode(&c0_be),
        hex::encode(&d1_be),
        hex::encode(&d0_be),
    )
}

pub fn generate_proof(
    wasm_path: &Path,
    r1cs_path: &Path,
    zkey_path: &Path,
    amount: u64,
    secret: u64,
) -> Result<GeneratedProof> {
    let zkey_file = std::fs::File::open(zkey_path)
        .with_context(|| format!("Failed to open zkey: {}", zkey_path.display()))?;
    let mut reader = std::io::BufReader::new(zkey_file);
    let (proving_key, _matrices) = read_zkey(&mut reader)
        .map_err(|e| anyhow::anyhow!("Failed to read zkey: {e}"))?;

    let cfg = CircomConfig::<Bn254>::new(wasm_path, r1cs_path)
        .map_err(|e| anyhow::anyhow!("Failed to load circuit from {} and {}: {e}", wasm_path.display(), r1cs_path.display()))?;

    let mut builder = CircomBuilder::new(cfg);
    builder.push_input("amount", amount as i64);
    builder.push_input("secret", secret as i64);

    let circom = builder.build()
        .map_err(|e| anyhow::anyhow!("Failed to build circuit: {e}"))?;

    let public_inputs = circom
        .get_public_inputs()
        .ok_or_else(|| anyhow::anyhow!("No public inputs in circuit"))?;

    let mut rng = thread_rng();
    let proof = Groth16::<Bn254, CircomReduction>::create_random_proof_with_reduction(
        circom, &proving_key, &mut rng,
    )
    .map_err(|e| anyhow::anyhow!("Failed to generate proof: {e}"))?;

    let proof_hex_a = g1_to_hex(&proof.a);
    let proof_hex_b = g2_to_hex(&proof.b);
    let proof_hex_c = g1_to_hex(&proof.c);
    let commitment = public_inputs[0].into_bigint().to_string();
    let nullifier = public_inputs[1].into_bigint().to_string();

    Ok(GeneratedProof {
        proof_hex_a,
        proof_hex_b,
        proof_hex_c,
        commitment,
        nullifier,
    })
}

pub fn proof_to_cli_json(p: &GeneratedProof) -> CliProofOutput {
    CliProofOutput {
        proof: CliProof {
            a: p.proof_hex_a.clone(),
            b: p.proof_hex_b.clone(),
            c: p.proof_hex_c.clone(),
        },
        public_inputs: vec![p.commitment.clone(), p.nullifier.clone()],
        commitment: p.commitment.clone(),
        nullifier: p.nullifier.clone(),
    }
}
