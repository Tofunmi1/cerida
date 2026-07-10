use crate::{db, engine, log, proof, stellar};
use anyhow::{anyhow, Result};
use serde_json;
use std::path::Path;

/// Funding-index scale: 1_000_000_000 = 1.0 (100%).
pub const FUNDING_SCALE: i128 = 1_000_000_000;

/// Apply a single CLOB fill to both sides.
/// - Opening sides increase/create positions.
/// - Closing sides settle the linked position.
/// - Protocol (MM) sides update the per-asset protocol counterparty position.
/// Returns the settlement tx_hash if a position was closed during this fill.
pub fn apply_fill(
    store: &db::SecretStore,
    perp_id: &str,
    fill: &engine::Fill,
    keys_dir: &Path,
) -> Option<String> {
    let taker = store.get(&fill.taker_id).ok()??;
    let maker = store.get(&fill.maker_id).ok()??;

    let mut taker_collateral = allocate_collateral(&taker, fill.size);
    let mut maker_collateral = allocate_collateral(&maker, fill.size);

    // Protocol (MM) sides mirror the user collateral that is entering the trade.
    if taker.protocol {
        taker_collateral = maker_collateral;
    }
    if maker.protocol {
        maker_collateral = taker_collateral;
    }

    let close_tx = process_side(
        store,
        perp_id,
        &fill.taker_id,
        &taker,
        fill.price,
        fill.size,
        fill.taker_side,
        taker_collateral,
        keys_dir,
    )
    .ok()?;
    // Maker-side errors are TEE-internal (protocol position accounting) — log but
    // don't discard the user's close_tx by propagating None.
    if let Err(e) = process_side(
        store,
        perp_id,
        &fill.maker_id,
        &maker,
        fill.price,
        fill.size,
        fill.taker_side.opposite(),
        maker_collateral,
        keys_dir,
    ) {
        log::error!("apply_fill: maker side error", "err", e.to_string());
    }
    close_tx
}

fn allocate_collateral(secrets: &db::OrderSecrets, fill_size: u64) -> i128 {
    if secrets.size == 0 {
        return secrets.collateral_amount;
    }
    let fraction = fill_size as f64 / secrets.size as f64;
    (secrets.collateral_amount as f64 * fraction) as i128
}

fn process_side(
    store: &db::SecretStore,
    perp_id: &str,
    cmt: &str,
    secrets: &db::OrderSecrets,
    fill_price: u64,
    fill_size: u64,
    side: engine::Side,
    collateral: i128,
    keys_dir: &Path,
) -> Result<Option<String>> {
    let asset_id_hex = secrets.asset_id_hex.clone().unwrap_or_default();

    if secrets.is_close {
        let pos_cmt = secrets
            .close_position_cmt
            .as_deref()
            .ok_or_else(|| anyhow!("close order missing position_cmt"))?;
        let tx_hash = close_position(store, perp_id, pos_cmt, fill_price, fill_size, keys_dir)?;
        return Ok(Some(tx_hash));
    }

    if secrets.protocol {
        let key = db::SecretStore::protocol_position_key(&asset_id_hex);
        increase_position(
            store,
            &key,
            side,
            fill_price,
            fill_size,
            collateral,
            secrets.leverage,
            &asset_id_hex,
            None,
        )
    } else {
        increase_position(
            store,
            cmt,
            side,
            fill_price,
            fill_size,
            collateral,
            secrets.leverage,
            &asset_id_hex,
            secrets.recipient.clone(),
        )
    }?;
    Ok(None)
}

fn increase_position(
    store: &db::SecretStore,
    cmt: &str,
    side: engine::Side,
    fill_price: u64,
    fill_size: u64,
    collateral: i128,
    leverage: u64,
    asset_id_hex: &str,
    recipient: Option<String>,
) -> Result<()> {
    let idx = store.get_funding_index(asset_id_hex)?;

    // Retrieve TP/SL prices from the order secrets if this is a user position.
    let (tp_price, sl_price) = if !cmt.starts_with("protocol_") {
        match store.get(cmt) {
            Ok(Some(s)) => (s.tp_price, s.sl_price),
            _ => (0, 0),
        }
    } else {
        (0, 0)
    };

    let mut state = match store.get_position_state(cmt)? {
        Some(s) => s,
        None => db::PositionState {
            collateral: 0,
            matched_price: 0,
            funding_at_open: idx,
            effective_collateral: 0,
            entry_price: 0,
            leverage,
            side: side as u64,
            partial_liq_done: false,
            asset_id: asset_id_hex.to_string(),
            size: 0,
            last_funding_index: idx,
            protocol: cmt.starts_with("protocol_"),
            remaining_size: 0,
            asset_num: 0,
            open_time_ns: engine::now_nanos(),
            tp_price,
            sl_price,
            recipient,
        },
    };

    // Apply any accrued funding before changing the position.
    apply_funding_to_state(&mut state, idx);

    // Weighted-average entry price.
    let new_size = state
        .remaining_size
        .saturating_add(fill_size);
    if state.entry_price == 0 {
        state.entry_price = fill_price;
    } else if new_size > 0 {
        let old_notional = state.remaining_size as u128 * state.entry_price as u128;
        let new_notional = fill_size as u128 * fill_price as u128;
        state.entry_price = ((old_notional + new_notional) / new_size as u128) as u64;
    }

    state.size = state.size.saturating_add(fill_size);
    state.remaining_size = new_size;
    state.collateral = state.collateral.saturating_add(collateral.max(0) as u64 as i128);
    state.effective_collateral = state.effective_collateral.saturating_add(collateral.max(0) as u64 as i128);
    state.matched_price = fill_price;
    state.leverage = leverage.max(1);
    state.side = side as u64;

    store.insert_position_state(cmt, &state)?;
    log::info!(
        "position increased",
        "cmt",
        engine::short_id(cmt),
        "side",
        state.side,
        "entry",
        state.entry_price,
        "size",
        state.remaining_size,
        "collateral",
        state.collateral
    );
    Ok(())
}

/// Close an existing position and settle PnL + accrued funding.
/// Currently supports only full closes; partial close orders must be sized to
/// exactly match the remaining position (or be rejected upstream).
pub fn close_position(
    store: &db::SecretStore,
    perp_id: &str,
    pos_cmt: &str,
    exit_price: u64,
    _close_size: u64,
    keys_dir: &Path,
) -> Result<String> {
    let mut state = store
        .get_position_state(pos_cmt)?
        .ok_or_else(|| anyhow!("close_position: position {} not found", pos_cmt))?;

    if state.remaining_size == 0 {
        return Ok(String::new());
    }

    // Settle any accrued funding into effective_collateral first.
    let idx = store.get_funding_index(&state.asset_id)?;
    apply_funding_to_state(&mut state, idx);

    let trade_pnl = compute_trade_pnl(state.side, state.entry_price, exit_price, state.remaining_size);

    // effective_collateral already includes funding accruals from
    // apply_funding_to_state; settlement is simply collateral + trade PnL.
    let settlement_amount = (state.effective_collateral + trade_pnl).max(0);

    // Mark fully closed locally.
    state.collateral = 0;
    state.effective_collateral = 0;
    state.remaining_size = 0;
    state.size = 0;
    store.insert_position_state(pos_cmt, &state)?;

    // Save recipient before state is consumed.
    let recipient = state.recipient.clone();

    let note = stellar::create_settlement_note(settlement_amount);
    let settle_tx = stellar::relay_settle_position(
        perp_id,
        pos_cmt,
        2, // Closed
        &note.note_cmt,
        settlement_amount,
        &note.blinding_hex,
        0,
        0,
        0,
    )?;

    let mut blinding_arr = [0u8; 32];
    let blinding_bytes = hex::decode(&note.blinding_hex).unwrap_or(vec![0u8; 32]);
    blinding_arr[..blinding_bytes.len().min(32)].copy_from_slice(&blinding_bytes);
    store.insert_note_amount(
        &note.note_cmt,
        &db::NoteAmount {
            amount: settlement_amount,
            blinding: blinding_arr,
            note_secret: note.note_secret,
        },
    )?;
    store.insert_settlement_note(pos_cmt, &note.note_cmt)?;

    // Auto-claim: withdraw settlement note to the user's Stellar wallet.
    if let Some(addr) = recipient {
        if settlement_amount > 0 {
            let (_, note_null_hex) = proof::compute_note_cmt_hex(settlement_amount as u64, note.note_secret);
                    let proof_out = proof::gen_note_proof(keys_dir, settlement_amount as u64, note.note_secret);
                    match proof_out {
                        Ok(out) => {
                            let proof_json = serde_json::json!({
                                "a": out.proof.proof.a,
                                "b": out.proof.proof.b,
                                "c": out.proof.proof.c,
                            }).to_string();
                            if let Err(e) = stellar::relay_withdraw_note(
                                perp_id, &note.note_cmt, &note_null_hex, &addr,
                                settlement_amount, &note.blinding_hex, &proof_json,
                            ) {
                        log::error!("auto-claim withdraw failed", "cmt", engine::short_id(pos_cmt), "err", e.to_string());
                    }
                }
                Err(e) => {
                    log::error!("auto-claim proof generation failed", "cmt", engine::short_id(pos_cmt), "err", e.to_string());
                }
            }
        }
    }

    log::info!(
        "position closed",
        "cmt",
        engine::short_id(pos_cmt),
        "exit",
        exit_price,
        "settlement",
        settlement_amount
    );

    Ok(settle_tx)
}

pub fn compute_trade_pnl(side: u64, entry_price: u64, exit_price: u64, size: u64) -> i128 {
    if entry_price == 0 {
        return 0;
    }
    let price_delta = exit_price as i128 - entry_price as i128;
    let raw = size as i128 * price_delta / entry_price as i128;
    if side == 0 {
        // Long
        raw
    } else {
        // Short
        -raw
    }
}

pub fn apply_funding_to_state(state: &mut db::PositionState, current_index: i128) {
    if state.remaining_size == 0 || current_index == state.last_funding_index {
        return;
    }
    let rate_delta = current_index - state.last_funding_index;
    let funding = state.remaining_size as i128 * rate_delta / FUNDING_SCALE;
    // Longs pay when index rises, shorts receive.
    let signed_funding = if state.side == 0 { -funding } else { funding };
    state.effective_collateral += signed_funding;
    state.last_funding_index = current_index;
}

/// Compute the global funding rate for an asset from mark and index prices.
/// Returns a per-interval rate in FUNDING_SCALE units.
/// Positive means longs pay shorts (premium to index).
pub fn funding_rate(mark_price: u64, index_price: u64, interval_hours: u64, max_annual_rate: i128) -> i128 {
    if index_price == 0 {
        return 0;
    }
    let premium = (mark_price as i128 - index_price as i128) * FUNDING_SCALE / index_price as i128;
    // Scale premium to the funding interval.
    let intervals_per_year = 365 * 24 / interval_hours.max(1);
    let interval_rate = premium / intervals_per_year as i128;
    interval_rate.clamp(-max_annual_rate, max_annual_rate)
}
