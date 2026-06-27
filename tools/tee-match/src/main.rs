mod db;
mod engine;
mod log;
mod proof;
mod serve;
mod stellar;

use anyhow::Result;
use clap::Parser;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn resolve_path(p: &Path) -> PathBuf {
    if p.is_relative() {
        static ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        let root = ROOT.get_or_init(|| {
            let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
            manifest.parent().unwrap().parent().unwrap().to_path_buf()
        });
        root.join(p)
    } else {
        p.to_path_buf()
    }
}

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
    let keys_dir = resolve_path(&cli.keys_dir);
    let start = Instant::now();

    match cli.command {
        Command::Init { db, side, price, size, leverage, asset, nonce, secret } => {
            log::info!("═══ TEE Init: Order Commitment Generation ═══",
                "db", format!("{}", db.display()),
                "side", side, "price", price, "size", size,
                "leverage", leverage, "asset", asset,
                "nonce", nonce
            );

            let store = db::SecretStore::open(&db)?;
            let secrets = db::OrderSecrets { side, price, size, leverage, asset, nonce, secret };

            log::debug!("Generating commitment proof via Circom",
                "circuit", "order_commitment",
                "keys_dir", format!("{}", keys_dir.display())
            );

            let out = proof::gen_commitment_proof(&keys_dir, &secrets)?;
            let cmt_hex = format!("{:0>64x}", out.public_inputs[0].parse::<num_bigint::BigUint>()?);
            store.insert(&cmt_hex, &secrets)?;

            log::info!("Order initialized and secrets persisted",
                "commitment", log::hex_snippet(&cmt_hex, 16),
                "total_time", log::duration_secs(&start.elapsed())
            );
            println!("{{\"commitment\":\"{cmt_hex}\"}}");
        }

        Command::CommitProof { db, cmt, out } => {
            log::info!("═══ TEE Commit-Proof: Generating Placement Proof ═══",
                "db", format!("{}", db.display()),
                "commitment", log::hex_snippet(&cmt, 16),
                "out_path", format!("{}", out.display())
            );

            let store = db::SecretStore::open(&db)?;
            log::debug!("Loading secrets from DB for commitment", "cmt", &cmt[..16]);
            let secrets = store.get(&cmt)?
                .ok_or_else(|| anyhow::anyhow!("secrets not found for {cmt}"))?;

            log::debug!("Proving commitment circuit", "side", secrets.side, "price", secrets.price);
            let result = proof::gen_commitment_proof(&keys_dir, &secrets)?;
            let proof_json = serde_json::json!({
                "a": result.proof.a,
                "b": result.proof.b,
                "c": result.proof.c,
            });
            std::fs::write(&out, serde_json::to_string(&proof_json)?)?;

            log::info!("Commitment proof written to disk",
                "path", format!("{}", out.display()),
                "total_time", log::duration_secs(&start.elapsed())
            );
        }

        Command::Serve { addr, db } => {
            log::info!("═══ TEE Match Server Launch ═══",
                "listen_addr", &addr,
                "db_path", format!("{}", db.display()),
                "keys_dir", format!("{}", keys_dir.display())
            );
            serve::run(&addr, db, keys_dir.clone())?;
        }

        Command::Match { db, perp, cmt_a, cmt_b, source } => {
            log::info!("═══ TEE Match: Two-Order Settlement ═══",
                "db", format!("{}", db.display()),
                "perp_contract", &perp[..8],
                "cmt_a", log::hex_snippet(&cmt_a, 16),
                "cmt_b", log::hex_snippet(&cmt_b, 16),
                "source", &source
            );

            let store = db::SecretStore::open(&db)?;

            log::debug!("Loading order A secrets from DB", "cmt_a", &cmt_a[..16]);
            let a = store.get(&cmt_a)?
                .ok_or_else(|| anyhow::anyhow!("secrets not found for cmt_a"))?;

            log::debug!("Loading order B secrets from DB", "cmt_b", &cmt_b[..16]);
            let b = store.get(&cmt_b)?
                .ok_or_else(|| anyhow::anyhow!("secrets not found for cmt_b"))?;

            log::info!("Orders loaded from DB",
                "order_a", format!("side={} price={} size={}", a.side, a.price, a.size),
                "order_b", format!("side={} price={} size={}", b.side, b.price, b.size)
            );

            log::debug!("Running matching engine", "side_a", a.side, "price_a", a.price, "side_b", b.side, "price_b", b.price);
            let params = engine::find_match(&a, &b)
                .ok_or_else(|| anyhow::anyhow!("orders are not matchable"))?;
            log::info!("Match parameters computed",
                "match_price", params.match_price,
                "match_size", params.match_size
            );

            log::debug!("Generating Groth16 match proof via Circom");
            let out = proof::gen_match_proof(&keys_dir, &a, &b, params.match_price, params.match_size)?;
            let proof_size = out.proof.a.len() + out.proof.b.len() + out.proof.c.len();
            log::info!("ZK match proof generated",
                "proof_total", log::bytes_label(proof_size / 2)
            );

            log::warning!("Submitting match to Soroban testnet",
                "contract", &perp[..8],
                "source", &source,
                "cmt_a", log::hex_snippet(&cmt_a, 10),
                "cmt_b", log::hex_snippet(&cmt_b, 10)
            );
            stellar::submit_match(&perp, &source, &cmt_a, &cmt_b, &out)?;
            log::info!("Match completed successfully",
                "total_time", log::duration_secs(&start.elapsed())
            );
        }
    }

    Ok(())
}
