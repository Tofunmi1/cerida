use crate::client::ServerClient;
use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

pub struct BenchmarkConfig {
    pub mm_count: usize,
    pub trader_count: usize,
    pub orders_per_mm: usize,
    pub server_addr: String,
    pub center_price: u64,
    pub order_size: u64,
}

struct OrderEntry {
    cmt: String,
    addr: String,
    identity: String,
    side: u64,
    price: u64,
    size: u64,
}

pub fn run_benchmark(wasm_dir: &Path, _keys_dir: &Path, cfg: BenchmarkConfig) -> Result<()> {
    let start = Instant::now();
    eprintln!("\n━━━ Real-World CLOB + E2E Benchmark ━━━");
    eprintln!("  {} MMs × {} orders, {} Traders", cfg.mm_count, cfg.orders_per_mm, cfg.trader_count);
    eprintln!("  Center price: {}, size: {}", cfg.center_price, cfg.order_size);
    eprintln!("  Strategy: seed limit orders → deploy → run market orders → auto-match on-chain → show book");
    eprintln!("{}", "━".repeat(60));

    let mut client = ServerClient::new(&cfg.server_addr);
    let source_pk = crate::stellar::source_pubkey()?;

    // ── Step 1: Create identities + init orders ─────────────────────────
    eprint!("\n[1/6] Creating identities & init orders… ");
    let t1 = Instant::now();

    let identities: Vec<(String, String)> = (0..cfg.mm_count)
        .map(|i| crate::stellar::generate_keypair(&format!("bm-mm-{i}")))
        .chain((0..cfg.trader_count)
            .map(|i| crate::stellar::generate_keypair(&format!("bm-tr-{i}"))))
        .collect();

    let mut orders: Vec<OrderEntry> = Vec::new();
    let mut nonce = 0u64;

    // MMs: limit orders alternating bid/ask around center
    for (mm_idx, (addr, identity)) in identities.iter().enumerate().take(cfg.mm_count) {
        for j in 0..cfg.orders_per_mm {
            let side = (j % 2) as u64;
            let shift = ((j / 2) as i64 + 1) * 2000;
            let price = if side == 0 {
                cfg.center_price.saturating_sub(shift as u64)
            } else {
                cfg.center_price.saturating_add(shift as u64)
            };
            let cmt = client.init_raw(side, price, cfg.order_size, 1, 0, nonce, nonce + 999)?;
            orders.push(OrderEntry { cmt, addr: addr.clone(), identity: identity.clone(), side, price, size: cfg.order_size });
            nonce += 1;
        }
    }
    // Traders: market orders (price=0)
    for (tr_idx, (addr, identity)) in identities.iter().enumerate().skip(cfg.mm_count) {
        let side = (tr_idx % 2) as u64;
        let cmt = client.init_raw(side, 0, cfg.order_size * 2, 1, 0, nonce, nonce + 999)?;
        orders.push(OrderEntry { cmt, addr: addr.clone(), identity: identity.clone(), side, price: 0, size: cfg.order_size * 2 });
        nonce += 1;
    }
    eprintln!("{} orders in {:.1}s", orders.len(), t1.elapsed().as_secs_f64());

    // ── Step 2: Fund ────────────────────────────────────────────────────
    eprint!("[2/6] Funding participants… ");
    let t2 = Instant::now();
    for (addr, _) in &identities {
        crate::stellar::fund(addr, "");
    }
    eprintln!("{:.1}s", t2.elapsed().as_secs_f64());

    // ── Step 3: Generate commit proofs in parallel ──────────────────────
    eprint!("[3/6] Generating commit proofs (parallel)… ");
    let t3 = Instant::now();
    let tmp_dir = std::env::temp_dir().join("tee-benchmark");
    std::fs::create_dir_all(&tmp_dir)?;

    let proof_results: Vec<Result<(String, String)>> = std::thread::scope(|s| {
        let mut handles = Vec::new();
        for o in &orders {
            let cmt = o.cmt.clone();
            let addr = cfg.server_addr.clone();
            let out_path = tmp_dir.join(format!("proof_{}.json", &cmt[..16]));
            handles.push(s.spawn(move || -> Result<(String, String)> {
                let c = ServerClient::new(&addr);
                c.commit_proof(&cmt, &out_path)?;
                let json = std::fs::read_to_string(&out_path)?;
                Ok((cmt, json))
            }));
        }
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let mut proofs: HashMap<String, String> = HashMap::new();
    for r in proof_results {
        let (cmt, json) = r?;
        proofs.insert(cmt, json);
    }
    eprintln!("{} proofs in {:.1}s", proofs.len(), t3.elapsed().as_secs_f64());

    // ── Step 4: Deploy + place limit orders on-chain ────────────────────
    eprint!("[4/6] Deploying & placing limit orders on-chain… ");
    let t4 = Instant::now();
    let (orderbook_id, perp_id, _source_pk, native_token) = crate::stellar::deploy_contracts(wasm_dir)?;
    client.set_onchain(&perp_id, &source_pk);
    crate::stellar::init_perp_engine(&perp_id, &source_pk, &native_token)?;

    for o in &orders {
        if o.side > 1 { continue; } // skip market orders (reserved for step 6)
        let pj = proofs.get(&o.cmt).unwrap();
        crate::stellar::invoke(&orderbook_id, &o.identity, &[
            "place_order", "--owner", &o.addr, "--commitment", &o.cmt,
            "--hint", &o.price.to_string(), "--proof", pj,
        ])?;
        crate::stellar::invoke(&perp_id, &o.identity, &[
            "deposit", "--who", &o.addr, "--amount", "1000000000",
        ])?;
        crate::stellar::invoke(&perp_id, &o.identity, &[
            "open_position", "--owner", &o.addr, "--commitment", &o.cmt,
            "--collateral", "1000000000",
            "--hint_price", &o.price.to_string(),
            "--hint_side", &o.side.to_string(),
            "--hint_leverage", "1", "--proof", pj,
        ])?;
    }
    eprintln!("{:.1}s", t4.elapsed().as_secs_f64());

    // ── Step 5: Seed CLOB with limit orders ─────────────────────────────
    eprint!("[5/6] Seeding CLOB orderbook… ");
    let t5 = Instant::now();
    for o in &orders {
        if o.side > 1 { continue; }
        let _ = client.place_order(&o.cmt, "limit", o.price, o.size)?;
    }
    let mk = client.get_market()?;
    eprintln!("{:.1}s → bid={:?} ask={:?} spread={:?} orders={}",
        t5.elapsed().as_secs_f64(),
        mk.best_bid.as_deref().unwrap_or("-"),
        mk.best_ask.as_deref().unwrap_or("-"),
        mk.spread.map(|s| s.to_string()).unwrap_or_else(|| "-".into()),
        mk.order_count,
    );

    // ── Step 6: Market orders → CLOB match → auto ZK proof + on-chain submit ─
    eprintln!("[6/6] Running market orders (auto-match on-chain)…");
    let _t6 = Instant::now();
    let mut total_matches = 0;

    for o in &orders {
        if o.side > 1 {
            eprint!("  → {} market size={}: ",
                if o.side == 0 { "Bid" } else { "Ask" }, o.size);

            match client.place_market(&o.cmt, o.size) {
                Ok(resp) => {
                    let matched = resp.fills.as_ref()
                        .map(|f| f.iter().filter(|x| x.nullifier_a.is_some()).count())
                        .unwrap_or(0);
                    total_matches += matched;
                    eprintln!("{} fills, {} on-chain ✓", resp.fills.as_ref().map_or(0, |f| f.len()), matched);

                    let mk = client.get_market()?;
                    eprintln!("       book: bid={:?} ask={:?} spread={:?} orders={}",
                        mk.best_bid.as_deref().unwrap_or("-"),
                        mk.best_ask.as_deref().unwrap_or("-"),
                        mk.spread.map(|s| s.to_string()).unwrap_or_else(|| "-".into()),
                        mk.order_count,
                    );
                }
                Err(e) => eprintln!("✗ {e}"),
            }
        }
    }

    eprintln!("\n{}", "━".repeat(60));
    eprintln!("━━━ BENCHMARK COMPLETE ━━━");
    eprintln!("  Total time: {:.1}s", start.elapsed().as_secs_f64());
    eprintln!("  Limit orders: {}", orders.iter().filter(|o| o.side <= 1).count());
    eprintln!("  Market orders: {}", orders.iter().filter(|o| o.side > 1).count());
    eprintln!("  On-chain matches: {}", total_matches);
    eprintln!("{}", "━".repeat(60));
    Ok(())
}
