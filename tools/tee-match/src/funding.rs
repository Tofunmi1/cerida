use crate::db::SecretStore;
use crate::log;
use crate::position;
use anyhow::Result;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Default interval between funding index updates (seconds = 1 hour).
pub const DEFAULT_INTERVAL_SECS: u64 = 3600;

/// Maximum annualised premium rate as a fraction of FUNDING_SCALE (default 10%).
const MAX_ANNUAL_RATE: i128 = position::FUNDING_SCALE / 10; // 10%

/// Spawn the funding-rate accrual thread.
/// Every `interval_secs` ticks, for every distinct asset-id found in open
/// positions, fetch the oracle price and accumulate the premium into the
/// global funding index.
pub fn spawn(store: Arc<SecretStore>, interval_secs: u64) -> std::thread::JoinHandle<()> {
    let interval = Duration::from_secs(interval_secs);
    std::thread::spawn(move || {
        let mut tick = 0u64;
        loop {
            tick += 1;
            let t = Instant::now();
            std::thread::sleep(interval);

            let mut updated = 0u64;
            let mut errors = 0u64;

            // Collect unique asset IDs from all open positions.
            let mut assets: Vec<String> = Vec::new();
            if let Ok(commitments) = store.list() {
                for cmt in commitments {
                    if !cmt.starts_with("pos_") {
                        continue;
                    }
                    let actual_cmt = &cmt[4..];
                    if let Ok(Some(state)) = store.get_position_state(actual_cmt) {
                        if state.remaining_size > 0 && !assets.contains(&state.asset_id) {
                            assets.push(state.asset_id.clone());
                        }
                    }
                }
            }

            for asset in &assets {
                match update_funding_index(&store, asset) {
                    Ok(()) => updated += 1,
                    Err(e) => {
                        log::error!("funding cron: index update failed", "asset", asset, "err", e.to_string());
                        errors += 1;
                    }
                }
            }

            log::info!(
                "funding cron tick",
                "tick",
                tick,
                "assets",
                assets.len(),
                "updated",
                updated,
                "errors",
                errors,
                "elapsed_ms",
                t.elapsed().as_millis()
            );
        }
    })
}

/// Fetch the mark price (Pyth) and index price (Pyth) for an asset,
/// compute the premium, and accumulate it into the global funding index.
pub fn update_funding_index(store: &SecretStore, asset_id_hex: &str) -> Result<()> {
    // For now we use the oracle price as both mark and index, meaning
    // zero premium and zero funding accrual. Replace the index price
    // with a separate feed when available.
    let mark = crate::liquidator::fetch_oracle_price(asset_id_hex)?;
    let index = mark; // TODO: separate index feed

    let interval_hours = DEFAULT_INTERVAL_SECS / 3600;
    let rate = position::funding_rate(mark, index, interval_hours.max(1), MAX_ANNUAL_RATE);

    let current = store.get_funding_index(asset_id_hex)?;
    let next = current.saturating_add(rate);
    store.set_funding_index(asset_id_hex, next)?;

    log::info!(
        "funding index updated",
        "asset",
        asset_id_hex,
        "rate",
        rate,
        "old_index",
        current,
        "new_index",
        next
    );
    Ok(())
}
