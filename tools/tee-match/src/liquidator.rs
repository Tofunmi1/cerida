use crate::db::SecretStore;
use crate::log;
use crate::stellar;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub fn spawn(
    store: Arc<SecretStore>,
    perp_id: String,
    interval_secs: u64,
) -> std::thread::JoinHandle<()> {
    let interval = Duration::from_secs(interval_secs);
    std::thread::spawn(move || {
        let mut scan_count = 0u64;
        loop {
            scan_count += 1;
            let t = Instant::now();
            let mut liq = 0u64;
            let mut checked = 0u64;
            let mut errors = 0u64;

            if let Ok(commitments) = store.list() {
                for cmt in commitments {
                    checked += 1;
                    match stellar::submit_liquidate(&perp_id, &cmt) {
                        Ok(()) => {
                            liq += 1;
                            log::info!(
                                "Liquidated position",
                                "cmt",
                                &cmt[..16],
                                "scan",
                                scan_count
                            );
                        }
                        Err(e) => {
                            let msg = e.to_string().to_lowercase();
                            if msg.contains("not under-collateralized")
                                || msg.contains("can only liquidate a matched")
                                || msg.contains("position not found")
                                || msg.contains("solvent")
                                || msg.contains("must be matched")
                            {
                                // Healthy — not an error
                                log::debug!(
                                    "Position healthy, skipping liquidation",
                                    "cmt",
                                    &cmt[..16],
                                    "reason",
                                    &msg[..msg.len().min(80)]
                                );
                            } else {
                                errors += 1;
                                log::warning!(
                                    "Liquidation error",
                                    "cmt",
                                    &cmt[..16],
                                    "err",
                                    e.to_string()
                                );
                            }
                        }
                    }
                    std::thread::sleep(Duration::from_secs(3));
                }
            }

            log::info!(
                "Liquidation scan complete",
                "scan",
                scan_count,
                "checked",
                checked,
                "liquidated",
                liq,
                "errors",
                errors,
                "took",
                log::duration_secs(&t.elapsed())
            );
            std::thread::sleep(interval);
        }
    })
}
