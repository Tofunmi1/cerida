use crate::{db, engine, log, proof, stellar};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
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
    log::info!("═══ Starting TEE Match Server ═══",
        "version", env!("CARGO_PKG_VERSION"),
        "listen_addr", addr
    );

    let start = Instant::now();
    let store = db::SecretStore::open(&db_path)?;
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
    let keys = Arc::new(keys_dir);

    for stream in listener.incoming() {
        let store = store.clone();
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
                    let resp = Response {
                        ok: false, commitment: None,
                        match_price: None, match_size: None,
                        nullifier_a: None, nullifier_b: None,
                        error: Some(format!("invalid JSON: {e}")),
                    };
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
                other => {
                    log::warning!("Unknown command", "cmd", other, "peer", &peer);
                    Response {
                        ok: false, commitment: None,
                        match_price: None, match_size: None,
                        nullifier_a: None, nullifier_b: None,
                        error: Some(format!("unknown cmd: {other}")),
                    }
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
    let secrets = db::OrderSecrets {
        side: req.side.unwrap_or(0),
        price: req.price.unwrap_or(0),
        size: req.size.unwrap_or(0),
        leverage: req.leverage.unwrap_or(1),
        asset: req.asset.unwrap_or(0),
        nonce: req.nonce.unwrap_or(0),
        secret: req.secret.unwrap_or(0),
    };

    log::info!("Initializing new order commitment",
        "side", secrets.side,
        "price", secrets.price,
        "size", secrets.size,
        "leverage", secrets.leverage,
        "asset", secrets.asset,
        "nonce", secrets.nonce
    );

    log::debug!("Generating ZK commitment proof via Circom",
        "circuit", "order_commitment",
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

    Response {
        ok: true,
        commitment: Some(cmt_hex),
        match_price: None, match_size: None,
        nullifier_a: None, nullifier_b: None,
        error: None,
    }
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

    log::debug!("Generating placement proof via Circom",
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

    Response {
        ok: true, commitment: None,
        match_price: None, match_size: None,
        nullifier_a: None, nullifier_b: None,
        error: None,
    }
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

    log::debug!("Loading order A from DB", "cmt_a", &cmt_a[..16]);
    let a = match store.get(cmt_a) {
        Ok(Some(s)) => s,
        Ok(None) => {
            log::error!("Order A secrets not found in DB", "cmt_a", &cmt_a[..16]);
            return err(format!("secrets not found for cmt_a"));
        }
        Err(e) => {
            log::error!("DB lookup failed for order A", "cmt_a", &cmt_a[..16], "err", e.to_string());
            return err(e);
        }
    };

    log::debug!("Loading order B from DB", "cmt_b", &cmt_b[..16]);
    let b = match store.get(cmt_b) {
        Ok(Some(s)) => s,
        Ok(None) => {
            log::error!("Order B secrets not found in DB", "cmt_b", &cmt_b[..16]);
            return err(format!("secrets not found for cmt_b"));
        }
        Err(e) => {
            log::error!("DB lookup failed for order B", "cmt_b", &cmt_b[..16], "err", e.to_string());
            return err(e);
        }
    };

    log::info!("Both orders loaded from DB",
        "order_a",
        format!("side={} price={} size={} leverage={}", a.side, a.price, a.size, a.leverage),
        "order_b",
        format!("side={} price={} size={} leverage={}", b.side, b.price, b.size, b.leverage),
        "db_lookup_time", log::duration_secs(&start.elapsed())
    );

    log::info!("Running matching engine", "side_a", a.side, "price_a", a.price, "side_b", b.side, "price_b", b.price);
    let params = match engine::find_match(&a, &b) {
        Some(p) => p,
        None => {
            log::warning!("Orders are not matchable",
                "price_a", a.price, "side_a", a.side,
                "price_b", b.price, "side_b", b.side
            );
            return err("orders are not matchable");
        }
    };

    log::info!("Match parameters computed",
        "match_price", params.match_price,
        "match_size", params.match_size,
        "match_notional", params.match_price as u128 * params.match_size as u128
    );

    log::debug!("Generating Groth16 match proof via Circom",
        "circuit", "order_match",
        "side_a", a.side, "price_a", a.price,
        "side_b", b.side, "price_b", b.price,
        "mp", params.match_price,
        "ms", params.match_size
    );

    let out = match proof::gen_match_proof(keys, &a, &b, params.match_price, params.match_size) {
        Ok(o) => o,
        Err(e) => {
            log::error!("Match proof generation failed", "err", e.to_string());
            return err(e);
        }
    };

    let proof_size = out.proof.a.len() + out.proof.b.len() + out.proof.c.len();
    log::info!("ZK match proof generated",
        "proof_a", format!("{} hex chars", out.proof.a.len()),
        "proof_b", format!("{} hex chars", out.proof.b.len()),
        "proof_c", format!("{} hex chars", out.proof.c.len()),
        "proof_total", log::bytes_label(proof_size / 2),
        "proof_gen_time", log::duration_secs(&start.elapsed())
    );

    log::warning!("Submitting match to Soroban testnet",
        "contract", &perp[..8],
        "source", source,
        "cmt_a", log::hex_snippet(cmt_a, 10),
        "cmt_b", log::hex_snippet(cmt_b, 10),
        "match_price", params.match_price,
        "match_size", params.match_size
    );

    if let Err(e) = stellar::submit_match(perp, source, cmt_a, cmt_b, &out) {
        log::error!("On-chain match submission failed",
            "contract", &perp[..8],
            "err", e.to_string()
        );
        return err(e);
    }

    log::info!("Match confirmed on-chain",
        "elapsed", log::duration_secs(&start.elapsed())
    );

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
    Response {
        ok: false, commitment: None,
        match_price: None, match_size: None,
        nullifier_a: None, nullifier_b: None,
        error: Some(s.to_string()),
    }
}
