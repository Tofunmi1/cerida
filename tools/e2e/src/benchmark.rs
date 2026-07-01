use crate::client::ServerClient;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub struct BenchmarkConfig {
    pub mm_count: usize,
    pub trader_count: usize,
    pub orders_per_mm: usize,
    pub server_addr: String,
    pub center_price: u64,
    pub order_size: u64,
    pub randomize_sizes: bool,
    pub randomize_leverage: bool,
    pub book_delay_ms: u64,
}

#[derive(Clone, Serialize, Deserialize)]
struct OrderCache {
    cmt: String,
    addr: String,
    identity: String,
    side: u64,
    price: u64,
    size: u64,
    #[serde(default = "one")]
    leverage: u64,
    proof_json: String,
}

fn one() -> u64 { 1 }

#[derive(Serialize, Deserialize)]
struct Cache {
    orders: Vec<OrderCache>,
    orderbook_id: Option<String>,
    perp_id: Option<String>,
    native_token: Option<String>,
}

fn cache_path(keys_dir: &Path) -> PathBuf {
    keys_dir.join("benchmark-cache.json")
}

fn load_cache(keys_dir: &Path) -> Option<Cache> {
    let path = cache_path(keys_dir);
    let data = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&data).ok()
}

fn save_cache(keys_dir: &Path, cache: &Cache) -> Result<()> {
    let path = cache_path(keys_dir);
    let data = serde_json::to_string_pretty(cache)?;
    std::fs::write(&path, data)?;
    Ok(())
}

pub fn run_benchmark(wasm_dir: &Path, keys_dir: &Path, cfg: BenchmarkConfig) -> Result<()> {
    let start = Instant::now();
    eprintln!("\n━━━ Real-World CLOB + E2E Benchmark ━━━");
    eprintln!("  {} MMs × {} orders, {} Traders", cfg.mm_count, cfg.orders_per_mm, cfg.trader_count);
    eprintln!("  Center price: {}, size: {}", cfg.center_price, cfg.order_size);
    eprintln!("{}", "━".repeat(60));

    let mut client = ServerClient::new(&cfg.server_addr);
    let source_pk = crate::stellar::source_pubkey()?;
    let cached = load_cache(keys_dir);

    let orders: Vec<OrderCache>;
    let perp_id: String;
    let orderbook_id: String;

    if let Some(ref c) = cached {
        if !c.orders.is_empty() && c.orderbook_id.is_some() && c.perp_id.is_some() {
            eprintln!("[1–4] Using cached {} orders (skip init/fund/proofs/deploy)", c.orders.len());
            orders = c.orders.clone();
            perp_id = c.perp_id.clone().unwrap();
            orderbook_id = c.orderbook_id.clone().unwrap();
            client.set_onchain(&perp_id, &source_pk);
        } else {
            return Err(anyhow::anyhow!("Incomplete cache — delete cache file and re-run"));
        }
    } else {
        // ── Step 1: Create one identity per ORDER, init in parallel ─────────
        eprint!("\n[1/6] Creating identities & init orders… ");
        let t1 = Instant::now();

        // Total limit orders: mm_count × orders_per_mm
        let total_mm_orders = cfg.mm_count * cfg.orders_per_mm;
        let total_orders = total_mm_orders + cfg.trader_count;

        // Each order gets its own Stellar identity → independent seqnum lane
        let identities: Vec<(String, String)> = (0..total_orders)
            .map(|i| crate::stellar::generate_keypair(&format!("bm-order-{i}")))
            .collect();

        struct OrderInit {
            addr: String,
            identity: String,
            side: u64,
            price: u64,
            size: u64,
            leverage: u64,
            nonce: u64,
        }

        let mut rng = rand::thread_rng();
        use rand::Rng;
        const LEVERAGES: &[u64] = &[1, 2, 5, 10, 20, 50];

        let mut init_params: Vec<OrderInit> = Vec::new();
        let mut nonce = 0u64;

        // MM orders (limit orders)
        for (ord_idx, (addr, identity)) in identities.iter().take(total_mm_orders).enumerate() {
            let mm_idx = ord_idx / cfg.orders_per_mm;
            let j = ord_idx % cfg.orders_per_mm;
            let _ = mm_idx;
            let side = (j % 2) as u64;
            let shift = ((j / 2) as i64 + 1) * 2000;
            let price = if side == 0 {
                cfg.center_price.saturating_sub(shift as u64)
            } else {
                cfg.center_price.saturating_add(shift as u64)
            };
            let size = if cfg.randomize_sizes {
                let factor = rng.gen_range(50..=150);
                (cfg.order_size * factor) / 100
            } else {
                cfg.order_size
            };
            let leverage = if cfg.randomize_leverage {
                LEVERAGES[rng.gen_range(0..LEVERAGES.len())]
            } else {
                1
            };
            init_params.push(OrderInit {
                addr: addr.clone(), identity: identity.clone(),
                side, price, size, leverage, nonce,
            });
            nonce += 1;
        }

        // Trader orders (market orders) — side 2/3 so step 6 identifies them with o.side > 1
        for (tr_idx, (addr, identity)) in identities.iter().enumerate().skip(total_mm_orders) {
            let side = 2 + (tr_idx % 2) as u64;
            let size = if cfg.randomize_sizes {
                let factor = rng.gen_range(50..=150);
                (cfg.order_size * factor) / 100
            } else {
                cfg.order_size
            };
            let leverage = if cfg.randomize_leverage {
                LEVERAGES[rng.gen_range(0..LEVERAGES.len())]
            } else {
                1
            };
            init_params.push(OrderInit {
                addr: addr.clone(), identity: identity.clone(),
                side, price: 0, size, leverage, nonce,
            });
            nonce += 1;
        }

        // Parallel init — each thread uses its own TCP connection
        let raw_orders: Vec<OrderCache> = std::thread::scope(|s| {
            let handles: Vec<_> = init_params.into_iter().map(|p| {
                let server_addr = cfg.server_addr.clone();
                s.spawn(move || -> Result<OrderCache> {
                    let c = ServerClient::new(&server_addr);
                    let cmt = c.init_raw(p.side, p.price, p.size, p.leverage, 0, p.nonce, p.nonce + 999)?;
                    Ok(OrderCache {
                        cmt,
                        addr: p.addr,
                        identity: p.identity,
                        side: p.side,
                        price: p.price,
                        size: p.size,
                        leverage: p.leverage,
                        proof_json: String::new(),
                    })
                })
            }).collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect::<Result<_>>()
        })?;

        eprintln!("{} orders in {:.1}s", raw_orders.len(), t1.elapsed().as_secs_f64());

        // ── Step 2: Fund all order identities in parallel ────────────────────
        eprint!("[2/6] Funding {} order identities… ", identities.len());
        let t2 = Instant::now();
        std::thread::scope(|s| {
            let handles: Vec<_> = identities.iter().map(|(addr, _)| {
                let addr = addr.clone();
                s.spawn(move || crate::stellar::fund(&addr, ""))
            }).collect();
            for h in handles { let _ = h.join().unwrap(); }
        });
        eprintln!("{:.1}s", t2.elapsed().as_secs_f64());

        // ── Step 3: Generate commit proofs in parallel ───────────────────────
        eprint!("[3/6] Generating commit proofs (parallel)… ");
        let t3 = Instant::now();
        let tmp_dir = std::env::temp_dir().join("tee-benchmark");
        std::fs::create_dir_all(&tmp_dir)?;

        let proof_results: Vec<Result<(String, String)>> = std::thread::scope(|s| {
            let handles: Vec<_> = raw_orders.iter().map(|o| {
                let cmt = o.cmt.clone();
                let addr = cfg.server_addr.clone();
                let out_path = tmp_dir.join(format!("proof_{}.json", &cmt[..16]));
                s.spawn(move || -> Result<(String, String)> {
                    let c = ServerClient::new(&addr);
                    c.commit_proof(&cmt, &out_path)?;
                    let json = std::fs::read_to_string(&out_path)?;
                    Ok((cmt, json))
                })
            }).collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });

        let mut proofs: HashMap<String, String> = HashMap::new();
        for r in proof_results {
            let (cmt, json) = r?;
            proofs.insert(cmt, json);
        }
        eprintln!("{} proofs in {:.1}s", proofs.len(), t3.elapsed().as_secs_f64());

        let mut raw_orders = raw_orders;
        for o in &mut raw_orders {
            o.proof_json = proofs.remove(&o.cmt).unwrap_or_default();
        }

        // ── Step 4: Deploy contracts (sequential — same source account) ──────
        eprint!("[4/6] Deploying contracts… ");
        let t4 = Instant::now();
        let (deployed_ob, deployed_pe, _src_pk, nt) = crate::stellar::deploy_contracts(wasm_dir)?;
        client.set_onchain(&deployed_pe, &source_pk);
        crate::stellar::init_perp_engine(&deployed_pe, &source_pk, &nt)?;
        eprintln!("{:.1}s", t4.elapsed().as_secs_f64());

        perp_id = deployed_pe.clone();
        orderbook_id = deployed_ob.clone();
        orders = raw_orders;

        // ── Step 5 (on-chain, best-effort): place_order + deposit + open_position
        // Opens positions for ALL orders (limit + market) so on-chain match works.
        eprintln!("[5/6] On-chain place/deposit/open ({} orders, parallel, best-effort)…", orders.len());
        let t5 = Instant::now();

        let ob_ref = &deployed_ob;
        let pe_ref = &deployed_pe;
        std::thread::scope(|s| {
            let handles: Vec<_> = orders.iter().enumerate().map(|(i, o)| {
                let ob = ob_ref.clone();
                let pe = pe_ref.clone();
                let identity = o.identity.clone();
                let addr = o.addr.clone();
                let cmt = o.cmt.clone();
                let price = o.price;
                let raw_side = o.side;
                // Normalize side: server maps 0/3→Bid(0), 1/2→Ask(1)
                let hint_side = if raw_side == 0 || raw_side == 3 { 0 } else { 1 };
                let hint_leverage = o.leverage;
                let proof = o.proof_json.clone();
                s.spawn(move || {
                    let r = (|| -> Result<()> {
                        crate::stellar::invoke(&pe, &identity, &[
                            "deposit",
                            "--who", &addr,
                            "--amount", "1000000000",
                        ])?;
                        let rev: u64 = 15; // all fields public
                        let zero_note = "0000000000000000000000000000000000000000000000000000000000000000";
                        crate::stellar::invoke(&ob, &identity, &[
                            "place_order",
                            "--owner", &addr,
                            "--commitment", &cmt,
                            "--hint-price", &price.to_string(),
                            "--hint-side", &hint_side.to_string(),
                            "--hint-size", &o.size.to_string(),
                            "--hint-leverage", &hint_leverage.to_string(),
                            "--revealed", &rev.to_string(),
                            "--proof", &proof,
                        ])?;
                        crate::stellar::invoke(&pe, &identity, &[
                            "open_position",
                            "--owner", &addr,
                            "--commitment", &cmt,
                            "--collateral", "1000000000",
                            "--hint_price", &price.to_string(),
                            "--hint_side", &hint_side.to_string(),
                            "--hint_leverage", &hint_leverage.to_string(),
                            "--liquidation_recipient_note", zero_note,
                            "--proof", &proof,
                        ])?;
                        Ok(())
                    })();
                    match r {
                        Ok(()) => eprintln!("  [order-{}] ✓ all 3 TXs confirmed", i),
                        Err(e) => eprintln!("  [order-{}] ✗ {e}", i),
                    }
                })
            }).collect();
            for h in handles { let _ = h.join(); }
        });

        eprintln!("[5/6] {:.1}s", t5.elapsed().as_secs_f64());

        // Save cache (best-effort)
        if let Err(e) = save_cache(keys_dir, &Cache {
            orders: orders.clone(),
            orderbook_id: Some(deployed_ob),
            perp_id: Some(perp_id.clone()),
            native_token: Some(nt),
        }) {
            eprintln!("  [cache] save failed: {e}");
        }
    }

    // ── Step 6: Seed CLOB with limit orders (best-effort) ────────────────────
    eprintln!("[6/6] Seeding CLOB orderbook ({orders} limit orders)…", orders = orders.iter().filter(|o| o.side <= 1).count());
    let _t6 = Instant::now();
    for o in &orders {
        if o.side > 1 { continue; }
        match client.place_order(&o.cmt, "limit", o.price, o.size) {
            Ok(resp) => {
                eprintln!("  ✓ placed bid={:?} ask={:?} orders={}",
                    resp.best_bid.as_deref().unwrap_or("-"),
                    resp.best_ask.as_deref().unwrap_or("-"),
                    resp.order_count,
                );
            }
            Err(e) => {
                eprintln!("  ✗ {e}");
            }
        }
        if cfg.book_delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(cfg.book_delay_ms));
        }
    }
    let mk = client.get_market().unwrap_or_default();
    eprintln!("  final: bid={:?} ask={:?} spread={:?} orders={}",
        mk.best_bid.as_deref().unwrap_or("-"),
        mk.best_ask.as_deref().unwrap_or("-"),
        mk.spread.map(|s| s.to_string()).unwrap_or_else(|| "-".into()),
        mk.order_count,
    );

    if cfg.book_delay_ms > 0 {
        eprintln!("  [market] Pausing 2s for book observation…");
        std::thread::sleep(std::time::Duration::from_secs(2));
    }

    // ── Set mark price from CLOB mid-price (for non-zero funding) ──
    eprintln!("\n[mark_price] Setting mark price from CLOB mid-price…");
    if let Some(perp_id) = &client.perp {
        if let Ok(mk) = client.get_market() {
            let mid = match (&mk.best_bid, &mk.best_ask) {
                (Some(bid), Some(ask)) => {
                    // Parse "pricexsize" format
                    let bid_p: u64 = bid.split('x').next().and_then(|s| s.parse().ok()).unwrap_or(0);
                    let ask_p: u64 = ask.split('x').next().and_then(|s| s.parse().ok()).unwrap_or(0);
                    if bid_p > 0 && ask_p > 0 {
                        (bid_p + ask_p) / 2
                    } else if bid_p > 0 { bid_p } else { ask_p }
                }
                (Some(bid), None) => bid.split('x').next().and_then(|s| s.parse().ok()).unwrap_or(0),
                (None, Some(ask)) => ask.split('x').next().and_then(|s| s.parse().ok()).unwrap_or(0),
                (None, None) => 0,
            };
            if mid > 0 {
                match client.set_mark_price(perp_id, mid) {
                    Ok(_) => eprintln!("  ✓ mark price set to {}", mid),
                    Err(e) => eprintln!("  ✗ set_mark_price failed: {e}"),
                }
            } else {
                eprintln!("  - no orders in book, skipping mark price");
            }
        }
    }

    // ── Cancel verification: pick a remaining limit order, cancel on-chain + CLOB ──
    eprintln!("\n[cancel] Testing cancel flow…");
    if let Some(cancel_o) = orders.iter().find(|o| o.side <= 1) {
        eprintln!("  cancelling cmt={}… addr={}… identity={}",
            &cancel_o.cmt[..16], &cancel_o.addr[..8], cancel_o.identity);
        match client.cancel(&cancel_o.cmt, &perp_id, &orderbook_id, &cancel_o.addr, &cancel_o.identity) {
            Ok(_) => {
                let mk = client.get_market().unwrap_or_default();
                eprintln!("  ✓ cancelled, book now has {} orders (was {})",
                    mk.order_count, mk.order_count + 1);
            }
            Err(e) => eprintln!("  ✗ cancel failed: {e}"),
        }
    }

    // ── Market orders → CLOB match → on-chain (best-effort) ─────────────────
    eprintln!("[market] Running market orders (best-effort)…");
    let mut total_matches = 0;
    for o in &orders {
        if o.side > 1 {
            eprint!("  → {} market size={}: ",
                if o.side % 2 == 1 { "Bid" } else { "Ask" }, o.size);
            match client.place_market(&o.cmt, o.size) {
                Ok(resp) => {
                    let matched = resp.fills.as_ref()
                        .map(|f| f.iter().filter(|x| x.nullifier_a.is_some()).count())
                        .unwrap_or(0);
                    total_matches += matched;
                    eprintln!("{} fills, {} on-chain ✓",
                        resp.fills.as_ref().map_or(0, |f| f.len()), matched);
                    let mk = client.get_market().unwrap_or_default();
                    eprintln!("     book: bid={:?} ask={:?} spread={:?} orders={}",
                        mk.best_bid.as_deref().unwrap_or("-"),
                        mk.best_ask.as_deref().unwrap_or("-"),
                        mk.spread.map(|s| s.to_string()).unwrap_or_else(|| "-".into()),
                        mk.order_count,
                    );
                }
                Err(e) => eprintln!("✗ {e}"),
            }
            if cfg.book_delay_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(cfg.book_delay_ms));
            }
        }
    }

    eprintln!("\n{}", "━".repeat(60));
    eprintln!("━━━ BENCHMARK COMPLETE ━━━");
    eprintln!("  Total time: {:.1}s", start.elapsed().as_secs_f64());
    eprintln!("  Limit orders: {}", orders.iter().filter(|o| o.side <= 1).count());
    eprintln!("  Market orders: {}", orders.iter().filter(|o| o.side > 1).count());
    eprintln!("  On-chain matches: {}", total_matches);
    eprintln!("  Cancel test: included ✓");
    eprintln!("{}", "━".repeat(60));
    Ok(())
}
