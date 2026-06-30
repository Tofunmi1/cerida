use crate::{db, engine, log, proof, stellar};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

static NEXT_REQ_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Deserialize, Debug)]
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
}

#[derive(Serialize, Default)]
struct Response {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    commitment: Option<String>,
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

struct MatchResultData {
    match_price: String,
    match_size: String,
    nullifier_a: String,
    nullifier_b: String,
}

#[derive(Serialize)]
struct LevelJson {
    price: u64,
    size: u64,
    orders: usize,
}

pub fn run(addr: &str, db_path: PathBuf, keys_dir: PathBuf) -> Result<()> {
    log::info!("═══ Starting TEE Match Server ═══",
        "version", env!("CARGO_PKG_VERSION"),
        "listen_addr", addr
    );

    let start = Instant::now();
    let store = db::SecretStore::open(&db_path)?;
    let book = engine::OrderBook::new();
    let listener = TcpListener::bind(addr)?;
    log::info!("TCP listener bound",
        "addr", addr,
        "took", log::duration_secs(&start.elapsed())
    );

    let local_addr = listener.local_addr().ok();
    log::info!("Awaiting client connections",
        "addr", format!("{}", local_addr.as_ref().map(|a| a.to_string()).unwrap_or_default()),
        "keys_dir", format!("{}", keys_dir.display())
    );

    let store = Arc::new(store);
    let book = Arc::new(Mutex::new(book));
    let keys = Arc::new(keys_dir);

    for stream in listener.incoming() {
        let store = store.clone();
        let book = book.clone();
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

            let peer = stream.peer_addr().map(|a| a.to_string()).unwrap_or_default();
            log::debug!("New TCP connection",
                "peer", &peer,
                "local_port", local_addr.as_ref().map(|a| a.port()).unwrap_or(0)
            );

            let mut reader = std::io::BufReader::new(&stream);
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => {
                    log::debug!("Client disconnected without sending request", "peer", &peer);
                    return;
                }
                Ok(n) => {
                    log::debug!("Raw request received",
                        "peer", &peer,
                        "bytes", n,
                        "preview", &line[..line.len().min(120)]
                    );
                }
            }

            let req: Request = match serde_json::from_str(&line) {
                Ok(r) => r,
                Err(e) => {
                    log::error!("Failed to parse request JSON",
                        "peer", &peer,
                        "raw", &line[..line.len().min(200)],
                        "err", e.to_string()
                    );
                    let resp = Response { ok: false, error: Some(format!("invalid JSON: {e}")), ..Default::default() };
                    let _ = writeln!(&mut stream, "{}", serde_json::to_string(&resp).unwrap());
                    return;
                }
            };

            log::info!("Processing command",
                "cmd", &req.cmd,
                "peer", &peer,
                "req_id", NEXT_REQ_ID.fetch_add(1, Ordering::Relaxed)
            );

            let resp = match req.cmd.as_str() {
                "init" => handle_init(&store, &keys, &req),
                "commit-proof" => handle_commit_proof(&store, &keys, &req),
                "match" => handle_match(&store, &keys, &req),
                "place" => handle_place(&store, &book, &keys, &req),
                "cancel" => handle_cancel(&store, &book, &keys, &req),
                "market" => handle_market(&store, &book, &keys, &req),
                "get_market" => handle_get_market(&book),
                other => {
                    log::warning!("Unknown command", "cmd", other, "peer", &peer);
                    Response { ok: false, error: Some(format!("unknown cmd: {other}")), ..Default::default() }
                }
            };

            let json = serde_json::to_string(&resp).unwrap();
            let _ = writeln!(&mut stream, "{json}");

            let elapsed = conn_start.elapsed();
            if resp.ok {
                log::info!("Command completed",
                    "peer", &peer,
                    "cmd", &req.cmd,
                    "elapsed", log::duration_secs(&elapsed)
                );
            } else {
                log::error!("Command failed",
                    "peer", &peer,
                    "cmd", &req.cmd,
                    "elapsed", log::duration_secs(&elapsed),
                    "error", resp.error.as_deref().unwrap_or("unknown")
                );
            }

            log::debug!("Connection closed",
                "peer", &peer,
                "duration", log::duration_secs(&elapsed)
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
    let side = match raw_side { 0 | 3 => 0, _ => 1 };
    let secrets = db::OrderSecrets {
        side,
        price: req.price.unwrap_or(0),
        size: req.size.unwrap_or(0),
        leverage: req.leverage.unwrap_or(1),
        asset: req.asset.unwrap_or(0),
        nonce: req.nonce.unwrap_or(0),
        secret: req.secret.unwrap_or(0),
        is_market,
    };

    log::info!("Initializing new order commitment",
        "raw_side", raw_side,
        "normalized_side", secrets.side,
        "is_market", secrets.is_market,
        "price", secrets.price,
        "size", secrets.size,
        "leverage", secrets.leverage,
        "asset", secrets.asset,
        "nonce", secrets.nonce
    );

    log::debug!("Generating commitment proof via native Rust circuits",
        "side", secrets.side,
        "price", secrets.price,
        "size", secrets.size
    );

    let out = match proof::gen_commitment_proof(keys, &secrets) {
        Ok(o) => o,
        Err(e) => {
            log::error!("Commitment proof generation failed", "err", e.to_string());
            return err(e);
        }
    };

    let cmt_hex = format!("{:0>64x}", out.public_inputs[0].parse::<num_bigint::BigUint>().unwrap());
    log::info!("Commitment computed successfully",
        "commitment", log::hex_snippet(&cmt_hex, 12),
        "full", &cmt_hex,
        "side", secrets.side,
        "price", secrets.price
    );

    if let Err(e) = store.insert(&cmt_hex, &secrets) {
        log::error!("Failed to store secrets in DB",
            "cmt", &cmt_hex[..16],
            "err", e.to_string()
        );
        return err(e);
    }

    log::info!("Order initialized and persisted",
        "commitment", log::hex_snippet(&cmt_hex, 12),
        "took", log::duration_secs(&start.elapsed())
    );

    Response { ok: true, commitment: Some(cmt_hex), ..Default::default() }
}

fn handle_commit_proof(store: &db::SecretStore, keys: &PathBuf, req: &Request) -> Response {
    let start = Instant::now();
    let cmt = match req.cmt.as_ref() {
        Some(c) => c,
        None => return err("missing cmt"),
    };
    let out_path = match req.out.as_ref() {
        Some(p) => p,
        None => return err("missing out path"),
    };

    log::info!("Generating commitment proof for on-chain placement",
        "commitment", log::hex_snippet(cmt, 12),
        "out_path", format!("{}", out_path.display())
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

    log::debug!("Generating placement proof via native Rust circuits",
        "side", secrets.side,
        "price", secrets.price,
        "size", secrets.size
    );

    let result = match proof::gen_commitment_proof(keys, &secrets) {
        Ok(r) => r,
        Err(e) => {
            log::error!("Commitment proof generation failed", "err", e.to_string());
            return err(e);
        }
    };

    let proof_json = serde_json::json!({"a": result.proof.a, "b": result.proof.b, "c": result.proof.c});
    match std::fs::write(out_path, serde_json::to_string(&proof_json).unwrap()) {
        Ok(_) => {
            let meta = std::fs::metadata(out_path).ok();
            log::info!("Commitment proof written to disk",
                "path", format!("{}", out_path.display()),
                "size", log::bytes_label(meta.map(|m| m.len() as usize).unwrap_or(0)),
                "proof_a_size", result.proof.a.len() / 2,
                "proof_b_size", result.proof.b.len() / 2,
                "took", log::duration_secs(&start.elapsed())
            );
        }
        Err(e) => {
            log::error!("Failed to write proof file",
                "path", format!("{}", out_path.display()),
                "err", e.to_string()
            );
            return err(e);
        }
    }

    Response { ok: true, ..Default::default() }
}

fn handle_match(store: &db::SecretStore, keys: &PathBuf, req: &Request) -> Response {
    let start = Instant::now();
    let cmt_a = match req.cmt_a.as_ref() {
        Some(c) => c,
        None => return err("missing cmt_a"),
    };
    let cmt_b = match req.cmt_b.as_ref() {
        Some(c) => c,
        None => return err("missing cmt_b"),
    };
    let perp = match req.perp.as_ref() {
        Some(p) => p,
        None => return err("missing perp"),
    };
    let source = match req.source.as_ref() {
        Some(s) => s,
        None => return err("missing source"),
    };

    log::info!("═══ Processing match request ═══",
        "cmt_a", log::hex_snippet(cmt_a, 12),
        "cmt_b", log::hex_snippet(cmt_b, 12),
        "perp_contract", &perp[..8],
        "source", source
    );

    // Direct /match is explicit, not from CLOB — no book rollback needed
    match do_match(store, keys, cmt_a, cmt_b, perp, source, engine::Side::Bid, 0, 0, None) {
        Some(r) => {
            log::info!("Match confirmed on-chain",
                "elapsed", log::duration_secs(&start.elapsed())
            );
            Response {
                ok: true,
                match_price: Some(r.match_price),
                match_size: Some(r.match_size),
                nullifier_a: Some(r.nullifier_a),
                nullifier_b: Some(r.nullifier_b),
                ..Default::default()
            }
        }
        None => {
            log::error!("Match failed",
                "cmt_a", log::hex_snippet(cmt_a, 12),
                "cmt_b", log::hex_snippet(cmt_b, 12),
                "elapsed", log::duration_secs(&start.elapsed())
            );
            err("match failed")
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

fn secrets_to_order(cmt: &str, secrets: &db::OrderSecrets, order_type: engine::OrderType) -> engine::Order {
    let price = match order_type {
        engine::OrderType::Market => 0,
        _ => secrets.price,
    };
    engine::Order {
        id: cmt.to_string(),
        side: if secrets.side == 0 { engine::Side::Bid } else { engine::Side::Ask },
        price,
        size: secrets.size,
        remaining: secrets.size,
        timestamp_ns: engine::now_nanos(),
        order_type,
    }
}

fn handle_place(store: &db::SecretStore, book: &Mutex<engine::OrderBook>, keys: &PathBuf, req: &Request) -> Response {
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
    // Patch in stop_price for stop orders
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
    log::info!("handle_place: placing order",
        "cmt", engine::short_id(cmt),
        "secrets_side", secrets.side,
        "secrets_price", secrets.price,
        "secrets_size", secrets.size,
        "order_side", order.side as u64,
        "order_price", order.price,
        "order_size", order.size,
        "order_type", format!("{:?}", order.order_type)
    );

    // Do CLOB match and collect book state, then release lock
    let (fills, best_bid, best_ask, spread, order_count) = {
        let mut book = book.lock().unwrap();
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

    let perp = req.perp.as_ref();
    let source = req.source.as_ref();

    let fill_json: Vec<FillJson> = fills.into_iter().map(|f| {
        let mut fj = FillJson {
            maker_id: engine::short_id(&f.maker_id).to_string(),
            price: f.price,
            size: f.size,
            match_price: None,
            match_size: None,
            nullifier_a: None,
            nullifier_b: None,
        };
        if let (Some(perp), Some(source)) = (perp, source) {
            let maker_side = f.taker_side.opposite();
            if let Some(result) = do_match(store, keys, cmt, &f.maker_id, perp, source, maker_side, f.price, f.size, Some(book)) {
                fj.match_price = Some(result.match_price);
                fj.match_size = Some(result.match_size);
                fj.nullifier_a = Some(result.nullifier_a);
                fj.nullifier_b = Some(result.nullifier_b);
            }
        }
        fj
    }).collect();

    log::info!("Order placed in book",
        "cmt", engine::short_id(cmt),
        "type", ot_str,
        "fills", fill_json.len(),
        "auto_matched", perp.is_some(),
        "took", log::duration_secs(&start.elapsed())
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

fn handle_cancel(store: &db::SecretStore, book: &Mutex<engine::OrderBook>, keys: &PathBuf, req: &Request) -> Response {
    let cmt = match req.cmt.as_ref() {
        Some(c) => c,
        None => return err("missing cmt"),
    };
    let perp = match req.perp.as_ref() {
        Some(p) => p,
        None => return err("missing perp"),
    };
    let orderbook = match req.orderbook.as_ref() {
        Some(o) => o,
        None => return err("missing orderbook"),
    };
    let owner = match req.owner.as_ref() {
        Some(o) => o,
        None => return err("missing owner"),
    };
    let source = req.source.as_deref().unwrap_or("e2e");

    let secrets = match store.get(cmt) {
        Ok(Some(s)) => s,
        Ok(None) => return err(format!("secrets not found for {cmt}")),
        Err(e) => return err(format!("db error: {e}")),
    };

    let out = match proof::gen_cancel_proof(keys, &secrets) {
        Ok(o) => o,
        Err(e) => return err(format!("cancel proof generation failed: {e}")),
    };

    let nullifier = format!("{:0>64x}", out.public_inputs[0].parse::<num_bigint::BigUint>().unwrap());

    if let Err(e) = stellar::submit_cancel(orderbook, perp, owner, cmt, &nullifier, &out) {
        log::error!("Cancel on-chain submission failed",
            "cmt", &cmt[..16],
            "err", e.to_string()
        );
        return err(format!("cancel on-chain submission failed: {e}"));
    }

    // Remove from CLOB book (best-effort; might already be filled/matched)
    let mut book = book.lock().unwrap();
    match book.cancel(cmt) {
        Ok(_) => {}
        Err(_) => log::warning!("Cancel: order not in CLOB book", "cmt", &cmt[..16]),
    }

    log::info!("Order cancelled on-chain and CLOB",
        "cmt", &cmt[..16],
        "nullifier", &nullifier[..16]
    );

    Response {
        ok: true,
        ..Default::default()
    }
}

fn handle_market(store: &db::SecretStore, book: &Mutex<engine::OrderBook>, keys: &PathBuf, req: &Request) -> Response {
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
    log::info!("handle_market: placing order",
        "cmt", engine::short_id(cmt),
        "secrets_side", secrets.side,
        "secrets_price", secrets.price,
        "secrets_size", secrets.size,
        "order_side", order.side as u64,
        "order_price", order.price,
        "order_size", order.size
    );

    // Do CLOB match and collect book state, then release lock
    let (fills, best_bid, best_ask, spread, order_count) = {
        let mut book = book.lock().unwrap();
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

    let perp = req.perp.as_ref();
    let source = req.source.as_ref();

    let fill_json: Vec<FillJson> = fills.into_iter().map(|f| {
        let mut fj = FillJson {
            maker_id: engine::short_id(&f.maker_id).to_string(),
            price: f.price,
            size: f.size,
            match_price: None,
            match_size: None,
            nullifier_a: None,
            nullifier_b: None,
        };
        if let (Some(perp), Some(source)) = (perp, source) {
            let maker_side = f.taker_side.opposite();
            if let Some(result) = do_match(store, keys, cmt, &f.maker_id, perp, source, maker_side, f.price, f.size, Some(book)) {
                fj.match_price = Some(result.match_price);
                fj.match_size = Some(result.match_size);
                fj.nullifier_a = Some(result.nullifier_a);
                fj.nullifier_b = Some(result.nullifier_b);
            }
        }
        fj
    }).collect();

    log::info!("Market order executed",
        "cmt", engine::short_id(cmt),
        "fills", fill_json.len(),
        "auto_matched", perp.is_some(),
        "took", log::duration_secs(&start.elapsed())
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

fn handle_get_market(book: &Mutex<engine::OrderBook>) -> Response {
    let book = book.lock().unwrap();
    Response {
        ok: true,
        best_bid: book.best_bid().map(|(p, s)| format!("{p}x{s}")),
        best_ask: book.best_ask().map(|(p, s)| format!("{p}x{s}")),
        spread: book.spread(),
        order_count: Some(book.order_count()),
        depth: Some(book.depth(engine::Side::Bid, 5).iter().map(|&(p, s, o)| LevelJson { price: p, size: s, orders: o }).collect()),
        bids: Some(book.depth(engine::Side::Bid, 10).iter().map(|&(p, s, o)| LevelJson { price: p, size: s, orders: o }).collect()),
        asks: Some(book.depth(engine::Side::Ask, 10).iter().map(|&(p, s, o)| LevelJson { price: p, size: s, orders: o }).collect()),
        ..Default::default()
    }
}

// ── Auto-match helper (proof + on-chain submission) ──────────────────────
fn do_match(
    store: &db::SecretStore,
    keys: &PathBuf,
    cmt_a: &str,
    cmt_b: &str,
    perp: &str,
    source: &str,
    maker_side: engine::Side,
    maker_price: u64,
    maker_size: u64,
    book: Option<&Mutex<engine::OrderBook>>,
) -> Option<MatchResultData> {
    let a = store.get(cmt_a).ok()??;
    let b = store.get(cmt_b).ok()??;

    let params = engine::find_match(&a, &b)?;

    let out = match proof::gen_match_proof(keys, &a, &b, params.match_price, params.match_size) {
        Ok(o) => o,
        Err(e) => {
            log::error!("Auto-match: proof generation failed", "cmt_a", &cmt_a[..16], "err", e.to_string());
            // Restore maker order to CLOB book (if applicable)
            if let Some(b) = book {
                b.lock().unwrap().restore_order(cmt_b, maker_side, maker_price, maker_size);
            }
            return None;
        }
    };

    if let Err(e) = stellar::submit_match(perp, source, cmt_a, cmt_b, &out) {
        log::error!("Auto-match: on-chain submission failed", "cmt_a", &cmt_a[..16], "err", e.to_string());
        // Restore maker order to CLOB book (if applicable)
        if let Some(b) = book {
            b.lock().unwrap().restore_order(cmt_b, maker_side, maker_price, maker_size);
        }
        return None;
    }

    let hex = |i: usize| -> String {
        format!("{:0>64x}", out.public_inputs[i].parse::<num_bigint::BigUint>().unwrap())
    };

    Some(MatchResultData {
        match_price: hex(2),
        match_size: hex(3),
        nullifier_a: hex(4),
        nullifier_b: hex(5),
    })
}

fn err(s: impl std::fmt::Display) -> Response {
    Response { ok: false, error: Some(s.to_string()), ..Default::default() }
}
