mod client;
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
    /// Full end-to-end via tee-match server (generate proofs via server)
    Server {
        #[arg(long, default_value = "127.0.0.1:9720")]
        server_addr: String,

        #[arg(long, default_value = "0")]
        side_a: u64,
        #[arg(long, default_value = "100000")]
        price_a: u64,
        #[arg(long, default_value = "1000000000")]
        size_a: u64,
        #[arg(long, default_value = "1")]
        leverage_a: u64,
        #[arg(long, default_value = "111")]
        nonce_a: u64,
        #[arg(long, default_value = "222")]
        secret_a: u64,
        #[arg(long, default_value = "1")]
        side_b: u64,
        #[arg(long, default_value = "99000")]
        price_b: u64,
        #[arg(long, default_value = "1000000000")]
        size_b: u64,
        #[arg(long, default_value = "1")]
        leverage_b: u64,
        #[arg(long, default_value = "333")]
        nonce_b: u64,
        #[arg(long, default_value = "444")]
        secret_b: u64,
        #[arg(long, default_value = "99500")]
        match_price: u64,
        #[arg(long, default_value = "1000000000")]
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
        Command::Server {
            server_addr,
            side_a, price_a, size_a, leverage_a, nonce_a, secret_a,
            side_b, price_b, size_b, leverage_b, nonce_b, secret_b,
            match_price: _, match_size: _,
        } => {
            let client = client::ServerClient::new(&server_addr);

            eprintln!("=== Initializing order A via server ===");
            let cmt_a_hex = client.init(
                side_a, price_a, size_a, leverage_a, 0, nonce_a, secret_a,
            )?;
            eprintln!("  commitment A: {}", cmt_a_hex);

            eprintln!("=== Initializing order B via server ===");
            let cmt_b_hex = client.init(
                side_b, price_b, size_b, leverage_b, 0, nonce_b, secret_b,
            )?;
            eprintln!("  commitment B: {}", cmt_b_hex);

            let tmp_a = std::env::temp_dir().join("e2e_proof_a.json");
            let tmp_b = std::env::temp_dir().join("e2e_proof_b.json");

            eprintln!("=== Generating commit proof A via server ===");
            client.commit_proof(&cmt_a_hex, &tmp_a)?;

            eprintln!("=== Generating commit proof B via server ===");
            client.commit_proof(&cmt_b_hex, &tmp_b)?;

            let proof_a_json = std::fs::read_to_string(&tmp_a)?;
            let proof_b_json = std::fs::read_to_string(&tmp_b)?;

            let ctx = stellar::deploy_and_place(
                wasm_dir,
                &proof_a_json, &proof_b_json,
                &cmt_a_hex, &cmt_b_hex,
                price_a, price_b, "0", "1",
            )?;
            eprintln!("  orderbook: {}", ctx.orderbook_id);
            eprintln!("  perp: {}", ctx.perp_id);

            eprintln!("\n=== Matching via server ===");
            let result = client.match_orders(
                &cmt_a_hex, &cmt_b_hex, &ctx.perp_id, stellar::SOURCE,
            )?;
            eprintln!("  match_price: {}", result.match_price);
            eprintln!("  match_size: {}", result.match_size);
            eprintln!("  nullifier_a: {}", result.nullifier_a);
            eprintln!("  nullifier_b: {}", result.nullifier_b);

            stellar::verify_match(&ctx, &result.nullifier_a, &result.nullifier_b)?;
        }
    }

    Ok(())
}
