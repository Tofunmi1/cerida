use crate::db::SecretStore;
use crate::engine::OrderBook;
use crate::log;
use crate::position;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Default interval between funding index updates (1 hour).
pub const DEFAULT_INTERVAL_SECS: u64 = 3600;

/// Maximum annualised premium rate as a fraction of FUNDING_SCALE (10%).
const MAX_ANNUAL_RATE: i128 = position::FUNDING_SCALE / 10;

/// EMA smoothing factor per hourly tick (α=0.25 → ~4h half-life).
const EMA_ALPHA: f64 = 0.25;

pub fn spawn(
    store: Arc<SecretStore>,
    books: Arc<RwLock<HashMap<u64, OrderBook>>>,
    interval_secs: u64,
) -> std::thread::JoinHandle<()> {
    let interval = Duration::from_secs(interval_secs);
    std::thread::spawn(move || {
        let mut tick = 0u64;
        // Small delay on first tick so the book has time to be seeded by the MM.
        std::thread::sleep(Duration::from_secs(60));
        loop {
            tick += 1;
            let t = Instant::now();

            // Collect unique (asset_id_hex, asset_num) pairs from open positions.
            let mut assets: Vec<(String, u64)> = Vec::new();
            if let Ok(commitments) = store.list() {
                for cmt in commitments {
                    if !cmt.starts_with("pos_") {
                        continue;
                    }
                    let actual_cmt = &cmt[4..];
                    if let Ok(Some(state)) = store.get_position_state(actual_cmt) {
                        if state.remaining_size > 0 {
                            let key = (state.asset_id.clone(), state.asset_num);
                            if !assets.contains(&key) {
                                assets.push(key);
                            }
                        }
                    }
                }
            }

            let mut updated = 0u64;
            let mut errors = 0u64;
            for (asset_id_hex, asset_num) in &assets {
                match update_funding_index(&store, &books, asset_id_hex, *asset_num) {
                    Ok(()) => updated += 1,
                    Err(e) => {
                        log::error!(
                            "funding cron: index update failed",
                            "asset", asset_id_hex,
                            "err", e.to_string()
                        );
                        errors += 1;
                    }
                }
            }

            log::info!(
                "funding cron tick",
                "tick", tick,
                "assets", assets.len(),
                "updated", updated,
                "errors", errors,
                "elapsed_ms", t.elapsed().as_millis()
            );

            std::thread::sleep(interval);
        }
    })
}

pub fn update_funding_index(
    store: &SecretStore,
    books: &RwLock<HashMap<u64, OrderBook>>,
    asset_id_hex: &str,
    asset_num: u64,
) -> Result<()> {
    let oracle_1e8 = crate::liquidator::fetch_oracle_price(asset_id_hex)?;
    let oracle_1e7 = oracle_1e8 / 10;
    if oracle_1e7 == 0 {
        return Ok(());
    }

    // Use CLOB mid as mark; fall back to oracle (zero premium) if book is empty.
    let mark_1e7: u64 = {
        let books_r = books.read().unwrap();
        if let Some(book) = books_r.get(&asset_num) {
            match (book.best_bid().map(|(p, _)| p), book.best_ask().map(|(p, _)| p)) {
                (Some(b), Some(a)) => (b + a) / 2,
                _ => oracle_1e7,
            }
        } else {
            oracle_1e7
        }
    };

    let interval_hours = DEFAULT_INTERVAL_SECS / 3600;
    let rate = position::funding_rate(mark_1e7, oracle_1e7, interval_hours.max(1), MAX_ANNUAL_RATE);

    // Accumulate into the per-asset funding index (applied to position P&L).
    let current_index = store.get_funding_index(asset_id_hex)?;
    store.set_funding_index(asset_id_hex, current_index.saturating_add(rate))?;

    // Update EMA of the 8h-equivalent premium for the display funding rate.
    let premium = (mark_1e7 as f64 - oracle_1e7 as f64) / oracle_1e7 as f64;
    let prev_ema = store.get_funding_premium_ema(asset_id_hex)?;
    let new_ema = if prev_ema == 0.0 {
        premium
    } else {
        prev_ema * (1.0 - EMA_ALPHA) + premium * EMA_ALPHA
    };
    store.set_funding_premium_ema(asset_id_hex, new_ema.clamp(-0.001, 0.001))?;

    log::info!(
        "funding index updated",
        "asset", asset_id_hex,
        "rate", rate,
        "mark_1e7", mark_1e7,
        "oracle_1e7", oracle_1e7,
        "premium_pct", format!("{:.4}", premium * 100.0),
        "ema_pct", format!("{:.4}", new_ema * 100.0),
        "new_index", current_index.saturating_add(rate)
    );
    Ok(())
}
