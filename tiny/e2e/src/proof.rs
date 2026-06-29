use anyhow::{Context, Result};
use ark_bn254::{Bn254, G1Affine, G2Affine};
use ark_circom::{CircomBuilder, CircomConfig, CircomReduction, read_zkey};
use ark_ff::{BigInteger, PrimeField};
use ark_groth16::Groth16;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use rand::thread_rng;
use std::path::Path;

// ─── Circom proof types ───────────────────────────────────────────────────────

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
        .map_err(|e| anyhow::anyhow!("Failed to load circuit: {e}"))?;

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

    Ok(GeneratedProof {
        proof_hex_a: g1_to_hex(&proof.a),
        proof_hex_b: g2_to_hex(&proof.b),
        proof_hex_c: g1_to_hex(&proof.c),
        commitment: public_inputs[0].into_bigint().to_string(),
        nullifier: public_inputs[1].into_bigint().to_string(),
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

// ─── SP1 proof types ──────────────────────────────────────────────────────────

/// Proof output from `sp1-host` (JSON format).
#[derive(Serialize, Deserialize, Clone)]
pub struct Sp1ProofOutput {
    pub proof: Sp1ProofHex,
    /// [vkey_hash_hex, committed_values_digest_hex]
    pub public_inputs: [String; 2],
    pub commitment: String,
    pub nullifier: String,
    pub vkey_hash: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Sp1ProofHex {
    pub a: String,
    pub b: String,
    pub c: String,
}

/// Compute SP1 commitment and nullifier client-side (same as guest program).
/// commitment = SHA256(amount_be || secret_be || 0x01)
/// nullifier  = SHA256(commitment || secret_be || 0x02)
pub fn sp1_commitment(amount: u64, secret: u64) -> ([u8; 32], [u8; 32]) {
    let commitment: [u8; 32] = {
        let mut h = Sha256::new();
        h.update(amount.to_be_bytes());
        h.update(secret.to_be_bytes());
        h.update([1u8]);
        h.finalize().into()
    };
    let nullifier: [u8; 32] = {
        let mut h = Sha256::new();
        h.update(commitment);
        h.update(secret.to_be_bytes());
        h.update([2u8]);
        h.finalize().into()
    };
    (commitment, nullifier)
}

/// Generate a mock SP1 proof (no real proving, instant).
/// The commitment/nullifier are correctly computed; the proof bytes are zeroed.
/// Only works with pool contracts built with placeholder VK (development mode).
pub fn sp1_mock_proof(amount: u64, secret: u64) -> Sp1ProofOutput {
    let (commitment, nullifier) = sp1_commitment(amount, secret);

    let committed_bytes: Vec<u8> = [commitment.as_slice(), nullifier.as_slice()].concat();
    let digest_bytes: [u8; 32] = Sha256::digest(&committed_bytes).into();

    Sp1ProofOutput {
        proof: Sp1ProofHex {
            a: "00".repeat(64),
            b: "00".repeat(128),
            c: "00".repeat(64),
        },
        public_inputs: ["00".repeat(32), hex::encode(digest_bytes)],
        commitment: hex::encode(commitment),
        nullifier: hex::encode(nullifier),
        vkey_hash: "00".repeat(32),
    }
}

/// Run `sp1-host` binary to generate a real or mock SP1 proof.
/// `sp1_host_bin`: path to the compiled `sp1-host` binary.
pub fn sp1_run_host(sp1_host_bin: &Path, amount: u64, secret: u64, real: bool) -> Result<Sp1ProofOutput> {
    let mut cmd = std::process::Command::new(sp1_host_bin);
    cmd.arg("--amount").arg(amount.to_string());
    cmd.arg("--secret").arg(secret.to_string());
    if real {
        cmd.arg("--real");
    }

    let output = cmd.output()
        .with_context(|| format!("Failed to run sp1-host at {}", sp1_host_bin.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("sp1-host failed:\n{stderr}");
    }

    let json_out = String::from_utf8(output.stdout)
        .map_err(|_| anyhow::anyhow!("sp1-host output is not valid UTF-8"))?;

    serde_json::from_str(&json_out)
        .map_err(|e| anyhow::anyhow!("Failed to parse sp1-host output: {e}\n{json_out}"))
}
