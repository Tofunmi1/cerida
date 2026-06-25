use anyhow::{Context, Result};
use ark_bn254::{Bn254, G1Affine, G2Affine};
use ark_circom::{CircomBuilder, CircomConfig, CircomReduction, read_zkey};
use ark_ff::{BigInteger, PrimeField};
use ark_groth16::Groth16;
use clap::{Parser, Subcommand};
use rand::thread_rng;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "prover")]
struct Cli {
    #[command(subcommand)]
    command: Command,

    #[arg(long, default_value = "../circuits/keys")]
    keys_dir: PathBuf,
}

#[derive(Subcommand)]
enum Command {
    OrderCommitment {
        #[arg(long)]
        side: u64,
        #[arg(long)]
        price: u64,
        #[arg(long)]
        size: u64,
        #[arg(long, default_value = "1")]
        leverage: u64,
        #[arg(long, default_value = "0")]
        asset_id: u64,
        #[arg(long)]
        nonce: u64,
        #[arg(long)]
        secret: u64,
    },
    OrderCancel {
        #[arg(long)]
        commitment: String,
        #[arg(long)]
        secret: u64,
    },
    OrderMatch {
        #[arg(long)]
        side_a: u64,
        #[arg(long)]
        price_a: u64,
        #[arg(long)]
        size_a: u64,
        #[arg(long, default_value = "1")]
        leverage_a: u64,
        #[arg(long, default_value = "0")]
        asset_id_a: u64,
        #[arg(long)]
        nonce_a: u64,
        #[arg(long)]
        secret_a: u64,
        #[arg(long)]
        side_b: u64,
        #[arg(long)]
        price_b: u64,
        #[arg(long)]
        size_b: u64,
        #[arg(long, default_value = "1")]
        leverage_b: u64,
        #[arg(long, default_value = "0")]
        asset_id_b: u64,
        #[arg(long)]
        nonce_b: u64,
        #[arg(long)]
        secret_b: u64,
        #[arg(long)]
        match_price: u64,
        #[arg(long)]
        match_size: u64,
    },
}

#[derive(Serialize)]
struct ProofOutput {
    proof: ProofHex,
    public_inputs: Vec<String>,
}

#[derive(Serialize)]
struct ProofHex {
    a: String,
    b: String,
    c: String,
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

fn generate(circuit_name: &str, keys_dir: &PathBuf, builder: CircomBuilder<Bn254>) -> Result<ProofOutput> {
    let zkey_path = keys_dir.join(format!("{}.zkey", circuit_name));
    let wasm_path = keys_dir.join(format!("{}_js/{}.wasm", circuit_name, circuit_name));
    let r1cs_path = keys_dir.join(format!("{}.r1cs", circuit_name));

    let zkey_file = std::fs::File::open(&zkey_path)
        .with_context(|| format!("Failed to open zkey: {}", zkey_path.display()))?;
    let mut reader = std::io::BufReader::new(zkey_file);
    let (proving_key, _matrices) = read_zkey(&mut reader)
        .map_err(|e| anyhow::anyhow!("Failed to read zkey: {e}"))?;

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

    Ok(ProofOutput {
        proof: ProofHex {
            a: g1_to_hex(&proof.a),
            b: g2_to_hex(&proof.b),
            c: g1_to_hex(&proof.c),
        },
        public_inputs: public_inputs.iter()
            .map(|f| f.into_bigint().to_string())
            .collect(),
    })
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let keys_dir = &cli.keys_dir;

    match cli.command {
        Command::OrderCommitment { side, price, size, leverage, asset_id, nonce, secret } => {
            let wasm = keys_dir.join("order_commitment_js/order_commitment.wasm");
            let r1cs = keys_dir.join("order_commitment.r1cs");
            let cfg = CircomConfig::<Bn254>::new(&wasm, &r1cs)
                .map_err(|e| anyhow::anyhow!("Failed to load circuit: {e}"))?;
            let mut builder = CircomBuilder::new(cfg);
            builder.push_input("side", side as i64);
            builder.push_input("price", price as i64);
            builder.push_input("size", size as i64);
            builder.push_input("leverage", leverage as i64);
            builder.push_input("asset", asset_id as i64);
            builder.push_input("nonce", nonce as i64);
            builder.push_input("secret", secret as i64);
            let out = generate("order_commitment", keys_dir, builder)?;
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        Command::OrderCancel { commitment, secret } => {
            let wasm = keys_dir.join("order_cancel_js/order_cancel.wasm");
            let r1cs = keys_dir.join("order_cancel.r1cs");
            let cfg = CircomConfig::<Bn254>::new(&wasm, &r1cs)
                .map_err(|e| anyhow::anyhow!("Failed to load circuit: {e}"))?;
            let mut builder = CircomBuilder::new(cfg);
            let cmt_fr: num_bigint::BigUint = commitment.parse()
                .map_err(|_| anyhow::anyhow!("Invalid commitment decimal"))?;
            builder.push_input("commitment", cmt_fr);
            builder.push_input("secret", secret as i64);
            let out = generate("order_cancel", keys_dir, builder)?;
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        Command::OrderMatch {
            side_a, price_a, size_a, leverage_a, asset_id_a, nonce_a, secret_a,
            side_b, price_b, size_b, leverage_b, asset_id_b, nonce_b, secret_b,
            match_price, match_size,
        } => {
            let wasm = keys_dir.join("order_match_js/order_match.wasm");
            let r1cs = keys_dir.join("order_match.r1cs");
            let cfg = CircomConfig::<Bn254>::new(&wasm, &r1cs)
                .map_err(|e| anyhow::anyhow!("Failed to load circuit: {e}"))?;
            let mut builder = CircomBuilder::new(cfg);
            builder.push_input("side_a", side_a as i64);
            builder.push_input("price_a", price_a as i64);
            builder.push_input("size_a", size_a as i64);
            builder.push_input("leverage_a", leverage_a as i64);
            builder.push_input("asset_a", asset_id_a as i64);
            builder.push_input("nonce_a", nonce_a as i64);
            builder.push_input("secret_a", secret_a as i64);
            builder.push_input("side_b", side_b as i64);
            builder.push_input("price_b", price_b as i64);
            builder.push_input("size_b", size_b as i64);
            builder.push_input("leverage_b", leverage_b as i64);
            builder.push_input("asset_b", asset_id_b as i64);
            builder.push_input("nonce_b", nonce_b as i64);
            builder.push_input("secret_b", secret_b as i64);
            builder.push_input("mp", match_price as i64);
            builder.push_input("ms", match_size as i64);
            let out = generate("order_match", keys_dir, builder)?;
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
    }

    Ok(())
}
