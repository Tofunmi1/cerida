// ── CER-PERP Keepers ─────────────────────────────────────────────
// Oracle price keeper + market maker + liquidator for 6 markets.
// Deployed separately from the TEE server.
// ─────────────────────────────────────────────────────────────────

mod market_maker;

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
    #[arg(long, default_value = "30")]
    liq_interval_secs: u64,
    #[arg(long)]
    no_oracle: bool,
    #[arg(long)]
    no_market_maker: bool,
    #[arg(long)]
    no_liquidator: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    eprintln!("═══ CER-PERP Keepers ═══");
    eprintln!("  perp_engine:  {}", cli.perp_id);
    eprintln!("  tee_server:   {}", cli.tee_addr);
    eprintln!("  markets:      {}", MARKETS.len());
    eprintln!("  oracle:       {} (interval={}s)", if cli.no_oracle { "OFF" } else { "ON" }, cli.oracle_interval_secs);
    eprintln!("  market_maker: {} (interval={}s)", if cli.no_market_maker { "OFF" } else { "ON" }, cli.mm_interval_secs);
    eprintln!("  liquidator:   {} (interval={}s)", if cli.no_liquidator { "OFF" } else { "ON" }, cli.liq_interval_secs);

    let perp = cli.perp_id.clone();
    let tee = cli.tee_addr.clone();

    if !cli.no_oracle {
        let perp = perp.clone();
        let interval = Duration::from_secs(cli.oracle_interval_secs);
        thread::spawn(move || oracle_loop(&perp, interval));
    }

    if !cli.no_market_maker {
        let tee_addr = tee.clone();
        let interval = cli.mm_interval_secs;
        let markets: Vec<market_maker::MarketConfig> = MARKETS.iter().enumerate().map(|(i, (_, price, lev))| {
            market_maker::MarketConfig {
                asset_id: (i + 1) as u64,
                base_price: *price,
                spread_bps: 100,        // 1% spread
                depth_levels: 2,
                size_per_level: 1_000_000,
                leverage: *lev,
                pool_size: 20,          // 40 commitments (20 bids + 20 asks)
            }
        }).collect();
        let config = market_maker::MmConfig { tee_addr: tee_addr.clone(), markets };
        thread::spawn(move || market_maker::run(config, interval));
    }

    if !cli.no_liquidator {
        let perp = perp.clone();
        let interval = Duration::from_secs(cli.liq_interval_secs);
        thread::spawn(move || liquidator_loop(&perp, interval));
    }

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
        thread::sleep(interval);
    }
}

// ── Liquidator ────────────────────────────────────────────────

fn liquidator_loop(perp_id: &str, interval: Duration) {
    let rpc = e2e::soroban_rpc::SorobanRpc::new();
    let source = e2e::stellar::SOURCE;
    let mut scanned = 0u64;

    loop {
        let t = Instant::now();
        let mut liq_count = 0;
        let mut checked = 0;

        // Walk all known positions by querying the asset list, then the
        // Position storage keys we've tracked locally (benchmark cache).
        // For now, scan a reverse mapping: check stored positions from the
        // benchmark cache file, or scan a pre-configured watchlist.

        if let Some(watchlist) = load_watchlist() {
            for cmt in &watchlist {
                checked += 1;
                match try_liquidate(&rpc, perp_id, source, cmt) {
                    Ok(true) => {
                        liq_count += 1;
                        eprintln!("  [liquidator] 💧 liquidated {}", &cmt[..16]);
                    }
                    Ok(false) => {} // healthy
                    Err(e) => eprintln!("  [liquidator] err {}: {}", &cmt[..8], e),
                }
                thread::sleep(Duration::from_secs(2));
            }
        }

        scanned += 1;
        eprintln!("  [liquidator] scan #{scanned}: {checked} checked, {liq_count} liquidated ({:.1}s)",
            t.elapsed().as_secs_f64());
        thread::sleep(interval);
    }
}

fn load_watchlist() -> Option<Vec<String>> {
    let path = "keepers/watchlist.json";
    let data = std::fs::read_to_string(path).ok()?;
    let cmts: Vec<String> = serde_json::from_str(&data).ok()?;
    if cmts.is_empty() { None } else { Some(cmts) }
}

/// Returns Ok(true) if liquidated, Ok(false) if healthy, Err on failure.
fn try_liquidate(
    rpc: &e2e::soroban_rpc::SorobanRpc,
    perp_id: &str,
    source: &str,
    cmt: &str,
) -> Result<bool> {
    use e2e::soroban_rpc::scval_bytes32;
    match rpc.invoke_xdr(perp_id, source, "liquidate", vec![
        scval_bytes32(cmt)?,
    ]) {
        Ok(_) => Ok(true),
        Err(e) => {
            let msg = e.to_string().to_lowercase();
            if msg.contains("not under-collateralized")
                || msg.contains("can only liquidate a matched")
                || msg.contains("position not found")
                || msg.contains("solvent")
            {
                Ok(false) // healthy or not ready, not an error
            } else {
                Err(e)
            }
        }
    }
}
