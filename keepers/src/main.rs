// ── CER-PERP Keepers ─────────────────────────────────────────────
// Oracle price keeper + market maker + liquidator.
// Prices fetched live from Pyth Network's Hermes REST API.
// ─────────────────────────────────────────────────────────────────

mod market_maker;
mod oracle;

use anyhow::Result;
use clap::Parser;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};

// ── Market catalog ─────────────────────────────────────────────────
// asset_id: numeric ID used in the TEE CLOB (0 = DEFAULT_ASSET = BTC)
// pyth_id:  Pyth price feed hex (without 0x)
// base_price: fallback if Pyth unavailable (7-decimal scale, 1e7 = $1)

struct Market {
    symbol: &'static str,
    asset_id: u64,
    pyth_id: &'static str,
    base_price: u64,
    category: market_maker::Category,
    base_size: u64,
    leverage: u64,
}

static MARKETS: &[Market] = &[
    Market {
        symbol: "BTC-PERP",
        asset_id: 0,
        pyth_id: "e62df6c8b4a85fe1a67db44dc12de5db330f7ac66b72dc658afedf0f4a415b43",
        base_price: 610_000_000_000,    // $61,000
        category: market_maker::Category::Crypto,
        base_size: 100_000,
        leverage: 50,
    },
    Market {
        symbol: "XRP-PERP",
        asset_id: 1,
        pyth_id: "ec5d399846a9209f3fe5881d70aae9268c94339ff9817e8d18ff19fa05eea1c8",
        base_price: 11_200_000,         // $1.12
        category: market_maker::Category::Crypto,
        base_size: 50_000_000,
        leverage: 20,
    },
    Market {
        symbol: "XLM-PERP",
        asset_id: 2,
        pyth_id: "b7a8eba68a997cd0210c2e1e4ee811ad2d174b3611c22d9ebf16f4cb7e9ba850",
        base_price: 1_100_000,          // $0.11
        category: market_maker::Category::Crypto,
        base_size: 100_000_000,
        leverage: 10,
    },
    Market {
        symbol: "SPACEX-PERP",
        asset_id: 3,
        pyth_id: "",   // no public Pyth feed — uses base_price
        base_price: 3_500_000_000,      // $350
        category: market_maker::Category::Rwa,
        base_size: 1_000_000,
        leverage: 10,
    },
    Market {
        symbol: "TSLA-PERP",
        asset_id: 4,
        pyth_id: "16dad506d7db8da01c87581c87ca897a012a153557d4d578c3b9c9e1bc0632f1",
        base_price: 3_900_000_000,      // $390
        category: market_maker::Category::Rwa,
        base_size: 1_000_000,
        leverage: 10,
    },
    Market {
        symbol: "OIL-PERP",
        asset_id: 5,
        pyth_id: "925ca92ff005ae943c158e3563f59698ce7e75c5a8c8dd43303a0a154887b3e6",
        base_price: 700_000_000,        // $70
        category: market_maker::Category::Rwa,
        base_size: 5_000_000,
        leverage: 10,
    },
    Market {
        symbol: "GOLD-PERP",
        asset_id: 6,
        pyth_id: "765d2ba906dbc32ca17cc11f5310a89e9ee1f6420508c63861f2f8ba4ee34bb2",
        base_price: 41_790_000_000,     // $4,179
        category: market_maker::Category::Rwa,
        base_size: 100_000,
        leverage: 20,
    },
];

#[derive(Parser)]
struct Cli {
    #[arg(long)]
    perp_id: String,
    #[arg(long, default_value = "127.0.0.1:9720")]
    tee_addr: String,
    #[arg(long, default_value = "30")]
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
    eprintln!("  markets:      {} ({})", MARKETS.len(),
        MARKETS.iter().map(|m| m.symbol).collect::<Vec<_>>().join(", "));
    eprintln!("  oracle:       {} (interval={}s)", if cli.no_oracle { "OFF" } else { "ON (Pyth)" }, cli.oracle_interval_secs);
    eprintln!("  market_maker: {} (interval={}s, levels={})", if cli.no_market_maker { "OFF" } else { "ON" }, cli.mm_interval_secs, market_maker::LEVELS);
    eprintln!("  liquidator:   {} (interval={}s)", if cli.no_liquidator { "OFF" } else { "ON" }, cli.liq_interval_secs);

    // Shared price store: symbol → scaled price (7 decimals)
    let prices: Arc<RwLock<HashMap<String, u64>>> = Arc::new(RwLock::new(
        MARKETS.iter().map(|m| (m.symbol.to_string(), m.base_price)).collect()
    ));

    let perp = cli.perp_id.clone();
    let tee  = cli.tee_addr.clone();

    if !cli.no_oracle {
        let prices_w = prices.clone();
        let perp_w   = perp.clone();
        let interval = Duration::from_secs(cli.oracle_interval_secs);
        thread::spawn(move || oracle_loop(&perp_w, interval, prices_w));
    }

    if !cli.no_market_maker {
        let prices_r = prices.clone();
        let tee_mm   = tee.clone();
        let interval = cli.mm_interval_secs;
        let mm_markets = MARKETS.iter().map(|m| market_maker::MarketConfig {
            symbol:     m.symbol,
            asset_id:   m.asset_id,
            category:   m.category,
            base_price: m.base_price,
            base_size:  m.base_size,
            leverage:   m.leverage,
        }).collect();
        let mm_cfg = market_maker::MmConfig {
            tee_addr: tee_mm,
            markets: mm_markets,
            prices: prices_r,
        };
        thread::spawn(move || market_maker::run(mm_cfg, interval));
    }

    if !cli.no_liquidator {
        let perp_liq = perp.clone();
        let interval = Duration::from_secs(cli.liq_interval_secs);
        thread::spawn(move || liquidator_loop(&perp_liq, interval));
    }

    loop {
        thread::sleep(Duration::from_secs(10));
    }
}

// ── Oracle loop ────────────────────────────────────────────────────

fn oracle_loop(perp_id: &str, interval: Duration, prices: Arc<RwLock<HashMap<String, u64>>>) {
    let source = e2e::stellar::SOURCE;
    let pyth_ids: Vec<&str> = MARKETS.iter().map(|m| m.pyth_id).collect();

    loop {
        let t = Instant::now();

        // 1. Fetch from Pyth
        match oracle::fetch(&pyth_ids) {
            Ok(pyth_map) => {
                let mut ok = 0;
                let mut err = 0;

                // Update shared price cache and submit on-chain mark prices
                let mut price_updates: Vec<(String, u64)> = Vec::new();

                for market in MARKETS {
                    let scaled = if market.pyth_id.is_empty() {
                        // No Pyth feed — keep base price
                        market.base_price
                    } else {
                        pyth_map.get(market.pyth_id)
                            .map(|p| p.scaled)
                            .unwrap_or(market.base_price)
                    };

                    let usd = scaled as f64 / 1e7;
                    eprintln!("  [oracle] {}: ${:.4}", market.symbol, usd);
                    price_updates.push((market.symbol.to_string(), scaled));

                    // Submit on-chain via stellar CLI
                    let asset_id = format!("{:0>64x}", market.asset_id);
                    match set_oracle_price(perp_id, &asset_id, scaled, source) {
                        Ok(()) => ok += 1,
                        Err(e) => {
                            err += 1;
                            eprintln!("  [oracle] {} set_price err: {e}", market.symbol);
                        }
                    }

                    thread::sleep(Duration::from_secs(2));
                }

                // Write all updates to shared map
                if let Ok(mut map) = prices.write() {
                    for (sym, price) in price_updates {
                        map.insert(sym, price);
                    }
                }

                eprintln!("  [oracle] tick: {ok} ok, {err} err ({:.1}s)", t.elapsed().as_secs_f64());
            }
            Err(e) => {
                eprintln!("  [oracle] pyth fetch failed: {e} (using cached prices)");
            }
        }

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

// ── Liquidator ─────────────────────────────────────────────────────

fn liquidator_loop(perp_id: &str, interval: Duration) {
    let rpc = e2e::soroban_rpc::SorobanRpc::new();
    let source = e2e::stellar::SOURCE;
    let mut scanned = 0u64;

    loop {
        let t = Instant::now();
        let mut liq_count = 0;
        let mut checked = 0;

        if let Some(watchlist) = load_watchlist() {
            for cmt in &watchlist {
                checked += 1;
                match try_liquidate(&rpc, perp_id, source, cmt) {
                    Ok(true) => {
                        liq_count += 1;
                        eprintln!("  [liquidator] liquidated {}", &cmt[..16]);
                    }
                    Ok(false) => {}
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
    let data = std::fs::read_to_string("keepers/watchlist.json").ok()?;
    let cmts: Vec<String> = serde_json::from_str(&data).ok()?;
    if cmts.is_empty() { None } else { Some(cmts) }
}

fn try_liquidate(
    rpc: &e2e::soroban_rpc::SorobanRpc,
    perp_id: &str,
    source: &str,
    cmt: &str,
) -> Result<bool> {
    use e2e::soroban_rpc::scval_bytes32;
    match rpc.invoke_xdr(perp_id, source, "liquidate", vec![scval_bytes32(cmt)?]) {
        Ok(_) => Ok(true),
        Err(e) => {
            let msg = e.to_string().to_lowercase();
            if msg.contains("not under-collateralized")
                || msg.contains("can only liquidate a matched")
                || msg.contains("position not found")
                || msg.contains("solvent")
            {
                Ok(false)
            } else {
                Err(e)
            }
        }
    }
}
