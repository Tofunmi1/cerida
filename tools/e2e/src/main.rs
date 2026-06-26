mod benchmark;
mod client;
mod proof;
mod stellar;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::Instant;

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
    /// Benchmark: 10 market makers + 10 traders on testnet via tee-match server
    Benchmark {
        #[arg(long, default_value = "10")]
        mms: usize,
        #[arg(long, default_value = "10")]
        traders: usize,
        #[arg(long, default_value = "5")]
        orders_per_mm: usize,
        #[arg(long, default_value = "3")]
        orders_per_trader: usize,
        #[arg(long, default_value = "127.0.0.1:9720")]
        server_addr: String,
        #[arg(long, default_value = "100000")]
        center_price: u64,
        #[arg(long, default_value = "5")]
        spread_pct: u64,
        #[arg(long, default_value = "10000000")]
        order_size: u64,
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
    let global_start = Instant::now();

    match cli.command {
        Command::GenProofs {
            side_a, price_a, size_a, leverage_a, nonce_a, secret_a,
            side_b, price_b, size_b, leverage_b, nonce_b, secret_b,
            match_price, match_size,
        } => {
            eprintln!("━━━ GenProofs ━━━");
            eprintln!("  Order A: side={} price={} size={} leverage={} nonce={}",
                side_a, price_a, size_a, leverage_a, nonce_a);
            eprintln!("  Order B: side={} price={} size={} leverage={} nonce={}",
                side_b, price_b, size_b, leverage_b, nonce_b);
            eprintln!("  Match: price={} size={}", match_price, match_size);

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
        Command::Benchmark {
            mms, traders, orders_per_mm, orders_per_trader,
            server_addr, center_price, spread_pct, order_size,
        } => {
            eprintln!("━━━ Benchmark ({mms}MM × {traders}T) ━━━");
            eprintln!("  Server: {server_addr}");
            eprintln!("  Orders: {orders_per_mm}/MM + {orders_per_trader}/T");
            eprintln!("  Market: center={center_price} spread={spread_pct}% size={order_size}");

            let cfg = benchmark::BenchmarkConfig {
                mm_count: mms, trader_count: traders,
                orders_per_mm, orders_per_trader,
                server_addr, center_price, spread_pct, order_size,
            };
            let _report = benchmark::run_benchmark(wasm_dir, keys_dir, cfg)?;

            eprintln!("\n━━━ BENCHMARK COMPLETE ({:.2}s) ━━━", global_start.elapsed().as_secs_f64());
        }
        Command::Full {
            side_a, price_a, size_a, leverage_a, nonce_a, secret_a,
            side_b, price_b, size_b, leverage_b, nonce_b, secret_b,
            match_price, match_size,
        } => {
            eprintln!("━━━ Full E2E (local ZK proofs) ━━━");
            eprintln!("  Orders: A(price={},side={},size={})  B(price={},side={},size={})",
                price_a, side_a, size_a, price_b, side_b, size_b);
            eprintln!("  Match: price={} size={}", match_price, match_size);

            eprintln!("── Generating ZK proofs ──");
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

            eprintln!("  commitment A: {} ({} hex chars)", &cmt_a_hex[..16], cmt_a_hex.len());
            eprintln!("  commitment B: {} ({} hex chars)", &cmt_b_hex[..16], cmt_b_hex.len());

            stellar::run_e2e(wasm_dir, &p_a, &p_b, &p_match, &cmt_a_hex, &cmt_b_hex)?;
        }
        Command::Server {
            server_addr,
            side_a, price_a, size_a, leverage_a, nonce_a, secret_a,
            side_b, price_b, size_b, leverage_b, nonce_b, secret_b,
            match_price: _, match_size: _,
        } => {
            eprintln!("━━━ E2E via TEE Match Server ━━━");
            eprintln!("  Server: {server_addr}");
            eprintln!("  Order A: side={} price={} size={} leverage={} nonce={}",
                side_a, price_a, size_a, leverage_a, nonce_a);
            eprintln!("  Order B: side={} price={} size={} leverage={} nonce={}",
                side_b, price_b, size_b, leverage_b, nonce_b);

            let client = client::ServerClient::new(&server_addr);

            // ── Step 1: Init orders via server ──
            eprintln!("\n── Step 1/5: Init orders via server ──");
            let cmt_a_hex = client.init(
                side_a, price_a, size_a, leverage_a, 0, nonce_a, secret_a,
            )?;
            eprintln!("  ✓ commitment A: {}", &cmt_a_hex[..16]);

            let cmt_b_hex = client.init(
                side_b, price_b, size_b, leverage_b, 0, nonce_b, secret_b,
            )?;
            eprintln!("  ✓ commitment B: {}", &cmt_b_hex[..16]);

            // ── Step 2: Generate place_order proofs via server ──
            eprintln!("\n── Step 2/5: Generate placement proofs via server ──");
            let tmp_a = std::env::temp_dir().join("e2e_proof_a.json");
            let tmp_b = std::env::temp_dir().join("e2e_proof_b.json");

            client.commit_proof(&cmt_a_hex, &tmp_a)?;
            client.commit_proof(&cmt_b_hex, &tmp_b)?;

            let proof_a_json = std::fs::read_to_string(&tmp_a)?;
            let proof_b_json = std::fs::read_to_string(&tmp_b)?;
            eprintln!("  ✓ proof A: {} bytes", proof_a_json.len());
            eprintln!("  ✓ proof B: {} bytes", proof_b_json.len());

            // ── Step 3: Deploy contracts, place orders, deposit, open positions ──
            eprintln!("\n── Step 3/5: Deploy and setup ──");
            let ctx = stellar::deploy_and_place(
                wasm_dir,
                &proof_a_json, &proof_b_json,
                &cmt_a_hex, &cmt_b_hex,
                price_a, price_b, "0", "1",
            )?;
            eprintln!("  ✓ orderbook: {}", ctx.orderbook_id);
            eprintln!("  ✓ perp: {}", ctx.perp_id);
            eprintln!("  ✓ admin: {}", ctx.source_pk);
            eprintln!("  ✓ alice: {}", ctx.alice.0);
            eprintln!("  ✓ bob: {}", ctx.bob.0);

            // ── Step 4: Match via server (server generates proof + submits on-chain) ──
            eprintln!("\n── Step 4/5: Match via server ──");
            let result = client.match_orders(
                &cmt_a_hex, &cmt_b_hex, &ctx.perp_id, stellar::SOURCE,
            )?;
            eprintln!("  ✓ match_price: {}", &result.match_price);
            eprintln!("  ✓ match_size:  {}", &result.match_size);
            eprintln!("  ✓ nullifier_a: {}", &result.nullifier_a);
            eprintln!("  ✓ nullifier_b: {}", &result.nullifier_b);

            // ── Step 5: Verify on-chain ──
            eprintln!("\n── Step 5/5: Verify on-chain ──");
            stellar::verify_match(&ctx, &result.nullifier_a, &result.nullifier_b)?;

            eprintln!("\n━━━ E2E COMPLETE ({:.2}s) ━━━", global_start.elapsed().as_secs_f64());
        }
    }

    Ok(())
}
