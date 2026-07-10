use crate::db::SecretStore;
use crate::log;
use crate::stellar;
use anyhow::Result;
use rand::Rng;
use serde_json;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

// ── Constants ───────────────────────────────────────────────────────
const MAINTENANCE_MARGIN_BPS: i128 = 500; // 5%

/// Spawn the liquidation scanner thread.
pub fn spawn(
    store: Arc<SecretStore>,
    perp_id: String,
    interval_secs: u64,
    keys_dir: Arc<PathBuf>,
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
                    // Skip non-position entries (notes, etc.)
                    if !cmt.starts_with("pos_") {
                        continue;
                    }
                    let actual_cmt = &cmt[4..]; // strip "pos_" prefix
                    checked += 1;
                    match check_and_liquidate(&perp_id, actual_cmt, &store, &keys_dir) {
                        Ok(Some(tx_hash)) => {
                            liq += 1;
                            log::info!("Liquidated position", "cmt", &actual_cmt[..16], "tx", &tx_hash[..16]);
                        }
                        Ok(None) => {} // not liquidatable
                        Err(e) => {
                            errors += 1;
                            log::error!("Liquidation check failed", "cmt", &actual_cmt[..16], "err", e.to_string());
                        }
                    }
                    std::thread::sleep(Duration::from_secs(3));
                }
            }

            log::info!(
                "Liquidation scan complete",
                "scan", scan_count,
                "checked", checked,
                "liquidated", liq,
                "errors", errors,
                "took", log::duration_secs(&t.elapsed())
            );
            std::thread::sleep(interval);
        }
    })
}

/// Check if a position should be liquidated and submit liquidation.
/// Returns Ok(Some(tx_hash)) if liquidated, Ok(None) if not liquidatable.
pub fn check_and_liquidate(
    perp_id: &str,
    commitment: &str,
    store: &SecretStore,
    keys_dir: &PathBuf,
) -> Result<Option<String>> {
    let state = match store.get_position_state(commitment)? {
        Some(s) => s,
        None => return Ok(None), // not in TEE DB
    };

    // Skip already-closed or zero-size positions before hitting oracle.
    if state.remaining_size == 0 || state.effective_collateral <= 0 {
        return Ok(None);
    }

    let oracle_price_1e8 = fetch_oracle_price(&state.asset_id)?;
    if oracle_price_1e8 == 0 {
        return Ok(None);
    }
    // Convert oracle from 1e8 → 1e7 scale (matching stored entry_price / collateral).
    let oracle_price = oracle_price_1e8 / 10;

    // Apply accrued funding before liquidation check.
    let mut state = state;
    let idx = store.get_funding_index(&state.asset_id).unwrap_or(0);
    crate::position::apply_funding_to_state(&mut state, idx);

    // Compute PnL and settlement
    let notional = state.collateral * state.leverage as i128;
    let pnl = if state.side == 0 {
        // Long
        (oracle_price as i128 - state.entry_price as i128) * notional / state.entry_price as i128
    } else {
        // Short
        (state.entry_price as i128 - oracle_price as i128) * notional / state.entry_price as i128
    };
    let settlement = state.effective_collateral + pnl;

    // Check if position is solvent
    let mm = state.effective_collateral * MAINTENANCE_MARGIN_BPS / 10_000;
    if settlement >= mm {
        return Ok(None); // solvent
    }

    // Determine if partial or full liquidation
    let is_partial = !state.partial_liq_done && settlement > 0;

    if is_partial {
        // Tier 1: Partial liquidation — liquidate half, keep position alive via settle_partial.
        let half_collateral = state.effective_collateral / 2;
        let half_settlement = settlement / 2;
        let reward = half_collateral * 100 / 10_000; // 1% of freed half-collateral
        let to_note = (half_settlement - reward).max(0);

        let reward_note = stellar::create_settlement_note(reward);
        let new_settlement_commitment = hex::encode(rand::thread_rng().gen::<[u8; 32]>());

        let tx_hash = stellar::relay_settle_partial(
            perp_id, commitment, &new_settlement_commitment,
            &reward_note.note_cmt, reward, &reward_note.blinding_hex,
        )?;

        // Update stored state — reduce effective collateral, mark partial done
        let mut new_state = state.clone();
        new_state.effective_collateral -= half_collateral;
        new_state.partial_liq_done = true;
        store.insert_position_state(commitment, &new_state)?;

        let mut arr = [0u8; 32];
        let decoded = hex::decode(&reward_note.blinding_hex).unwrap_or(vec![0u8; 32]);
        let n = decoded.len().min(32);
        arr[..n].copy_from_slice(&decoded[..n]);
        store.insert_note_amount(&reward_note.note_cmt, &crate::db::NoteAmount {
            amount: reward,
            blinding: arr,
            note_secret: reward_note.note_secret,
        })?;

        log::info!("Partial liquidation executed", "cmt", &commitment[..16], "reward", reward, "tx", &tx_hash[..16]);
        Ok(Some(tx_hash))
    } else {
        // Tier 2: Full liquidation
        let eff = state.effective_collateral;
        let base_reward = eff * 150 / 10_000; // 1.5%
        let ins_fee = eff * 50 / 10_000; // 0.5%
        let total_fees = base_reward + ins_fee;

        let (actual_reward, ins_delta, to_note) = if settlement >= total_fees {
            (base_reward, ins_fee, settlement - total_fees)
        } else if settlement >= base_reward {
            (base_reward, settlement - base_reward, 0i128)
        } else {
            let shortfall = base_reward - settlement;
            // No insurance fund lookup on-chain anymore — TEE decides
            (settlement, 0i128, 0i128)
        };

        // Save recipient before state is consumed.
        let recipient = state.recipient.clone();

        let settlement = stellar::create_settlement_note(to_note);

        let tx_hash = stellar::relay_settle_position(
            perp_id, commitment, 4, // Liquidated
            &settlement.note_cmt, to_note, &settlement.blinding_hex,
            actual_reward, ins_delta, 0,
        )?;

        // Remove from TEE DB
        store.insert_position_state(commitment, &crate::db::PositionState {
            effective_collateral: 0,
            ..state
        })?;

        let mut arr = [0u8; 32];
        let decoded = hex::decode(&settlement.blinding_hex).unwrap_or(vec![0u8; 32]);
        let n = decoded.len().min(32);
        arr[..n].copy_from_slice(&decoded[..n]);
        store.insert_note_amount(&settlement.note_cmt, &crate::db::NoteAmount {
            amount: to_note,
            blinding: arr,
            note_secret: settlement.note_secret,
        })?;
        store.insert_settlement_note(commitment, &settlement.note_cmt)?;

        // Auto-claim: withdraw settlement note to the user's Stellar wallet.
        if let Some(ref addr) = recipient {
            if to_note > 0 {
                let (_, note_null_hex) = crate::proof::compute_note_cmt_hex(to_note as u64, settlement.note_secret);
                match crate::proof::gen_note_proof(keys_dir, to_note as u64, settlement.note_secret) {
                    Ok(out) => {
                        let proof_json = serde_json::json!({
                            "a": out.proof.proof.a,
                            "b": out.proof.proof.b,
                            "c": out.proof.proof.c,
                        }).to_string();
                        if let Err(e) = stellar::relay_withdraw_note(
                            perp_id, &settlement.note_cmt, &note_null_hex, addr,
                            to_note, &settlement.blinding_hex, &proof_json,
                        ) {
                            log::error!("liquidation auto-claim withdraw failed", "cmt", &commitment[..16], "err", e.to_string());
                        }
                    }
                    Err(e) => {
                        log::error!("liquidation auto-claim proof failed", "cmt", &commitment[..16], "err", e.to_string());
                    }
                }
            }
        }

        log::info!("Full liquidation executed", "cmt", &commitment[..16], "reward", actual_reward);
        Ok(Some(tx_hash))
    }
}

/// Fetch the latest price from Pyth Hermes for the given price feed ID (hex string).
/// Normalises to 8 decimal places (same precision as Pyth native expo=-8).
/// Returns 0 if the feed ID is empty/invalid or the request fails.
pub fn fetch_oracle_price(price_feed_id: &str) -> Result<u64> {
    // Skip empty or all-zeros feed IDs (legacy positions with no real asset).
    if price_feed_id.is_empty() || price_feed_id.chars().all(|c| c == '0') {
        return Ok(0);
    }

    let base = std::env::var("HERMES_URL")
        .unwrap_or_else(|_| "https://hermes.pyth.network".to_string());
    let url = format!("{}/v2/updates/price/latest?ids[]={}", base, price_feed_id);

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()?;

    let body = client
        .get(&url)
        .header("Accept-Encoding", "identity")
        .send()
        .map_err(|e| anyhow::anyhow!("Pyth request failed: {e}"))?
        .text()
        .map_err(|e| anyhow::anyhow!("Pyth read body failed: {e}"))?;

    let resp: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| anyhow::anyhow!("Pyth JSON parse failed: {e} body={}", &body[..body.len().min(200)]))?;

    let parsed = match resp["parsed"].as_array().and_then(|a| a.first()) {
        Some(p) => p.clone(),
        None => return Ok(0), // unknown/unsupported feed — skip gracefully
    };

    let raw_price: i64 = parsed["price"]["price"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Hermes: missing price string"))?
        .parse()?;

    let expo: i32 = parsed["price"]["expo"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("Hermes: missing expo"))? as i32;

    // Normalise to 8 decimal places: price_8dec = raw_price * 10^(expo+8)
    let shift = expo + 8;
    let price_u64 = if raw_price <= 0 {
        0u64
    } else if shift >= 0 {
        (raw_price as u64).saturating_mul(10u64.pow(shift as u32))
    } else {
        (raw_price as u64) / 10u64.pow((-shift) as u32)
    };

    Ok(price_u64)
}
