// ── 32-Level Market Maker ─────────────────────────────────────────
// Quotes a symmetric grid of 32 bids + 32 asks per market, priced from Pyth.
//
// Design goals:
//   - Boot in seconds, not minutes (batch fast-init to the TEE).
//   - Keep the book full after fills (live-depth reconciliation + fast refill).
//   - Nice-looking ladder: one order per level, smooth geometric sizes,
//     tight symmetric spread around a Pyth/book midpoint.
// ─────────────────────────────────────────────────────────────────

use e2e::client::{BatchItem, ServerClient};
use rand::Rng;
use std::collections::HashMap;
use std::thread;
use std::time::{Duration, Instant};

pub const LEVELS: usize = 32;
const REFRESH_THRESHOLD: f64 = 0.0015; // rebuild grid if mid moves > 0.15%
const QUOTE_TTL_SECS: u64 = 120;       // cancel orphaned quotes after 2 min
const BATCH_SIZE: usize = 128;        // max commitments per batch-fast-init call

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Crypto,
    Rwa,
}

#[derive(Clone)]
pub struct MarketConfig {
    pub symbol: &'static str,
    pub asset_id: u64,
    pub pyth_id: &'static str, // empty = use base_price only
    pub category: Category,
    pub base_price: u64,       // fallback price in 7-decimal scale
    pub base_size: u64,        // smallest order size (level 1)
    pub leverage: u64,
}

#[derive(Clone)]
pub struct MmConfig {
    pub tee_addr: String,
    pub markets: Vec<MarketConfig>,
}

// ── Internal state ─────────────────────────────────────────────────

struct SlotSpec {
    side: u64,    // 0 = bid, 1 = ask
    level: usize, // 1..=LEVELS
    price: u64,
    size: u64,
    nonce: u64,
    secret: u64,
    asset_id: u64,
    leverage: u64,
}

struct Slot {
    cmt: String,
    side: u64,
    level: usize,
    price: u64,
    size: u64,
}

struct ActiveQuote {
    side: u64,
    level: usize,
    price: u64,
    size: u64,
    placed_at: Instant,
}

struct Market {
    cfg: MarketConfig,
    mid_at_gen: u64, // midpoint used for the current grid
    active: HashMap<String, ActiveQuote>,
    next_nonce: u64,
}

// ── Spread / size calculations ─────────────────────────────────────

fn spread_bps(cat: Category, level: usize) -> u64 {
    // Level 1 starts at the tightest spread; each step outward widens.
    let i = level.saturating_sub(1) as u64;
    match cat {
        Category::Crypto => 5 + 3 * i,
        Category::Rwa => 10 + 5 * i,
    }
}

fn level_size(base: u64, level: usize) -> u64 {
    let mut s = base as u128;
    for _ in 0..level.saturating_sub(1) {
        s = s * 108 / 100;
    }
    (s as u64).max(1)
}

fn level_size_jittered<R: Rng>(base: u64, level: usize, rng: &mut R) -> u64 {
    let base_size = level_size(base, level);
    // +/- 10% jitter keeps the ladder organic without looking messy.
    let jitter = 0.9 + rng.gen::<f64>() * 0.2;
    ((base_size as f64) * jitter).max(1.0) as u64
}

fn level_price(mid: u64, cat: Category, side: u64, level: usize) -> u64 {
    let bps = spread_bps(cat, level);
    let delta = mid.saturating_mul(bps) / 10_000;
    if side == 0 {
        mid.saturating_sub(delta)
    } else {
        mid.saturating_add(delta)
    }
}

fn price_change_ratio(a: u64, b: u64) -> f64 {
    if b == 0 {
        return 1.0;
    }
    let diff = if a > b { a - b } else { b - a };
    diff as f64 / b as f64
}

fn parse_best(s: &str) -> Option<u64> {
    s.split('x').next()?.parse().ok()
}

/// Pick a fair midpoint. Use Pyth when it sits inside the live spread;
/// otherwise anchor to the live book mid so we do not quote through walls.
fn fair_mid(pyth_mid: u64, best_bid: Option<u64>, best_ask: Option<u64>) -> u64 {
    match (best_bid, best_ask) {
        (Some(bid), Some(ask)) if bid < ask => {
            if pyth_mid > bid && pyth_mid < ask {
                pyth_mid
            } else {
                let book_mid = (bid + ask) / 2;
                // Pyth has moved >2% away from the book — book is stale, trust Pyth.
                if price_change_ratio(pyth_mid, book_mid) > 0.02 {
                    pyth_mid
                } else {
                    book_mid
                }
            }
        }
        // One-sided wall: stay with Pyth when it is on the sane side,
        // otherwise quote just past the wall to avoid an empty side.
        (Some(bid), None) => if pyth_mid > bid { pyth_mid } else { bid },
        (None, Some(ask)) => if pyth_mid < ask { pyth_mid } else { ask },
        _ => pyth_mid,
    }
}

/// Ensure our grid does not cross existing liquidity.
fn quote_mid(mid: u64, side: u64, best_bid: Option<u64>, best_ask: Option<u64>) -> u64 {
    let tick = (mid / 100_000).max(1);
    match side {
        0 => {
            if let Some(ask) = best_ask {
                mid.min(ask.saturating_sub(tick))
            } else {
                mid
            }
        }
        1 => {
            if let Some(bid) = best_bid {
                mid.max(bid.saturating_add(tick))
            } else {
                mid
            }
        }
        _ => mid,
    }
}

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

// ── Spec / commitment helpers ──────────────────────────────────────

fn spec_for_level(
    cfg: &MarketConfig,
    side: u64,
    level: usize,
    mid: u64,
    nonce: u64,
) -> SlotSpec {
    let mut rng = rand::thread_rng();
    SlotSpec {
        side,
        level,
        price: level_price(mid, cfg.category, side, level),
        size: level_size_jittered(cfg.base_size, level, &mut rng),
        nonce,
        secret: rng.gen(),
        asset_id: cfg.asset_id,
        leverage: cfg.leverage,
    }
}

fn grid_specs(cfg: &MarketConfig, bid_mid: u64, ask_mid: u64, nonce: &mut u64) -> Vec<SlotSpec> {
    let mut specs = Vec::with_capacity(LEVELS * 2);
    for side in [0u64, 1u64] {
        let mid = if side == 0 { bid_mid } else { ask_mid };
        for level in 1..=LEVELS {
            specs.push(spec_for_level(cfg, side, level, mid, *nonce));
            *nonce += 1;
        }
    }
    specs
}

fn commit_specs(client: &ServerClient, specs: &[SlotSpec], pyth_id: &str) -> Vec<Slot> {
    let mut slots = Vec::with_capacity(specs.len());
    for chunk in specs.chunks(BATCH_SIZE) {
        let items: Vec<BatchItem> = chunk
            .iter()
            .map(|s| BatchItem {
                side: Some(s.side),
                price: Some(s.price),
                size: Some(s.size),
                leverage: Some(s.leverage),
                asset: Some(s.asset_id),
                nonce: Some(s.nonce),
                secret: Some(s.secret),
                protocol: Some(true),
                asset_id_hex: Some(pyth_id.to_string()),
                collateral_amount: Some(0),
            })
            .collect();
        match client.batch_fast_init(&items) {
            Ok(cmts) => {
                if cmts.len() != chunk.len() {
                    eprintln!(
                        "  [mm] batch-fast-init returned {} commitments for {} specs",
                        cmts.len(),
                        chunk.len()
                    );
                }
                for (spec, cmt) in chunk.iter().zip(cmts.into_iter()) {
                    slots.push(Slot {
                        cmt,
                        side: spec.side,
                        level: spec.level,
                        price: spec.price,
                        size: spec.size,
                    });
                }
            }
            Err(e) => {
                eprintln!("  [mm] batch-fast-init failed for {} specs: {e}", chunk.len());
            }
        }
    }
    slots
}

fn place_slots(mkt: &mut Market, client: &ServerClient, slots: Vec<Slot>) {
    // Sort by level, alternating side so the book always has both bid and ask levels.
    let mut slots_by_level: Vec<Vec<&Slot>> = (0..=LEVELS).map(|_| Vec::new()).collect();
    for slot in &slots {
        if slot.level <= LEVELS {
            slots_by_level[slot.level as usize].push(slot);
        }
    }
    for level in 1..=LEVELS {
        for slot in &slots_by_level[level] {
            match client.place_order(&slot.cmt, "limit", slot.price, slot.size) {
                Ok(_) => {
                    mkt.active.insert(
                        slot.cmt.clone(),
                        ActiveQuote {
                            side: slot.side,
                            level: slot.level,
                            price: slot.price,
                            size: slot.size,
                            placed_at: Instant::now(),
                        },
                    );
                }
                Err(e) => {
                    eprintln!(
                        "  [mm] {} place side={} lvl={}: {e}",
                        mkt.cfg.symbol, slot.side, slot.level
                    );
                }
            }
        }
    }
}

fn cancel_all(mkt: &mut Market, client: &ServerClient) {
    for cmt in mkt.active.keys() {
        let _ = client.cancel_order(cmt);
    }
    mkt.active.clear();
}

// ── Reconciliation ─────────────────────────────────────────────────

fn reconcile_and_cancel(
    mkt: &mut Market,
    client: &ServerClient,
    bid_mid: u64,
    ask_mid: u64,
    best_bid: Option<u64>,
    best_ask: Option<u64>,
    ttl: Duration,
) {
    let resp = match client.get_market_asset(Some(mkt.cfg.asset_id)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("  [mm] {} reconcile get_market failed: {e}", mkt.cfg.symbol);
            return;
        }
    };

    let mut bid_depth: HashMap<u64, (usize, u64)> = HashMap::new();
    let mut ask_depth: HashMap<u64, (usize, u64)> = HashMap::new();
    if let Some(bids) = resp.bids {
        for l in bids {
            bid_depth.insert(l.price, (l.orders, l.size));
        }
    }
    if let Some(asks) = resp.asks {
        for l in asks {
            ask_depth.insert(l.price, (l.orders, l.size));
        }
    }

    let mut to_cancel: Vec<String> = Vec::new();
    for (cmt, q) in &mkt.active {
        // TTL expiry
        if q.placed_at.elapsed() > ttl {
            to_cancel.push(cmt.clone());
            continue;
        }

        // Price drift: our grid point moved away from target
        let target_mid = if q.side == 0 { bid_mid } else { ask_mid };
        let target = level_price(target_mid, mkt.cfg.category, q.side, q.level);
        if price_change_ratio(q.price, target) > REFRESH_THRESHOLD {
            to_cancel.push(cmt.clone());
            continue;
        }

        // Cross check
        if q.side == 0 && best_ask.map(|a| q.price >= a).unwrap_or(false) {
            to_cancel.push(cmt.clone());
            continue;
        }
        if q.side == 1 && best_bid.map(|b| q.price <= b).unwrap_or(false) {
            to_cancel.push(cmt.clone());
            continue;
        }

        // Fill detection: the live level is gone or significantly smaller.
        let actual = if q.side == 0 {
            bid_depth.get(&q.price)
        } else {
            ask_depth.get(&q.price)
        };
        let filled = match actual {
            None => true,
            Some(&(orders, size)) => {
                orders < 1 || size < ((q.size as f64) * 0.70) as u64
            }
        };
        if filled {
            to_cancel.push(cmt.clone());
        }
    }

    if !to_cancel.is_empty() {
        eprintln!(
            "  [mm] {} cancelling {} stale/filled quotes",
            mkt.cfg.symbol,
            to_cancel.len()
        );
        for cmt in &to_cancel {
            let _ = client.cancel_order(cmt);
            mkt.active.remove(cmt);
        }
    }
}

// ── Main loop ──────────────────────────────────────────────────────

pub fn run(config: MmConfig, interval_secs: u64) {
    eprintln!(
        "  [mm] initializing {} markets at {} levels/side",
        config.markets.len(),
        LEVELS
    );

    // Drop stale quotes left by a previous keeper run so the book starts clean.
    let clear_client = ServerClient::new(&config.tee_addr);
    for cfg in &config.markets {
        if let Err(e) = clear_client.clear_book(cfg.asset_id) {
            eprintln!("  [mm] clear-book {} failed: {e}", cfg.symbol);
        }
    }

    let init_ids: Vec<&str> = config
        .markets
        .iter()
        .map(|m| m.pyth_id)
        .filter(|id| !id.is_empty())
        .collect();
    let init_prices = crate::oracle::fetch(&init_ids).unwrap_or_default();

    // Bootstrap each market in parallel so we are not serialising 7 grids.
    let tee_addr = config.tee_addr.clone();
    let mut handles = Vec::with_capacity(config.markets.len());
    for cfg in &config.markets {
        let addr = tee_addr.clone();
        let cfg = cfg.clone();
        let prices = init_prices.clone();
        handles.push(thread::spawn(move || bootstrap_market(cfg, addr, prices)));
    }
    let mut markets: Vec<Market> = handles
        .into_iter()
        .map(|h| h.join().expect("market bootstrap thread panicked"))
        .collect();

    let mut tick = 0u64;
    loop {
        tick += 1;
        let t = Instant::now();
        let client = ServerClient::new(&config.tee_addr);

        let pyth_ids: Vec<&str> = markets
            .iter()
            .map(|m| m.cfg.pyth_id)
            .filter(|id| !id.is_empty())
            .collect();
        let prices = crate::oracle::fetch(&pyth_ids).unwrap_or_default();

        for mkt in &mut markets {
            let mid = if !mkt.cfg.pyth_id.is_empty() {
                prices
                    .get(mkt.cfg.pyth_id)
                    .map(|p| p.scaled)
                    .unwrap_or(mkt.cfg.base_price)
            } else {
                mkt.cfg.base_price
            };
            refresh_market(mkt, &client, mid, interval_secs);
        }

        let total_active: usize = markets.iter().map(|m| m.active.len()).sum();
        let max_missing: usize = markets
            .iter()
            .map(|m| missing_level_count(&m.active))
            .max()
            .unwrap_or(0);
        let sleep_secs = if max_missing >= LEVELS {
            (interval_secs / 6).max(5)
        } else {
            interval_secs
        };
        eprintln!(
            "  [mm] tick #{tick}: active={total_active} missing={max_missing} sleep={sleep_secs}s ({:.1}s)",
            t.elapsed().as_secs_f64()
        );

        thread::sleep(Duration::from_secs(sleep_secs));
    }
}

fn bootstrap_market(
    cfg: MarketConfig,
    tee_addr: String,
    prices: HashMap<String, crate::oracle::PricePoint>,
) -> Market {
    let client = ServerClient::new(&tee_addr);
    let mid = if !cfg.pyth_id.is_empty() {
        prices
            .get(cfg.pyth_id)
            .map(|p| p.scaled)
            .unwrap_or(cfg.base_price)
    } else {
        cfg.base_price
    };
    let (best_bid, best_ask) = fetch_book_bounds(&client, cfg.asset_id);
    let fair = fair_mid(mid, best_bid, best_ask);
    let bid_mid = quote_mid(fair, 0, best_bid, best_ask);
    let ask_mid = quote_mid(fair, 1, best_bid, best_ask);

    eprintln!(
        "  [mm] {} bootstrapping grid at bid_mid=${:.2} ask_mid=${:.2} (fair ${:.2})",
        cfg.symbol,
        bid_mid as f64 / 1e7,
        ask_mid as f64 / 1e7,
        fair as f64 / 1e7
    );

    let mut next_nonce = 0u64;
    let specs = grid_specs(&cfg, bid_mid, ask_mid, &mut next_nonce);
    let slots = commit_specs(&client, &specs, cfg.pyth_id);

    let mut mkt = Market {
        cfg,
        mid_at_gen: mid,
        active: HashMap::new(),
        next_nonce,
    };
    place_slots(&mut mkt, &client, slots);

    eprintln!(
        "  [mm] {} placed {} active quotes",
        mkt.cfg.symbol,
        mkt.active.len()
    );
    mkt
}

fn missing_level_count(active: &HashMap<String, ActiveQuote>) -> usize {
    let mut bid = vec![false; LEVELS + 1];
    let mut ask = vec![false; LEVELS + 1];
    for q in active.values() {
        if q.level >= 1 && q.level <= LEVELS {
            if q.side == 0 {
                bid[q.level] = true;
            } else {
                ask[q.level] = true;
            }
        }
    }
    let mut missing = 0usize;
    for level in 1..=LEVELS {
        if !bid[level] {
            missing += 1;
        }
        if !ask[level] {
            missing += 1;
        }
    }
    missing
}

fn refresh_market(mkt: &mut Market, client: &ServerClient, mid: u64, interval_secs: u64) {
    let ttl = Duration::from_secs(QUOTE_TTL_SECS.max(interval_secs));
    let (best_bid, best_ask) = fetch_book_bounds(client, mkt.cfg.asset_id);
    let fair = fair_mid(mid, best_bid, best_ask);
    let bid_mid = quote_mid(fair, 0, best_bid, best_ask);
    let ask_mid = quote_mid(fair, 1, best_bid, best_ask);

    // Full grid rebuild when the midpoint moved enough.
    if price_change_ratio(fair, mkt.mid_at_gen) > REFRESH_THRESHOLD {
        eprintln!(
            "  [mm] {} price moved {:.2}%, rebuilding grid (fair ${:.2})",
            mkt.cfg.symbol,
            price_change_ratio(fair, mkt.mid_at_gen) * 100.0,
            fair as f64 / 1e7
        );
        cancel_all(mkt, client);
        let specs = grid_specs(&mkt.cfg, bid_mid, ask_mid, &mut mkt.next_nonce);
        let slots = commit_specs(client, &specs, mkt.cfg.pyth_id);
        place_slots(mkt, client, slots);
        mkt.mid_at_gen = mid;
        return;
    }

    // Partial refresh: remove stale/filled quotes and refill missing levels.
    reconcile_and_cancel(mkt, client, bid_mid, ask_mid, best_bid, best_ask, ttl);

    let mut bid_covered = vec![false; LEVELS + 1];
    let mut ask_covered = vec![false; LEVELS + 1];
    for q in mkt.active.values() {
        if q.level >= 1 && q.level <= LEVELS {
            if q.side == 0 {
                bid_covered[q.level] = true;
            } else {
                ask_covered[q.level] = true;
            }
        }
    }

    let mut specs = Vec::new();
    for side in [0u64, 1u64] {
        let covered = if side == 0 { &bid_covered } else { &ask_covered };
        let target_mid = if side == 0 { bid_mid } else { ask_mid };
        for level in 1..=LEVELS {
            if !covered[level] {
                specs.push(spec_for_level(
                    &mkt.cfg,
                    side,
                    level,
                    target_mid,
                    mkt.next_nonce,
                ));
                mkt.next_nonce += 1;
            }
        }
    }

    if !specs.is_empty() {
        let slots = commit_specs(client, &specs, mkt.cfg.pyth_id);
        place_slots(mkt, client, slots);
    }
}
