mod db;
mod engine;
mod log;
mod proof;
mod serve;
mod stellar;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "tee-match")]
struct Cli {
    #[arg(long, default_value = "circuits/keys")]
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
    /// Run as a persistent TCP server
    Serve {
        #[arg(long, default_value = "0.0.0.0:9720")]
        addr: String,
        #[arg(long)]
        db: PathBuf,
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
            log::info!("Initializing TEE order", "side", side, "price", price, "nonce", nonce);
            let store = db::SecretStore::open(&db)?;
            let secrets = db::OrderSecrets { side, price, size, leverage, asset, nonce, secret };
            log::debug!("Generating commitment proof via Circom", "circuit", "order_commitment");
            let out = proof::gen_commitment_proof(keys_dir, &secrets)?;
            let cmt_hex = format!("{:0>64x}", out.public_inputs[0].parse::<num_bigint::BigUint>()?);
            store.insert(&cmt_hex, &secrets)?;
            log::info!("Order initialized and secrets stored", "commitment", &cmt_hex[..16]);
            println!("{{\"commitment\":\"{cmt_hex}\"}}");
        }
        Command::CommitProof { db, cmt, out } => {
            log::info!("Generating commitment proof for on-chain placement", "cmt", &cmt[..16]);
            let store = db::SecretStore::open(&db)?;
            log::debug!("Loading secrets from DB");
            let secrets = store.get(&cmt)?
                .ok_or_else(|| anyhow::anyhow!("secrets not found for {cmt}"))?;
            log::debug!("Proving commitment circuit");
            let result = proof::gen_commitment_proof(keys_dir, &secrets)?;
            let proof_json = serde_json::json!({
                "a": result.proof.a,
                "b": result.proof.b,
                "c": result.proof.c,
            });
            std::fs::write(&out, serde_json::to_string(&proof_json)?)?;
            log::info!("Commitment proof written to disk", "path", format!("{}", out.display()));
        }
        Command::Serve { addr, db } => {
            log::info!("Starting TEE match server", "addr", &addr, "db", format!("{}", db.display()));
            serve::run(&addr, db, keys_dir.clone())?;
        }
        Command::Match { db, perp, cmt_a, cmt_b, source } => {
            log::info!("Matching two orders", "cmt_a", &cmt_a[..16], "cmt_b", &cmt_b[..16]);
            let store = db::SecretStore::open(&db)?;
            log::debug!("Loading order secrets from DB");
            let a = store.get(&cmt_a)?
                .ok_or_else(|| anyhow::anyhow!("secrets not found for cmt_a"))?;
            let b = store.get(&cmt_b)?
                .ok_or_else(|| anyhow::anyhow!("secrets not found for cmt_b"))?;

            log::debug!("Running matching engine", "side_a", a.side, "price_a", a.price, "side_b", b.side, "price_b", b.price);
            let params = engine::find_match(&a, &b)
                .ok_or_else(|| anyhow::anyhow!("orders are not matchable"))?;
            log::info!("Match parameters computed", "match_price", params.match_price, "match_size", params.match_size);

            log::debug!("Generating Groth16 match proof via Circom");
            let out = proof::gen_match_proof(keys_dir, &a, &b, params.match_price, params.match_size)?;
            let proof_size = out.proof.a.len() + out.proof.b.len() + out.proof.c.len();
            log::info!("ZK match proof generated", "proof_size", format!("{proof_size} B"));

            log::warning!("Submitting match to Soroban testnet", "contract", &perp[..8], "source", &source);
            stellar::submit_match(&perp, &source, &cmt_a, &cmt_b, &out)?;
            log::info!("Match transaction confirmed on-chain");
        }
    }

    Ok(())
}
