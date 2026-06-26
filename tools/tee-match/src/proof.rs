use crate::db::OrderSecrets;
use crate::log;
use anyhow::{Context, Result};
use ark_bn254::{Bn254, G1Affine, G2Affine};
use ark_circom::{CircomBuilder, CircomConfig, CircomReduction, read_zkey};
use ark_ff::{BigInteger, PrimeField};
use ark_groth16::Groth16;
use rand::thread_rng;
use serde::Serialize;
use std::path::Path;
use std::time::Instant;

#[derive(Serialize)]
pub struct ProofHex {
    pub a: String,
    pub b: String,
    pub c: String,
}

pub struct MatchProof {
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
        hex::encode(d0_be)
    )
}

fn generate(circuit: &str, keys_dir: &Path, builder: CircomBuilder<Bn254>) -> Result<MatchProof> {
    let start = Instant::now();
    log::debug!("Starting Groth16 proof generation", "circuit", circuit);
    log::debug!("Looking up proving key", "zkey", format!("{circuit}.zkey"));

    let zkey_path = keys_dir.join(format!("{circuit}.zkey"));
    let zkey_file = std::fs::File::open(&zkey_path)
        .with_context(|| format!("Failed to open zkey: {}", zkey_path.display()))?;
    let zkey_size = zkey_file.metadata().map(|m| m.len()).unwrap_or(0);
    log::debug!("Proving key file loaded",
        "path", format!("{}", zkey_path.display()),
        "size", log::bytes_label(zkey_size as usize)
    );

    let mut reader = std::io::BufReader::new(zkey_file);
    let (proving_key, _matrices) =
        read_zkey(&mut reader).map_err(|e| anyhow::anyhow!("Failed to read zkey: {e}"))?;

    log::debug!("Zkey parsed, building circuit witness");
    let circom = builder
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build circuit: {e}"))?;
    let public_inputs = circom
        .get_public_inputs()
        .ok_or_else(|| anyhow::anyhow!("No public inputs in circuit"))?;
    let num_pub = public_inputs.len();

    log::debug!("Circuit built successfully",
        "public_inputs", num_pub,
        "took", log::duration_secs(&start.elapsed())
    );

    log::debug!("Running Groth16 prover (multi-scalar exp)…");
    let prove_start = Instant::now();
    let mut rng = thread_rng();
    let proof = Groth16::<Bn254, CircomReduction>::create_random_proof_with_reduction(
        circom, &proving_key, &mut rng
    )
    .map_err(|e| anyhow::anyhow!("Failed to generate proof: {e}"))?;

    let proof_time = prove_start.elapsed();
    let result = MatchProof {
        proof: ProofHex {
            a: g1_to_hex(&proof.a),
            b: g2_to_hex(&proof.b),
            c: g1_to_hex(&proof.c),
        },
        public_inputs: public_inputs
            .iter()
            .map(|f| f.into_bigint().to_string())
            .collect(),
    };

    let total = start.elapsed();
    log::info!("ZK proof generated",
        "circuit", circuit,
        "public_inputs", num_pub,
        "prove_time", log::duration_secs(&proof_time),
        "total_time", log::duration_secs(&total)
    );

    Ok(result)
}

pub fn gen_commitment_proof(keys_dir: &Path, secrets: &OrderSecrets) -> Result<MatchProof> {
    let wasm = keys_dir.join("order_commitment_js/order_commitment.wasm");
    let r1cs = keys_dir.join("order_commitment.r1cs");
    log::debug!("Loading Circom circuit",
        "circuit", "order_commitment",
        "wasm", format!("{}", wasm.display()),
        "r1cs", format!("{}", r1cs.display())
    );
    let cfg = CircomConfig::<Bn254>::new(&wasm, &r1cs)
        .map_err(|e| anyhow::anyhow!("Failed to load circuit: {e}"))?;
    let mut builder = CircomBuilder::new(cfg);
    builder.push_input("side", secrets.side as i64);
    builder.push_input("price", secrets.price as i64);
    builder.push_input("size", secrets.size as i64);
    builder.push_input("leverage", secrets.leverage as i64);
    builder.push_input("asset", secrets.asset as i64);
    builder.push_input("nonce", secrets.nonce as i64);
    builder.push_input("secret", secrets.secret as i64);
    log::debug!("Commitment circuit inputs prepared",
        "side", secrets.side, "price", secrets.price, "size", secrets.size,
        "leverage", secrets.leverage, "asset", secrets.asset,
        "nonce", secrets.nonce
    );
    generate("order_commitment", keys_dir, builder)
}

pub fn gen_match_proof(
    keys_dir: &Path,
    a: &OrderSecrets,
    b: &OrderSecrets,
    mp: u64,
    ms: u64,
) -> Result<MatchProof> {
    let wasm = keys_dir.join("order_match_js/order_match.wasm");
    let r1cs = keys_dir.join("order_match.r1cs");
    log::debug!("Loading Circom circuit",
        "circuit", "order_match",
        "wasm", format!("{}", wasm.display()),
        "r1cs", format!("{}", r1cs.display())
    );
    let cfg = CircomConfig::<Bn254>::new(&wasm, &r1cs)
        .map_err(|e| anyhow::anyhow!("Failed to load circuit: {e}"))?;
    let mut builder = CircomBuilder::new(cfg);
    builder.push_input("side_a", a.side as i64);
    builder.push_input("price_a", a.price as i64);
    builder.push_input("size_a", a.size as i64);
    builder.push_input("leverage_a", a.leverage as i64);
    builder.push_input("asset_a", a.asset as i64);
    builder.push_input("nonce_a", a.nonce as i64);
    builder.push_input("secret_a", a.secret as i64);
    builder.push_input("side_b", b.side as i64);
    builder.push_input("price_b", b.price as i64);
    builder.push_input("size_b", b.size as i64);
    builder.push_input("leverage_b", b.leverage as i64);
    builder.push_input("asset_b", b.asset as i64);
    builder.push_input("nonce_b", b.nonce as i64);
    builder.push_input("secret_b", b.secret as i64);
    builder.push_input("mp", mp as i64);
    builder.push_input("ms", ms as i64);
    log::debug!("Match circuit inputs prepared",
        "side_a", a.side, "price_a", a.price, "size_a", a.size,
        "side_b", b.side, "price_b", b.price, "size_b", b.size,
        "match_price", mp, "match_size", ms
    );
    generate("order_match", keys_dir, builder)
}
