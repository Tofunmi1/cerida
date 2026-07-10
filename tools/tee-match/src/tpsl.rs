use crate::db::SecretStore;
use crate::log;
use crate::position;
use anyhow::Result;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Interval between TP/SL trigger checks (seconds).
const CHECK_INTERVAL_SECS: u64 = 5;

/// Spawn the TP/SL monitor background thread.
/// Every `CHECK_INTERVAL_SECS` ticks, iterates all open positions and
/// checks whether the current oracle price has crossed any TP/SL threshold.
/// When triggered, executes a market close via `position::close_position`.
pub fn spawn(store: Arc<SecretStore>, perp_id: String, keys_dir: Arc<std::path::PathBuf>) -> std::thread::JoinHandle<()> {
    let interval = Duration::from_secs(CHECK_INTERVAL_SECS);
    std::thread::spawn(move || {
        let mut tick = 0u64;
        loop {
            tick += 1;
            std::thread::sleep(interval);

            let t = Instant::now();
            let mut triggered = 0u64;
            let mut errors = 0u64;

            if let Ok(commitments) = store.list_positions() {
                for cmt in commitments {
                    let state = match store.get_position_state(&cmt) {
                        Ok(Some(s)) => s,
                        _ => continue,
                    };

                    if state.remaining_size == 0 || state.protocol {
                        continue;
                    }

                    let tp = state.tp_price;
                    let sl = state.sl_price;
                    if tp == 0 && sl == 0 {
                        continue;
                    }

                    // fetch_oracle_price returns 1e8 scale; prices stored in TEE use 1e7.
                    let oracle_price = match crate::liquidator::fetch_oracle_price(&state.asset_id) {
                        Ok(p) if p > 0 => p / 10,
                        _ => continue,
                    };

                    let should_close = if state.side == 0 {
                        // Long
                        (tp > 0 && oracle_price >= tp) || (sl > 0 && oracle_price <= sl)
                    } else {
                        // Short
                        (tp > 0 && oracle_price <= tp) || (sl > 0 && oracle_price >= sl)
                    };

                    if !should_close {
                        continue;
                    }

                    log::info!(
                        "TP/SL triggered",
                        "cmt",
                        &cmt[..16],
                        "side",
                        state.side,
                        "oracle",
                        oracle_price,
                        "tp",
                        tp,
                        "sl",
                        sl
                    );

                    let keys = keys_dir.clone();
                    match position::close_position(&store, &perp_id, &cmt, oracle_price, state.remaining_size, &keys) {
                        Ok(_tx_hash) => {
                            // Clear TP/SL from position state so it won't trigger again.
                            let mut updated = state;
                            updated.tp_price = 0;
                            updated.sl_price = 0;
                            let _ = store.insert_position_state(&cmt, &updated);
                            triggered += 1;
                        }
                        Err(e) => {
                            log::error!("TP/SL close failed", "cmt", &cmt[..16], "err", e.to_string());
                            errors += 1;
                        }
                    }
                }
            }

            if triggered > 0 || errors > 0 {
                log::info!(
                    "TP/SL monitor tick",
                    "tick",
                    tick,
                    "triggered",
                    triggered,
                    "errors",
                    errors,
                    "elapsed_ms",
                    t.elapsed().as_millis()
                );
            }
        }
    })
}
