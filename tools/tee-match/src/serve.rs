use crate::{db, engine, log, proof, stellar};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Deserialize)]
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
    cmt_a: Option<String>,
    cmt_b: Option<String>,
    source: Option<String>,
}

#[derive(Serialize)]
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
    error: Option<String>,
}

pub fn run(addr: &str, db_path: PathBuf, keys_dir: PathBuf) -> Result<()> {
    let store = db::SecretStore::open(&db_path)?;
    let listener = TcpListener::bind(addr)?;
    log::info!("Bound TCP listener", "addr", addr);
    log::debug!("Database opened", "path", format!("{}", db_path.display()));
    log::info!("Awaiting match requests on port 9720");

    let store = Arc::new(store);
    let keys = Arc::new(keys_dir);

    for stream in listener.incoming() {
        let store = store.clone();
        let keys = keys.clone();
        std::thread::spawn(move || {
            use std::io::{BufRead, Write};
            let start = std::time::Instant::now();

            let mut stream = match stream {
                Ok(s) => s,
                Err(e) => {
                    log::error!("TCP accept failed", "err", e);
                    return;
                }
            };

            let peer = stream.peer_addr().map(|a| a.to_string()).unwrap_or_default();
            log::debug!("New TCP connection established", "peer", &peer);

            let mut reader = std::io::BufReader::new(&stream);
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => {
                    log::debug!("Client disconnected without sending request", "peer", &peer);
                    return;
                }
                Ok(n) => log::debug!("Received request", "peer", &peer, "bytes", n),
            }

            let req: Request = match serde_json::from_str(&line) {
                Ok(r) => r,
                Err(e) => {
                    log::error!("Failed to parse request JSON", "peer", &peer, "err", &e);
                    let resp = Response { ok: false, commitment: None, match_price: None, match_size: None, nullifier_a: None, nullifier_b: None, error: Some(format!("invalid JSON: {e}")) };
                    let _ = writeln!(&mut stream, "{}", serde_json::to_string(&resp).unwrap());
                    return;
                }
            };

            log::info!("Processing request", "cmd", &req.cmd, "peer", &peer);

            let resp = match req.cmd.as_str() {
                "init" => handle_init(&store, &keys, &req),
                "commit-proof" => handle_commit_proof(&store, &keys, &req),
                "match" => handle_match(&store, &keys, &req),
                other => {
                    log::warning!("Unknown command received", "cmd", other, "peer", &peer);
                    Response { ok: false, commitment: None, match_price: None, match_size: None, nullifier_a: None, nullifier_b: None, error: Some(format!("unknown cmd: {other}")) }
                }
            };

            let json = serde_json::to_string(&resp).unwrap();
            let _ = writeln!(&mut stream, "{json}");

            let elapsed = start.elapsed();
            if resp.ok {
                log::info!("Request completed", "peer", &peer, "cmd", &req.cmd, "elapsed", format!("{:.3}s", elapsed.as_secs_f64()));
            } else {
                log::error!("Request failed", "peer", &peer, "cmd", &req.cmd, "elapsed", format!("{:.3}s", elapsed.as_secs_f64()), "error", resp.error.as_deref().unwrap_or("unknown"));
            }

            log::debug!("Connection closed", "peer", &peer, "duration", format!("{:.3}s", elapsed.as_secs_f64()));
        });
    }
    Ok(())
}

fn handle_init(store: &db::SecretStore, keys: &PathBuf, req: &Request) -> Response {
    let secrets = db::OrderSecrets {
        side: req.side.unwrap_or(0),
        price: req.price.unwrap_or(0),
        size: req.size.unwrap_or(0),
        leverage: req.leverage.unwrap_or(1),
        asset: req.asset.unwrap_or(0),
        nonce: req.nonce.unwrap_or(0),
        secret: req.secret.unwrap_or(0),
    };
    log::info!("Initializing new order", "side", secrets.side, "price", secrets.price, "nonce", secrets.nonce);
    log::debug!("Generating commitment via Circom order_commitment circuit");
    let out = match proof::gen_commitment_proof(keys, &secrets) {
        Ok(o) => o,
        Err(e) => return err(e),
    };
    let cmt_hex = format!("{:0>64x}", out.public_inputs[0].parse::<num_bigint::BigUint>().unwrap());
    if let Err(e) = store.insert(&cmt_hex, &secrets) {
        return err(e);
    }
    log::info!("Order stored in DB", "commitment", &cmt_hex[..16]);
    Response { ok: true, commitment: Some(cmt_hex), match_price: None, match_size: None, nullifier_a: None, nullifier_b: None, error: None }
}

fn handle_commit_proof(store: &db::SecretStore, keys: &PathBuf, req: &Request) -> Response {
    let cmt = match req.cmt.as_ref() {
        Some(c) => c,
        None => return err("missing cmt"),
    };
    let out_path = match req.out.as_ref() {
        Some(p) => p,
        None => return err("missing out path"),
    };
    log::info!("Generating commitment proof for on-chain placement", "cmt", &cmt[..16]);
    log::debug!("Looking up secrets in DB", "cmt", &cmt[..16]);
    let secrets = match store.get(cmt) {
        Ok(Some(s)) => s,
        Ok(None) => return err(format!("secrets not found for {cmt}")),
        Err(e) => return err(e),
    };
    log::debug!("Proving commitment circuit via Circom");
    let result = match proof::gen_commitment_proof(keys, &secrets) {
        Ok(r) => r,
        Err(e) => return err(e),
    };
    let proof_json = serde_json::json!({"a": result.proof.a, "b": result.proof.b, "c": result.proof.c});
    if let Err(e) = std::fs::write(out_path, serde_json::to_string(&proof_json).unwrap()) {
        return err(e);
    }
    log::info!("Commitment proof written", "path", format!("{}", out_path.display()));
    Response { ok: true, commitment: None, match_price: None, match_size: None, nullifier_a: None, nullifier_b: None, error: None }
}

fn handle_match(store: &db::SecretStore, keys: &PathBuf, req: &Request) -> Response {
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

    log::info!("Loading order pair from DB", "cmt_a", &cmt_a[..16], "cmt_b", &cmt_b[..16]);
    let a = match store.get(cmt_a) {
        Ok(Some(s)) => s,
        Ok(None) => return err(format!("secrets not found for cmt_a")),
        Err(e) => return err(e),
    };
    let b = match store.get(cmt_b) {
        Ok(Some(s)) => s,
        Ok(None) => return err(format!("secrets not found for cmt_b")),
        Err(e) => return err(e),
    };

    log::info!("Running matching engine", "side_a", a.side, "price_a", a.price, "side_b", b.side, "price_b", b.price);
    let params = match engine::find_match(&a, &b) {
        Some(p) => p,
        None => return err("orders are not matchable"),
    };
    log::info!("Match parameters computed", "match_price", params.match_price, "match_size", params.match_size);

    log::debug!("Generating Groth16 match proof via Circom");
    let out = match proof::gen_match_proof(keys, &a, &b, params.match_price, params.match_size) {
        Ok(o) => o,
        Err(e) => return err(e),
    };
    let proof_size = out.proof.a.len() + out.proof.b.len() + out.proof.c.len();
    log::info!("ZK match proof generated", "proof_size", format!("{proof_size} B"));

    log::warning!("Submitting match to Soroban testnet", "contract", &perp[..8], "source", source);
    if let Err(e) = stellar::submit_match(perp, source, cmt_a, cmt_b, &out) {
        return err(e);
    }
    log::info!("Match transaction confirmed on-chain");

    let hex = |i: usize| -> String {
        format!("{:0>64x}", out.public_inputs[i].parse::<num_bigint::BigUint>().unwrap())
    };
    Response {
        ok: true,
        commitment: None,
        match_price: Some(hex(2)),
        match_size: Some(hex(3)),
        nullifier_a: Some(hex(4)),
        nullifier_b: Some(hex(5)),
        error: None,
    }
}

fn err(s: impl std::fmt::Display) -> Response {
    Response { ok: false, commitment: None, match_price: None, match_size: None, nullifier_a: None, nullifier_b: None, error: Some(s.to_string()) }
}
