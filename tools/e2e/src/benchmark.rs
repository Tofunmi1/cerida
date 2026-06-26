use crate::client::ServerClient;
use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

/// A market participant (MM or Trader)
pub struct Participant {
    pub label: String,
    pub address: String,
    pub identity: String,
    /// Order commitments and secrets stored on the tee-match server
    pub commitments: Vec<(String, CommitmentParams)>,
}

#[derive(Clone)]
pub struct CommitmentParams {
    pub side: u64,
    pub price: u64,
    pub size: u64,
    pub leverage: u64,
    pub asset: u64,
    pub nonce: u64,
    pub secret: u64,
}

impl CommitmentParams {
    #[allow(dead_code)]
    pub fn bid(price: u64, size: u64, nonce: u64, secret: u64) -> Self {
        Self { side: 0, price, size, leverage: 1, asset: 0, nonce, secret }
    }
}

/// Benchmark configuration: N market makers, N traders, orders each
pub struct BenchmarkConfig {
    pub mm_count: usize,
    pub trader_count: usize,
    pub orders_per_mm: usize,
    pub orders_per_trader: usize,
    pub server_addr: String,
    pub center_price: u64,
    pub spread_pct: u64,    // e.g. 5 = 5% spread away from center
    pub order_size: u64,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            mm_count: 10,
            trader_count: 10,
            orders_per_mm: 5,
            orders_per_trader: 5,
            server_addr: "127.0.0.1:9720".into(),
            center_price: 100_000,
            spread_pct: 5,
            order_size: 10_000_000,
        }
    }
}

/// Phase timing report
pub struct PhaseTiming {
    label: &'static str,
    elapsed: std::time::Duration,
}

pub struct BenchmarkReport {
    pub participants: Vec<Participant>,
    pub phases: Vec<PhaseTiming>,
    pub total_orders: usize,
    pub total_fills: usize,
    pub total_commit_proofs: usize,
    pub total_match_proofs: usize,
    pub orderbook_id: Option<String>,
    pub perp_id: Option<String>,
    pub total_elapsed: std::time::Duration,
}

pub fn run_benchmark(wasm_dir: &Path, _keys_dir: &Path, cfg: BenchmarkConfig) -> Result<BenchmarkReport> {
    let global_start = Instant::now();
    eprintln!("\n{}", "━".repeat(60));
    eprintln!("━━━ TEE E2E Benchmark ━━━");
    eprintln!("{} MMs × {} orders + {} Traders × {} orders",
        cfg.mm_count, cfg.orders_per_mm, cfg.trader_count, cfg.orders_per_trader);
    eprintln!("Center price: {}, spread: {}%, size: {}",
        cfg.center_price, cfg.spread_pct, cfg.order_size);
    eprintln!("Server: {}", cfg.server_addr);
    eprintln!("{}", "━".repeat(60));

    let mut report = BenchmarkReport {
        participants: Vec::new(),
        phases: Vec::new(),
        total_orders: 0,
        total_fills: 0,
        total_commit_proofs: 0,
        total_match_proofs: 0,
        orderbook_id: None,
        perp_id: None,
        total_elapsed: std::time::Duration::default(),
    };

    // ── Phase 1: Create identities ──────────────────────────────────────
    eprintln!("\n── Phase 1/6: Creating {} participant identities ──",
        cfg.mm_count + cfg.trader_count);

    let t1 = Instant::now();
    let source_pk = crate::stellar::source_pubkey()?;
    eprintln!("  [admin] source: {}", source_pk);

    for i in 0..cfg.mm_count {
        let (addr, identity) = crate::stellar::generate_keypair(&format!("bm-mm-{i}"));
        eprintln!("  [MM-{i}] {} (identity: {})", &addr[..8], identity);
        report.participants.push(Participant {
            label: format!("MM-{i}"),
            address: addr,
            identity,
            commitments: Vec::new(),
        });
    }
    for i in 0..cfg.trader_count {
        let (addr, identity) = crate::stellar::generate_keypair(&format!("bm-tr-{i}"));
        eprintln!("  [T-{i}] {} (identity: {})", &addr[..8], identity);
        report.participants.push(Participant {
            label: format!("T-{i}"),
            address: addr,
            identity,
            commitments: Vec::new(),
        });
    }
    report.phases.push(PhaseTiming { label: "Identity creation", elapsed: t1.elapsed() });
    eprintln!("  ✓ Identities created ({:.2}s)", t1.elapsed().as_secs_f64());

    // ── Fund everyone ───────────────────────────────────────────────────
    eprintln!("\n── Funding all participants via friendbot ──");
    let t_fund = Instant::now();
    for p in &report.participants {
        crate::stellar::fund(&p.address, &p.label);
    }
    eprintln!("  ✓ All funded ({:.2}s)", t_fund.elapsed().as_secs_f64());

    // ── Phase 2: Init all orders on tee-match server ────────────────────
    eprintln!("\n── Phase 2/6: Init {} orders on server ──",
        cfg.mm_count * cfg.orders_per_mm + cfg.trader_count * cfg.orders_per_trader);

    let t2 = Instant::now();
    let client = ServerClient::new(&cfg.server_addr);

    // Market makers: place bid/ask orders at varying prices
    let mut all_orders: Vec<(String, CommitmentParams)> = Vec::new();

    for (mm_idx, p) in report.participants.iter_mut().enumerate().take(cfg.mm_count) {
        for j in 0..cfg.orders_per_mm {
            let shift = (j as i64 - (cfg.orders_per_mm / 2) as i64) * (cfg.center_price as i64 / 100);
            let price = (cfg.center_price as i64 + shift) as u64;
            let side = if j % 2 == 0 { 0 } else { 1 };
            let label = format!("bm-mm{mm_idx}-{j}");
            let params = CommitmentParams {
                side, price, size: cfg.order_size,
                leverage: 1, asset: 0,
                nonce: (mm_idx * cfg.orders_per_mm + j) as u64,
                secret: (mm_idx * 1000 + j) as u64,
            };
            let cmt = client.init_raw(
                params.side, params.price, params.size,
                params.leverage, params.asset,
                params.nonce, params.secret,
            )?;
            eprintln!("  [init] {} cmt={}… price={} side={}", &label, &cmt[..12], price, side);
            all_orders.push((cmt.clone(), params.clone()));
            p.commitments.push((cmt.clone(), params.clone()));
        }
    }

    // Traders: place aggressive orders at the center
    for (t_idx, p) in report.participants.iter_mut().skip(cfg.mm_count).enumerate() {
        for j in 0..cfg.orders_per_trader {
            let side = if j % 2 == 0 { 0 } else { 1 };
            let price = if side == 0 {
                (cfg.center_price as f64 * (1.0 + cfg.spread_pct as f64 / 100.0)) as u64
            } else {
                (cfg.center_price as f64 * (1.0 - cfg.spread_pct as f64 / 100.0)) as u64
            };
            let label = format!("bm-tr{t_idx}-{j}");
            let params = CommitmentParams {
                side, price, size: cfg.order_size / 2,
                leverage: 1, asset: 0,
                nonce: 1000 + (t_idx * cfg.orders_per_trader + j) as u64,
                secret: 2000 + (t_idx * cfg.orders_per_trader + j) as u64,
            };
            let cmt = client.init_raw(
                params.side, params.price, params.size,
                params.leverage, params.asset,
                params.nonce, params.secret,
            )?;
            eprintln!("  [init] {} cmt={}… price={} side={}", &label, &cmt[..12], price, side);
            all_orders.push((cmt.clone(), params.clone()));
            p.commitments.push((cmt, params));
        }
    }

    report.total_orders = all_orders.len();
    report.phases.push(PhaseTiming { label: "Order init (server)", elapsed: t2.elapsed() });
    eprintln!("  ✓ {} orders initialized ({:.2}s)", all_orders.len(), t2.elapsed().as_secs_f64());

    // ── Phase 3: Generate commitment proofs (parallel) ──────────────────
    eprintln!("\n── Phase 3/6: Generating {} commitment proofs (parallel) ──", all_orders.len());

    let t3 = Instant::now();
    let tmp_dir = std::env::temp_dir().join("tee-benchmark");
    std::fs::create_dir_all(&tmp_dir)?;

    // Use scoped threads for parallelism
    let proof_results: Vec<Result<(String, String)>> = std::thread::scope(|s| {
        let mut handles = Vec::new();
        for (cmt, _params) in &all_orders {
            let cmt = cmt.clone();
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

    // Collect proofs by commitment
    let mut proofs: HashMap<String, String> = HashMap::new();
    for r in proof_results {
        let (cmt, json) = r?;
        proofs.insert(cmt, json);
    }

    report.total_commit_proofs = proofs.len();
    report.phases.push(PhaseTiming { label: "Commit proofs (parallel)", elapsed: t3.elapsed() });
    eprintln!("  ✓ {} proofs generated ({:.2}s)", proofs.len(), t3.elapsed().as_secs_f64());

    // ── Phase 4: Deploy contracts + setup ───────────────────────────────
    eprintln!("\n── Phase 4/6: Deploy contracts & setup on-chain ──");

    let t4 = Instant::now();
    let (orderbook_id, perp_id, source_pk, native_token) = crate::stellar::deploy_contracts(wasm_dir)?;
    crate::stellar::init_perp_engine(&perp_id, &source_pk, &native_token)?;
    eprintln!("  ✓ perp engine initialized");
    eprintln!("  ✓ orderbook: {}", orderbook_id);
    eprintln!("  ✓ perp: {}", perp_id);

    report.orderbook_id = Some(orderbook_id.clone());
    report.perp_id = Some(perp_id.clone());
    report.phases.push(PhaseTiming { label: "Contract deployment", elapsed: t4.elapsed() });

    // ── Phase 5: Place orders, deposit, open positions on-chain ─────────
    eprintln!("\n── Phase 5/6: On-chain setup for {} participants ──",
        report.participants.len());

    let t5 = Instant::now();
    let mut all_placed: Vec<(String, CommitmentParams)> = Vec::new();

    for p in &report.participants {
        let identity = &p.identity;
        let address = &p.address;
        for (cmt, params) in &p.commitments {
            let proof_json = proofs.get(cmt)
                .ok_or_else(|| anyhow::anyhow!("proof not found for {}", cmt))?;

            // Place order on-chain (orderbook contract)
            eprintln!("  [place] {} order {}… price={}", &p.label, &cmt[..12], params.price);
            crate::stellar::invoke(
                &orderbook_id, identity, &[
                    "place_order", "--owner", address, "--commitment", cmt,
                    "--hint", &params.price.to_string(),
                    "--proof", proof_json,
                ],
            )?;

            // Deposit collateral
            eprintln!("  [deposit] {} {}", &p.label, COLLATERAL);
            crate::stellar::invoke(
                &perp_id, identity, &[
                    "deposit", "--who", address, "--amount", &COLLATERAL.to_string(),
                ],
            )?;

            // Open position
            eprintln!("  [open] {} {}… hint={}", &p.label, &cmt[..12], params.price);
            crate::stellar::invoke(
                &perp_id, identity, &[
                    "open_position", "--owner", address, "--commitment", cmt,
                    "--collateral", &COLLATERAL.to_string(),
                    "--hint_price", &params.price.to_string(),
                    "--hint_side", &params.side.to_string(),
                    "--hint_leverage", "1",
                    "--proof", proof_json,
                ],
            )?;

            all_placed.push((cmt.clone(), params.clone()));
        }
    }

    report.phases.push(PhaseTiming { label: "On-chain setup", elapsed: t5.elapsed() });
    eprintln!("  ✓ {} orders placed/setup ({:.2}s)", all_placed.len(), t5.elapsed().as_secs_f64());

    // ── Phase 6: Match on-chain ─────────────────────────────────────────
    eprintln!("\n── Phase 6/6: Matching orders on-chain ──");

    let t6 = Instant::now();
    let mut total_fills = 0;

    // Match pairs: iterate bids vs asks
    let bids: Vec<_> = all_placed.iter()
        .filter(|(_, p)| p.side == 0)
        .collect();
    let asks: Vec<_> = all_placed.iter()
        .filter(|(_, p)| p.side == 1)
        .collect();

    eprintln!("  Matching {} bids × {} asks", bids.len(), asks.len());

    for (bid_cmt, bid_p) in &bids {
        for (ask_cmt, ask_p) in &asks {
            if bid_p.price < ask_p.price {
                continue; // spread not crossed
            }
            // Compute match params
            let mp = (bid_p.price + ask_p.price) / 2;
            let ms = bid_p.size.min(ask_p.size);
            if ms == 0 { continue; }

            eprintln!("  [match] bid={}… price={} × ask={}… price={} → mp={} ms={}",
                &bid_cmt[..12], bid_p.price, &ask_cmt[..12], ask_p.price, mp, ms);

            // Generate match proof via server
            let match_proof = client.match_proof_json(
                bid_cmt, ask_cmt, &perp_id, crate::stellar::SOURCE,
            )?;

            // Submit match on-chain
            crate::stellar::invoke(
                &perp_id, crate::stellar::SOURCE, &[
                    "match_positions",
                    "--cmt_a", bid_cmt,
                    "--cmt_b", ask_cmt,
                    "--nullifier_a", "0",
                    "--nullifier_b", "0",
                    "--match_price", &crate::stellar::hex_field(&mp.to_string()),
                    "--match_size", &crate::stellar::hex_field(&ms.to_string()),
                    "--proof", &match_proof,
                ],
            )?;

            total_fills += 1;
            report.total_match_proofs += 1;

            // Limit to reasonable matches for now
            if total_fills >= 10 { break; }
        }
        if total_fills >= 10 { break; }
    }

    report.total_fills = total_fills;
    report.phases.push(PhaseTiming { label: "On-chain matching", elapsed: t6.elapsed() });

    // ── Final report ────────────────────────────────────────────────────
    report.total_elapsed = global_start.elapsed();
    eprintln!("\n{}", "━".repeat(60));
    eprintln!("━━━ BENCHMARK COMPLETE ━━━");
    eprintln!("Total time: {:.2}s", report.total_elapsed.as_secs_f64());
    eprintln!("Participants: {} MM + {} T", cfg.mm_count, cfg.trader_count);
    eprintln!("Orders initialized: {}", report.total_orders);
    eprintln!("Commit proofs: {}", report.total_commit_proofs);
    eprintln!("On-chain fills: {}", report.total_fills);
    for phase in &report.phases {
        eprintln!("  {}: {:.2}s", phase.label, phase.elapsed.as_secs_f64());
    }
    eprintln!("{}", "━".repeat(60));

    Ok(report)
}

const COLLATERAL: i128 = 1_000_000_000;
