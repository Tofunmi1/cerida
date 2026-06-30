use std::path::PathBuf;

use anyhow::{Context, Result};
use ark_bn254::Fr;
use ark_ff::PrimeField;
use ark_serialize::CanonicalSerialize;
use clap::{Parser, Subcommand};
use rust_circuits::{prove_cancel, prove_commitment, prove_match};

#[derive(Parser)]
#[command(name = "rust-prover")]
struct Cli {
    #[command(subcommand)]
    command: Command,
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
    /// Run setup for all circuits, saving pk.bin and vk.json
    Setup {
        #[arg(long, default_value = "circuits/keys")]
        out_dir: PathBuf,
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

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Setup { out_dir } => {
            use rust_circuits::{setup_all, vk_to_json};
            std::fs::create_dir_all(&out_dir)?;
            let mut rng = rand::thread_rng();

            let results = setup_all(&mut rng)?;
            let names = ["order_commitment", "order_cancel", "order_match"];
            for (name, (pk, vk)) in names.iter().zip(results.iter()) {
                eprintln!("Setting up {}…", name);
                let pk_path = out_dir.join(format!("{}.pk.bin", name));
                let mut pk_bytes = Vec::new();
                pk.serialize_compressed(&mut pk_bytes)?;
                std::fs::write(&pk_path, &pk_bytes)
                    .with_context(|| format!("Writing {pk_path:?}"))?;
                eprintln!("  pk: {pk_path:?} ({} bytes)", pk_bytes.len());
                let vk_json = vk_to_json(vk);
                let vk_path = out_dir.join(format!("{}_vk.json", name));
                let vk_json_str = serde_json::to_string_pretty(&vk_json)?;
                std::fs::write(&vk_path, &vk_json_str)
                    .with_context(|| format!("Writing {vk_path:?}"))?;
                eprintln!("  vk: {vk_path:?} ({} bytes)", vk_json_str.len());
            }
        }
        Command::OrderCommitment { side, price, size, leverage, asset_id, nonce, secret } => {
            let out = prove_commitment(
                Fr::from(side), Fr::from(price), Fr::from(size), Fr::from(leverage),
                Fr::from(asset_id), Fr::from(0), Fr::from(nonce), Fr::from(secret),
            )?;
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        Command::OrderCancel { commitment, secret } => {
            let cmt: num_bigint::BigUint = commitment
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid commitment decimal: {commitment}"))?;
            let cmt_fr = Fr::from_be_bytes_mod_order(&cmt.to_bytes_be());
            let out = prove_cancel(cmt_fr, Fr::from(secret))?;
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        Command::OrderMatch {
            side_a, price_a, size_a, leverage_a, asset_id_a, nonce_a, secret_a,
            side_b, price_b, size_b, leverage_b, asset_id_b, nonce_b, secret_b,
            match_price, match_size,
        } => {
            let out = prove_match(
                Fr::from(side_a), Fr::from(price_a), Fr::from(size_a), Fr::from(leverage_a),
                Fr::from(asset_id_a), Fr::from(0), Fr::from(nonce_a), Fr::from(secret_a),
                Fr::from(side_b), Fr::from(price_b), Fr::from(size_b), Fr::from(leverage_b),
                Fr::from(asset_id_b), Fr::from(0), Fr::from(nonce_b), Fr::from(secret_b),
                Fr::from(match_price), Fr::from(match_size),
            )?;
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
    }

    Ok(())
}
