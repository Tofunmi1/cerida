// ── CER-PERP Keepers ─────────────────────────────────────────────
// Oracle price keeper + market maker + liquidator.
// Prices fetched live from Pyth Network's Hermes REST API.
// ─────────────────────────────────────────────────────────────────

mod market_maker;
pub mod oracle;

use anyhow::Result;
use clap::Parser;
use std::thread;
use std::time::Duration;

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
    #[arg(long, default_value = "15")]
    mm_interval_secs: u64,
    #[arg(long)]
    no_market_maker: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    eprintln!("═══ CER-PERP Keepers ═══");
    eprintln!("  perp_engine:  {}", cli.perp_id);
    eprintln!("  tee_server:   {}", cli.tee_addr);
    eprintln!("  market_maker: {} (interval={}s, levels={})", if cli.no_market_maker { "OFF" } else { "ON" }, cli.mm_interval_secs, market_maker::LEVELS);

    if !cli.no_market_maker {
        let tee_mm   = cli.tee_addr.clone();
        let interval = cli.mm_interval_secs;
        let mm_markets: Vec<market_maker::MarketConfig> = MARKETS.iter().map(|m| market_maker::MarketConfig {
            symbol:     m.symbol,
            asset_id:   m.asset_id,
            pyth_id:    m.pyth_id,
            category:   m.category,
            base_price: m.base_price,
            base_size:  m.base_size,
            leverage:   m.leverage,
        }).collect();
        thread::spawn(move || {
            loop {
                let cfg = market_maker::MmConfig {
                    tee_addr: tee_mm.clone(),
                    markets:  mm_markets.clone(),
                };
                let result = std::panic::catch_unwind(|| market_maker::run(cfg, interval));
                if let Err(e) = result {
                    let msg = e.downcast_ref::<&str>().copied()
                        .or_else(|| e.downcast_ref::<String>().map(|s| s.as_str()))
                        .unwrap_or("unknown panic");
                    eprintln!("  [mm] thread panicked: {msg} — restarting in 15s");
                }
                thread::sleep(Duration::from_secs(15));
            }
        });
    }

    loop {
        thread::sleep(Duration::from_secs(10));
    }
}

