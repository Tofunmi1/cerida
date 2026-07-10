mod benchmark;
mod client;
mod proof;
mod soroban_rpc;
mod stellar;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::time::Instant;
use rust_circuits;

/// Extract a u64 from a Soroban ScVal debug string like `U64(3900000000)`.
fn parse_u64_from_scval(s: &str) -> u64 {
    s.split(|c: char| !c.is_ascii_digit())
        .filter(|seg| !seg.is_empty())
        .last()
        .and_then(|seg| seg.parse().ok())
        .unwrap_or(0)
}

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
    /// Full shielded pool flow: pool.deposit → pool.withdraw → perp.deposit_note → open_position
    ShieldedPool {
        /// USDC denomination per pool slot (stroops)
        #[arg(long, default_value = "1000000000")]
        denomination: u128,
        /// Secret scalar for the pool note
        #[arg(long, default_value = "271828182")]
        pool_secret: u64,
        /// Nullifier scalar for the pool note
        #[arg(long, default_value = "314159265")]
        pool_nullifier: u64,
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
    /// Full private round-trip: pool.deposit → open_position_from_pool → cancel → withdraw_to_pool → pool.withdraw
    /// No deposit→position address link on-chain.
    FullPrivate {
        /// Pool denomination in token stroops (amount locked per slot)
        #[arg(long, default_value = "1000000000")]
        denomination: u128,
        /// Secret scalar for the deposit pool note (pool_leaf = Poseidon2(secret, nullifier, 30))
        #[arg(long, default_value = "271828182")]
        pool_secret: u64,
        /// Nullifier scalar for the deposit pool note
        #[arg(long, default_value = "314159265")]
        pool_nullifier: u64,
        /// Secret for the position commitment (OrderCommitment circuit)
        #[arg(long, default_value = "161803398")]
        pos_secret: u64,
        /// Secret for the cancel/refund note (NoteSpend circuit)
        #[arg(long, default_value = "141421356")]
        cancel_secret: u64,
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
        /// Defaults to a random value — commitments must be unique on-chain
        #[arg(long)]
        nonce_a: Option<u64>,
        /// Defaults to a random value — commitments must be unique on-chain
        #[arg(long)]
        secret_a: Option<u64>,
        #[arg(long, default_value = "1")]
        side_b: u64,
        #[arg(long, default_value = "99000")]
        price_b: u64,
        #[arg(long, default_value = "1000000000")]
        size_b: u64,
        #[arg(long, default_value = "1")]
        leverage_b: u64,
        /// Defaults to a random value — commitments must be unique on-chain
        #[arg(long)]
        nonce_b: Option<u64>,
        /// Defaults to a random value — commitments must be unique on-chain
        #[arg(long)]
        secret_b: Option<u64>,
        #[arg(long, default_value = "99500")]
        match_price: u64,
        #[arg(long, default_value = "1000000000")]
        match_size: u64,
    },
    /// Deploy contracts + register 6 markets (GOLD/SPY/TSLA/BTC/ETH/SOL) on testnet
    Deploy,

    /// Deploy ShieldedPool + PerpEngine for the privacy flow and configure the TEE account.
    /// Use this instead of `deploy` when running the tee-match server in production.
    DeployPrivate {
        /// Public key (G...) or identity name of the TEE signing account.
        /// The tee-match server's STELLAR_RELAYER_SECRET pubkey goes here.
        #[arg(long)]
        tee_pubkey: String,
        /// Pool denomination in token stroops (default 1 USDC = 10_000_000)
        #[arg(long, default_value = "10000000")]
        denomination: u128,
    },

    /// Deploy a fresh orderbook contract and print its contract ID.
    DeployOrderbook,

    /// Install new perp-engine WASM and upgrade the live contract in-place.
    /// Preserves all storage (positions, commitments). Requires protocol admin key.
    Upgrade {
        /// Contract ID of the live perp-engine to upgrade
        #[arg(long)]
        perp_id: String,
    },

    /// Update the TEE account stored in the perp-engine contract to match STELLAR_SOURCE_SECRET.
    /// Run this on the TEE VM so that settle_position/settle_partial auth passes.
    SetTeeAccount {
        #[arg(long)]
        perp_id: String,
    },

    /// Reset per-asset oracle config + TWAP samples so the deviation guard is bypassed.
    /// Use after fixing inflated base prices so the oracle keeper can push correct prices.
    ResetOracle {
        #[arg(long)]
        perp_id: String,
        /// Comma-separated asset IDs to reset (e.g. "1,2,3,4,5,6")
        #[arg(long, default_value = "1,2,3,4,5,6")]
        asset_ids: String,
    },

    /// Diagnose oracle state for all markets: print stored TWAP and current config.
    DiagnoseOracle {
        #[arg(long)]
        perp_id: String,
    },

    /// Step oracle price down gradually to work around the 50% TWAP deviation guard.
    StepDown {
        #[arg(long)]
        perp_id: String,
        /// Asset ID to step down (numeric)
        #[arg(long)]
        asset_id: u64,
        /// Target price in 7-decimal scale (1e7 = $1)
        #[arg(long)]
        target: u64,
        /// Max steps before giving up
        #[arg(long, default_value = "30")]
        max_steps: u32,
    },
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
        Command::ShieldedPool { denomination, pool_secret, pool_nullifier } => {
            eprintln!("━━━ ShieldedPool E2E ━━━");
            eprintln!("  pool.deposit → pool.withdraw → perp.deposit_note → open_position");
            stellar::shielded_pool_e2e(&wasm_dir, &keys_dir, denomination, pool_secret, pool_nullifier)?;
            eprintln!("\n━━━ COMPLETE ({:.2}s) ━━━", global_start.elapsed().as_secs_f64());
        }
        Command::FullPrivate { denomination, pool_secret, pool_nullifier, pos_secret, cancel_secret } => {
            eprintln!("━━━ FullPrivate E2E ━━━");
            eprintln!("  pool.deposit → open_position_from_pool → cancel → withdraw_to_pool → pool.withdraw");
            eprintln!("  No on-chain link between depositor address and position commitment.");
            stellar::full_private_e2e(
                &wasm_dir, &keys_dir,
                denomination, pool_secret, pool_nullifier, pos_secret, cancel_secret,
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
            // Fresh values per run unless pinned — a reused (nonce, secret) pair
            // recreates a commitment that already exists on-chain and place_order traps.
            let nonce_a = nonce_a.unwrap_or_else(rand::random);
            let secret_a = secret_a.unwrap_or_else(rand::random);
            let nonce_b = nonce_b.unwrap_or_else(rand::random);
            let secret_b = secret_b.unwrap_or_else(rand::random);

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

        Command::DeployPrivate { tee_pubkey, denomination } => {
            use e2e::soroban_rpc::{scval_address, SorobanRpc};
            eprintln!("━━━ Deploy Privacy Contracts ━━━");
            eprintln!("  TEE account: {}", tee_pubkey);
            eprintln!("  Denomination: {} stroops", denomination);

            let usdc_sac = stellar::deploy_usdc_sac()?;
            eprintln!("  ✓ USDC SAC: {}", usdc_sac);

            let pool_wasm = wasm_dir.join("shielded_pool.wasm");
            let pool_salt: [u8; 32] = rand::random();
            let pool_wasm_bytes = std::fs::read(&pool_wasm)?;
            let pool_wasm_hash = e2e::soroban_rpc::install_wasm_via_rpc(&pool_wasm_bytes, stellar::SOURCE)?;
            let pool_id = e2e::soroban_rpc::deploy_contract_via_rpc(&pool_wasm_hash, pool_salt, stellar::SOURCE)?;
            eprintln!("  ✓ shielded-pool: {}", pool_id);

            // Initialize pool with USDC token + denomination + empty merkle root
            let rpc = SorobanRpc::new();
            let empty_root_hex = {
                use rust_circuits::{compute_empty_root, fr_to_biguint};
                format!("{:0>64x}", fr_to_biguint(&compute_empty_root()))
            };
            rpc.invoke_xdr(&pool_id, stellar::SOURCE, "initialize", vec![
                scval_address(&usdc_sac)?,
                stellar_xdr::ScVal::U128(stellar_xdr::UInt128Parts {
                    hi: (denomination >> 64) as u64,
                    lo: denomination as u64,
                }),
                e2e::soroban_rpc::scval_bytes32(&empty_root_hex)?,
            ])?;
            eprintln!("  ✓ shielded-pool initialized (denom={})", denomination);

            let perp_wasm = wasm_dir.join("perp_engine.wasm");
            let perp_salt: [u8; 32] = rand::random();
            let perp_wasm_bytes = std::fs::read(&perp_wasm)?;
            let perp_wasm_hash = e2e::soroban_rpc::install_wasm_via_rpc(&perp_wasm_bytes, stellar::SOURCE)?;
            let perp_id = e2e::soroban_rpc::deploy_contract_via_rpc(&perp_wasm_hash, perp_salt, stellar::SOURCE)?;
            eprintln!("  ✓ perp-engine: {}", perp_id);

            stellar::init_perp_engine(&perp_id, stellar::SOURCE, &usdc_sac)?;
            eprintln!("  ✓ perp-engine initialized + default asset registered");

            stellar::multi_market_setup(&perp_id)?;
            eprintln!("  ✓ 6 markets registered");

            // Resolve TEE pubkey (allow identity name or G... address)
            let tee_pk = if tee_pubkey.starts_with('G') {
                tee_pubkey.clone()
            } else {
                std::process::Command::new("stellar")
                    .args(["keys", "address", &tee_pubkey])
                    .output()
                    .ok()
                    .and_then(|o| String::from_utf8(o.stdout).ok())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| anyhow::anyhow!("cannot resolve identity '{tee_pubkey}'"))?
            };

            let admin_pk = stellar::source_pubkey()?;
            rpc.invoke_xdr(&perp_id, stellar::SOURCE, "set_tee_account", vec![
                scval_address(&admin_pk)?,
                scval_address(&tee_pk)?,
            ])?;
            eprintln!("  ✓ TEE account set to {}", tee_pk);

            eprintln!("\n━━━ DEPLOY COMPLETE ━━━");
            eprintln!("  ShieldedPool: {}", pool_id);
            eprintln!("  PerpEngine:   {}", perp_id);
            eprintln!("  USDC SAC:     {}", usdc_sac);
            eprintln!("  TEE account:  {}", tee_pk);
            eprintln!("\n  keeper flags:");
            eprintln!("    --perp-id {} --tee-addr <tee-match-2-ip>:9720", perp_id);
        }

        Command::DeployOrderbook => {
            eprintln!("━━━ Deploy Orderbook ━━━");
            let ob_wasm = wasm_dir.join("orderbook.wasm");
            eprintln!("  WASM: {:?} ({} bytes)", &ob_wasm,
                std::fs::metadata(&ob_wasm).map(|m| m.len()).unwrap_or(0));
            let wasm_bytes = std::fs::read(&ob_wasm)?;
            let wasm_hash = e2e::soroban_rpc::install_wasm_via_rpc(&wasm_bytes, stellar::SOURCE)?;
            let salt: [u8; 32] = rand::random();
            let contract_id = e2e::soroban_rpc::deploy_contract_via_rpc(&wasm_hash, salt, stellar::SOURCE)?;
            eprintln!("  ✓ Orderbook deployed: {}", contract_id);
            eprintln!("\n  Update .env:");
            eprintln!("    VITE_ORDERBOOK_ID={}", contract_id);
        }

        Command::Upgrade { perp_id } => {
            use e2e::soroban_rpc::{scval_bytes32, SorobanRpc};
            eprintln!("━━━ Upgrade perp-engine ━━━");
            let wasm_path = wasm_dir.join("perp_engine.wasm");
            eprintln!("  installing WASM from {:?}...", wasm_path);
            let wasm_hash = e2e::soroban_rpc::install_wasm_via_rpc(
                &std::fs::read(&wasm_path)?,
                stellar::SOURCE,
            )?;
            eprintln!("  ✓ WASM installed, hash: {}", &wasm_hash);
            let admin_pk = stellar::source_pubkey()?;
            let rpc = SorobanRpc::new();
            rpc.invoke_xdr(&perp_id, stellar::SOURCE, "upgrade", vec![
                e2e::soroban_rpc::scval_address(&admin_pk)?,
                scval_bytes32(&wasm_hash)?,
            ])?;
            eprintln!("  ✓ contract upgraded to new WASM");
        }

        Command::SetTeeAccount { perp_id } => {
            use e2e::soroban_rpc::{pubkey_from_secret, scval_address, SorobanRpc};
            eprintln!("━━━ Set TEE Account ━━━");
            let admin_pk = stellar::source_pubkey()?;
            eprintln!("  admin pubkey: {}", admin_pk);
            // If STELLAR_SOURCE_SECRET is set, derive the TEE signing pubkey from it.
            // This matches what the TEE container uses at runtime (signing_source reads the env var).
            let tee_pk = if let Ok(secret) = std::env::var("STELLAR_SOURCE_SECRET") {
                let pk = pubkey_from_secret(&secret)?;
                eprintln!("  TEE pubkey (from STELLAR_SOURCE_SECRET): {}", pk);
                pk
            } else {
                eprintln!("  TEE pubkey (same as admin — STELLAR_SOURCE_SECRET not set): {}", admin_pk);
                admin_pk.clone()
            };
            let rpc = SorobanRpc::new();
            rpc.invoke_xdr(&perp_id, stellar::SOURCE, "set_tee_account", vec![
                scval_address(&admin_pk)?,
                scval_address(&tee_pk)?,
            ])?;
            eprintln!("  ✓ TEE account updated to {}", tee_pk);
            eprintln!("    settle_position / settle_partial will now pass require_tee_auth");
        }

        Command::ResetOracle { perp_id, asset_ids } => {
            use e2e::soroban_rpc::{scval_address, scval_bytes32, SorobanRpc};
            let admin_pk = stellar::source_pubkey()?;
            let rpc = SorobanRpc::new();
            for id_str in asset_ids.split(',') {
                let id: u64 = id_str.trim().parse()?;
                let asset_hex = format!("{:0>64x}", id);
                eprintln!("  resetting oracle for asset {}...", id);
                rpc.invoke_xdr(&perp_id, stellar::SOURCE, "reset_asset_oracle", vec![
                    scval_address(&admin_pk)?,
                    scval_bytes32(&asset_hex)?,
                ])?;
                eprintln!("  ✓ asset {} oracle reset", id);
            }
        }

        Command::DiagnoseOracle { perp_id } => {
            use e2e::soroban_rpc::SorobanRpc;
            let rpc = SorobanRpc::new();
            let names = ["BTC", "TSLA", "XLM", "XRP", "SPACEX", "OIL", "GOLD"];
            eprintln!("━━━ Oracle diagnosis for {} ━━━", &perp_id[..8]);
            for id in 0u64..7 {
                let asset_hex = format!("{:0>64x}", id);
                match rpc.invoke_view_xdr(&perp_id, stellar::SOURCE, "get_asset_twap", vec![
                    e2e::soroban_rpc::scval_bytes32(&asset_hex)?,
                ]) {
                    Ok(result) => {
                        let twap = parse_u64_from_scval(&result);
                        eprintln!("  asset {} ({}): twap={} (${:.4})", id, names[id as usize], twap, twap as f64 / 1e7);
                    }
                    Err(e) => eprintln!("  asset {}: ERROR = {}", id, e),
                }
            }
        }

        Command::StepDown { perp_id, asset_id, target, max_steps } => {
            use e2e::soroban_rpc::{scval_address, scval_bytes32, scval_u64, SorobanRpc};
            // TWAP_WINDOW from contract — 8 samples in ring buffer
            const RING: u64 = 8;
            let admin_pk = stellar::source_pubkey()?;
            let rpc = SorobanRpc::new();
            let asset_hex = format!("{:0>64x}", asset_id);
            eprintln!("━━━ StepDown asset {} → target={} ━━━", asset_id, target);

            for step in 0..max_steps {
                // Read current TWAP via simulation (no TX)
                let twap_result = rpc.invoke_view_xdr(&perp_id, stellar::SOURCE, "get_asset_twap", vec![
                    scval_bytes32(&asset_hex)?,
                ])?;
                let twap = parse_u64_from_scval(&twap_result);
                eprintln!("  step {}: twap={} (${:.4}), target={} (${:.4})", step, twap, twap as f64/1e7, target, target as f64/1e7);

                if twap == 0 {
                    eprintln!("  twap=0, setting target directly");
                    rpc.invoke_xdr(&perp_id, stellar::SOURCE, "set_asset_price", vec![
                        scval_bytes32(&asset_hex)?,
                        scval_address(&admin_pk)?,
                        scval_u64(target),
                    ])?;
                    eprintln!("  ✓ done");
                    break;
                }

                let dev = target.abs_diff(twap);
                let within_limit = dev == 0 || dev * 10_000 / twap <= 4900;
                if within_limit {
                    eprintln!("  within deviation — setting target directly");
                    rpc.invoke_xdr(&perp_id, stellar::SOURCE, "set_asset_price", vec![
                        scval_bytes32(&asset_hex)?,
                        scval_address(&admin_pk)?,
                        scval_u64(target),
                    ])?;
                    // Fill remaining ring buffer slots with target so TWAP converges fast
                    for fill in 1..RING {
                        eprintln!("  filling ring slot {}/{}", fill + 1, RING);
                        let mut retries = 3u32;
                        loop {
                            match rpc.invoke_xdr(&perp_id, stellar::SOURCE, "set_asset_price", vec![
                                scval_bytes32(&asset_hex)?,
                                scval_address(&admin_pk)?,
                                scval_u64(target),
                            ]) {
                                Ok(_) => break,
                                Err(e) if retries > 0 && {
                                    let s = e.to_string();
                                    s.contains("BAD_SEQ") || s.contains("TRY_AGAIN") || s.contains("sendTransaction: ERROR:")
                                } => {
                                    eprintln!("    transient error ({}), retrying in 5s…", e);
                                    std::thread::sleep(std::time::Duration::from_secs(5));
                                    retries -= 1;
                                }
                                Err(e) => return Err(e),
                            }
                        }
                    }
                    eprintln!("  ✓ done — TWAP ring filled with target");
                    break;
                }

                // Step toward target using max allowed step (50% deviation limit).
                // Then fill the entire ring buffer (RING calls) with this step price so the
                // TWAP equals step_price after this batch, not just after many single steps.
                let step_price = if target < twap {
                    twap / 2  // 50% of twap, exactly at deviation limit
                } else {
                    twap + twap / 2  // 150% of twap
                };
                eprintln!("  stepping to {} (${:.4}), filling {} ring slots", step_price, step_price as f64/1e7, RING);
                for slot in 0..RING {
                    eprintln!("    slot {}/{}", slot + 1, RING);
                    // Retry once on txBAD_SEQ (sequence race on rapid submissions)
                    let mut retries = 3u32;
                    loop {
                        match rpc.invoke_xdr(&perp_id, stellar::SOURCE, "set_asset_price", vec![
                            scval_bytes32(&asset_hex)?,
                            scval_address(&admin_pk)?,
                            scval_u64(step_price),
                        ]) {
                            Ok(_) => break,
                            Err(e) if retries > 0 && {
                                let s = e.to_string();
                                s.contains("BAD_SEQ") || s.contains("TRY_AGAIN") || s.contains("sendTransaction: ERROR:")
                            } => {
                                eprintln!("    transient error ({}), retrying in 5s…", e);
                                std::thread::sleep(std::time::Duration::from_secs(5));
                                retries -= 1;
                            }
                            Err(e) => return Err(e),
                        }
                    }
                }
                eprintln!("  batch done — TWAP should now ≈ {}", step_price);
            }
        }
    }

    Ok(())
}
