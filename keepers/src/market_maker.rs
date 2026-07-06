// ── 32-Level Market Maker ─────────────────────────────────────────
// Quotes a grid of 32 bids + 32 asks per market, priced from Pyth.
//
// Spread profile (percentage-based, not fixed ticks):
//   Crypto: level i → (5 + 3*i) bps from mid
//   RWA:    level i → (10 + 5*i) bps from mid
//
// Size profile (geometric growth outward):
//   level i → base_size * 1.08^(i-1)
//   → inner quotes are small (fill fast), outer quotes are large
//
// Pool management:
//   Pre-generates 2× buffer (128 commitments) per market.
//   On price movement > REFRESH_THRESHOLD, cancels stale quotes
//   and replenishes from the pool at updated price levels.
// ─────────────────────────────────────────────────────────────────

use e2e::client::ServerClient;
use rand::Rng;
use std::collections::HashMap;
use std::thread;
use std::time::{Duration, Instant};

pub const LEVELS: usize = 32;
const REFRESH_THRESHOLD: f64 = 0.005; // re-quote if mid moves > 0.5%
const QUOTE_TTL_SECS: u64 = 300;      // cancel quotes older than 5 minutes
const POOL_BUFFER: usize = LEVELS * 2; // how many extras to pre-generate per side

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Crypto,
    Rwa,
}

pub struct MarketConfig {
    pub symbol: &'static str,
    pub asset_id: u64,
    pub pyth_id: &'static str,  // Pyth feed ID; empty = use base_price only
    pub category: Category,
    pub base_price: u64,   // fallback price in 7-decimal scale
    pub base_size: u64,    // smallest order size (level 1 bid/ask)
    pub leverage: u64,
}

pub struct MmConfig {
    pub tee_addr: String,
    pub markets: Vec<MarketConfig>,
}

// ── Internal state ─────────────────────────────────────────────────

struct Slot {
    cmt: String,
    side: u64,   // 0 = bid, 1 = ask
    level: usize,
    price: u64,
    size: u64,
}

struct ActiveQuote {
    side: u64,
    level: usize,
    price: u64,
    placed_at: Instant,
}

struct Market {
    cfg: MarketConfig,
    mid_at_gen: u64,     // price when the pool was last generated
    pool: Vec<Slot>,     // pre-generated but unplaced commitments
    active: HashMap<String, ActiveQuote>,
    next_nonce: u64,
}

// ── Spread / size calculations ─────────────────────────────────────

fn spread_bps(cat: Category, level: usize) -> u64 {
    let i = level as u64;
    match cat {
        Category::Crypto => 5 + 3 * i,
        Category::Rwa    => 10 + 5 * i,
    }
}

fn level_size(base: u64, level: usize) -> u64 {
    // Geometric growth: base * 1.08^(level-1) approximated with integer ops
    // Use 1000 as the multiplier base and divide at the end
    let mut s = base as u128;
    for _ in 0..level.saturating_sub(1) {
        s = s * 108 / 100;
    }
    (s as u64).max(1)
}

fn level_price(mid: u64, cat: Category, side: u64, level: usize) -> u64 {
    let bps = spread_bps(cat, level);
    let delta = mid.saturating_mul(bps) / 10_000;
    if side == 0 { mid.saturating_sub(delta) } else { mid.saturating_add(delta) }
}

/// Parse a "price x size" string returned by the TEE market snapshot.
fn parse_best(s: &str) -> Option<u64> {
    s.split('x').next()?.parse().ok()
}

/// Choose a quoting midpoint that does not cross existing book liquidity.
/// - Asks must be priced strictly above the current best bid.
/// - Bids must be priced strictly below the current best ask.
/// This prevents the MM from walking into a stale wall and leaving the book one-sided.
fn quote_mid(mid: u64, side: u64, best_bid: Option<u64>, best_ask: Option<u64>) -> u64 {
    let tick = (mid / 100_000).max(1);
    match side {
        0 => {
            // Bids: stay below the best ask.
            if let Some(ask) = best_ask { mid.min(ask.saturating_sub(tick)) } else { mid }
        }
        1 => {
            // Asks: stay above the best bid.
            if let Some(bid) = best_bid { mid.max(bid.saturating_add(tick)) } else { mid }
        }
        _ => mid,
    }
}

/// Fetch current best bid/ask from the TEE for a single asset.
fn fetch_book_bounds(client: &ServerClient, asset_id: u64) -> (Option<u64>, Option<u64>) {
    match client.get_market_asset(Some(asset_id)) {
        Ok(resp) => {
            let bid = resp.best_bid.as_deref().and_then(parse_best);
            let ask = resp.best_ask.as_deref().and_then(parse_best);
            (bid, ask)
        }
        Err(e) => {
            eprintln!("  [mm] get-market failed: {e}");
            (None, None)
        }
    }
}

// ── Pool generation ────────────────────────────────────────────────

fn gen_pool(
    client: &ServerClient,
    cfg: &MarketConfig,
    bid_mid: u64,
    ask_mid: u64,
    nonce: &mut u64,
) -> Vec<Slot> {
    let mut rng = rand::thread_rng();
    let mut pool = Vec::with_capacity((LEVELS + POOL_BUFFER) * 2);

    for side in [0u64, 1u64] {
        let mid = if side == 0 { bid_mid } else { ask_mid };
        // Generate LEVELS active slots + POOL_BUFFER extras
        for level in 1..=(LEVELS + POOL_BUFFER) {
            let price = level_price(mid, cfg.category, side, level.min(LEVELS));
            let size = level_size(cfg.base_size, level.min(LEVELS));
            let secret: u64 = rng.gen();

            match client.fast_init(side, price, size, cfg.leverage, cfg.asset_id, *nonce, secret) {
                Ok(cmt) => pool.push(Slot { cmt, side, level, price, size }),
                Err(e) => eprintln!("  [mm] {} fast-init side={side} lvl={level}: {e}", cfg.symbol),
            }
            *nonce += 1;
        }
    }

    pool
}

// ── Main loop ──────────────────────────────────────────────────────

pub fn run(config: MmConfig, interval_secs: u64) {
    let interval = Duration::from_secs(interval_secs);

    eprintln!("  [mm] initializing {} markets at {} levels/side", config.markets.len(), LEVELS);

    let client = ServerClient::new(&config.tee_addr);

    // Fetch live prices upfront for initial pool generation
    let init_ids: Vec<&str> = config.markets.iter().map(|m| m.pyth_id).filter(|id| !id.is_empty()).collect();
    let init_prices = crate::oracle::fetch(&init_ids).unwrap_or_default();

    let mut markets: Vec<Market> = config.markets.iter().map(|cfg| {
        let mid = if !cfg.pyth_id.is_empty() {
            init_prices.get(cfg.pyth_id).map(|p| p.scaled).unwrap_or(cfg.base_price)
        } else {
            cfg.base_price
        };
        let (best_bid, best_ask) = fetch_book_bounds(&client, cfg.asset_id);
        let bid_mid = quote_mid(mid, 0, best_bid, best_ask);
        let ask_mid = quote_mid(mid, 1, best_bid, best_ask);
        let mut nonce = 0u64;
        eprintln!("  [mm] {} generating pool at bid_mid=${:.2} ask_mid=${:.2} (pyth ${:.2})",
            cfg.symbol, bid_mid as f64 / 1e7, ask_mid as f64 / 1e7, mid as f64 / 1e7);
        let t = Instant::now();
        let pool = gen_pool(&client, cfg, bid_mid, ask_mid, &mut nonce);
        eprintln!("  [mm] {} pool={} slots in {:.1}s", cfg.symbol, pool.len(), t.elapsed().as_secs_f64());

        Market {
            cfg: MarketConfig {
                symbol: cfg.symbol,
                asset_id: cfg.asset_id,
                pyth_id: cfg.pyth_id,
                category: cfg.category,
                base_price: cfg.base_price,
                base_size: cfg.base_size,
                leverage: cfg.leverage,
            },
            mid_at_gen: mid,
            pool,
            active: HashMap::new(),
            next_nonce: nonce,
        }
    }).collect();

    // ── Initial placement ──
    for mkt in &mut markets {
        let client = ServerClient::new(&config.tee_addr);
        initial_place(mkt, &client);
    }

    let mut tick = 0u64;
    loop {
        tick += 1;
        let t = Instant::now();
        let client = ServerClient::new(&config.tee_addr);

        // Fetch live prices for all markets that have a Pyth feed
        let pyth_ids: Vec<&str> = markets.iter()
            .map(|m| m.cfg.pyth_id)
            .filter(|id| !id.is_empty())
            .collect();
        let prices = crate::oracle::fetch(&pyth_ids).unwrap_or_default();

        for mkt in &mut markets {
            let mid = if !mkt.cfg.pyth_id.is_empty() {
                prices.get(mkt.cfg.pyth_id)
                    .map(|p| p.scaled)
                    .unwrap_or(mkt.cfg.base_price)
            } else {
                mkt.cfg.base_price
            };
            refresh_market(mkt, &client, mid, interval_secs);
        }

        let total_active: usize = markets.iter().map(|m| m.active.len()).sum();
        let total_pool: usize = markets.iter().map(|m| m.pool.len()).sum();
        eprintln!("  [mm] tick #{tick}: active={total_active} pool={total_pool} ({:.1}s)",
            t.elapsed().as_secs_f64());

        thread::sleep(interval);
    }
}

fn initial_place(mkt: &mut Market, client: &ServerClient) {
    // Place one bid + one ask per level (levels 1..=LEVELS)
    let mut bid_counts = vec![0usize; LEVELS + 1];
    let mut ask_counts = vec![0usize; LEVELS + 1];
    let mut to_place: Vec<Slot> = Vec::new();

    // Drain desired levels from pool
    let pool = std::mem::take(&mut mkt.pool);
    let mut remaining = Vec::new();

    for slot in pool {
        if slot.level > LEVELS {
            remaining.push(slot);
            continue;
        }
        let counts = if slot.side == 0 { &mut bid_counts } else { &mut ask_counts };
        if counts[slot.level] == 0 {
            counts[slot.level] += 1;
            to_place.push(slot);
        } else {
            remaining.push(slot);
        }
    }
    mkt.pool = remaining;

    for slot in to_place {
        match client.place_order(&slot.cmt, "limit", slot.price, slot.size) {
            Ok(_) => {
                mkt.active.insert(slot.cmt.clone(), ActiveQuote {
                    side: slot.side,
                    level: slot.level,
                    price: slot.price,
                    placed_at: Instant::now(),
                });
            }
            Err(e) => {
                eprintln!("  [mm] {} place side={} lvl={}: {e}", mkt.cfg.symbol, slot.side, slot.level);
                // Return to pool for retry on next tick
                mkt.pool.push(slot);
            }
        }
    }

    eprintln!("  [mm] {} placed {} active quotes", mkt.cfg.symbol, mkt.active.len());
}

fn refresh_market(mkt: &mut Market, client: &ServerClient, mid: u64, interval_secs: u64) {
    let ttl = Duration::from_secs(QUOTE_TTL_SECS.max(interval_secs * 3));
    let price_drift = price_change_ratio(mid, mkt.mid_at_gen);
    let (best_bid, best_ask) = fetch_book_bounds(client, mkt.cfg.asset_id);
    let bid_mid = quote_mid(mid, 0, best_bid, best_ask);
    let ask_mid = quote_mid(mid, 1, best_bid, best_ask);

    let needs_full_refresh = price_drift > REFRESH_THRESHOLD;

    if needs_full_refresh {
        eprintln!("  [mm] {} price moved {:.2}%, refreshing all quotes (pyth ${:.2}, bid_mid ${:.2}, ask_mid ${:.2})",
            mkt.cfg.symbol,
            price_drift * 100.0,
            mid as f64 / 1e7,
            bid_mid as f64 / 1e7,
            ask_mid as f64 / 1e7,
        );
        // Cancel all active quotes
        let cmts: Vec<String> = mkt.active.keys().cloned().collect();
        for cmt in cmts {
            let _ = client.cancel_order(&cmt);
        }
        mkt.active.clear();

        // Regenerate pool at new mid
        let pool = gen_pool(client, &mkt.cfg, bid_mid, ask_mid, &mut mkt.next_nonce);
        mkt.pool = pool;
        mkt.mid_at_gen = mid;

        initial_place(mkt, client);
        return;
    }

    // Partial refresh: cancel stale or mispriced quotes, replenish missing levels
    let mut to_cancel: Vec<String> = Vec::new();
    for (cmt, q) in &mkt.active {
        if q.placed_at.elapsed() > ttl {
            to_cancel.push(cmt.clone());
            continue;
        }
        let target_mid = if q.side == 0 { bid_mid } else { ask_mid };
        let target = level_price(target_mid, mkt.cfg.category, q.side, q.level);
        // Cancel if the quote drifted away from the target grid (>0.5%) or crosses the book.
        if price_change_ratio(q.price, target) > REFRESH_THRESHOLD {
            to_cancel.push(cmt.clone());
            continue;
        }
        if q.side == 0 && best_ask.map(|a| q.price >= a).unwrap_or(false) {
            to_cancel.push(cmt.clone());
        }
        if q.side == 1 && best_bid.map(|b| q.price <= b).unwrap_or(false) {
            to_cancel.push(cmt.clone());
        }
    }
    for cmt in &to_cancel {
        let _ = client.cancel_order(cmt);
        mkt.active.remove(cmt);
    }

    // Track which levels are covered
    let mut bid_covered = vec![false; LEVELS + 1];
    let mut ask_covered = vec![false; LEVELS + 1];
    for q in mkt.active.values() {
        if q.level <= LEVELS {
            if q.side == 0 { bid_covered[q.level] = true; }
            else { ask_covered[q.level] = true; }
        }
    }

    // Place missing levels from pool.  Pool slots were generated at startup; if their
    // price no longer matches the current grid they will simply fail to be selected
    // (each level keeps only one active quote) and get regenerated below.
    let pool = std::mem::take(&mut mkt.pool);
    let mut remaining = Vec::new();
    for slot in pool {
        if slot.level > LEVELS {
            remaining.push(slot);
            continue;
        }
        let covered = if slot.side == 0 { &mut bid_covered } else { &mut ask_covered };
        if !covered[slot.level] {
            match client.place_order(&slot.cmt, "limit", slot.price, slot.size) {
                Ok(_) => {
                    covered[slot.level] = true;
                    mkt.active.insert(slot.cmt.clone(), ActiveQuote {
                        side: slot.side,
                        level: slot.level,
                        price: slot.price,
                        placed_at: Instant::now(),
                    });
                }
                Err(e) => {
                    eprintln!("  [mm] {} place side={} lvl={}: {e}", mkt.cfg.symbol, slot.side, slot.level);
                    remaining.push(slot);
                }
            }
        } else {
            remaining.push(slot);
        }
    }
    mkt.pool = remaining;

    // Generate fresh commitments for levels still uncovered (pool had no slots for them)
    let mut rng = rand::thread_rng();
    for side in [0u64, 1u64] {
        let covered = if side == 0 { &bid_covered } else { &ask_covered };
        let target_mid = if side == 0 { bid_mid } else { ask_mid };
        for level in 1..=LEVELS {
            if covered[level] { continue; }
            let price = level_price(target_mid, mkt.cfg.category, side, level);
            let size = level_size(mkt.cfg.base_size, level);
            let secret: u64 = rng.gen();
            match client.fast_init(side, price, size, mkt.cfg.leverage, mkt.cfg.asset_id, mkt.next_nonce, secret) {
                Ok(cmt) => {
                    mkt.next_nonce += 1;
                    match client.place_order(&cmt, "limit", price, size) {
                        Ok(_) => {
                            mkt.active.insert(cmt, ActiveQuote { side, level, price, placed_at: Instant::now() });
                        }
                        Err(e) => eprintln!("  [mm] {} fresh place side={side} lvl={level}: {e}", mkt.cfg.symbol),
                    }
                }
                Err(e) => eprintln!("  [mm] {} fast-init side={side} lvl={level}: {e}", mkt.cfg.symbol),
            }
        }
    }

    // Replenish pool if low, using the side-specific mids so buffer slots stay useful.
    let want_pool = POOL_BUFFER * 2;
    if mkt.pool.len() < want_pool {
        let to_gen = want_pool - mkt.pool.len();
        let mut rng = rand::thread_rng();
        for _ in 0..to_gen {
            let side: u64 = rng.gen_range(0..2);
            let level: usize = rng.gen_range(1..=LEVELS);
            let target_mid = if side == 0 { bid_mid } else { ask_mid };
            let price = level_price(target_mid, mkt.cfg.category, side, level);
            let size = level_size(mkt.cfg.base_size, level);
            let secret: u64 = rng.gen();
            if let Ok(cmt) = client.fast_init(
                side, price, size, mkt.cfg.leverage,
                mkt.cfg.asset_id, mkt.next_nonce, secret,
            ) {
                mkt.pool.push(Slot { cmt, side, level, price, size });
                mkt.next_nonce += 1;
            }
        }
    }
}

fn price_change_ratio(a: u64, b: u64) -> f64 {
    if b == 0 { return 1.0; }
    let diff = if a > b { a - b } else { b - a };
    diff as f64 / b as f64
}
