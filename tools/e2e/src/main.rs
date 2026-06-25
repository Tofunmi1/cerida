mod proof;
mod stellar;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "e2e")]
struct Cli {
    #[command(subcommand)]
    command: Command,

    #[arg(long, default_value = "../circuits/keys")]
    keys_dir: PathBuf,

    #[arg(long, default_value = "../contracts/target/wasm32v1-none/release")]
    wasm_dir: PathBuf,
}

#[derive(Subcommand)]
enum Command {
    /// Generate proofs for two orders and their match
    GenProofs {
        #[arg(long, default_value = "0")]
        side_a: u64,
        #[arg(long, default_value = "100000")]
        price_a: u64,
        #[arg(long, default_value = "1000")]
        size_a: u64,
        #[arg(long, default_value = "1")]
        leverage_a: u64,
        #[arg(long, default_value = "1")]
        nonce_a: u64,
        #[arg(long, default_value = "42")]
        secret_a: u64,
        #[arg(long, default_value = "1")]
        side_b: u64,
        #[arg(long, default_value = "99000")]
        price_b: u64,
        #[arg(long, default_value = "500")]
        size_b: u64,
        #[arg(long, default_value = "1")]
        leverage_b: u64,
        #[arg(long, default_value = "2")]
        nonce_b: u64,
        #[arg(long, default_value = "99")]
        secret_b: u64,
        #[arg(long, default_value = "99500")]
        match_price: u64,
        #[arg(long, default_value = "500")]
        match_size: u64,
    },
    /// Full end-to-end: generate proofs, deploy, place, match
    Full {
        #[arg(long, default_value = "0")]
        side_a: u64,
        #[arg(long, default_value = "100000")]
        price_a: u64,
        #[arg(long, default_value = "1000")]
        size_a: u64,
        #[arg(long, default_value = "1")]
        leverage_a: u64,
        #[arg(long, default_value = "1")]
        nonce_a: u64,
        #[arg(long, default_value = "42")]
        secret_a: u64,
        #[arg(long, default_value = "1")]
        side_b: u64,
        #[arg(long, default_value = "99000")]
        price_b: u64,
        #[arg(long, default_value = "500")]
        size_b: u64,
        #[arg(long, default_value = "1")]
        leverage_b: u64,
        #[arg(long, default_value = "2")]
        nonce_b: u64,
        #[arg(long, default_value = "99")]
        secret_b: u64,
        #[arg(long, default_value = "99500")]
        match_price: u64,
        #[arg(long, default_value = "500")]
        match_size: u64,
    },
}

fn decimal_to_hex(s: &str) -> String {
    let n: num_bigint::BigUint = s.parse().expect("Invalid decimal");
    format!("{:0>64x}", n)
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let keys_dir = &cli.keys_dir;
    let wasm_dir = &cli.wasm_dir;

    match cli.command {
        Command::GenProofs {
            side_a, price_a, size_a, leverage_a, nonce_a, secret_a,
            side_b, price_b, size_b, leverage_b, nonce_b, secret_b,
            match_price, match_size,
        } => {
            let p_a = proof::gen_commitment(
                keys_dir, side_a, price_a, size_a, leverage_a, 0, nonce_a, secret_a,
            )?;
            let p_b = proof::gen_commitment(
                keys_dir, side_b, price_b, size_b, leverage_b, 0, nonce_b, secret_b,
            )?;
            let p_match = proof::gen_match(
                keys_dir,
                side_a, price_a, size_a, leverage_a, 0, nonce_a, secret_a,
                side_b, price_b, size_b, leverage_b, 0, nonce_b, secret_b,
                match_price, match_size,
            )?;

            let output = serde_json::json!({
                "commit_a": {
                    "proof": p_a.proof,
                    "commitment": decimal_to_hex(&p_a.public_inputs[0]),
                },
                "commit_b": {
                    "proof": p_b.proof,
                    "commitment": decimal_to_hex(&p_b.public_inputs[0]),
                },
                "match": {
                    "proof": p_match.proof,
                    "cmt_a": decimal_to_hex(&p_match.public_inputs[0]),
                    "cmt_b": decimal_to_hex(&p_match.public_inputs[1]),
                    "match_price": decimal_to_hex(&p_match.public_inputs[2]),
                    "match_size": decimal_to_hex(&p_match.public_inputs[3]),
                    "nullifier_a": decimal_to_hex(&p_match.public_inputs[4]),
                    "nullifier_b": decimal_to_hex(&p_match.public_inputs[5]),
                },
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        Command::Full {
            side_a, price_a, size_a, leverage_a, nonce_a, secret_a,
            side_b, price_b, size_b, leverage_b, nonce_b, secret_b,
            match_price, match_size,
        } => {
            eprintln!("=== Generating proofs ===");
            let p_a = proof::gen_commitment(
                keys_dir, side_a, price_a, size_a, leverage_a, 0, nonce_a, secret_a,
            )?;
            let p_b = proof::gen_commitment(
                keys_dir, side_b, price_b, size_b, leverage_b, 0, nonce_b, secret_b,
            )?;
            let p_match = proof::gen_match(
                keys_dir,
                side_a, price_a, size_a, leverage_a, 0, nonce_a, secret_a,
                side_b, price_b, size_b, leverage_b, 0, nonce_b, secret_b,
                match_price, match_size,
            )?;

            let cmt_a_hex = decimal_to_hex(&p_a.public_inputs[0]);
            let cmt_b_hex = decimal_to_hex(&p_b.public_inputs[0]);

            eprintln!("  commitment A: {}", cmt_a_hex);
            eprintln!("  commitment B: {}", cmt_b_hex);

            stellar::run_e2e(wasm_dir, &p_a, &p_b, &p_match, &cmt_a_hex, &cmt_b_hex)?;
        }
    }

    Ok(())
}
