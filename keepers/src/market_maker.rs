// ── Market Maker ─────────────────────────────────────────────────
// Quote grid market maker using fast-init commitments.
// Connects to TEE server for order management.
// Pre-generates commitment pools per market for instant quoting.
// ─────────────────────────────────────────────────────────────────

use anyhow::Result;
use e2e::client::ServerClient;
use rand::Rng;
use std::collections::{HashMap, VecDeque};
use std::thread;
use std::time::{Duration, Instant};

pub struct MmConfig {
    pub tee_addr: String,
    pub markets: Vec<MarketConfig>,
}

pub struct MarketConfig {
    pub asset_id: u64,
    pub base_price: u64,
    pub spread_bps: u64,
    pub depth_levels: u64,
    pub size_per_level: u64,
    pub leverage: u64,
    pub pool_size: u64,
}

struct MarketState {
    asset: u64,
    base_price: u64,
    spread_bps: u64,
    depth_levels: u64,
    size: u64,
    leverage: u64,
    pool: VecDeque<Commitment>,
    active_quotes: HashMap<String, QuoteInfo>,
    next_nonce: u64,
}

struct Commitment {
    cmt: String,
    side: u64,
    price: u64,
    size: u64,
}

struct QuoteInfo {
    side: u64,
    price: u64,
    placed_at: Instant,
}

pub fn run(config: MmConfig, interval_secs: u64) {
    let interval = Duration::from_secs(interval_secs);
    let mut rng = rand::thread_rng();

    // ── Initialize per-market state ──
    let mut states: HashMap<u64, MarketState> = HashMap::new();
    for mc in &config.markets {
        let mut pool = VecDeque::new();
        let mut nonce = 0u64;

        // Pre-generate commitment pool via fast-init
        let start = Instant::now();
        for _ in 0..mc.pool_size {
            // Bid side
            for side in [0u64, 1u64] {
                let price = if side == 0 {
                    mc.base_price.saturating_sub(mc.spread_bps * mc.base_price / 10_000)
                } else {
                    mc.base_price.saturating_add(mc.spread_bps * mc.base_price / 10_000)
                };
                let secret: u64 = rng.gen();

                match ServerClient::new(&config.tee_addr).fast_init(
                    side, price, mc.size_per_level, mc.leverage,
                    mc.asset_id, nonce, secret,
                ) {
                    Ok(cmt) => {
                        pool.push_back(Commitment { cmt: cmt.clone(), side, price, size: mc.size_per_level });
                        eprintln!("  [mm] asset={} side={} price={} cmt={}...", mc.asset_id, side, price, &cmt[..12]);
                    }
                    Err(e) => eprintln!("  [mm] fast-init failed: {e}"),
                }
                nonce += 1;
            }
        }
        eprintln!("  [mm] asset={} pool={} generated in {:.1}s",
            mc.asset_id, pool.len(), start.elapsed().as_secs_f64());

        states.insert(mc.asset_id, MarketState {
            asset: mc.asset_id,
            base_price: mc.base_price,
            spread_bps: mc.spread_bps,
            depth_levels: mc.depth_levels,
            size: mc.size_per_level,
            leverage: mc.leverage,
            pool,
            active_quotes: HashMap::new(),
            next_nonce: nonce,
        });
    }

    // ── Main quoting loop ──
    let tee = config.tee_addr.clone();
    let mut tick = 0u64;
    loop {
        tick += 1;
        let t = Instant::now();
        let client = ServerClient::new(&tee);

        for mc in &config.markets {
            let state = states.get_mut(&mc.asset_id).unwrap();

            if let Ok(market) = client.get_market() {
                let mid = match (market.best_bid, market.best_ask) {
                    (Some(bid_str), Some(ask_str)) => {
                        let bid: u64 = bid_str.split('x').next().and_then(|s| s.parse().ok()).unwrap_or(mc.base_price);
                        let ask: u64 = ask_str.split('x').next().and_then(|s| s.parse().ok()).unwrap_or(mc.base_price);
                        (bid + ask) / 2
                    }
                    _ => mc.base_price,
                };

                let spread_amount = mid * mc.spread_bps / 10_000;
                let desired_bid = mid.saturating_sub(spread_amount);
                let desired_ask = mid.saturating_add(spread_amount);

                // Check which quotes need refreshing
                let mut place_bid = true;
                let mut place_ask = true;

                for (cmt, qi) in &state.active_quotes {
                    let too_old = qi.placed_at.elapsed() > Duration::from_secs(interval_secs * 3);
                    let too_far = if qi.side == 0 {
                        qi.price < desired_bid || qi.price > desired_bid
                    } else {
                        qi.price > desired_ask || qi.price < desired_ask
                    };
                    if too_old || too_far {
                        let _ = client.cancel_order(cmt);
                    } else if qi.side == 0 {
                        place_bid = false;
                    } else {
                        place_ask = false;
                    }
                }

                // Place new quotes from pool
                if place_bid || state.active_quotes.is_empty() {
                    if let Some(cmt_info) = state.pool.pop_front() {
                        let cmt = cmt_info.cmt.clone();
                        let side = cmt_info.side;
                        match client.place_order(&cmt, "limit", cmt_info.price, cmt_info.size) {
                            Ok(_) => {
                                state.active_quotes.insert(cmt, QuoteInfo { side, price: cmt_info.price, placed_at: Instant::now() });
                            }
                            Err(e) => eprintln!("  [mm] place bid failed: {e}"),
                        }
                    }
                }

                if place_ask || state.active_quotes.len() < 2 {
                    if let Some(cmt_info) = state.pool.pop_front() {
                        let cmt = cmt_info.cmt.clone();
                        let side = cmt_info.side;
                        match client.place_order(&cmt, "limit", cmt_info.price, cmt_info.size) {
                            Ok(_) => {
                                state.active_quotes.insert(cmt, QuoteInfo { side, price: cmt_info.price, placed_at: Instant::now() });
                            }
                            Err(e) => eprintln!("  [mm] place ask failed: {e}"),
                        }
                    }
                }

                // Replenish pool if low
                if state.pool.len() < mc.pool_size as usize / 2 {
                    let mut rng = rand::thread_rng();
                    let to_generate = mc.pool_size as usize - state.pool.len();
                    for _ in 0..to_generate {
                        let side: u64 = rng.gen_range(0..2);
                        let price = if side == 0 { desired_bid } else { desired_ask };
                        let secret: u64 = rng.gen();
                        if let Ok(cmt) = client.fast_init(
                            side, price, mc.size_per_level, mc.leverage,
                            mc.asset_id, state.next_nonce, secret,
                        ) {
                            state.pool.push_back(Commitment { cmt, side, price, size: mc.size_per_level });
                            state.next_nonce += 1;
                        }
                    }
                }
            }
        }

        eprintln!("  [mm] tick #{tick}: pool={} active={} ({:.1}s)",
            states.values().map(|s| s.pool.len()).sum::<usize>(),
            states.values().map(|s| s.active_quotes.len()).sum::<usize>(),
            t.elapsed().as_secs_f64()
        );
        thread::sleep(interval);
    }
}
