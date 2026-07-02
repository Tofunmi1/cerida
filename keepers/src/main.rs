// ── CER-PERP Keepers ─────────────────────────────────────────────
// Oracle price keeper + market maker for 6 markets.
// Deployed separately from the TEE server.
// Connects to Stellar testnet RPC + TEE match server.
// ─────────────────────────────────────────────────────────────────

use anyhow::Result;
use clap::Parser;
use std::thread;
use std::time::{Duration, Instant};

const MARKETS: &[(&str, u64, u64)] = &[
    ("GOLD", 24000000000,  50),
    ("SPY",  54000000000,  10),
    ("TSLA", 24000000000,  10),
    ("BTC",  6000000000000, 50),
    ("ETH",  300000000000,  50),
    ("SOL",  14000000000,  50),
];

#[derive(Parser)]
struct Cli {
    #[arg(long)]
    perp_id: String,
    #[arg(long, default_value = "127.0.0.1:9720")]
    tee_addr: String,
    #[arg(long, default_value = "300")]
    oracle_interval_secs: u64,
    #[arg(long, default_value = "60")]
    mm_interval_secs: u64,
    #[arg(long)]
    no_oracle: bool,
    #[arg(long)]
    no_market_maker: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    eprintln!("═══ CER-PERP Keepers ═══");
    eprintln!("  perp_engine: {}", cli.perp_id);
    eprintln!("  tee_server:  {}", cli.tee_addr);
    eprintln!("  markets:     {}", MARKETS.len());
    eprintln!("  oracle:      {} (interval={}s)", if cli.no_oracle { "OFF" } else { "ON" }, cli.oracle_interval_secs);
    eprintln!("  market_maker: {} (interval={}s)", if cli.no_market_maker { "OFF" } else { "ON" }, cli.mm_interval_secs);

    let perp = cli.perp_id.clone();
    let tee = cli.tee_addr.clone();

    if !cli.no_oracle {
        let perp = perp.clone();
        let interval = Duration::from_secs(cli.oracle_interval_secs);
        thread::spawn(move || oracle_loop(&perp, interval));
    }

    if !cli.no_market_maker {
        thread::spawn(move || mm_loop(&tee, Duration::from_secs(cli.mm_interval_secs)));
    }

    // Keep main thread alive
    loop {
        thread::sleep(Duration::from_secs(10));
    }
}

fn oracle_loop(perp_id: &str, interval: Duration) {
    let source = e2e::stellar::SOURCE;
    loop {
        let t = Instant::now();
        let mut ok = 0;
        let mut err = 0;
        for (i, (name, price, _)) in MARKETS.iter().enumerate() {
            let asset_id = format!("{:0>64x}", i + 1);
            match set_oracle_price(perp_id, &asset_id, *price, source) {
                Ok(()) => ok += 1,
                Err(e) => {
                    err += 1;
                    eprintln!("  [oracle] {name} set_price: {e}");
                }
            }
            thread::sleep(Duration::from_secs(3));
        }
        eprintln!("  [oracle] tick: {ok} updated, {err} errors ({:.1}s)", t.elapsed().as_secs_f64());
        thread::sleep(interval);
    }
}

fn set_oracle_price(perp_id: &str, asset_id: &str, price: u64, source: &str) -> Result<()> {
    use e2e::soroban_rpc::{scval_address, scval_bytes32, scval_u64, SorobanRpc};
    let rpc = SorobanRpc::new();
    let admin = e2e::stellar::source_pubkey()?;
    rpc.invoke_xdr(perp_id, source, "set_asset_price", vec![
        scval_bytes32(asset_id)?,
        scval_address(&admin)?,
        scval_u64(price),
    ])?;
    Ok(())
}

fn mm_loop(_tee_addr: &str, interval: Duration) {
    loop {
        // TODO: market making — place bid/ask orders via TEE server
        thread::sleep(interval);
    }
}
