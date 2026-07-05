mod db;
mod engine;
mod liquidator;
mod log;
mod proof;
mod serve;
mod stellar;

#[cfg(feature = "secure")]
mod attestation;
#[cfg(feature = "secure")]
mod crypto;
#[cfg(feature = "secure")]
mod kms;

use anyhow::Result;
use clap::Parser;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn resolve_path(p: &Path) -> PathBuf {
    let base = std::env::current_dir().unwrap_or_else(|_| {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    });
    let joined = if p.is_relative() {
        base.join(p)
    } else {
        p.to_path_buf()
    };
    std::fs::canonicalize(&joined).unwrap_or(joined)
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
    /// Run as a persistent TCP server (dev mode)
    Serve {
        #[arg(long, default_value = "0.0.0.0:9720")]
        addr: String,
        #[arg(long)]
        db: PathBuf,
        #[arg(long)]
        perp_id: Option<String>,
        #[arg(long, default_value = "300")]
        liquidator_interval_secs: u64,
        #[arg(long)]
        http_port: Option<u16>,
    },
    /// Run as a secure HTTP server with attestation + encryption
    #[cfg(feature = "secure")]
    ServeSecure {
        #[arg(long, default_value = "0.0.0.0:9721")]
        addr: String,
        #[arg(long)]
        db: PathBuf,
        #[arg(long)]
        perp_id: Option<String>,
        #[arg(long, default_value = "300")]
        liquidator_interval_secs: u64,
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
        Command::Init {
            db,
            side,
            price,
            size,
            leverage,
            asset,
            nonce,
            secret,
        } => {
            log::info!(
                "═══ TEE Init: Order Commitment Generation ═══",
                "db",
                format!("{}", db.display()),
                "side",
                side,
                "price",
                price,
                "size",
                size,
                "leverage",
                leverage,
                "asset",
                asset,
                "nonce",
                nonce
            );

            let sled_db = db::open_db(&db)?;
            let store = db::SecretStore::open(&sled_db)?;
            let is_market = side >= 2;
            let side = match side {
                0 | 3 => 0,
                _ => 1,
            };
            let secrets = db::OrderSecrets {
                side,
                price,
                size,
                leverage,
                asset,
                nonce,
                secret,
                is_market,
            };

            log::debug!(
                "Generating commitment proof via native Rust circuits",
                "circuit",
                "order_commitment"
            );

            let out = proof::gen_commitment_proof(&keys_dir, &secrets)?;
            let cmt_hex = format!(
                "{:0>64x}",
                out.public_inputs[0].parse::<num_bigint::BigUint>()?
            );
            store.insert(&cmt_hex, &secrets)?;

            log::info!(
                "Order initialized and secrets persisted",
                "commitment",
                log::hex_snippet(&cmt_hex, 16),
                "total_time",
                log::duration_secs(&start.elapsed())
            );
            println!("{{\"commitment\":\"{cmt_hex}\"}}");
        }

        Command::CommitProof { db, cmt, out } => {
            log::info!(
                "═══ TEE Commit-Proof: Generating Placement Proof ═══",
                "db",
                format!("{}", db.display()),
                "commitment",
                log::hex_snippet(&cmt, 16),
                "out_path",
                format!("{}", out.display())
            );

            let sled_db = db::open_db(&db)?;
            let store = db::SecretStore::open(&sled_db)?;
            log::debug!("Loading secrets from DB for commitment", "cmt", &cmt[..16]);
            let secrets = store
                .get(&cmt)?
                .ok_or_else(|| anyhow::anyhow!("secrets not found for {cmt}"))?;

            log::debug!(
                "Proving commitment circuit",
                "side",
                secrets.side,
                "price",
                secrets.price
            );
            let result = proof::gen_commitment_proof(&keys_dir, &secrets)?;
            let proof_json = serde_json::json!({
                "a": result.proof.a,
                "b": result.proof.b,
                "c": result.proof.c,
            });
            std::fs::write(&out, serde_json::to_string(&proof_json)?)?;

            log::info!(
                "Commitment proof written to disk",
                "path",
                format!("{}", out.display()),
                "total_time",
                log::duration_secs(&start.elapsed())
            );
        }

        Command::Serve {
            addr,
            db,
            perp_id,
            liquidator_interval_secs,
            http_port,
        } => {
            log::info!(
                "═══ TEE Match Server Launch ═══",
                "listen_addr",
                &addr,
                "db_path",
                format!("{}", db.display()),
                "keys_dir",
                format!("{}", keys_dir.display()),
                "liquidator",
                perp_id.is_some()
            );
            serve::run(
                &addr,
                db,
                keys_dir.clone(),
                perp_id,
                liquidator_interval_secs,
                http_port,
            )?;
        }

        #[cfg(feature = "secure")]
        Command::ServeSecure {
            addr,
            db,
            perp_id,
            liquidator_interval_secs,
        } => {
            log::info!(
                "═══ TEE Secure Server Launch ═══",
                "listen_addr",
                &addr,
                "db_path",
                format!("{}", db.display()),
                "keys_dir",
                format!("{}", keys_dir.display()),
                "liquidator",
                perp_id.is_some()
            );
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(serve::secure::run_secure(
                &addr,
                db,
                keys_dir.clone(),
                perp_id,
                liquidator_interval_secs,
            ))?;
        }

        Command::Match {
            db: _db,
            perp: _perp,
            cmt_a: _cmt_a,
            cmt_b: _cmt_b,
            source: _source,
        } => {
            log::warning!(
                "═══ TEE Match: Obsolete ═══",
                "note",
                "On-chain matching removed in Phase 2. Matching is handled by the CLOB server."
            );
        }
    }

    Ok(())
}
