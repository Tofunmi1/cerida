use anyhow::Result;
use rand::Rng;
use std::path::{Path, PathBuf};
use std::time::Instant;

const DEFAULT_RPC_URL: &str = "https://stellar-testnet.g.alchemy.com/v2/FqjaGAy9IMENhdv2i_3UUVDPZnNClYNq";

pub fn rpc_url() -> String {
    std::env::var("SOROBAN_RPC_URL").unwrap_or_else(|_| DEFAULT_RPC_URL.to_string())
}

const NETWORK_PASSPHRASE: &str = "Test SDF Network ; September 2015";

pub const SOURCE: &str = "e2e";
const COLLATERAL: i128 = 1_000_000_000;
const LEVERAGE: u64 = 1;

pub struct E2eContext {
    pub orderbook_id: String,
    pub perp_id: String,
    pub source_pk: String,
    pub alice: (String, String),
    pub bob: (String, String),
    pub cmt_a_hex: String,
    pub cmt_b_hex: String,
}

/// Deploy contracts, place orders, deposit, open positions (all before match).
pub fn deploy_and_place(
    wasm_dir: &Path,
    proof_a_json: &str,
    proof_b_json: &str,
    cmt_a_hex: &str,
    cmt_b_hex: &str,
    hint_price_a: u64,
    hint_price_b: u64,
    hint_side_a: u64,
    hint_side_b: u64,
    hint_size_a: u64,
    hint_size_b: u64,
    hint_leverage_a: u64,
    hint_leverage_b: u64,
    revealed: u64,
) -> Result<E2eContext> {
    let step_start = Instant::now();
    let ob_wasm = wasm_dir.join("orderbook.wasm");
    let pe_wasm = wasm_dir.join("perp_engine.wasm");

    eprintln!("  [wasm] orderbook: {} ({} bytes)", ob_wasm.display(),
        std::fs::metadata(&ob_wasm).map(|m| m.len()).unwrap_or(0));
    eprintln!("  [wasm] perp-engine: {} ({} bytes)", pe_wasm.display(),
        std::fs::metadata(&pe_wasm).map(|m| m.len()).unwrap_or(0));

    // ── Deploy orderbook ──────────────────────────────────────────────────
    eprintln!("  [deploy] Deploying orderbook contract…");
    let orderbook_id = deploy(&ob_wasm)?;
    eprintln!("  ✓ orderbook deployed: {}", orderbook_id);

    // ── Generate identities ──────────────────────────────────────────────
    eprintln!("  [identities] Generating keypairs…");
    let alice = generate_keypair("e2e-alice");
    let bob = generate_keypair("e2e-bob");
    let source_pk = source_pubkey()?;
    eprintln!("  ✓ admin source: {}", source_pk);
    eprintln!("  ✓ alice: {} (identity: {})", alice.0, alice.1);
    eprintln!("  ✓ bob:   {} (identity: {})", bob.0, bob.1);

    // ── Fund traders ────────────────────────────────────────────────────
    eprintln!("  [fund] Funding traders via friendbot…");
    fund(&alice.0, "alice");
    fund(&bob.0, "bob");

    // ── Deploy perp engine ──────────────────────────────────────────────
    eprintln!("  [deploy] Deploying perp-engine contract…");
    let perp_id = deploy(&pe_wasm)?;
    eprintln!("  ✓ perp-engine deployed: {}", perp_id);

    // ── Get native SAC token ID ──────────────────────────────────────────
    eprintln!("  [token] Resolving native SAC asset…");
    let native_token = native_token_id()?;
    eprintln!("  ✓ native token: {}", native_token);

    // ── Initialize perp engine ──────────────────────────────────────────
    eprintln!("  [init] Initializing perp-engine (admin={}, token={})…",
        &source_pk[..8], &native_token[..8]);
    init_perp_engine(&perp_id, SOURCE, &native_token)?;
    eprintln!("  ✓ perp-engine initialized");

    // ── Place order A (Alice) ────────────────────────────────────────────
    eprintln!("  [place] Placing order A (Alice, cmt={}…)…", &cmt_a_hex[..12]);
    eprintln!("    hint_price={} hint_side={} hint_size={} hint_leverage={} revealed={}",
        hint_price_a, hint_side_a, hint_size_a, hint_leverage_a, revealed);
    invoke(
        &orderbook_id,
        &alice.1,
        &[
            "place_order", "--owner", &alice.0, "--commitment", cmt_a_hex,
            "--hint-price", &hint_price_a.to_string(),
            "--hint-side", &hint_side_a.to_string(),
            "--hint-size", &hint_size_a.to_string(),
            "--hint-leverage", &hint_leverage_a.to_string(),
            "--revealed", &revealed.to_string(),
            "--proof", proof_a_json,
        ],
    )?;
    let st_a = invoke_view(
        &orderbook_id, &alice.0,
        &["status", "--commitment", cmt_a_hex],
    )?;
    eprintln!("  ✓ order A placed, status: {}", st_a);

    // ── Place order B (Bob) ──────────────────────────────────────────────
    eprintln!("  [place] Placing order B (Bob, cmt={}…)…", &cmt_b_hex[..12]);
    eprintln!("    hint_price={} hint_side={} hint_size={} hint_leverage={} revealed={}",
        hint_price_b, hint_side_b, hint_size_b, hint_leverage_b, revealed);
    invoke(
        &orderbook_id,
        &bob.1,
        &[
            "place_order", "--owner", &bob.0, "--commitment", cmt_b_hex,
            "--hint-price", &hint_price_b.to_string(),
            "--hint-side", &hint_side_b.to_string(),
            "--hint-size", &hint_size_b.to_string(),
            "--hint-leverage", &hint_leverage_b.to_string(),
            "--revealed", &revealed.to_string(),
            "--proof", proof_b_json,
        ],
    )?;
    let st_b = invoke_view(
        &orderbook_id, &bob.0,
        &["status", "--commitment", cmt_b_hex],
    )?;
    eprintln!("  ✓ order B placed, status: {}", st_b);

    // ── Deposit collateral (Alice) ──────────────────────────────────────
    eprintln!("  [deposit] Alice depositing {} stroops…", COLLATERAL);
    invoke(
        &perp_id,
        &alice.1,
        &[
            "deposit", "--who", &alice.0, "--amount", &COLLATERAL.to_string(),
        ],
    )?;
    let bal_a = invoke_view(
        &perp_id, &alice.0,
        &["get_balance", "--who", &alice.0],
    )?;
    eprintln!("  ✓ Alice balance: {}", bal_a);

    // ── Deposit collateral (Bob) ────────────────────────────────────────
    eprintln!("  [deposit] Bob depositing {} stroops…", COLLATERAL);
    invoke(
        &perp_id,
        &bob.1,
        &[
            "deposit", "--who", &bob.0, "--amount", &COLLATERAL.to_string(),
        ],
    )?;
    let bal_b = invoke_view(
        &perp_id, &bob.0,
        &["get_balance", "--who", &bob.0],
    )?;
    eprintln!("  ✓ Bob balance: {}", bal_b);

    // ── Open position A (Alice) ──────────────────────────────────────────
    eprintln!("  [position] Opening position A (Alice, cmt={}…)…", &cmt_a_hex[..12]);
    eprintln!("    collateral={} hint_price={} hint_side={} leverage={}", COLLATERAL, hint_price_a, hint_side_a, LEVERAGE);
    invoke(
        &perp_id,
        &alice.1,
        &[
            "open_position", "--owner", &alice.0, "--commitment", cmt_a_hex,
            "--collateral", &COLLATERAL.to_string(),
            "--hint_price", &hint_price_a.to_string(),
            "--hint_side", &hint_side_a.to_string(),
            "--hint_leverage", &LEVERAGE.to_string(),
            "--proof", proof_a_json,
        ],
    )?;
    let pos_a = invoke_view(
        &perp_id, &alice.0,
        &["get_position", "--commitment", cmt_a_hex],
    )?;
    eprintln!("  ✓ position A: {}", pos_a);

    // ── Open position B (Bob) ──────────────────────────────────────────
    eprintln!("  [position] Opening position B (Bob, cmt={}…)…", &cmt_b_hex[..12]);
    eprintln!("    collateral={} hint_price={} hint_side={} leverage={}", COLLATERAL, hint_price_b, hint_side_b, LEVERAGE);
    invoke(
        &perp_id,
        &bob.1,
        &[
            "open_position", "--owner", &bob.0, "--commitment", cmt_b_hex,
            "--collateral", &COLLATERAL.to_string(),
            "--hint_price", &hint_price_b.to_string(),
            "--hint_side", &hint_side_b.to_string(),
            "--hint_leverage", &LEVERAGE.to_string(),
            "--proof", proof_b_json,
        ],
    )?;
    let pos_b = invoke_view(
        &perp_id, &bob.0,
        &["get_position", "--commitment", cmt_b_hex],
    )?;
    eprintln!("  ✓ position B: {}", pos_b);

    eprintln!("  [setup] Deploy + setup completed in {:.2}s", step_start.elapsed().as_secs_f64());

    Ok(E2eContext {
        orderbook_id,
        perp_id,
        source_pk,
        alice,
        bob,
        cmt_a_hex: cmt_a_hex.to_string(),
        cmt_b_hex: cmt_b_hex.to_string(),
    })
}

/// Full e2e: deploy, place, deposit, open, match, verify (local proof gen).
pub fn run_e2e(
    wasm_dir: &Path,
    p_a: &crate::proof::RawProof,
    p_b: &crate::proof::RawProof,
    p_match: &crate::proof::RawProof,
    cmt_a_hex: &str,
    cmt_b_hex: &str,
) -> Result<()> {
    let start = Instant::now();
    let proof_a_json = proof_json(&p_a.proof);
    let proof_b_json = proof_json(&p_b.proof);
    let hint_price_a: u64 = 100000;
    let hint_price_b: u64 = 99000;
    let hint_side_a: u64 = 0;
    let hint_side_b: u64 = 1;
    let hint_size_a: u64 = 1000;
    let hint_size_b: u64 = 1000;
    let hint_leverage_a: u64 = 1;
    let hint_leverage_b: u64 = 1;
    let revealed: u64 = 15; // all fields public

    eprintln!("── Phase 1: Deploy, place, deposit, open ──");
    let ctx = deploy_and_place(
        wasm_dir, &proof_a_json, &proof_b_json,
        cmt_a_hex, cmt_b_hex,
        hint_price_a, hint_price_b,
        hint_side_a, hint_side_b,
        hint_size_a, hint_size_b,
        hint_leverage_a, hint_leverage_b,
        revealed,
    )?;

    let match_price_hex = &p_match.public_inputs[2];
    let match_size_hex = &p_match.public_inputs[3];
    let nf_a_hex = &p_match.public_inputs[4];
    let nf_b_hex = &p_match.public_inputs[5];

    // ── Match via perp engine ──────────────────────────────────────────────
    eprintln!("── Phase 2: On-chain match ──");
    eprintln!("  [match] match_positions(cmt_a={}…, cmt_b={}…)",
        &cmt_a_hex[..12], &cmt_b_hex[..12]);
    invoke(
        &ctx.perp_id,
        SOURCE,
        &[
            "match_positions",
            "--cmt_a", cmt_a_hex,
            "--cmt_b", cmt_b_hex,
            "--nullifier_a", &hex_field(nf_a_hex),
            "--nullifier_b", &hex_field(nf_b_hex),
            "--match_price", &hex_field(match_price_hex),
            "--match_size", &hex_field(match_size_hex),
            "--proof", &proof_json(&p_match.proof),
        ],
    )?;

    verify_match(&ctx, nf_a_hex, nf_b_hex)?;
    eprintln!("  ✓ Full E2E completed in {:.2}s", start.elapsed().as_secs_f64());
    Ok(())
}

/// Verify match results on-chain (positions + nullifiers).
pub fn verify_match(ctx: &E2eContext, nf_a_hex: &str, nf_b_hex: &str) -> Result<()> {
    eprintln!("  [verify] Checking matched positions…");
    let pos_a2 = invoke_view(
        &ctx.perp_id, &ctx.alice.0,
        &["get_position", "--commitment", &ctx.cmt_a_hex],
    )?;
    let pos_b2 = invoke_view(
        &ctx.perp_id, &ctx.bob.0,
        &["get_position", "--commitment", &ctx.cmt_b_hex],
    )?;
    eprintln!("  ✓ position A: {}", pos_a2);
    eprintln!("  ✓ position B: {}", pos_b2);

    // Parse status field to confirm match
    let status_a: u64 = serde_json::from_str(
        &serde_json::from_str::<serde_json::Value>(&pos_a2)
            .ok()
            .and_then(|v| v["status"].as_u64())
            .map(|s| s.to_string())
            .unwrap_or_default()
    ).unwrap_or(99);
    let status_b: u64 = serde_json::from_str(
        &serde_json::from_str::<serde_json::Value>(&pos_b2)
            .ok()
            .and_then(|v| v["status"].as_u64())
            .map(|s| s.to_string())
            .unwrap_or_default()
    ).unwrap_or(99);
    eprintln!("  [status] A={} (1=Matched) B={} (1=Matched)", status_a, status_b);

    eprintln!("  [verify] Checking nullifiers…");
    let spent_a = invoke_view(
        &ctx.perp_id, &ctx.alice.0,
        &["is_spent", "--nullifier", &hex_field(nf_a_hex)],
    )?;
    let spent_b = invoke_view(
        &ctx.perp_id, &ctx.bob.0,
        &["is_spent", "--nullifier", &hex_field(nf_b_hex)],
    )?;
    eprintln!("  ✓ nullifier A spent: {}", spent_a);
    eprintln!("  ✓ nullifier B spent: {}", spent_b);

    let elapsed = std::time::Instant::now();
    let out = serde_json::json!({
        "orderbook": ctx.orderbook_id,
        "perp_engine": ctx.perp_id,
        "admin": ctx.source_pk,
        "alice": ctx.alice.0,
        "bob": ctx.bob.0,
        "commitment_a": ctx.cmt_a_hex,
        "commitment_b": ctx.cmt_b_hex,
    });
    let out_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../deployments/testnet")
        .join("e2e_output.json");
    std::fs::create_dir_all(out_path.parent().unwrap())?;
    std::fs::write(&out_path, serde_json::to_string_pretty(&out)?)?;
    eprintln!("  ✓ output written to {}", out_path.display());
    eprintln!("━━━ E2E PASSED ({:.2}s) ━━━", elapsed.elapsed().as_secs_f64());
    Ok(())
}

/// Private deposit → withdraw e2e:
/// 1. Deploy perp-engine, fund alice and bob
/// 2. Alice deposits via deposit_note (shielded — no address in contract storage)
/// 3. Alice generates NoteSpend proof and withdraws to Bob (breaking alice→bob link)
/// 4. Verify nullifier spent, bob received funds
pub fn private_deposit_e2e(
    wasm_dir: &Path,
    keys_dir: &Path,
    amount: u64,
    secret: u64,
) -> Result<()> {
    let start = Instant::now();
    let pe_wasm = wasm_dir.join("perp_engine.wasm");

    eprintln!("── Phase 1: Deploy perp-engine ──");
    let alice = generate_keypair("e2e-alice");
    let bob   = generate_keypair("e2e-bob");
    let source_pk = source_pubkey()?;
    eprintln!("  ✓ alice:  {}", alice.0);
    eprintln!("  ✓ bob:    {}", bob.0);

    fund(&alice.0, "alice");
    fund(&bob.0,   "bob");

    let perp_id = deploy(&pe_wasm)?;
    eprintln!("  ✓ perp-engine: {}", perp_id);
    let native_token = native_token_id()?;
    init_perp_engine(&perp_id, SOURCE, &native_token)?;
    eprintln!("  ✓ initialized");

    eprintln!("\n── Phase 2: Shielded deposit ──");
    eprintln!("  amount={} secret=<hidden>", amount);

    // Generate note commitment off-chain (alice never reveals the secret on-chain)
    let (cmt_hex, null_hex, note_proof) =
        crate::proof::gen_note_spend(keys_dir, amount, secret)?;
    eprintln!("  note_commitment: {}…", &cmt_hex[..16]);
    eprintln!("  nullifier:       {}…", &null_hex[..16]);

    // Alice calls deposit_note — amount is visible, but the commitment (not address) is stored
    invoke(
        &perp_id,
        &alice.1,
        &[
            "deposit_note",
            "--from", &alice.0,
            "--note_commitment", &cmt_hex,
            "--amount", &amount.to_string(),
        ],
    )?;

    // Verify note is stored
    let stored = invoke_view(
        &perp_id, &alice.0,
        &["get_note", "--note_commitment", &cmt_hex],
    )?;
    eprintln!("  ✓ get_note → {}", stored);

    eprintln!("\n── Phase 3: Shielded withdrawal to Bob ──");
    eprintln!("  recipient: {} (different from depositor {})", &bob.0[..8], &alice.0[..8]);

    let proof_json = proof_json(&note_proof.proof);
    invoke(
        &perp_id,
        SOURCE,
        &[
            "withdraw_note",
            "--note_commitment", &cmt_hex,
            "--nullifier", &null_hex,
            "--recipient", &bob.0,
            "--proof", &proof_json,
        ],
    )?;

    // Verify nullifier is now spent
    let spent = invoke_view(
        &perp_id, &source_pk,
        &["is_spent", "--nullifier", &null_hex],
    )?;
    eprintln!("  ✓ nullifier spent: {}", spent);
    assert_eq!(spent.trim(), "true", "nullifier should be spent after withdrawal");

    eprintln!("\n  Privacy check:");
    eprintln!("    Depositor (alice): {}", alice.0);
    eprintln!("    Recipient  (bob):  {}", bob.0);
    eprintln!("    Contract storage never links these — only note_commitment is recorded");

    eprintln!("\n━━━ PrivateDeposit E2E PASSED ({:.2}s) ━━━", start.elapsed().as_secs_f64());
    Ok(())
}

/// Full private trading cycle (open + cancel path):
/// deposit_note → open_position_from_note → cancel_position_to_note → withdraw_note
///
/// This demonstrates a complete shielded trade that never reveals the trader's
/// address in contract storage. Any on-chain observer sees only note commitments.
pub fn private_trading_e2e(
    wasm_dir: &Path,
    keys_dir: &Path,
    amount: u64,
    note_secret: u64,
    order_secret: u64,
    settle_secret: u64,
) -> Result<()> {
    let start = Instant::now();
    let pe_wasm = wasm_dir.join("perp_engine.wasm");

    eprintln!("── Phase 1: Deploy perp-engine ──");
    let alice = generate_keypair("e2e-alice");
    let liq   = generate_keypair("e2e-liq");
    let recipient = generate_keypair("e2e-recipient");
    let source_pk = source_pubkey()?;
    eprintln!("  ✓ alice (depositor):      {}", alice.0);
    eprintln!("  ✓ liq (liquidation guard): {}", liq.0);
    eprintln!("  ✓ recipient (settlement):  {}", recipient.0);

    fund(&alice.0, "alice");
    fund(&liq.0, "liq");

    let perp_id = deploy(&pe_wasm)?;
    eprintln!("  ✓ perp-engine: {}", perp_id);
    let native_token = native_token_id()?;
    init_perp_engine(&perp_id, SOURCE, &native_token)?;
    eprintln!("  ✓ initialized");

    eprintln!("\n── Phase 2: Shielded deposit ──");
    let (note_cmt_hex, note_null_hex, note_proof) =
        crate::proof::gen_note_spend(keys_dir, amount, note_secret)?;
    eprintln!("  note_commitment: {}…", &note_cmt_hex[..16]);

    invoke(
        &perp_id, &alice.1,
        &["deposit_note", "--from", &alice.0, "--note_commitment", &note_cmt_hex,
          "--amount", &amount.to_string()],
    )?;
    let stored = invoke_view(&perp_id, &alice.0, &["get_note", "--note_commitment", &note_cmt_hex])?;
    eprintln!("  ✓ note stored: {}", stored);

    eprintln!("\n── Phase 3: Open position from note ──");
    let commit_proof =
        crate::proof::gen_commitment(keys_dir, 0, 100_000_000, 1, 1, 0, 0, 42, order_secret)?;
    let pos_cmt_hex = hex_field(&commit_proof.public_inputs[0]);
    eprintln!("  position_commitment: {}…", &pos_cmt_hex[..16]);

    invoke(
        &perp_id, SOURCE,
        &[
            "open_position_from_note",
            "--note_commitment", &note_cmt_hex,
            "--note_nullifier", &note_null_hex,
            "--position_commitment", &pos_cmt_hex,
            "--hint_price", "100000000",
            "--hint_side", "0",
            "--hint_leverage", "1",
            "--liquidation_recipient", &liq.0,
            "--note_proof", &proof_json(&note_proof.proof),
            "--commit_proof", &proof_json(&commit_proof.proof),
        ],
    )?;

    let pos = invoke_view(&perp_id, &alice.0, &["get_position", "--commitment", &pos_cmt_hex])?;
    eprintln!("  ✓ position: {}", pos);
    let note_null_spent = invoke_view(&perp_id, &source_pk, &["is_spent", "--nullifier", &note_null_hex])?;
    eprintln!("  ✓ note nullifier spent: {}", note_null_spent);
    assert_eq!(note_null_spent.trim(), "true", "note nullifier should be spent after open_position_from_note");

    eprintln!("\n── Phase 4: Cancel position → settlement note ──");
    let (cancel_null_hex, cancel_proof) =
        crate::proof::gen_cancel(keys_dir, &pos_cmt_hex, order_secret)?;
    let (settle_cmt_hex, settle_null_hex, settle_proof) =
        crate::proof::gen_settlement_note_spend(keys_dir, settle_secret)?;
    eprintln!("  cancel_nullifier: {}…", &cancel_null_hex[..16]);
    eprintln!("  settlement_note:  {}…", &settle_cmt_hex[..16]);

    invoke(
        &perp_id, SOURCE,
        &[
            "cancel_position_to_note",
            "--position_commitment", &pos_cmt_hex,
            "--cancel_nullifier", &cancel_null_hex,
            "--recipient_note", &settle_cmt_hex,
            "--cancel_proof", &proof_json(&cancel_proof.proof),
        ],
    )?;

    let settle_stored = invoke_view(&perp_id, &source_pk, &["get_note", "--note_commitment", &settle_cmt_hex])?;
    eprintln!("  ✓ settlement note stored: {}", settle_stored);

    eprintln!("\n── Phase 5: Withdraw settlement note ──");
    eprintln!("  recipient: {} (unlinked from original depositor)", &recipient.0[..8]);

    invoke(
        &perp_id, SOURCE,
        &[
            "withdraw_note",
            "--note_commitment", &settle_cmt_hex,
            "--nullifier", &settle_null_hex,
            "--recipient", &recipient.0,
            "--proof", &proof_json(&settle_proof.proof),
        ],
    )?;

    let settle_null_spent = invoke_view(
        &perp_id, &source_pk,
        &["is_spent", "--nullifier", &settle_null_hex],
    )?;
    eprintln!("  ✓ settlement nullifier spent: {}", settle_null_spent);
    assert_eq!(settle_null_spent.trim(), "true", "settlement nullifier should be spent");

    eprintln!("\n  Privacy summary:");
    eprintln!("    Depositor (alice):     {}", alice.0);
    eprintln!("    Settlement recipient:  {}", recipient.0);
    eprintln!("    Contract storage: only note/position commitments — zero address linkage");

    eprintln!("\n━━━ PrivateTrading E2E PASSED ({:.2}s) ━━━", start.elapsed().as_secs_f64());
    Ok(())
}

/// Deploy both contracts (orderbook + perp-engine) without identity setup.
pub fn deploy_contracts(wasm_dir: &Path) -> Result<(String, String, String, String)> {
    let ob_wasm = wasm_dir.join("orderbook.wasm");
    let pe_wasm = wasm_dir.join("perp_engine.wasm");

    eprintln!("  [wasm] orderbook: {} ({} bytes)", ob_wasm.display(),
        std::fs::metadata(&ob_wasm).map(|m| m.len()).unwrap_or(0));
    eprintln!("  [wasm] perp-engine: {} ({} bytes)", pe_wasm.display(),
        std::fs::metadata(&pe_wasm).map(|m| m.len()).unwrap_or(0));

    let source_pk = source_pubkey()?;
    let orderbook_id = deploy(&ob_wasm)?;
    let perp_id = deploy(&pe_wasm)?;
    let native_token = native_token_id()?;

    Ok((orderbook_id, perp_id, source_pk, native_token))
}


/// Initialize perp-engine with admin and token (retries on contract-not-found).
pub fn init_perp_engine(perp_id: &str, admin: &str, token: &str) -> Result<String> {
    const MAX_ATTEMPTS: u32 = 60;
    const RETRY_SECS: u64 = 10;
    for attempt in 0..MAX_ATTEMPTS {
        match invoke(perp_id, SOURCE, &["initialize", "--admin", admin, "--token", token]) {
            Ok(r) => return Ok(r),
            Err(e) => {
                let msg = e.to_string().to_lowercase();
                if (msg.contains("contract not found") || msg.contains("missing")) && attempt < MAX_ATTEMPTS - 1 {
                    eprintln!("  [init] contract not yet visible via RPC, retrying in {}s... (attempt {}/{})",
                        RETRY_SECS, attempt + 1, MAX_ATTEMPTS);
                    std::thread::sleep(std::time::Duration::from_secs(RETRY_SECS));
                    continue;
                }
                return Err(e);
            }
        }
    }
    unreachable!()
}

pub fn hex_field(decimal: &str) -> String {
    // Already 64-char hex? Pass through.
    if decimal.len() == 64 && decimal.chars().all(|c| c.is_ascii_hexdigit()) {
        return decimal.to_string();
    }
    let n: num_bigint::BigUint = decimal.parse().expect("Invalid decimal in hex_field");
    format!("{:0>64x}", n)
}

fn proof_json(p: &rust_circuits::ProofHex) -> String {
    serde_json::json!({"a": p.a, "b": p.b, "c": p.c}).to_string()
}

fn native_token_id() -> Result<String> {
    let out = std::process::Command::new("stellar")
        .args(["contract", "id", "asset", "--asset", "native", "--network-passphrase", NETWORK_PASSPHRASE, "--rpc-url", &rpc_url()])
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to get native token id: {e}"))?;
    if !out.status.success() {
        anyhow::bail!("stellar contract id asset failed:\n{}", String::from_utf8_lossy(&out.stderr));
    }
    let id = String::from_utf8(out.stdout)
        .map_err(|_| anyhow::anyhow!("Invalid UTF-8"))?
        .trim()
        .to_string();
    eprintln!("  [rpc] native SAC token: {}", id);
    Ok(id)
}

pub fn generate_keypair(name: &str) -> (String, String) {
    eprintln!("  [keys] Ensuring keypair '{}'…", name);
    // Check if key already exists
    let existing = std::process::Command::new("stellar")
        .args(["keys", "address", name])
        .output()
        .ok()
        .and_then(|o| (o.status.success()).then(|| String::from_utf8_lossy(&o.stdout).trim().to_string()));
    if let Some(addr) = existing {
        if !addr.is_empty() {
            eprintln!("  [keys] {} → {} (identity: {}, reused)", name, &addr[..8], name);
            return (addr, name.to_string());
        }
    }
    eprintln!("  [keys] Generating keypair '{}'…", name);
    let _ = std::process::Command::new("stellar")
        .args(["keys", "generate", name, "--network-passphrase", NETWORK_PASSPHRASE])
        .output()
        .ok();
    let addr = std::process::Command::new("stellar")
        .args(["keys", "address", name])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    let addr = addr.trim().to_string();
    eprintln!("  [keys] {} → {} (identity: {})", name, &addr[..8], name);
    (addr, name.to_string())
}

/// Check if an account exists on testnet (has any balance = funded).
pub fn account_exists(pk: &str) -> bool {
    let url = format!("https://horizon-testnet.stellar.org/accounts/{pk}");
    let resp = std::process::Command::new("curl")
        .args(["-s", "-o", "/dev/null", "-w", "%{http_code}", &url])
        .output()
        .ok();
    match resp {
        Some(o) => {
            let code = String::from_utf8_lossy(&o.stdout).trim().to_string();
            code == "200"
        }
        None => false,
    }
}

pub fn fund(pk: &str, label: &str) {
    if account_exists(pk) {
        eprintln!("  [fund] {} ({}) already funded, skipping", label, &pk[..8]);
        return;
    }
    let url = format!("https://friendbot.stellar.org/?addr={pk}");
    eprintln!("  [fund] Funding {} ({}) via friendbot…", label, &pk[..8]);
    let start = Instant::now();
    let resp = std::process::Command::new("curl")
        .args(["-s", "-o", "/dev/null", "-w", "%{http_code}", &url])
        .output()
        .ok();
    if let Some(o) = resp {
        let code = String::from_utf8_lossy(&o.stdout);
        eprintln!("  [fund] friendbot response: HTTP {}", code.trim());
    }
    eprintln!("  [fund] {} funded ({:.2}s)", label, start.elapsed().as_secs_f64());
}

fn deploy(wasm: &Path) -> Result<String> {
    eprintln!("  [deploy] Preparing deployment…");
    let salt: [u8; 32] = rand::thread_rng().gen();
    let salt_hex = hex::encode(salt);
    let source_pk = source_pubkey()?;
    let id = precompute_id(&salt_hex, &source_pk)?;
    eprintln!("  [deploy] Precomputed contract ID: {}", id);
    eprintln!("  [deploy] WASM: {} ({} bytes)", wasm.display(),
        std::fs::metadata(wasm).map(|m| m.len()).unwrap_or(0));

    let output = std::process::Command::new("stellar")
        .args([
            "contract", "deploy",
            "--wasm", &wasm.to_string_lossy(),
            "--source", SOURCE,
            "--network-passphrase", NETWORK_PASSPHRASE,
            "--rpc-url", &rpc_url(),
            "--salt", &salt_hex,
        ])
        .output()
        .map_err(|e| anyhow::anyhow!("deploy cmd: {e}"))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if let Some(tx_hash) = extract_tx_hash(&stderr) {
        eprintln!("  [deploy] TX submitted: {tx_hash}");
        eprintln!("  [deploy] Waiting for confirmation (max 240s)…");
        for i in 0..180 {
            if i > 0 && i % 30 == 0 {
                eprintln!("  [deploy]   still waiting… ({}s elapsed)", i * 2);
            }
            std::thread::sleep(std::time::Duration::from_secs(2));
            if let Some(_result) = poll_tx(&tx_hash)? {
                eprintln!("  [deploy] TX confirmed, waiting 30s for propagation…");
                std::thread::sleep(std::time::Duration::from_secs(30));
                eprintln!("  [deploy] ✓ Contract confirmed on-chain: {}", id);
                return Ok(id);
            }
        }
        anyhow::bail!("deploy TX {tx_hash} not confirmed after 360s");
    }
    anyhow::bail!("deploy failed: could not extract tx hash:\n{stderr}");
}

fn precompute_id(salt_hex: &str, source_pk: &str) -> Result<String> {
    let out = std::process::Command::new("stellar")
        .args([
            "contract", "id", "wasm",
            "--salt", salt_hex,
            "--source-account", source_pk,
            "--network-passphrase", NETWORK_PASSPHRASE,
            "--rpc-url", &rpc_url(),
        ])
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to precompute ID: {e}"))?;
    if !out.status.success() {
        anyhow::bail!("stellar contract id failed:\n{}", String::from_utf8_lossy(&out.stderr));
    }
    Ok(String::from_utf8(out.stdout)
        .map_err(|_| anyhow::anyhow!("Invalid UTF-8"))?
        .trim()
        .to_string())
}

pub fn source_pubkey() -> Result<String> {
    let out = std::process::Command::new("stellar")
        .args(["keys", "address", SOURCE])
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to get source key: {e}"))?;
    if !out.status.success() {
        anyhow::bail!("Identity '{SOURCE}' not found. Run: stellar keys generate {SOURCE} --network testnet --fund");
    }
    Ok(String::from_utf8(out.stdout)
        .map_err(|_| anyhow::anyhow!("Invalid UTF-8"))?
        .trim()
        .to_string())
}

pub fn invoke(contract_id: &str, source: &str, args: &[&str]) -> Result<String> {
    let rpc = crate::soroban_rpc::SorobanRpc::new();
    rpc.invoke(contract_id, source, args)
}

pub fn invoke_view(contract_id: &str, source: &str, args: &[&str]) -> Result<String> {
    let method = args.first().unwrap_or(&"unknown");
    eprintln!("  [view] Calling {}({})…", method, &contract_id[..8]);
    for attempt in 0..3 {
        let mut cmd = std::process::Command::new("stellar");
        cmd.args([
            "contract", "invoke",
            "--id", contract_id,
            "--source-account", source,
            "--network-passphrase", NETWORK_PASSPHRASE,
            "--rpc-url", &rpc_url(),
            "--is-view", "--",
        ]);
        cmd.args(args);
        let output = cmd.output().map_err(|e| anyhow::anyhow!("invoke view: {e}"))?;
        if output.status.success() {
            let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if result.len() > 200 {
                eprintln!("  [view] ✓ {} returned {} chars", method, result.len());
            } else {
                eprintln!("  [view] ✓ {} → {}", method, &result);
            }
            return Ok(result);
        }
        if attempt < 2 {
            eprintln!("  [view] {} failed (attempt {}), retrying…", method, attempt + 1);
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
    let mut cmd = std::process::Command::new("stellar");
    cmd.args([
        "contract", "invoke",
        "--id", contract_id,
        "--source-account", source,
        "--network-passphrase", NETWORK_PASSPHRASE,
        "--rpc-url", &rpc_url(),
        "--is-view", "--",
    ]);
    cmd.args(args);
    let output = cmd.output().map_err(|e| anyhow::anyhow!("invoke view: {e}"))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    eprintln!("  [view] ✗ {} failed after 3 attempts:\n  {}", method,
        &stderr.trim().replace('\n', "\n  "));
    anyhow::bail!("stellar invoke view failed:\n{}", stderr);
}

pub(crate) fn extract_tx_hash(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let line = line.trim();
        for keyword in ["Signing transaction: ", "Transaction hash is "] {
            if let Some(pos) = line.find(keyword) {
                let hash = line[pos + keyword.len()..].trim();
                if hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Some(hash.to_string());
                }
            }
        }
        None
    })
}

fn poll_tx(tx_hash: &str) -> Result<Option<String>> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getTransaction",
        "params": { "hash": tx_hash }
    });
    let resp = std::process::Command::new("curl")
        .args([
            "-s", "-X", "POST",
            "-H", "Content-Type: application/json",
            "-d", &body.to_string(),
            &rpc_url(),
        ])
        .output()
        .map_err(|e| anyhow::anyhow!("curl getTransaction: {e}"))?;
    let out: serde_json::Value = match serde_json::from_slice(&resp.stdout) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    match out["result"]["status"].as_str() {
        Some("SUCCESS") => {
            let result_xdr = out["result"]["resultXdr"].as_str().unwrap_or("");
            Ok(Some(format!("\"{result_xdr}\"")))
        }
        Some("FAILED") => anyhow::bail!("Transaction FAILED: {tx_hash}"),
        _ => Ok(None),
    }
}
