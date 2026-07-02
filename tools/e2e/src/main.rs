mod benchmark;
mod client;
mod proof;
mod soroban_rpc;
mod stellar;

use anyhow::Result;
use clap::{Parser, Subcommand};
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
    let joined = if p.is_relative() { base.join(p) } else { p.to_path_buf() };
    // Canonicalize to resolve `..` components into a clean absolute path
    std::fs::canonicalize(&joined).unwrap_or(joined)
}

#[derive(Parser)]
#[command(name = "e2e")]
struct Cli {
    #[command(subcommand)]
    command: Command,

    #[arg(long, default_value = "circuits/keys")]
    keys_dir: PathBuf,

    #[arg(long, default_value = "target/wasm32v1-none/release")]
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
    /// Benchmark: seed CLOB with limit orders, run market orders, submit matches on-chain
    Benchmark {
        #[arg(long, default_value = "2")]
        mms: usize,
        #[arg(long, default_value = "2")]
        traders: usize,
        #[arg(long, default_value = "1")]
        orders_per_mm: usize,
        #[arg(long, default_value = "127.0.0.1:9720")]
        server_addr: String,
        #[arg(long, default_value = "100000")]
        center_price: u64,
        #[arg(long, default_value = "5000000")]
        order_size: u64,
        /// Randomize order sizes between 0.5x and 1.5x of --order-size
        #[arg(long)]
        randomize_sizes: bool,
        /// Randomize leverage across orders (1/2/5/10/20/50)
        #[arg(long)]
        randomize_leverage: bool,
        /// ms delay between each order placement (for book TUI observation)
        #[arg(long, default_value = "0")]
        book_delay_ms: u64,
    },
    /// Private deposit → shielded withdrawal: proves no on-chain address↔note link
    PrivateDeposit {
        /// Amount to deposit (in token stroops)
        #[arg(long, default_value = "1000000000")]
        amount: u64,
        /// Secret scalar for note commitment (keep this private!)
        #[arg(long, default_value = "314159265")]
        secret: u64,
    },
    /// Private trading cycle: deposit_note → open_position_from_note → cancel_position_to_note → withdraw_note
    PrivateTrading {
        /// Collateral amount in token stroops
        #[arg(long, default_value = "1000000000")]
        amount: u64,
        /// Secret for the deposit note commitment (Poseidon2(amount, note_secret))
        #[arg(long, default_value = "271828182")]
        note_secret: u64,
        /// Secret for the order commitment (authorizes open + cancel)
        #[arg(long, default_value = "314159265")]
        order_secret: u64,
        /// Secret for the settlement note (Poseidon2(0, settle_secret))
        #[arg(long, default_value = "161803398")]
        settle_secret: u64,
    },
    /// Full end-to-end via tee-match server (generate proofs via server)
    Server {
        #[arg(long, default_value = "127.0.0.1:9720")]
        server_addr: String,

        /// Skip deployment and use an existing perp-engine contract ID
        #[arg(long)]
        perp: Option<String>,

        /// Skip deployment and use an existing orderbook contract ID
        #[arg(long)]
        orderbook: Option<String>,

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
    /// Deploy contracts + register 6 markets (GOLD/SPY/TSLA/BTC/ETH/SOL) on testnet
    Deploy,
}

fn decimal_to_hex(s: &str) -> String {
    let n: num_bigint::BigUint = s.parse().expect("Invalid decimal");
    format!("{:0>64x}", n)
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let keys_dir = resolve_path(&cli.keys_dir);
    let wasm_dir = resolve_path(&cli.wasm_dir);
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
                &keys_dir, side_a, price_a, size_a, leverage_a, 0, 0, nonce_a, secret_a, false,
            )?;
            let p_b = proof::gen_commitment(
                &keys_dir, side_b, price_b, size_b, leverage_b, 0, 0, nonce_b, secret_b, false,
            )?;
            let p_match = proof::gen_match(
                &keys_dir,
                side_a, price_a, size_a, leverage_a, 0, 0, nonce_a, secret_a,
                side_b, price_b, size_b, leverage_b, 0, 0, nonce_b, secret_b,
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
        Command::PrivateTrading { amount, note_secret, order_secret, settle_secret } => {
            eprintln!("━━━ PrivateTrading E2E ━━━");
            eprintln!("  Proves: full shielded trading cycle with zero address linkage");
            eprintln!("  deposit_note → open_position_from_note → cancel_position_to_note → withdraw_note");
            stellar::private_trading_e2e(
                &wasm_dir, &keys_dir,
                amount, note_secret, order_secret, settle_secret,
            )?;
            eprintln!("\n━━━ COMPLETE ({:.2}s) ━━━", global_start.elapsed().as_secs_f64());
        }
        Command::PrivateDeposit { amount, secret } => {
            eprintln!("━━━ PrivateDeposit E2E ━━━");
            eprintln!("  Proves: deposit_note breaks address↔note link on-chain");
            eprintln!("  Depositor (alice) → shielded note → Recipient (bob)");
            stellar::private_deposit_e2e(&wasm_dir, &keys_dir, amount, secret)?;
            eprintln!("\n━━━ COMPLETE ({:.2}s) ━━━", global_start.elapsed().as_secs_f64());
        }
        Command::Benchmark {
            mms, traders, orders_per_mm,
            server_addr, center_price, order_size, randomize_sizes, randomize_leverage, book_delay_ms,
        } => {
            eprintln!("━━━ Benchmark ({mms}MM × {traders}T) ━━━");

            let cfg = benchmark::BenchmarkConfig {
                mm_count: mms, trader_count: traders,
                orders_per_mm,
                server_addr, center_price, order_size,
                randomize_sizes, randomize_leverage, book_delay_ms,
            };
            benchmark::run_benchmark(&wasm_dir, &keys_dir, cfg)?;

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
                &keys_dir, side_a, price_a, size_a, leverage_a, 0, 0, nonce_a, secret_a, false,
            )?;
            let p_b = proof::gen_commitment(
                &keys_dir, side_b, price_b, size_b, leverage_b, 0, 0, nonce_b, secret_b, false,
            )?;
            let p_match = proof::gen_match(
                &keys_dir,
                side_a, price_a, size_a, leverage_a, 0, 0, nonce_a, secret_a,
                side_b, price_b, size_b, leverage_b, 0, 0, nonce_b, secret_b,
                match_price, match_size,
            )?;

            let cmt_a_hex = decimal_to_hex(&p_a.public_inputs[0]);
            let cmt_b_hex = decimal_to_hex(&p_b.public_inputs[0]);

            eprintln!("  commitment A: {} ({} hex chars)", &cmt_a_hex[..16], cmt_a_hex.len());
            eprintln!("  commitment B: {} ({} hex chars)", &cmt_b_hex[..16], cmt_b_hex.len());

            stellar::run_e2e(&wasm_dir, &keys_dir, &p_a, &p_b, &p_match, &cmt_a_hex, &cmt_b_hex)?;
        }
        Command::Server {
            server_addr,
            side_a, price_a, size_a, leverage_a, nonce_a, secret_a,
            side_b, price_b, size_b, leverage_b, nonce_b, secret_b,
            match_price: _, match_size: _, perp, orderbook,
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

            // ── Step 3: Setup (deploy or reuse existing contracts) ──
            eprintln!("\n── Step 3/5: Setup ──");
            let ctx = if let (Some(perp_id), Some(ob_id)) = (perp, orderbook) {
                eprintln!("  Using existing contracts (skip deploy)");
                stellar::setup_with_existing(
                    &keys_dir,
                    &perp_id, &ob_id,
                    &proof_a_json, &proof_b_json,
                    &cmt_a_hex, &cmt_b_hex,
                    price_a, price_b,
                    side_a, side_b,
                    size_a, size_b,
                    leverage_a, leverage_b,
                    15, &"0000000000000000000000000000000000000000000000000000000000000000",
                )?
            } else {
                stellar::deploy_and_place(
                    &wasm_dir, &keys_dir,
                    &proof_a_json, &proof_b_json,
                    &cmt_a_hex, &cmt_b_hex,
                    price_a, price_b,
                    side_a, side_b,
                    size_a, size_b,
                    leverage_a, leverage_b,
                    15, &"0000000000000000000000000000000000000000000000000000000000000000",
                )?
            };
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
        Command::Deploy => {
            eprintln!("━━━ Testnet Deployment ━━━");
            let (orderbook_id, perp_id, source_pk, usdc_sac) = stellar::deploy_contracts(&wasm_dir)?;
            eprintln!("  ✓ orderbook: {}", orderbook_id);
            eprintln!("  ✓ perp-engine: {}", perp_id);
            eprintln!("  ✓ source_pk: {}", source_pk);
            eprintln!("  ✓ USDC SAC: {}", usdc_sac);

            stellar::init_perp_engine(&perp_id, stellar::SOURCE, &usdc_sac)?;
            eprintln!("  ✓ perp-engine initialized + default asset registered");

            stellar::multi_market_setup(&perp_id)?;
            eprintln!("  ✓ 6 markets registered (GOLD/SPY/TSLA/BTC/ETH/SOL)");

            eprintln!("\n━━━ DEPLOY COMPLETE ━━━");
            eprintln!("  Orderbook: {}", orderbook_id);
            eprintln!("  PerpEngine: {}", perp_id);
        }
    }

    Ok(())
}
