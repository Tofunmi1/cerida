mod db;
mod engine;
mod log;
mod proof;
mod stellar;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "tee-match")]
struct Cli {
    #[arg(long, default_value = "../circuits/keys")]
    keys_dir: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Initialize a secrets file by generating a commitment and storing its secrets
    Init {
        #[arg(long)]
        db: PathBuf,
        #[arg(long)]
        side: u64,
        #[arg(long)]
        price: u64,
        #[arg(long)]
        size: u64,
        #[arg(long, default_value = "1")]
        leverage: u64,
        #[arg(long, default_value = "0")]
        asset: u64,
        #[arg(long)]
        nonce: u64,
        #[arg(long)]
        secret: u64,
    },
    /// Generate a commitment proof JSON file (for place_order)
    CommitProof {
        #[arg(long)]
        db: PathBuf,
        #[arg(long)]
        cmt: String,
        #[arg(long)]
        out: PathBuf,
    },
    /// Match two orders: verify, generate proof, submit on-chain
    Match {
        #[arg(long)]
        db: PathBuf,
        #[arg(long)]
        perp: String,
        #[arg(long)]
        cmt_a: String,
        #[arg(long)]
        cmt_b: String,
        #[arg(long)]
        source: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let keys_dir = &cli.keys_dir;

    match cli.command {
        Command::Init { db, side, price, size, leverage, asset, nonce, secret } => {
            log::header("TEE Init");
            let store = db::SecretStore::open(&db)?;
            let secrets = db::OrderSecrets { side, price, size, leverage, asset, nonce, secret };
            log::info("generating commitment proof...");
            let out = proof::gen_commitment_proof(keys_dir, &secrets)?;
            let cmt_hex = format!("{:0>64x}", out.public_inputs[0].parse::<num_bigint::BigUint>()?);
            store.insert(&cmt_hex, &secrets)?;
            log::value("commitment", &cmt_hex);
            log::ok("secrets stored");
            println!("{{\"commitment\":\"{cmt_hex}\"}}");
        }
        Command::CommitProof { db, cmt, out } => {
            log::header("TEE Commit-Proof");
            let store = db::SecretStore::open(&db)?;
            log::info("loading secrets...");
            let secrets = store.get(&cmt)?
                .ok_or_else(|| anyhow::anyhow!("secrets not found for {cmt}"))?;
            log::info("generating commitment proof...");
            let result = proof::gen_commitment_proof(keys_dir, &secrets)?;
            let proof_json = serde_json::json!({
                "a": result.proof.a,
                "b": result.proof.b,
                "c": result.proof.c,
            });
            std::fs::write(&out, serde_json::to_string(&proof_json)?)?;
            log::ok(&format!("proof written to {}", out.display()));
        }
        Command::Match { db, perp, cmt_a, cmt_b, source } => {
            log::header("TEE Match");
            let store = db::SecretStore::open(&db)?;
            log::info("loading secrets...");
            let a = store.get(&cmt_a)?
                .ok_or_else(|| anyhow::anyhow!("secrets not found for cmt_a"))?;
            let b = store.get(&cmt_b)?
                .ok_or_else(|| anyhow::anyhow!("secrets not found for cmt_b"))?;

            log::info("running matching engine...");
            let params = engine::find_match(&a, &b)
                .ok_or_else(|| anyhow::anyhow!("orders are not matchable"))?;
            log::value("match_price", &params.match_price.to_string());
            log::value("match_size", &params.match_size.to_string());

            log::info("generating match proof...");
            let out = proof::gen_match_proof(keys_dir, &a, &b, params.match_price, params.match_size)?;
            log::ok("match proof generated");

            log::info("submitting on-chain...");
            stellar::submit_match(&perp, &source, &cmt_a, &cmt_b, &out)?;
            log::ok("match submitted on-chain");
        }
    }

    Ok(())
}
