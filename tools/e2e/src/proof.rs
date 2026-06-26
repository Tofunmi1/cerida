use anyhow::{Context, Result};
use ark_bn254::{Bn254, G1Affine, G2Affine};
use ark_circom::{CircomBuilder, CircomConfig, CircomReduction, read_zkey};
use ark_ff::{BigInteger, PrimeField};
use ark_groth16::Groth16;
use rand::thread_rng;
use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
pub struct ProofHex {
    pub a: String,
    pub b: String,
    pub c: String,
}

#[derive(Serialize)]
pub struct RawProof {
    pub proof: ProofHex,
    pub public_inputs: Vec<String>,
}

fn g1_to_hex(g1: &G1Affine) -> String {
    let x_be = g1.x.into_bigint().to_bytes_be();
    let y_be = g1.y.into_bigint().to_bytes_be();
    format!("{}{}", hex::encode(x_be), hex::encode(y_be))
}

fn g2_to_hex(g2: &G2Affine) -> String {
    let c0_be = g2.x.c0.into_bigint().to_bytes_be();
    let c1_be = g2.x.c1.into_bigint().to_bytes_be();
    let d0_be = g2.y.c0.into_bigint().to_bytes_be();
    let d1_be = g2.y.c1.into_bigint().to_bytes_be();
    format!(
        "{}{}{}{}",
        hex::encode(c1_be),
        hex::encode(c0_be),
        hex::encode(d1_be),
        hex::encode(d0_be),
    )
}

fn run(_wasm: &Path, _r1cs: &Path, zkey: &Path, builder: CircomBuilder<Bn254>) -> Result<RawProof> {
    let zkey_file =
        std::fs::File::open(zkey).with_context(|| format!("Failed to open zkey: {}", zkey.display()))?;
    let mut reader = std::io::BufReader::new(zkey_file);
    let (proving_key, _matrices) =
        read_zkey(&mut reader).map_err(|e| anyhow::anyhow!("Failed to read zkey: {e}"))?;

    let circom = builder
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build circuit: {e}"))?;
    let public_inputs = circom
        .get_public_inputs()
        .ok_or_else(|| anyhow::anyhow!("No public inputs in circuit"))?;

    let mut rng = thread_rng();
    let proof = Groth16::<Bn254, CircomReduction>::create_random_proof_with_reduction(
        circom,
        &proving_key,
        &mut rng,
    )
    .map_err(|e| anyhow::anyhow!("Failed to generate proof: {e}"))?;

    Ok(RawProof {
        proof: ProofHex {
            a: g1_to_hex(&proof.a),
            b: g2_to_hex(&proof.b),
            c: g1_to_hex(&proof.c),
        },
        public_inputs: public_inputs.iter().map(|f| f.into_bigint().to_string()).collect(),
    })
}

pub fn gen_commitment(
    keys_dir: &Path,
    side: u64,
    price: u64,
    size: u64,
    leverage: u64,
    asset: u64,
    nonce: u64,
    secret: u64,
) -> Result<RawProof> {
    let wasm = keys_dir.join("order_commitment_js/order_commitment.wasm");
    let r1cs = keys_dir.join("order_commitment.r1cs");
    let zkey = keys_dir.join("order_commitment.zkey");
    let cfg =
        CircomConfig::<Bn254>::new(&wasm, &r1cs).map_err(|e| anyhow::anyhow!("Failed to load circuit: {e}"))?;
    let mut builder = CircomBuilder::new(cfg);
    builder.push_input("side", side as i64);
    builder.push_input("price", price as i64);
    builder.push_input("size", size as i64);
    builder.push_input("leverage", leverage as i64);
    builder.push_input("asset", asset as i64);
    builder.push_input("nonce", nonce as i64);
    builder.push_input("secret", secret as i64);
    run(&wasm, &r1cs, &zkey, builder)
}

pub fn gen_match(
    keys_dir: &Path,
    side_a: u64,
    price_a: u64,
    size_a: u64,
    leverage_a: u64,
    asset_a: u64,
    nonce_a: u64,
    secret_a: u64,
    side_b: u64,
    price_b: u64,
    size_b: u64,
    leverage_b: u64,
    asset_b: u64,
    nonce_b: u64,
    secret_b: u64,
    mp: u64,
    ms: u64,
) -> Result<RawProof> {
    let wasm = keys_dir.join("order_match_js/order_match.wasm");
    let r1cs = keys_dir.join("order_match.r1cs");
    let zkey = keys_dir.join("order_match.zkey");
    let cfg =
        CircomConfig::<Bn254>::new(&wasm, &r1cs).map_err(|e| anyhow::anyhow!("Failed to load circuit: {e}"))?;
    let mut builder = CircomBuilder::new(cfg);
    builder.push_input("side_a", side_a as i64);
    builder.push_input("price_a", price_a as i64);
    builder.push_input("size_a", size_a as i64);
    builder.push_input("leverage_a", leverage_a as i64);
    builder.push_input("asset_a", asset_a as i64);
    builder.push_input("nonce_a", nonce_a as i64);
    builder.push_input("secret_a", secret_a as i64);
    builder.push_input("side_b", side_b as i64);
    builder.push_input("price_b", price_b as i64);
    builder.push_input("size_b", size_b as i64);
    builder.push_input("leverage_b", leverage_b as i64);
    builder.push_input("asset_b", asset_b as i64);
    builder.push_input("nonce_b", nonce_b as i64);
    builder.push_input("secret_b", secret_b as i64);
    builder.push_input("mp", mp as i64);
    builder.push_input("ms", ms as i64);
    run(&wasm, &r1cs, &zkey, builder)
}
