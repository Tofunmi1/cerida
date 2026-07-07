use crate::{db, engine, log, proof, stellar};
// cancel-proof: added 2026-07-03
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;

static NEXT_REQ_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Deserialize, Debug, Default)]
struct BatchItem {
    side: Option<u64>,
    price: Option<u64>,
    size: Option<u64>,
    leverage: Option<u64>,
    asset: Option<u64>,
    nonce: Option<u64>,
    secret: Option<u64>,
    protocol: Option<bool>,
    asset_id_hex: Option<String>,
    collateral_amount: Option<i128>,
}

#[derive(Deserialize, Debug, Default)]
struct Request {
    cmd: String,
    side: Option<u64>,
    price: Option<u64>,
    size: Option<u64>,
    leverage: Option<u64>,
    asset: Option<u64>,
    nonce: Option<u64>,
    secret: Option<u64>,
    cmt: Option<String>,
    out: Option<PathBuf>,
    perp: Option<String>,
    orderbook: Option<String>,
    cmt_a: Option<String>,
    cmt_b: Option<String>,
    source: Option<String>,
    owner: Option<String>,
    order_type: Option<String>,
    stop_price: Option<u64>,
    amount: Option<u64>,
    batch: Option<Vec<BatchItem>>,
    protocol: Option<bool>,
    asset_id_hex: Option<String>,
    collateral_amount: Option<i128>,
    is_close: Option<bool>,
    close_position_cmt: Option<String>,
    tp_price: Option<u64>,
    sl_price: Option<u64>,
}

#[derive(Serialize, Default)]
struct Response {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    commitment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    commitments: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    note_cmt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    note_null: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    proof: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    match_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    match_size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    nullifier_a: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    nullifier_b: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fills: Option<Vec<FillJson>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    best_bid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    best_ask: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    spread: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    depth: Option<Vec<LevelJson>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bids: Option<Vec<LevelJson>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    asks: Option<Vec<LevelJson>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tx_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
struct FillJson {
    maker_id: String,
    price: u64,
    size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    match_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    match_size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    nullifier_a: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    nullifier_b: Option<String>,
}

#[derive(Serialize)]
struct LevelJson {
    price: u64,
    size: u64,
    orders: usize,
}

pub fn run(
    addr: &str,
    db_path: PathBuf,
    keys_dir: PathBuf,
    perp_id: Option<String>,
    liquidator_interval_secs: u64,
    http_port: Option<u16>,
) -> Result<()> {
    log::info!(
        "═══ Starting TEE Match Server ═══",
        "version",
        env!("CARGO_PKG_VERSION"),
        "listen_addr",
        addr
    );

    let start = Instant::now();
    let sled_db = db::open_db(&db_path)?;
    let store = db::SecretStore::open(&sled_db)?;
    let book_store = db::BookStore::open(&sled_db)?;
    let books = book_store.load_all()?;
    let listener = TcpListener::bind(addr)?;
    log::info!(
        "TCP listener bound",
        "addr",
        addr,
        "took",
        log::duration_secs(&start.elapsed())
    );

    let local_addr = listener.local_addr().ok();
    log::info!(
        "Awaiting client connections",
        "addr",
        format!(
            "{}",
            local_addr
                .as_ref()
                .map(|a| a.to_string())
                .unwrap_or_default()
        ),
        "keys_dir",
        format!("{}", keys_dir.display())
    );

    let fills = db::FillLedger::open(&sled_db)?;
    log::info!("Fill audit trail ready", "existing_entries", fills.count());

    let store = Arc::new(store);
    let books = Arc::new(RwLock::new(books));
    let book_store = Arc::new(book_store);
    let fills = Arc::new(fills);
    let keys = Arc::new(keys_dir);

    // Spawn liquidator if perp_id is configured
    if let Some(ref perp) = perp_id {
        let perp = perp.clone();
        let liq_store = store.clone();
        let interval = liquidator_interval_secs;
        log::info!(
            "Starting liquidator thread",
            "perp_id",
            &perp[..12],
            "interval_secs",
            interval
        );
        crate::liquidator::spawn(liq_store, perp, interval);
    }

    // Spawn the funding-rate cron to periodically update the global funding index.
    crate::funding::spawn(store.clone(), crate::funding::DEFAULT_INTERVAL_SECS);

    // Spawn TP/SL monitor if perp_id is configured
    if let Some(ref perp) = perp_id {
        crate::tpsl::spawn(store.clone(), perp.clone(), keys.clone());
    }

    // Spawn HTTP server if http_port is set (for frontend access)
    #[cfg(feature = "secure")]
    if let Some(port) = http_port {
        let http_store = store.clone();
        let http_books = books.clone();
        let http_book_store = book_store.clone();
        let http_fills = fills.clone();
        let http_keys = keys.clone();
        let http_addr = format!("0.0.0.0:{port}");
        log::info!("Starting HTTP server", "addr", &http_addr);
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                http::run_http(
                    &http_addr,
                    http_store,
                    http_books,
                    http_book_store,
                    http_fills,
                    http_keys,
                )
                .await
                .unwrap();
            });
        });
    }

    for stream in listener.incoming() {
        let store = store.clone();
        let books = books.clone();
        let book_store = book_store.clone();
        let fills = fills.clone();
        let keys = keys.clone();
        std::thread::spawn(move || {
            use std::io::{BufRead, Write};
            let conn_start = Instant::now();

            let mut stream = match stream {
                Ok(s) => s,
                Err(e) => {
                    log::error!("TCP accept failed", "err", e.to_string());
                    return;
                }
            };

            let peer = stream
                .peer_addr()
                .map(|a| a.to_string())
                .unwrap_or_default();
            log::debug!(
                "New TCP connection",
                "peer",
                &peer,
                "local_port",
                local_addr.as_ref().map(|a| a.port()).unwrap_or(0)
            );

            let mut reader = std::io::BufReader::new(&stream);
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => {
                    log::debug!("Client disconnected without sending request", "peer", &peer);
                    return;
                }
                Ok(n) => {
                    log::debug!(
                        "Raw request received",
                        "peer",
                        &peer,
                        "bytes",
                        n,
                        "preview",
                        &line[..line.len().min(120)]
                    );
                }
            }

            let req: Request = match serde_json::from_str(&line) {
                Ok(r) => r,
                Err(e) => {
                    log::error!(
                        "Failed to parse request JSON",
                        "peer",
                        &peer,
                        "raw",
                        &line[..line.len().min(200)],
                        "err",
                        e.to_string()
                    );
                    let resp = Response {
                        ok: false,
                        error: Some(format!("invalid JSON: {e}")),
                        ..Default::default()
                    };
                    let _ = writeln!(&mut stream, "{}", serde_json::to_string(&resp).unwrap());
                    return;
                }
            };

            log::info!(
                "Processing command",
                "cmd",
                &req.cmd,
                "peer",
                &peer,
                "req_id",
                NEXT_REQ_ID.fetch_add(1, Ordering::Relaxed)
            );

            let resp = match req.cmd.as_str() {
                "init" => handle_init(&store, &keys, &req),
                "fast-init" => handle_fast_init(&store, &req),
                "batch-fast-init" => handle_batch_fast_init(&store, &req),
                "clear-book" => handle_clear_book(&book_store, &books, &req),
                "commit-proof" => handle_commit_proof(&store, &keys, &req),
                "cancel-proof" => handle_cancel_proof(&store, &keys, &req),
                "note-proof" => handle_note_proof(&keys, &req),
                "note-cmt" => handle_note_cmt(&req),
                "settle" => handle_settle(&store, &keys, &req),
                "place" => handle_place(&store, &book_store, &fills, &books, &keys, &req),
                "cancel" => handle_cancel(&store, &book_store, &books, &keys, &req),
                "market" => handle_market(&store, &book_store, &fills, &books, &keys, &req),
                "get_market" => handle_get_market(&books, &req),
                other => {
                    log::warning!("Unknown command", "cmd", other, "peer", &peer);
                    Response {
                        ok: false,
                        error: Some(format!("unknown cmd: {other}")),
                        ..Default::default()
                    }
                }
            };

            let json = serde_json::to_string(&resp).unwrap();
            let _ = writeln!(&mut stream, "{json}");

            let elapsed = conn_start.elapsed();
            if resp.ok {
                log::info!(
                    "Command completed",
                    "peer",
                    &peer,
                    "cmd",
                    &req.cmd,
                    "elapsed",
                    log::duration_secs(&elapsed)
                );
            } else {
                log::error!(
                    "Command failed",
                    "peer",
                    &peer,
                    "cmd",
                    &req.cmd,
                    "elapsed",
                    log::duration_secs(&elapsed),
                    "error",
                    resp.error.as_deref().unwrap_or("unknown")
                );
            }

            log::debug!(
                "Connection closed",
                "peer",
                &peer,
                "duration",
                log::duration_secs(&elapsed)
            );
        });
    }
    Ok(())
}

fn handle_init(store: &db::SecretStore, keys: &PathBuf, req: &Request) -> Response {
    let start = Instant::now();
    let raw_side = req.side.unwrap_or(0);
    let is_market = raw_side >= 2;
    // Normalize: 0/3 → Bid(0), 1/2 → Ask(1) so circuits always see 0/1
    let side = match raw_side {
        0 | 3 => 0,
        _ => 1,
    };
    let secrets = db::OrderSecrets {
        side,
        price: req.price.unwrap_or(0),
        size: req.size.unwrap_or(0),
        leverage: req.leverage.unwrap_or(1),
        asset: req.asset.unwrap_or(0),
        nonce: req.nonce.unwrap_or(0),
        secret: req.secret.unwrap_or(0),
        is_market,
        is_close: req.is_close.unwrap_or(false),
        close_position_cmt: req.close_position_cmt.clone(),
        protocol: req.protocol.unwrap_or(false),
        asset_id_hex: req.asset_id_hex.clone(),
        collateral_amount: req.collateral_amount.unwrap_or(0),
        tp_price: req.tp_price.unwrap_or(0),
        sl_price: req.sl_price.unwrap_or(0),
    };

    log::info!(
        "Initializing new order commitment",
        "raw_side",
        raw_side,
        "normalized_side",
        secrets.side,
        "is_market",
        secrets.is_market,
        "price",
        secrets.price,
        "size",
        secrets.size,
        "leverage",
        secrets.leverage,
        "asset",
        secrets.asset,
        "nonce",
        secrets.nonce
    );

    log::debug!(
        "Generating commitment proof via native Rust circuits",
        "side",
        secrets.side,
        "price",
        secrets.price,
        "size",
        secrets.size
    );

    let out = match proof::gen_commitment_proof(keys, &secrets) {
        Ok(o) => o,
        Err(e) => {
            log::error!("Commitment proof generation failed", "err", e.to_string());
            return err(e);
        }
    };

    let cmt_hex = format!(
        "{:0>64x}",
        out.public_inputs[0].parse::<num_bigint::BigUint>().unwrap()
    );
    log::info!(
        "Commitment computed successfully",
        "commitment",
        log::hex_snippet(&cmt_hex, 12),
        "full",
        &cmt_hex,
        "side",
        secrets.side,
        "price",
        secrets.price
    );

    if let Err(e) = store.insert(&cmt_hex, &secrets) {
        log::error!(
            "Failed to store secrets in DB",
            "cmt",
            &cmt_hex[..16],
            "err",
            e.to_string()
        );
        return err(e);
    }

    log::info!(
        "Order initialized and persisted",
        "commitment",
        log::hex_snippet(&cmt_hex, 12),
        "took",
        log::duration_secs(&start.elapsed())
    );

    Response {
        ok: true,
        commitment: Some(cmt_hex),
        ..Default::default()
    }
}

fn handle_clear_book(
    book_store: &db::BookStore,
    books: &RwLock<HashMap<u64, engine::OrderBook>>,
    req: &Request,
) -> Response {
    let asset = req.asset.unwrap_or(0);
    {
        let mut books = books.write().unwrap();
        books.insert(asset, engine::OrderBook::new());
    }
    if let Err(e) = book_store.save_book(asset, &engine::OrderBook::new()) {
        return err(format!("failed to persist cleared book: {e}"));
    }
    log::info!("Cleared order book", "asset", asset);
    Response {
        ok: true,
        ..Default::default()
    }
}

fn handle_batch_fast_init(store: &db::SecretStore, req: &Request) -> Response {
    let start = Instant::now();
    let batch = match req.batch.as_ref() {
        Some(b) if !b.is_empty() => b,
        _ => return err("missing or empty batch"),
    };

    let mut items = Vec::with_capacity(batch.len());
    let mut commitments = Vec::with_capacity(batch.len());
    for item in batch {
        let raw_side = item.side.unwrap_or(0);
        let is_market = raw_side >= 2;
        let side = match raw_side {
            0 | 3 => 0,
            _ => 1,
        };
        let secrets = db::OrderSecrets {
            side,
            price: item.price.unwrap_or(0),
            size: item.size.unwrap_or(0),
            leverage: item.leverage.unwrap_or(1),
            asset: item.asset.unwrap_or(0),
            nonce: item.nonce.unwrap_or(0),
            secret: item.secret.unwrap_or(0),
            is_market,
            is_close: false,
            close_position_cmt: None,
            protocol: item.protocol.unwrap_or(false),
            asset_id_hex: item.asset_id_hex.clone(),
        collateral_amount: item.collateral_amount.unwrap_or(0),
        tp_price: 0,
        sl_price: 0,
    };
    let cmt_hex = proof::compute_commitment_hex(&secrets);
    commitments.push(cmt_hex.clone());
    items.push((cmt_hex, secrets));
    }

    if let Err(e) = store.insert_batch(&items) {
        return err(e);
    }

    log::info!(
        "Batch fast init: commitments computed and stored",
        "count",
        commitments.len(),
        "took",
        log::duration_secs(&start.elapsed())
    );

    Response {
        ok: true,
        commitments: Some(commitments),
        ..Default::default()
    }
}

fn handle_fast_init(store: &db::SecretStore, req: &Request) -> Response {
    let start = Instant::now();
    let raw_side = req.side.unwrap_or(0);
    let is_market = raw_side >= 2;
    let side = match raw_side {
        0 | 3 => 0,
        _ => 1,
    };
    let secrets = db::OrderSecrets {
        side,
        price: req.price.unwrap_or(0),
        size: req.size.unwrap_or(0),
        leverage: req.leverage.unwrap_or(1),
        asset: req.asset.unwrap_or(0),
        nonce: req.nonce.unwrap_or(0),
        secret: req.secret.unwrap_or(0),
        is_market,
        is_close: req.is_close.unwrap_or(false),
        close_position_cmt: req.close_position_cmt.clone(),
        protocol: req.protocol.unwrap_or(false),
        asset_id_hex: req.asset_id_hex.clone(),
        collateral_amount: req.collateral_amount.unwrap_or(0),
        tp_price: req.tp_price.unwrap_or(0),
        sl_price: req.sl_price.unwrap_or(0),
    };
    let cmt_hex = proof::compute_commitment_hex(&secrets);
    log::info!(
        "Fast init: commitment computed (no proof)",
        "cmt",
        log::hex_snippet(&cmt_hex, 12),
        "side",
        secrets.side,
        "price",
        secrets.price,
        "took",
        log::duration_secs(&start.elapsed())
    );

    if let Err(e) = store.insert(&cmt_hex, &secrets) {
        return err(e);
    }

    Response {
        ok: true,
        commitment: Some(cmt_hex),
        ..Default::default()
    }
}

fn handle_commit_proof(store: &db::SecretStore, keys: &PathBuf, req: &Request) -> Response {
    let start = Instant::now();
    let cmt = match req.cmt.as_ref() {
        Some(c) => c,
        None => return err("missing cmt"),
    };

    log::info!(
        "Generating commitment proof for on-chain placement",
        "commitment",
        log::hex_snippet(cmt, 12)
    );

    log::debug!("Looking up secrets in DB", "cmt", &cmt[..16]);
    let secrets = match store.get(cmt) {
        Ok(Some(s)) => s,
        Ok(None) => {
            log::error!("Secrets not found in DB", "cmt", &cmt[..16]);
            return err(format!("secrets not found for {cmt}"));
        }
        Err(e) => {
            log::error!("DB lookup failed", "cmt", &cmt[..16], "err", e.to_string());
            return err(e);
        }
    };

    log::debug!(
        "Generating placement proof via native Rust circuits",
        "side",
        secrets.side,
        "price",
        secrets.price,
        "size",
        secrets.size
    );

    let result = match proof::gen_commitment_proof(keys, &secrets) {
        Ok(r) => r,
        Err(e) => {
            log::error!("Commitment proof generation failed", "err", e.to_string());
            return err(e);
        }
    };

    let proof_json =
        serde_json::json!({"a": result.proof.a, "b": result.proof.b, "c": result.proof.c});

    // Always return proof in response for frontend use
    let proof_str = serde_json::to_string(&proof_json).unwrap();

    // Also write to disk if out path provided
    if let Some(out_path) = req.out.as_ref() {
        match std::fs::write(out_path, &proof_str) {
            Ok(_) => log::info!(
                "Commitment proof written to disk",
                "path",
                format!("{}", out_path.display()),
                "size",
                log::bytes_label(proof_str.len())
            ),
            Err(e) => log::error!(
                "Failed to write proof file",
                "path",
                format!("{}", out_path.display()),
                "err",
                e.to_string()
            ),
        }
    }

    log::info!(
        "Commitment proof generated",
        "cmt",
        log::hex_snippet(cmt, 12),
        "proof_size",
        proof_str.len(),
        "took",
        log::duration_secs(&start.elapsed())
    );

    Response {
        ok: true,
        proof: Some(proof_str),
        ..Default::default()
    }
}

/// Generate a cancel/close proof for a position. Returns proof JSON + nullifier.
/// The frontend uses this to build + sign `cancel_position_to_note` on-chain.
fn handle_cancel_proof(store: &db::SecretStore, keys: &PathBuf, req: &Request) -> Response {
    let start = Instant::now();
    let cmt = match req.cmt.as_ref() {
        Some(c) => c,
        None => return err("missing cmt"),
    };

    log::info!(
        "Generating cancel proof",
        "commitment",
        log::hex_snippet(cmt, 12)
    );

    let secrets = match store.get(cmt) {
        Ok(Some(s)) => s,
        Ok(None) => return err(format!("secrets not found for {cmt}")),
        Err(e) => return err(e),
    };

    let result = match proof::gen_cancel_proof(keys, &secrets) {
        Ok(r) => r,
        Err(e) => return err(e),
    };

    let nullifier = format!(
        "{:0>64x}",
        result.public_inputs[0]
            .parse::<num_bigint::BigUint>()
            .unwrap()
    );
    let proof_json =
        serde_json::json!({"a": result.proof.a, "b": result.proof.b, "c": result.proof.c});
    let proof_str = serde_json::to_string(&proof_json).unwrap();

    log::info!(
        "Cancel proof generated",
        "cmt",
        log::hex_snippet(cmt, 12),
        "nullifier",
        log::hex_snippet(&nullifier, 12),
        "took",
        log::duration_secs(&start.elapsed())
    );

    Response {
        ok: true,
        commitment: Some(nullifier.clone()),
        proof: Some(proof_str),
        ..Default::default()
    }
}

/// Generate a NoteSpend Groth16 proof for a shielded deposit note.
/// Request: {cmd:"note-proof", amount:<u64>, secret:<u64>}
/// Response: {ok:true, note_cmt:<hex>, note_null:<hex>, proof:<json>}
fn handle_note_proof(keys: &PathBuf, req: &Request) -> Response {
    let start = Instant::now();
    let amount = match req.amount {
        Some(a) => a,
        None => return err("missing amount"),
    };
    let secret = match req.secret {
        Some(s) => s,
        None => return err("missing secret"),
    };
    log::info!("Generating note spend proof", "amount", amount);
    match proof::gen_note_proof(keys, amount, secret) {
        Ok(out) => {
            let proof_str = serde_json::json!({
                "a": out.proof.proof.a,
                "b": out.proof.proof.b,
                "c": out.proof.proof.c,
            })
            .to_string();
            log::info!(
                "Note spend proof generated",
                "note_cmt",
                log::hex_snippet(&out.note_cmt, 12),
                "took",
                log::duration_secs(&start.elapsed())
            );
            Response {
                ok: true,
                note_cmt: Some(out.note_cmt),
                note_null: Some(out.note_null),
                proof: Some(proof_str),
                ..Default::default()
            }
        }
        Err(e) => {
            log::error!("Note proof failed", "err", e.to_string());
            err("note proof generation failed")
        }
    }
}

/// Fast note commitment hash — no ZK proof, sub-millisecond.
/// Request: {cmd:"note-cmt", amount:<u64>, secret:<u64>}
/// Response: {ok:true, note_cmt:<hex>, note_null:<hex>}
fn handle_note_cmt(req: &Request) -> Response {
    let amount = match req.amount {
        Some(a) => a,
        None => return err("missing amount"),
    };
    let secret = match req.secret {
        Some(s) => s,
        None => return err("missing secret"),
    };
    let (note_cmt, note_null) = proof::compute_note_cmt_hex(amount, secret);
    Response {
        ok: true,
        note_cmt: Some(note_cmt),
        note_null: Some(note_null),
        ..Default::default()
    }
}

fn handle_settle(
    store: &db::SecretStore,
    keys: &PathBuf,
    req: &Request,
) -> Response {
    let start = Instant::now();
    let cmt = match req.cmt.as_ref() {
        Some(c) => c,
        None => return err("missing cmt"),
    };
    let perp = match req.perp.as_ref() {
        Some(p) => p,
        None => return err("missing perp"),
    };
    let status = match req.cmd.as_str() {
        "settle-close" => 2u32,
        "settle-liq" => 4u32,
        _ => return err("unknown settle type (use settle-close or settle-liq)"),
    };

    let source = req.source.as_deref().unwrap_or("e2e");
    let zero = "0".repeat(64);

    let settlement_amount = req.amount.unwrap_or(0) as i128;
    let reward = 0i128;
    let ins_delta = 0i128;
    let bad_debt = 0i128;
    let sn = stellar::create_settlement_note(settlement_amount);

    match stellar::relay_settle_position(
        perp, cmt, status, &sn.note_cmt, settlement_amount, &sn.blinding_hex,
        reward, ins_delta, bad_debt,
    ) {
        Ok(tx_hash) => {
            let _ = store.insert_note_amount(&sn.note_cmt, &db::NoteAmount {
                amount: settlement_amount,
                blinding: hex_to_arr32(&sn.blinding_hex),
                note_secret: sn.note_secret,
            });
            let _ = store.insert_settlement_note(cmt, &sn.note_cmt);
            log::info!("Settle position done", "cmt", &cmt[..16], "tx", &tx_hash[..16], "took", log::duration_secs(&start.elapsed()));
            Response {
                ok: true,
                tx_hash: Some(tx_hash),
                ..Default::default()
            }
        }
        Err(e) => {
            log::error!("Settle position failed", "err", e.to_string());
            err(e)
        }
    }
}

fn parse_order_type(s: &str) -> Option<engine::OrderType> {
    match s {
        "limit" => Some(engine::OrderType::Limit),
        "market" => Some(engine::OrderType::Market),
        "ioc" => Some(engine::OrderType::IOC),
        "fok" => Some(engine::OrderType::FOK),
        "stop_limit" => Some(engine::OrderType::StopLimit { stop_price: 0 }),
        "stop_market" => Some(engine::OrderType::StopMarket { stop_price: 0 }),
        _ => None,
    }
}

fn secrets_to_order(
    cmt: &str,
    secrets: &db::OrderSecrets,
    order_type: engine::OrderType,
) -> engine::Order {
    let price = match order_type {
        engine::OrderType::Market => 0,
        _ => secrets.price,
    };
    engine::Order {
        id: cmt.to_string(),
        side: if secrets.side == 0 {
            engine::Side::Bid
        } else {
            engine::Side::Ask
        },
        price,
        size: secrets.size,
        remaining: secrets.size,
        timestamp_ns: engine::now_nanos(),
        order_type,
        asset: secrets.asset,
    }
}

fn handle_place(
    store: &db::SecretStore,
    book_store: &db::BookStore,
    fills: &db::FillLedger,
    books: &RwLock<HashMap<u64, engine::OrderBook>>,
    keys: &PathBuf,
    req: &Request,
) -> Response {
    let start = Instant::now();
    let cmt = match req.cmt.as_ref() {
        Some(c) => c,
        None => return err("missing cmt"),
    };
    let ot_str = req.order_type.as_deref().unwrap_or("limit");
    let mut ot = match parse_order_type(ot_str) {
        Some(o) => o,
        None => return err(format!("unknown order_type: {ot_str}")),
    };
    if let engine::OrderType::StopLimit { ref mut stop_price } = ot {
        *stop_price = req.stop_price.unwrap_or(0);
    }
    if let engine::OrderType::StopMarket { ref mut stop_price } = ot {
        *stop_price = req.stop_price.unwrap_or(0);
    }

    let secrets = match store.get(cmt) {
        Ok(Some(s)) => s,
        Ok(None) => return err(format!("secrets not found for {cmt}")),
        Err(e) => return err(format!("db error: {e}")),
    };

    let order = secrets_to_order(cmt, &secrets, ot);
    let asset = order.asset;
    log::info!(
        "handle_place: placing order",
        "cmt",
        engine::short_id(cmt),
        "asset",
        asset,
        "secrets_side",
        secrets.side,
        "secrets_price",
        secrets.price,
        "secrets_size",
        secrets.size,
        "order_side",
        order.side as u64,
        "order_price",
        order.price,
        "order_size",
        order.size,
        "order_type",
        format!("{:?}", order.order_type)
    );

    // Phase 1: Mutate book (write lock)
    let (book_fills, best_bid, best_ask, spread, order_count) = {
        let mut books = books.write().unwrap();
        let book = books.entry(asset).or_insert_with(|| {
            log::info!("Creating new OrderBook", "asset", asset);
            engine::OrderBook::new()
        });
        let fills = match book.place(order) {
            Ok(f) => f,
            Err(e) => return err(format!("place failed: {e}")),
        };
        let bb = book.best_bid().map(|(p, s)| format!("{p}x{s}"));
        let ba = book.best_ask().map(|(p, s)| format!("{p}x{s}"));
        let sp = book.spread();
        let oc = book.order_count();
        (fills, bb, ba, sp, oc)
    };

    let perp = match req.perp {
        Some(ref p) => p.clone(),
        None => return Response {
            ok: true,
            best_bid,
            best_ask,
            spread,
            order_count: Some(order_count),
            fills: Some(book_fills.into_iter().map(|f| FillJson {
                maker_id: engine::short_id(&f.maker_id).to_string(),
                price: f.price,
                size: f.size,
                match_price: None,
                match_size: None,
                nullifier_a: None,
                nullifier_b: None,
            }).collect()),
            ..Default::default()
        },
    };
    let perp = perp.as_str();

    // Phase 2: Attempt on-chain matches + audit trail
    let fill_json: Vec<FillJson> = book_fills
        .into_iter()
        .map(|f| {
            let fj = FillJson {
                maker_id: engine::short_id(&f.maker_id).to_string(),
                price: f.price,
                size: f.size,
                match_price: None,
                match_size: None,
                nullifier_a: None,
                nullifier_b: None,
            };
            let maker_side = f.taker_side.opposite();
            let _ = fills.record(cmt, &f.maker_id, f.price, f.size, asset, "pending");
            match crate::position::apply_fill(store, perp, &f, keys) {
                Some(()) => {
                    let _ = fills.record(cmt, &f.maker_id, f.price, f.size, asset, "confirmed");
                }
                None => {
                    let _ = fills.record(cmt, &f.maker_id, f.price, f.size, asset, "failed");
                    // Restore maker to CLOB and persist immediately
                    let mut books = books.write().unwrap();
                    if let Some(book) = books.get_mut(&asset) {
                        book.restore_order(&f.maker_id, maker_side, f.price, f.size);
                        if let Err(e) = book_store.save_book(asset, book) {
                            log::error!(
                                "Failed to persist after restore",
                                "err",
                                e.to_string()
                            );
                        }
                    }
                }
            }
            fj
        })
        .collect();

    // Phase 3: Persist final book state (read lock)
    {
        let books = books.read().unwrap();
        if let Some(book) = books.get(&asset) {
            if let Err(e) = book_store.save_book(asset, book) {
                log::error!("Failed to persist OrderBook", "err", e.to_string());
            }
        }
    }

    log::info!(
        "Order placed in book",
        "cmt",
        engine::short_id(cmt),
        "asset",
        asset,
        "type",
        ot_str,
        "fills",
        fill_json.len(),
        "auto_matched",
        true,
        "took",
        log::duration_secs(&start.elapsed())
    );

    Response {
        ok: true,
        fills: Some(fill_json),
        best_bid,
        best_ask,
        spread,
        order_count: Some(order_count),
        ..Default::default()
    }
}

fn handle_cancel(
    store: &db::SecretStore,
    book_store: &db::BookStore,
    books: &RwLock<HashMap<u64, engine::OrderBook>>,
    keys: &PathBuf,
    req: &Request,
) -> Response {
    let cmt = match req.cmt.as_ref() {
        Some(c) => c,
        None => return err("missing cmt"),
    };

    let secrets = match store.get(cmt) {
        Ok(Some(s)) => s,
        Ok(None) => return err(format!("secrets not found for {cmt}")),
        Err(e) => return err(format!("db error: {e}")),
    };
    let asset = secrets.asset;

    // On-chain cancel (if perp/orderbook/owner are provided)
    if let (Some(perp), Some(orderbook), Some(owner)) = (
        req.perp.as_ref(),
        req.orderbook.as_ref(),
        req.owner.as_ref(),
    ) {
        let out = match proof::gen_cancel_proof(keys, &secrets) {
            Ok(o) => o,
            Err(e) => return err(format!("cancel proof generation failed: {e}")),
        };

        let nullifier = format!(
            "{:0>64x}",
            out.public_inputs[0].parse::<num_bigint::BigUint>().unwrap()
        );
        let source = req.source.as_deref().unwrap_or("e2e");

        if let Err(e) =
            stellar::submit_cancel(orderbook, perp, owner, cmt, &nullifier, &out, source)
        {
            log::error!(
                "Cancel on-chain submission failed",
                "cmt",
                &cmt[..16],
                "err",
                e.to_string()
            );
            return err(format!("cancel on-chain submission failed: {e}"));
        }
        log::info!(
            "Order cancelled on-chain",
            "cmt",
            &cmt[..16],
            "nullifier",
            &nullifier[..16]
        );
    }

    // CLOB cancel (always)
    {
        let mut books = books.write().unwrap();
        if let Some(book) = books.get_mut(&asset) {
            match book.cancel(cmt) {
                Ok(_) => {}
                Err(_) => log::warning!("Cancel: order not in CLOB book", "cmt", &cmt[..16]),
            }
            if let Err(e) = book_store.save_book(asset, book) {
                log::error!("Failed to persist OrderBook", "err", e.to_string());
            }
        }
    }

    log::info!("Order cancelled on CLOB", "cmt", &cmt[..16]);
    Response {
        ok: true,
        ..Default::default()
    }
}

fn handle_market(
    store: &db::SecretStore,
    book_store: &db::BookStore,
    fills: &db::FillLedger,
    books: &RwLock<HashMap<u64, engine::OrderBook>>,
    keys: &PathBuf,
    req: &Request,
) -> Response {
    let start = Instant::now();
    let cmt = match req.cmt.as_ref() {
        Some(c) => c,
        None => return err("missing cmt"),
    };
    let secrets = match store.get(cmt) {
        Ok(Some(s)) => s,
        Ok(None) => return err(format!("secrets not found for {cmt}")),
        Err(e) => return err(format!("db error: {e}")),
    };

    let order = secrets_to_order(cmt, &secrets, engine::OrderType::Market);
    let asset = order.asset;
    log::info!(
        "handle_market: placing order",
        "cmt",
        engine::short_id(cmt),
        "asset",
        asset,
        "secrets_side",
        secrets.side,
        "secrets_price",
        secrets.price,
        "secrets_size",
        secrets.size,
        "order_side",
        order.side as u64,
        "order_price",
        order.price,
        "order_size",
        order.size
    );

    // Phase 1: Mutate book (write lock)
    let (book_fills, best_bid, best_ask, spread, order_count) = {
        let mut books = books.write().unwrap();
        let book = books.entry(asset).or_insert_with(|| {
            log::info!("Creating new OrderBook", "asset", asset);
            engine::OrderBook::new()
        });
        let fills = match book.place(order) {
            Ok(f) => f,
            Err(e) => return err(format!("market order failed: {e}")),
        };
        let bb = book.best_bid().map(|(p, s)| format!("{p}x{s}"));
        let ba = book.best_ask().map(|(p, s)| format!("{p}x{s}"));
        let sp = book.spread();
        let oc = book.order_count();
        (fills, bb, ba, sp, oc)
    };

    let perp = match req.perp {
        Some(ref p) => p.clone(),
        None => return Response {
            ok: true,
            best_bid,
            best_ask,
            spread,
            order_count: Some(order_count),
            fills: Some(book_fills.into_iter().map(|f| FillJson {
                maker_id: engine::short_id(&f.maker_id).to_string(),
                price: f.price,
                size: f.size,
                match_price: None,
                match_size: None,
                nullifier_a: None,
                nullifier_b: None,
            }).collect()),
            ..Default::default()
        },
    };
    let perp = perp.as_str();

    // Phase 2: Attempt on-chain matches + audit trail
    let fill_json: Vec<FillJson> = book_fills
        .into_iter()
        .map(|f| {
            let fj = FillJson {
                maker_id: engine::short_id(&f.maker_id).to_string(),
                price: f.price,
                size: f.size,
                match_price: None,
                match_size: None,
                nullifier_a: None,
                nullifier_b: None,
            };
            let maker_side = f.taker_side.opposite();
            let _ = fills.record(cmt, &f.maker_id, f.price, f.size, asset, "pending");
            match crate::position::apply_fill(store, perp, &f, keys) {
                Some(()) => {
                    let _ = fills.record(cmt, &f.maker_id, f.price, f.size, asset, "confirmed");
                }
                None => {
                    let _ = fills.record(cmt, &f.maker_id, f.price, f.size, asset, "failed");
                    // Restore maker to CLOB and persist immediately
                    let mut books = books.write().unwrap();
                    if let Some(book) = books.get_mut(&asset) {
                        book.restore_order(&f.maker_id, maker_side, f.price, f.size);
                        if let Err(e) = book_store.save_book(asset, book) {
                            log::error!(
                                "Failed to persist after restore",
                                "err",
                                e.to_string()
                            );
                        }
                    }
                }
            }
            fj
        })
        .collect();

    // Phase 3: Persist final book state (read lock)
    {
        let books = books.read().unwrap();
        if let Some(book) = books.get(&asset) {
            if let Err(e) = book_store.save_book(asset, book) {
                log::error!("Failed to persist OrderBook", "err", e.to_string());
            }
        }
    }

    log::info!(
        "Market order executed",
        "cmt",
        engine::short_id(cmt),
        "asset",
        asset,
        "fills",
        fill_json.len(),
        "auto_matched",
        true,
        "took",
        log::duration_secs(&start.elapsed())
    );

    Response {
        ok: true,
        fills: Some(fill_json),
        best_bid,
        best_ask,
        spread,
        order_count: Some(order_count),
        ..Default::default()
    }
}



fn handle_get_market(books: &RwLock<HashMap<u64, engine::OrderBook>>, req: &Request) -> Response {
    let asset = req.asset.unwrap_or(0);
    let books = books.read().unwrap();
    if let Some(book) = books.get(&asset) {
        Response {
            ok: true,
            best_bid: book.best_bid().map(|(p, s)| format!("{p}x{s}")),
            best_ask: book.best_ask().map(|(p, s)| format!("{p}x{s}")),
            spread: book.spread(),
            order_count: Some(book.order_count()),
            depth: Some(
                book.depth(engine::Side::Bid, 32)
                    .iter()
                    .map(|&(p, s, o)| LevelJson {
                        price: p,
                        size: s,
                        orders: o,
                    })
                    .collect(),
            ),
            bids: Some(
                book.depth(engine::Side::Bid, 32)
                    .iter()
                    .map(|&(p, s, o)| LevelJson {
                        price: p,
                        size: s,
                        orders: o,
                    })
                    .collect(),
            ),
            asks: Some(
                book.depth(engine::Side::Ask, 32)
                    .iter()
                    .map(|&(p, s, o)| LevelJson {
                        price: p,
                        size: s,
                        orders: o,
                    })
                    .collect(),
            ),
            ..Default::default()
        }
    } else {
        Response {
            ok: true,
            order_count: Some(0),
            ..Default::default()
        }
    }
}

fn hex_to_arr32(s: &str) -> [u8; 32] {
    let bytes = hex::decode(s).unwrap_or_else(|_| vec![0u8; 32]);
    let mut arr = [0u8; 32];
    let len = bytes.len().min(32);
    arr[..len].copy_from_slice(&bytes[..len]);
    arr
}

fn err(s: impl std::fmt::Display) -> Response {
    Response {
        ok: false,
        error: Some(s.to_string()),
        ..Default::default()
    }
}

// ── Secure HTTP Server (Attestation + Encryption) ───────────────────
// TLS is handled by a reverse proxy (GCP LB, nginx, or sidecar).
// This server provides the application-layer security: attestation + AEAD.

#[cfg(feature = "secure")]
pub mod secure {
    use super::*;
    use crate::attestation;
    use crate::crypto;
    use axum::{
        extract::{Query, State},
        routing::{get, post},
        Json, Router,
    };
    use std::sync::Arc as StdArc;

    #[derive(Clone)]
    pub struct SecureState {
        pub store: StdArc<db::SecretStore>,
        pub books: StdArc<RwLock<HashMap<u64, engine::OrderBook>>>,
        pub book_store: StdArc<db::BookStore>,
        pub fills: StdArc<db::FillLedger>,
        pub keys_dir: PathBuf,
        pub attestation_policy: StdArc<attestation::AttestationPolicy>,
    }

    pub async fn run_secure(
        addr: &str,
        db_path: PathBuf,
        keys_dir: PathBuf,
        perp_id: Option<String>,
        liquidator_interval_secs: u64,
    ) -> Result<()> {
        let sled_db = db::open_db(&db_path)?;
        let store = db::SecretStore::open(&sled_db)?;
        let book_store = db::BookStore::open(&sled_db)?;
        let books = book_store.load_all()?;
        let fills = db::FillLedger::open(&sled_db)?;

        let store_arc = StdArc::new(store);

        if let Some(ref perp) = perp_id {
            let liq_store = store_arc.clone();
            let perp = perp.clone();
            let interval = liquidator_interval_secs;
            log::info!(
                "Starting liquidator thread",
                "perp_id",
                &perp[..12],
                "interval_secs",
                interval
            );
            crate::liquidator::spawn(liq_store, perp, interval);
        }

        // Spawn the funding-rate cron to periodically update the global funding index.
        crate::funding::spawn(store_arc.clone(), crate::funding::DEFAULT_INTERVAL_SECS);

        // Spawn TP/SL monitor (only when perp_id is configured)
        if let Some(ref perp_inner) = perp_id {
            crate::tpsl::spawn(store_arc.clone(), perp_inner.clone(), StdArc::new(keys_dir.clone()));
        }

        let state = SecureState {
            store: store_arc,
            books: StdArc::new(RwLock::new(books)),
            book_store: StdArc::new(book_store),
            fills: StdArc::new(fills),
            keys_dir,
            attestation_policy: StdArc::new(attestation::AttestationPolicy::default()),
        };

        let app = Router::new()
            .route("/attestation", get(handle_attestation))
            .route("/init", post(handle_init_secure))
            .route("/place", post(handle_place_secure))
            .route("/cancel", post(handle_cancel_secure))
            .route("/settle", post(handle_settle_secure))
            .route("/market", post(handle_market_secure))
            .route("/get_market", get(handle_get_market_secure))
            .with_state(state);

        let listener = tokio::net::TcpListener::bind(addr).await?;

        log::info!(
            "Secure HTTP server listening",
            "addr",
            addr,
            "note",
            "TLS must be terminated by a reverse proxy"
        );

        axum::serve(listener, app).await?;
        Ok(())
    }

    /// GET /attestation?nonce=<hex> — request an attestation token bound to TLS EKM.
    async fn handle_attestation(
        State(state): State<SecureState>,
        Query(params): Query<HashMap<String, String>>,
    ) -> Json<serde_json::Value> {
        let nonce_hex = params.get("nonce").cloned().unwrap_or_default();
        let nonce = hex::decode(&nonce_hex).unwrap_or_default();

        match attestation::request_attestation_token("https://sts.googleapis.com", &[], "OIDC") {
            Ok(token) => {
                let policy = &*state.attestation_policy;
                match attestation::verify_attestation_token(&token, policy, &nonce) {
                    Ok(claims) => {
                        log::info!(
                            "Attestation token verified",
                            "hwmodel",
                            &claims.hwmodel,
                            "dbgstat",
                            &claims.dbgstat
                        );
                        Json(serde_json::json!({"ok": true, "token": token}))
                    }
                    Err(e) => {
                        log::error!("Attestation verification failed", "err", e.to_string());
                        Json(serde_json::json!({"ok": false, "error": e.to_string()}))
                    }
                }
            }
            Err(e) => {
                log::error!("Attestation request failed", "err", e.to_string());
                Json(serde_json::json!({"ok": false, "error": e.to_string()}))
            }
        }
    }

    /// POST /init — encrypted order init. Body: { "encrypted": "<base64>" }
    async fn handle_init_secure(
        State(state): State<SecureState>,
        Json(payload): Json<serde_json::Value>,
    ) -> Json<serde_json::Value> {
        // The DEK is provided via CER_DEK env var (set by the GCP Confidential Space launcher
        // after unwrapping from KMS at startup).
        let dek_hex = match std::env::var("CER_DEK") {
            Ok(v) => v,
            Err(_) => return Json(serde_json::json!({"ok": false, "error": "CER_DEK not set"})),
        };
        let dek_bytes = match hex::decode(&dek_hex) {
            Ok(v) if v.len() == 32 => {
                let mut key = [0u8; 32];
                key.copy_from_slice(&v);
                key
            }
            _ => return Json(serde_json::json!({"ok": false, "error": "invalid CER_DEK"})),
        };

        let encrypted_b64 = match payload["encrypted"].as_str() {
            Some(s) => s,
            None => return Json(serde_json::json!({"ok": false, "error": "missing encrypted"})),
        };

        let encrypted =
            match base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encrypted_b64)
            {
                Ok(v) => v,
                Err(e) => {
                    return Json(serde_json::json!({"ok": false, "error": format!("b64: {e}")}))
                }
            };

        if encrypted.len() < 12 {
            return Json(serde_json::json!({"ok": false, "error": "too short"}));
        }

        let mut nonce = [0u8; 12];
        nonce.copy_from_slice(&encrypted[..12]);
        let payload = crypto::EncryptedPayload {
            nonce,
            ciphertext: encrypted[12..].to_vec(),
        };

        let plaintext = match crypto::decrypt(&dek_bytes, &payload) {
            Ok(p) => p,
            Err(e) => {
                return Json(serde_json::json!({"ok": false, "error": format!("decrypt: {e}")}))
            }
        };

        let req: Request = match serde_json::from_slice(&plaintext) {
            Ok(r) => r,
            Err(e) => return Json(serde_json::json!({"ok": false, "error": format!("json: {e}")})),
        };

        let resp = handle_init(&state.store, &state.keys_dir, &req);
        Json(serde_json::json!(resp))
    }

    /// Helper: decrypt an encrypted request body and parse it into a Request.
    /// Returns (dek_bytes, parsed_request).
    fn decrypt_request(payload: &serde_json::Value) -> Result<([u8; 32], Request), String> {
        let dek_hex = std::env::var("CER_DEK").map_err(|_| "CER_DEK not set".to_string())?;
        let dek_bytes: [u8; 32] = hex::decode(&dek_hex)
            .map_err(|_| "invalid CER_DEK hex".to_string())
            .and_then(|v| {
                v.try_into()
                    .map_err(|_| "CER_DEK must be 32 bytes".to_string())
            })?;

        let encrypted_b64 = payload["encrypted"].as_str().ok_or("missing encrypted")?;
        let encrypted =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encrypted_b64)
                .map_err(|e| format!("b64: {e}"))?;
        if encrypted.len() < 12 {
            return Err("too short".to_string());
        }
        let mut nonce = [0u8; 12];
        nonce.copy_from_slice(&encrypted[..12]);
        let ep = crypto::EncryptedPayload {
            nonce,
            ciphertext: encrypted[12..].to_vec(),
        };
        let plaintext = crypto::decrypt(&dek_bytes, &ep).map_err(|e| format!("decrypt: {e}"))?;
        let req: Request = serde_json::from_slice(&plaintext).map_err(|e| format!("json: {e}"))?;
        Ok((dek_bytes, req))
    }

    async fn handle_place_secure(
        State(state): State<SecureState>,
        Json(payload): Json<serde_json::Value>,
    ) -> Json<serde_json::Value> {
        let req = match decrypt_request(&payload) {
            Ok((_, r)) => r,
            Err(e) => return Json(serde_json::json!({"ok": false, "error": e})),
        };
        let resp = handle_place(
            &state.store,
            &state.book_store,
            &state.fills,
            &state.books,
            &state.keys_dir,
            &req,
        );
        Json(serde_json::json!(resp))
    }

    async fn handle_cancel_secure(
        State(state): State<SecureState>,
        Json(payload): Json<serde_json::Value>,
    ) -> Json<serde_json::Value> {
        let req = match decrypt_request(&payload) {
            Ok((_, r)) => r,
            Err(e) => return Json(serde_json::json!({"ok": false, "error": e})),
        };
        let resp = handle_cancel(
            &state.store,
            &state.book_store,
            &state.books,
            &state.keys_dir,
            &req,
        );
        Json(serde_json::json!(resp))
    }

    async fn handle_market_secure(
        State(state): State<SecureState>,
        Json(payload): Json<serde_json::Value>,
    ) -> Json<serde_json::Value> {
        let req = match decrypt_request(&payload) {
            Ok((_, r)) => r,
            Err(e) => return Json(serde_json::json!({"ok": false, "error": e})),
        };
        let resp = handle_market(
            &state.store,
            &state.book_store,
            &state.fills,
            &state.books,
            &state.keys_dir,
            &req,
        );
        Json(serde_json::json!(resp))
    }

    // Public endpoints (no encryption): get_market, set_mark_price
    async fn handle_get_market_secure(
        State(state): State<SecureState>,
        Query(params): Query<HashMap<String, String>>,
    ) -> Json<serde_json::Value> {
        let req = Request {
            cmd: "get_market".to_string(),
            asset: params.get("asset").and_then(|v| v.parse().ok()),
            ..Default::default() // rest of fields use defaults
        };
        let resp = handle_get_market(&state.books, &req);
        Json(serde_json::json!(resp))
    }

    async fn handle_settle_secure(
        State(state): State<SecureState>,
        Json(payload): Json<serde_json::Value>,
    ) -> Json<serde_json::Value> {
        let req = match decrypt_request(&payload) {
            Ok((_, r)) => r,
            Err(e) => return Json(serde_json::json!({"ok": false, "error": e})),
        };
        let resp = handle_settle(&state.store, &state.keys_dir, &req);
        Json(serde_json::json!(resp))
    }
}

// ── HTTP Server (no encryption, for frontend/demo access) ──────────
// Exposes the same commands as TCP via HTTP POST /<cmd> endpoints.
// No attestation or encryption — TLS is terminated by the LB/proxy.
// Only compiled with the `secure` feature (same deps as attestation).

#[cfg(feature = "secure")]
pub mod http {
    use super::*;
    use axum::{
        extract::{Path, Query, State},
        routing::{get, post},
        Json, Router,
    };
    use std::sync::Arc as StdArc;
    use tower_http::cors::{Any, CorsLayer};

    /// Relay request queued for the next batch window.
    struct PendingRelay {
        perp: String,
        orderbook: String,
        position_cmt: String,
        sealed_params: Vec<u8>,
        collateral_amount: i128,
        collateral_blinding: String,
        settlement_commitment: String,
        portfolio_key: String,
        asset_id: String,
        commit_proof: String,
        // Note-based path (open_position_from_note)
        note_cmt: String,
        note_null: String,
        note_proof: String,
        // Pool-based path (open_position_from_pool) — if pool_id is set, use pool path
        pool_id: String,
        pool_root: String,
        pool_nullifier_hash: String,
        pool_spend_proof: String,
        liq_recipient_note: String,
    }

    type RelayQueue = StdArc<std::sync::Mutex<Vec<PendingRelay>>>;

    /// Pre-signed deposit TX XDR queued for batch submission.
    /// The user signs deposit_note themselves (Stellar requires it for token auth),
    /// but the TEE batches and shuffles submissions to break timing correlation.
    struct PendingDeposit {
        signed_xdr: String,
    }
    type DepositQueue = StdArc<std::sync::Mutex<Vec<PendingDeposit>>>;

    type LimitRelayStore = StdArc<std::sync::Mutex<HashMap<String, PendingRelay>>>;

    #[derive(Clone)]
    pub struct HttpState {
        pub store: StdArc<db::SecretStore>,
        pub books: StdArc<RwLock<HashMap<u64, engine::OrderBook>>>,
        pub book_store: StdArc<db::BookStore>,
        pub fills: StdArc<db::FillLedger>,
        pub keys_dir: PathBuf,
        relay_queue: RelayQueue,
        deposit_queue: DepositQueue,
        store_for_relay: StdArc<db::SecretStore>,
        limit_relay_store: LimitRelayStore,
    }

    /// How long the TEE waits before flushing and shuffling the relay queue.
    /// Longer = more privacy (more orders mix together).
    const RELAY_BATCH_SECS: u64 = 10;

    pub async fn run_http(
        addr: &str,
        store: StdArc<db::SecretStore>,
        books: StdArc<RwLock<HashMap<u64, engine::OrderBook>>>,
        book_store: StdArc<db::BookStore>,
        fills: StdArc<db::FillLedger>,
        keys_dir: StdArc<PathBuf>,
    ) -> Result<()> {
        let relay_queue: RelayQueue = StdArc::new(std::sync::Mutex::new(Vec::new()));
        let deposit_queue: DepositQueue = StdArc::new(std::sync::Mutex::new(Vec::new()));
        let limit_relay_store: LimitRelayStore = StdArc::new(std::sync::Mutex::new(HashMap::new()));

        // Deposit batch task: every RELAY_BATCH_SECS, drain deposit queue, shuffle,
        // and submit each pre-signed XDR. The user signed the TX (Stellar requires it),
        // but submitting in a shuffled batch breaks timing correlation with position opens.
        {
            let dq = deposit_queue.clone();
            tokio::spawn(async move {
                use rand::seq::SliceRandom;
                let mut interval = tokio::time::interval(
                    std::time::Duration::from_secs(RELAY_BATCH_SECS),
                );
                interval.tick().await;
                loop {
                    interval.tick().await;
                    let mut batch: Vec<PendingDeposit> = {
                        let mut guard = dq.lock().unwrap();
                        std::mem::take(&mut *guard)
                    };
                    if batch.is_empty() { continue; }
                    batch.shuffle(&mut rand::thread_rng());
                    log::info!("deposit batch: flushing", "count", batch.len());
                    for item in batch {
                        let xdr = item.signed_xdr.clone();
                        if let Err(e) = tokio::task::spawn_blocking(move || {
                            let rpc = e2e::soroban_rpc::SorobanRpc::new();
                            rpc.send_transaction(&xdr)
                        }).await {
                            log::error!("deposit relay failed: {e}");
                        }
                    }
                }
            });
        }

        // Batch relay task: every RELAY_BATCH_SECS, drain the queue, shuffle it,
        // then submit each relay to Stellar in randomised order.
        // This breaks timing correlation between a user's HTTP request and the
        // on-chain transaction — an observer cannot map deposit time → trade time.
        {
            let q = relay_queue.clone();
            let store_for_relay = store.clone();
            tokio::spawn(async move {
                use rand::seq::SliceRandom;
                let mut interval = tokio::time::interval(
                    std::time::Duration::from_secs(RELAY_BATCH_SECS),
                );
                interval.tick().await; // discard immediate first tick
                loop {
                    interval.tick().await;
                    let mut batch: Vec<PendingRelay> = {
                        let mut guard = q.lock().unwrap();
                        std::mem::take(&mut *guard)
                    };
                    if batch.is_empty() {
                        continue;
                    }
                    batch.shuffle(&mut rand::thread_rng());
                    log::info!("relay batch: flushing", "count", batch.len());
                    for item in batch {
                        let cmt_preview = item.position_cmt[..item.position_cmt.len().min(16)].to_string();
                        let position_cmt = item.position_cmt.clone();
                        let store_ref = store_for_relay.clone();
                        let result = tokio::task::spawn_blocking(move || {
                            if !item.pool_id.is_empty() {
                                stellar::relay_open_position_from_pool(
                                    &item.perp,
                                    &item.orderbook,
                                    &item.pool_id,
                                    &item.pool_root,
                                    &item.pool_nullifier_hash,
                                    &item.position_cmt,
                                    &item.sealed_params,
                                    item.collateral_amount,
                                    &item.collateral_blinding,
                                    &item.settlement_commitment,
                                    &item.liq_recipient_note,
                                    &item.portfolio_key,
                                    &item.asset_id,
                                    &item.pool_spend_proof,
                                    &item.commit_proof,
                                    &store_ref,
                                )
                            } else {
                                stellar::relay_open_position(
                                    &item.perp,
                                    &item.orderbook,
                                    &item.note_cmt,
                                    &item.note_null,
                                    &item.position_cmt,
                                    &item.sealed_params,
                                    item.collateral_amount,
                                    &item.collateral_blinding,
                                    &item.settlement_commitment,
                                    &item.portfolio_key,
                                    &item.asset_id,
                                    &item.note_proof,
                                    &item.commit_proof,
                                    &store_ref,
                                )
                            }
                        })
                        .await;
                        match result {
                            Ok(Ok(hash)) => {
                                log::info!(
                                    "relay batch: position opened",
                                    "cmt", &cmt_preview,
                                    "tx_hash", &hash
                                );
                                let _ = store_for_relay.insert_position_tx(&position_cmt, &hash);
                            }
                            Ok(Err(e)) => log::error!(
                                "relay batch: submission failed",
                                "cmt", &cmt_preview,
                                "err", e.to_string()
                            ),
                            Err(e) => log::error!(
                                "relay batch: task panic",
                                "err", e.to_string()
                            ),
                        }
                    }
                }
            });
        }

        let state = HttpState {
            store: store.clone(),
            books: books.clone(),
            book_store: book_store.clone(),
            fills: fills.clone(),
            keys_dir: (*keys_dir).clone(),
            relay_queue,
            deposit_queue,
            store_for_relay: store.clone(),
            limit_relay_store,
        };

        let app = Router::new()
            .route("/init", post(handle_http_init))
            .route("/fast-init", post(handle_http_fast_init))
            .route("/commit-proof", post(handle_http_commit_proof))
            .route("/cancel-proof", post(handle_http_cancel_proof))
            .route("/note-proof", post(handle_http_note_proof))
            .route("/note-cmt", post(handle_http_note_cmt))
            .route("/place", post(handle_http_place))
            .route("/cancel", post(handle_http_cancel))
            .route("/match", post(handle_http_match))
            .route("/market", post(handle_http_market))
            .route("/get-market", get(handle_http_get_market))
            .route("/settle", post(handle_http_settle))
            .route(
                "/relay/open-position",
                post(handle_http_relay_open_position),
            )
            .route(
                "/relay/place-limit",
                post(handle_http_relay_place_limit),
            )
            .route(
                "/relay/close-position",
                post(handle_http_relay_close_position),
            )
            .route(
                "/relay/open-position-pool",
                post(handle_http_relay_open_position_pool),
            )
            .route(
                "/relay/withdraw-to-pool",
                post(handle_http_relay_withdraw_to_pool),
            )
            .route(
                "/relay/cancel-position",
                post(handle_http_relay_cancel_position),
            )
            .route(
                "/relay/deposit-note",
                post(handle_http_relay_deposit_note),
            )
            .route("/note-amount", get(handle_http_note_amount))
            .route("/relay/position-tx", get(handle_http_position_tx))
            .route(
                "/relay/withdraw-settlement",
                post(handle_http_relay_withdraw_settlement),
            )
            .layer(
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any),
            )
            .with_state(state);

        let listener = tokio::net::TcpListener::bind(addr).await?;
        log::info!("HTTP server listening", "addr", addr);
        axum::serve(listener, app).await?;
        Ok(())
    }

    async fn handle_http_init(
        State(state): State<HttpState>,
        Json(req): Json<Request>,
    ) -> Json<serde_json::Value> {
        Json(serde_json::json!(handle_init(
            &state.store,
            &state.keys_dir,
            &req
        )))
    }

    async fn handle_http_fast_init(
        State(state): State<HttpState>,
        Json(req): Json<Request>,
    ) -> Json<serde_json::Value> {
        Json(serde_json::json!(handle_fast_init(&state.store, &req)))
    }

    async fn handle_http_commit_proof(
        State(state): State<HttpState>,
        Json(req): Json<Request>,
    ) -> Json<serde_json::Value> {
        Json(serde_json::json!(handle_commit_proof(
            &state.store,
            &state.keys_dir,
            &req
        )))
    }

    async fn handle_http_cancel_proof(
        State(state): State<HttpState>,
        Json(req): Json<Request>,
    ) -> Json<serde_json::Value> {
        Json(serde_json::json!(handle_cancel_proof(
            &state.store,
            &state.keys_dir,
            &req
        )))
    }

    async fn handle_http_note_proof(
        State(state): State<HttpState>,
        Json(req): Json<Request>,
    ) -> Json<serde_json::Value> {
        Json(serde_json::json!(handle_note_proof(&state.keys_dir, &req)))
    }

    async fn handle_http_note_cmt(
        State(state): State<HttpState>,
        Json(req): Json<Request>,
    ) -> Json<serde_json::Value> {
        Json(serde_json::json!(handle_note_cmt(&req)))
    }

    async fn handle_http_place(
        State(state): State<HttpState>,
        Json(req): Json<Request>,
    ) -> Json<serde_json::Value> {
        Json(serde_json::json!(handle_place(
            &state.store,
            &state.book_store,
            &state.fills,
            &state.books,
            &state.keys_dir,
            &req
        )))
    }

    async fn handle_http_cancel(
        State(state): State<HttpState>,
        Json(req): Json<Request>,
    ) -> Json<serde_json::Value> {
        Json(serde_json::json!(handle_cancel(
            &state.store,
            &state.book_store,
            &state.books,
            &state.keys_dir,
            &req
        )))
    }

    async fn handle_http_match(
        State(state): State<HttpState>,
        Json(req): Json<Request>,
    ) -> Json<serde_json::Value> {
        Json(serde_json::json!(handle_settle(
            &state.store,
            &state.keys_dir,
            &req
        )))
    }

    async fn handle_http_market(
        State(state): State<HttpState>,
        Json(req): Json<Request>,
    ) -> Json<serde_json::Value> {
        Json(serde_json::json!(handle_market(
            &state.store,
            &state.book_store,
            &state.fills,
            &state.books,
            &state.keys_dir,
            &req
        )))
    }

    async fn handle_http_get_market(
        State(state): State<HttpState>,
        axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
    ) -> Json<serde_json::Value> {
        let asset = params.get("asset").and_then(|v| v.parse().ok());
        let req = Request {
            cmd: "get_market".to_string(),
            asset,
            ..Default::default()
        };
        Json(serde_json::json!(handle_get_market(&state.books, &req)))
    }

    #[derive(serde::Deserialize)]
    struct RelayOpenPositionReq {
        perp: String,
        orderbook: String,
        note_cmt: String,
        note_null: String,
        position_cmt: String,
        #[serde(default)]
        sealed_params: Option<String>,
        collateral_amount: i128,
        collateral_blinding: String,
        settlement_commitment: String,
        #[serde(default)]
        portfolio_key: Option<String>,
        #[serde(default)]
        asset_id: Option<String>,
        note_proof: String,
        commit_proof: String,
    }

    async fn handle_http_relay_open_position(
        State(state): State<HttpState>,
        Json(req): Json<RelayOpenPositionReq>,
    ) -> Json<serde_json::Value> {
        let zeros = "0".repeat(64);
        let portfolio_key = req.portfolio_key.as_deref().unwrap_or(&zeros).to_string();
        let asset_id = req.asset_id.as_deref().unwrap_or(&zeros).to_string();

        // Build sealed_params from DB-stored order secrets (TEE seals them, never sent by frontend)
        let secrets = match state.store.get(&req.position_cmt) {
            Ok(Some(mut secrets)) => {
                secrets.asset_id_hex = Some(asset_id.clone());
                secrets.collateral_amount = req.collateral_amount;
                secrets.protocol = false;
                secrets.is_close = false;
                let _ = state.store.insert(&req.position_cmt, &secrets);
                secrets
            }
            Ok(None) => return Json(serde_json::json!({ "ok": false, "error": "secrets not found for commitment" })),
            Err(e) => return Json(serde_json::json!({ "ok": false, "error": format!("db error: {e}") })),
        };
        let sealed_bytes = match stellar::seal_from_secrets(&secrets) {
            Ok(b) => b,
            Err(e) => return Json(serde_json::json!({ "ok": false, "error": format!("seal: {e}") })),
        };

        // Place the order into the CLOB immediately so market orders can match.
        // For market orders this generates fills and updates position state; the
        // on-chain open_position relay still runs in the batch queue afterwards.
        let order_type = if secrets.is_market {
            engine::OrderType::Market
        } else {
            engine::OrderType::Limit
        };
        let order = secrets_to_order(&req.position_cmt, &secrets, order_type);
        let asset = order.asset;
        let book_fills = {
            let mut books = state.books.write().unwrap();
            let book = books.entry(asset).or_insert_with(engine::OrderBook::new);
            match book.place(order) {
                Ok(fills) => fills,
                Err(e) => return Json(serde_json::json!({ "ok": false, "error": format!("clob: {e}") })),
            }
        };
        for fill in &book_fills {
            crate::position::apply_fill(&state.store, &req.perp, fill, &state.keys_dir);
        }
        {
            let books = state.books.read().unwrap();
            if let Some(book) = books.get(&asset) {
                if let Err(e) = state.book_store.save_book(asset, book) {
                    log::error!("open relay: failed to persist book", "err", e.to_string());
                }
            }
        }

        let cmt_preview = req.position_cmt[..req.position_cmt.len().min(16)].to_string();

        // Queue into the batch relay window for timing-correlation privacy.
        // Returns immediately so the HTTP proxy (Vercel) never times out.
        let pending = PendingRelay {
            perp: req.perp,
            orderbook: req.orderbook,
            position_cmt: req.position_cmt,
            sealed_params: sealed_bytes,
            collateral_amount: req.collateral_amount,
            collateral_blinding: req.collateral_blinding,
            settlement_commitment: req.settlement_commitment,
            portfolio_key,
            asset_id,
            commit_proof: req.commit_proof,
            note_cmt: req.note_cmt,
            note_null: req.note_null,
            note_proof: req.note_proof,
            pool_id: String::new(),
            pool_root: String::new(),
            pool_nullifier_hash: String::new(),
            pool_spend_proof: String::new(),
            liq_recipient_note: String::new(),
        };

        state.relay_queue.lock().unwrap().push(pending);
        log::info!("Note relay queued (batch window)", "cmt", &cmt_preview);
        Json(serde_json::json!({ "ok": true, "queued": true }))
    }

    /// Place a limit order into the CLOB without immediately opening on-chain.
    /// The PendingRelay is stored in limit_relay_store keyed by position_cmt.
    /// When a counter-order crosses, both sides are pushed to relay_queue together.
    async fn handle_http_relay_place_limit(
        State(state): State<HttpState>,
        Json(req): Json<RelayOpenPositionReq>,
    ) -> Json<serde_json::Value> {
        let zeros = "0".repeat(64);
        let portfolio_key = req.portfolio_key.as_deref().unwrap_or(&zeros).to_string();
        let asset_id = req.asset_id.as_deref().unwrap_or(&zeros).to_string();
        let cmt = req.position_cmt.clone();
        let cmt_preview = cmt[..cmt.len().min(16)].to_string();

        // Seal params from stored secrets and annotate with trade metadata.
        let sealed_bytes = match state.store.get(&cmt) {
            Ok(Some(mut secrets)) => {
                secrets.asset_id_hex = Some(asset_id.clone());
                secrets.collateral_amount = req.collateral_amount;
                secrets.protocol = false;
                secrets.is_close = false;
                let _ = state.store.insert(&cmt, &secrets);
                match stellar::seal_from_secrets(&secrets) {
                    Ok(b) => b,
                    Err(e) => return Json(serde_json::json!({ "ok": false, "error": format!("seal: {e}") })),
                }
            }
            Ok(None) => return Json(serde_json::json!({ "ok": false, "error": "secrets not found" })),
            Err(e) => return Json(serde_json::json!({ "ok": false, "error": format!("db: {e}") })),
        };

        let pending = PendingRelay {
            perp: req.perp.clone(),
            orderbook: req.orderbook,
            position_cmt: cmt.clone(),
            sealed_params: sealed_bytes,
            collateral_amount: req.collateral_amount,
            collateral_blinding: req.collateral_blinding,
            settlement_commitment: req.settlement_commitment,
            portfolio_key,
            asset_id,
            commit_proof: req.commit_proof,
            note_cmt: req.note_cmt,
            note_null: req.note_null,
            note_proof: req.note_proof,
            pool_id: String::new(),
            pool_root: String::new(),
            pool_nullifier_hash: String::new(),
            pool_spend_proof: String::new(),
            liq_recipient_note: String::new(),
        };

        // Store relay params for when this limit order is matched
        state.limit_relay_store.lock().unwrap().insert(cmt.clone(), pending);

        // Add to CLOB as a limit order
        let secrets = match state.store.get(&cmt) {
            Ok(Some(s)) => s,
            Ok(None) => return Json(serde_json::json!({ "ok": false, "error": "secrets not found" })),
            Err(e) => return Json(serde_json::json!({ "ok": false, "error": format!("db: {e}") })),
        };
        let order = secrets_to_order(&cmt, &secrets, engine::OrderType::Limit);
        let asset = order.asset;

        let book_fills = {
            let mut books = state.books.write().unwrap();
            let book = books.entry(asset).or_insert_with(engine::OrderBook::new);
            match book.place(order) {
                Ok(fills) => fills,
                Err(e) => return Json(serde_json::json!({ "ok": false, "error": format!("clob: {e}") })),
            }
        };

        // Update position state for each fill before queuing on-chain relays.
        for fill in &book_fills {
            crate::position::apply_fill(&state.store, &req.perp, fill, &state.keys_dir);
        }

        // For each fill: relay whichever sides have PendingRelay.
        // MM orders have no PendingRelay (no on-chain deposit) — user still gets filled.
        let filled = !book_fills.is_empty();
        for fill in &book_fills {
            let maker_pending = state.limit_relay_store.lock().unwrap().remove(&fill.maker_id);
            let taker_pending = state.limit_relay_store.lock().unwrap().remove(&cmt);
            let mut q = state.relay_queue.lock().unwrap();
            match (maker_pending, taker_pending) {
                (Some(maker), Some(taker)) => {
                    // User vs user — open both positions on-chain
                    q.push(maker);
                    q.push(taker);
                    log::info!("limit fill: user vs user, queued both",
                        "maker", &fill.maker_id[..fill.maker_id.len().min(16)],
                        "taker", &cmt_preview, "price", fill.price);
                }
                (None, Some(taker)) => {
                    // MM maker — only open the user's (taker) position
                    q.push(taker);
                    log::info!("limit fill: mm maker, queued taker only",
                        "maker", &fill.maker_id[..fill.maker_id.len().min(16)],
                        "taker", &cmt_preview, "price", fill.price);
                }
                (Some(maker), None) => {
                    // MM taker (shouldn't happen in this path, taker is always the incoming order)
                    q.push(maker);
                }
                (None, None) => {
                    log::error!("limit fill: neither side has relay params",
                        "maker", &fill.maker_id[..fill.maker_id.len().min(16)],
                        "taker", &cmt_preview);
                }
            }
        }

        // Persist book state
        {
            let books = state.books.read().unwrap();
            if let Some(book) = books.get(&asset) {
                if let Err(e) = state.book_store.save_book(asset, book) {
                    log::error!("limit place: failed to persist book", "err", e.to_string());
                }
            }
        }

        log::info!("limit order placed", "cmt", &cmt_preview, "filled", filled);
        Json(serde_json::json!({ "ok": true, "queued": true, "filled": filled }))
    }

    #[derive(serde::Deserialize)]
    struct RelayClosePositionReq {
        perp: String,
        close_cmt: String,
        position_cmt: String,
        position_secret: u64,
        settlement_commitment: String,
    }

    async fn handle_http_relay_close_position(
        State(state): State<HttpState>,
        Json(req): Json<RelayClosePositionReq>,
    ) -> Json<serde_json::Value> {
        // Authorize: caller must know the original position secret.
        let position_secrets = match state.store.get(&req.position_cmt) {
            Ok(Some(s)) => s,
            Ok(None) => return Json(serde_json::json!({ "ok": false, "error": "position secrets not found" })),
            Err(e) => return Json(serde_json::json!({ "ok": false, "error": format!("db: {e}") })),
        };
        if position_secrets.secret != req.position_secret {
            return Json(serde_json::json!({ "ok": false, "error": "invalid position secret" }));
        }

        let mut close_secrets = match state.store.get(&req.close_cmt) {
            Ok(Some(s)) => s,
            Ok(None) => return Json(serde_json::json!({ "ok": false, "error": "close order secrets not found" })),
            Err(e) => return Json(serde_json::json!({ "ok": false, "error": format!("db: {e}") })),
        };
        if !close_secrets.is_close {
            return Json(serde_json::json!({ "ok": false, "error": "close_cmt is not a close order" }));
        }
        if close_secrets.close_position_cmt.as_deref() != Some(&req.position_cmt) {
            return Json(serde_json::json!({ "ok": false, "error": "close order does not target this position" }));
        }

        // Inherit asset id from the position if the close order was created without one.
        if close_secrets.asset_id_hex.is_none() {
            close_secrets.asset_id_hex = position_secrets.asset_id_hex.clone();
        }
        // The close order carries the user's pre-committed settlement destination.
        // We overwrite the stored close secrets settlement commitment so the TEE
        // settlement note is created for the user's chosen destination.
        let _ = state.store.insert(&req.close_cmt, &close_secrets);

        let order_type = if close_secrets.is_market {
            engine::OrderType::Market
        } else {
            engine::OrderType::Limit
        };
        let order = secrets_to_order(&req.close_cmt, &close_secrets, order_type);
        let asset = order.asset;

        let book_fills = {
            let mut books = state.books.write().unwrap();
            let book = books.entry(asset).or_insert_with(engine::OrderBook::new);
            match book.place(order) {
                Ok(fills) => fills,
                Err(e) => return Json(serde_json::json!({ "ok": false, "error": format!("clob: {e}") })),
            }
        };

        for fill in &book_fills {
            crate::position::apply_fill(&state.store, &req.perp, fill, &state.keys_dir);
        }

        // Persist book state
        {
            let books = state.books.read().unwrap();
            if let Some(book) = books.get(&asset) {
                if let Err(e) = state.book_store.save_book(asset, book) {
                    log::error!("close place: failed to persist book", "err", e.to_string());
                }
            }
        }

        Json(serde_json::json!({
            "ok": true,
            "filled": !book_fills.is_empty()
        }))
    }

    #[derive(serde::Deserialize)]
    struct RelayOpenPositionPoolReq {
        perp: String,
        orderbook: String,
        pool_id: String,
        pool_root: String,
        pool_nullifier_hash: String,
        position_cmt: String,
        #[serde(default)]
        sealed_params: Option<String>,
        collateral_amount: i128,
        collateral_blinding: String,
        settlement_commitment: String,
        #[serde(default)]
        portfolio_key: Option<String>,
        #[serde(default)]
        asset_id: Option<String>,
        #[serde(default)]
        liq_recipient_note: Option<String>,
        spend_proof: String,
        commit_proof: String,
    }

    async fn handle_http_relay_open_position_pool(
        State(state): State<HttpState>,
        Json(req): Json<RelayOpenPositionPoolReq>,
    ) -> Json<serde_json::Value> {
        let zeros = "0".repeat(64);
        let portfolio_key = req.portfolio_key.as_deref().unwrap_or(&zeros).to_string();
        let asset_id = req.asset_id.as_deref().unwrap_or(&zeros).to_string();
        let liq_recipient = req.liq_recipient_note.as_deref().unwrap_or(&zeros).to_string();
        let sealed_bytes = hex::decode(req.sealed_params.as_deref().unwrap_or("")).unwrap_or_default();

        // Queue into batch relay for timing-correlation privacy (30s shuffle window)
        let pending = PendingRelay {
            perp: req.perp,
            orderbook: req.orderbook,
            position_cmt: req.position_cmt.clone(),
            sealed_params: sealed_bytes,
            collateral_amount: req.collateral_amount,
            collateral_blinding: req.collateral_blinding,
            settlement_commitment: req.settlement_commitment,
            portfolio_key,
            asset_id,
            commit_proof: req.commit_proof,
            note_cmt: String::new(),
            note_null: String::new(),
            note_proof: String::new(),
            pool_id: req.pool_id,
            pool_root: req.pool_root,
            pool_nullifier_hash: req.pool_nullifier_hash,
            pool_spend_proof: req.spend_proof,
            liq_recipient_note: liq_recipient,
        };

        state.relay_queue.lock().unwrap().push(pending);

        let cmt_preview = req.position_cmt[..req.position_cmt.len().min(16)].to_string();
        log::info!("Pool relay queued (batch window)", "cmt", &cmt_preview);
        Json(serde_json::json!({ "ok": true, "queued": true }))
    }

    #[derive(serde::Deserialize)]
    struct RelayWithdrawToPoolReq {
        perp: String,
        pool_id: String,
        note_cmt: String,
        nullifier: String,
        amount: i128,
        blinding: String,
        new_pool_leaf: String,
        new_pool_root: String,
        remainder_note: String,
        remainder_blinding: String,
        note_spend_proof: String,
        pool_insert_proof: String,
    }

    async fn handle_http_relay_withdraw_to_pool(
        State(state): State<HttpState>,
        Json(req): Json<RelayWithdrawToPoolReq>,
    ) -> Json<serde_json::Value> {
        let cmt_preview = req.note_cmt[..req.note_cmt.len().min(16)].to_string();
        let result = tokio::task::spawn_blocking(move || {
            stellar::relay_withdraw_to_pool(
                &req.perp,
                &req.pool_id,
                &req.note_cmt,
                &req.nullifier,
                req.amount,
                &req.blinding,
                &req.new_pool_leaf,
                &req.new_pool_root,
                &req.remainder_note,
                &req.remainder_blinding,
                &req.note_spend_proof,
                &req.pool_insert_proof,
            )
        })
        .await;

        match result {
            Ok(Ok(tx_hash)) => {
                log::info!("withdraw_to_pool relayed", "note_cmt", &cmt_preview, "tx", &tx_hash[..16]);
                Json(serde_json::json!({ "ok": true, "tx_hash": tx_hash }))
            }
            Ok(Err(e)) => {
                log::error!("withdraw_to_pool failed", "err", e.to_string());
                Json(serde_json::json!({ "ok": false, "error": e.to_string() }))
            }
            Err(e) => Json(serde_json::json!({ "ok": false, "error": format!("task panic: {e}") })),
        }
    }

    #[derive(serde::Deserialize)]
    struct RelayCancelPositionReq {
        perp: String,
        position_cmt: String,
        cancel_nullifier: String,
        cancel_proof: String,   // JSON proof string
        recipient: String,      // Stellar address to receive refunded tokens
    }

    async fn handle_http_relay_cancel_position(
        State(state): State<HttpState>,
        Json(req): Json<RelayCancelPositionReq>,
    ) -> Json<serde_json::Value> {
        let cmt_preview = req.position_cmt[..req.position_cmt.len().min(16)].to_string();
        let keys_dir = state.keys_dir.clone();

        let result = tokio::task::spawn_blocking(move || {
            stellar::relay_cancel_position(
                &req.perp,
                &req.position_cmt,
                &req.cancel_nullifier,
                &req.cancel_proof,
                &req.recipient,
                &keys_dir,
                &state.store,
            )
        })
        .await;

        match result {
            Ok(Ok(tx_hash)) => {
                log::info!("relay: cancel + withdraw complete", "cmt", &cmt_preview, "tx", &tx_hash[..16]);
                Json(serde_json::json!({ "ok": true, "tx_hash": tx_hash }))
            }
            Ok(Err(e)) => {
                log::error!("relay: cancel failed", "cmt", &cmt_preview, "err", e.to_string());
                Json(serde_json::json!({ "ok": false, "error": e.to_string() }))
            }
            Err(e) => Json(serde_json::json!({ "ok": false, "error": format!("task panic: {e}") })),
        }
    }

    async fn handle_http_relay_deposit_note(
        State(state): State<HttpState>,
        Json(body): Json<serde_json::Value>,
    ) -> Json<serde_json::Value> {
        let signed_xdr = match body["signed_xdr"].as_str() {
            Some(x) if !x.is_empty() => x.to_string(),
            _ => return Json(serde_json::json!({ "ok": false, "error": "missing signed_xdr" })),
        };
        state.deposit_queue.lock().unwrap().push(PendingDeposit { signed_xdr });
        log::info!("deposit relay queued (batch window)");
        Json(serde_json::json!({ "ok": true, "queued": true }))
    }

    async fn handle_http_note_amount(
        State(state): State<HttpState>,
        axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
    ) -> Json<serde_json::Value> {
        let cmt = match params.get("cmt") {
            Some(c) => c.clone(),
            None => return Json(serde_json::json!({ "ok": false, "error": "missing cmt" })),
        };
        match state.store.get_note_amount(&cmt) {
            Ok(Some(note)) => Json(serde_json::json!({
                "ok": true,
                "amount": note.amount,
                "blinding": hex::encode(note.blinding),
            })),
            Ok(None) => Json(serde_json::json!({ "ok": false, "error": "note not found" })),
            Err(e) => Json(serde_json::json!({ "ok": false, "error": e.to_string() })),
        }
    }

    #[derive(serde::Deserialize)]
    struct WithdrawSettlementReq {
        perp: String,
        position_cmt: String,
        recipient: String,
    }

    async fn handle_http_relay_withdraw_settlement(
        State(state): State<HttpState>,
        Json(req): Json<WithdrawSettlementReq>,
    ) -> Json<serde_json::Value> {
        let cmt_preview = req.position_cmt[..req.position_cmt.len().min(16)].to_string();
        let keys_dir = state.keys_dir.clone();

        let result = tokio::task::spawn_blocking(move || -> anyhow::Result<String> {
            // 1. Look up settlement note commitment for this position
            let note_cmt = state
                .store
                .get_settlement_note(&req.position_cmt)?
                .ok_or_else(|| anyhow::anyhow!("no settlement note found for position"))?;

            // 2. Look up NoteAmount
            let note = state
                .store
                .get_note_amount(&note_cmt)?
                .ok_or_else(|| anyhow::anyhow!("note amount not found"))?;

            // 3. Regenerate note nullifier from amount + note_secret
            let (_, note_null_hex) =
                crate::proof::compute_note_cmt_hex(note.amount as u64, note.note_secret);
            let blinding_hex = hex::encode(note.blinding);

            // 4. Generate ZK note-spend proof
            let note_proof_out =
                crate::proof::gen_note_proof(&keys_dir, note.amount as u64, note.note_secret)?;
            let proof_json = serde_json::json!({
                "a": note_proof_out.proof.proof.a,
                "b": note_proof_out.proof.proof.b,
                "c": note_proof_out.proof.proof.c,
            })
            .to_string();

            // 5. Submit withdraw_note to transfer tokens to recipient
            stellar::relay_withdraw_note(
                &req.perp,
                &note_cmt,
                &note_null_hex,
                &req.recipient,
                note.amount,
                &blinding_hex,
                &proof_json,
            )
        })
        .await;

        match result {
            Ok(Ok(tx_hash)) => {
                log::info!(
                    "relay: withdraw-settlement complete",
                    "cmt",
                    &cmt_preview,
                    "tx",
                    &tx_hash[..16]
                );
                Json(serde_json::json!({ "ok": true, "tx_hash": tx_hash }))
            }
            Ok(Err(e)) => {
                log::error!(
                    "relay: withdraw-settlement failed",
                    "cmt",
                    &cmt_preview,
                    "err",
                    e.to_string()
                );
                Json(serde_json::json!({ "ok": false, "error": e.to_string() }))
            }
            Err(e) => Json(serde_json::json!({
                "ok": false,
                "error": format!("task panic: {e}")
            })),
        }
    }

    async fn handle_http_position_tx(
        State(state): State<HttpState>,
        axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
    ) -> Json<serde_json::Value> {
        let cmt = match params.get("cmt") {
            Some(c) => c.clone(),
            None => return Json(serde_json::json!({ "ok": false, "error": "missing cmt" })),
        };
        match state.store.get_position_tx(&cmt) {
            Ok(Some(tx_hash)) => Json(serde_json::json!({ "ok": true, "tx_hash": tx_hash })),
            Ok(None) => Json(serde_json::json!({ "ok": true, "tx_hash": null })),
            Err(e) => Json(serde_json::json!({ "ok": false, "error": e.to_string() })),
        }
    }

    #[derive(serde::Deserialize)]
    struct SettleReq {
        perp: String,
        commitment: String,
        cmd: String,
        amount: Option<u64>,
        source: Option<String>,
    }

    async fn handle_http_settle(
        State(state): State<HttpState>,
        Json(req): Json<SettleReq>,
    ) -> Json<serde_json::Value> {
        let source = req.source.clone().unwrap_or_else(|| "e2e".to_string());
        let request = Request {
            cmd: req.cmd.clone(),
            perp: Some(req.perp),
            cmt: Some(req.commitment),
            amount: req.amount,
            source: Some(source),
            ..Default::default()
        };
        let resp = handle_settle(&state.store, &state.keys_dir, &request);
        Json(serde_json::json!(resp))
    }
}
