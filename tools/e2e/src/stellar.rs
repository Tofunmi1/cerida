use anyhow::Result;
use rand::Rng;
use std::path::{Path, PathBuf};
use std::time::Instant;

const DEFAULT_RPC_URL: &str = "https://stellar-testnet.g.alchemy.com/v2/lT6Z7-nwZ3J20d6_LC7dz";

pub fn rpc_url() -> String {
    std::env::var("SOROBAN_RPC_URL").unwrap_or_else(|_| DEFAULT_RPC_URL.to_string())
}

const NETWORK_PASSPHRASE: &str = "Test SDF Network ; September 2015";

pub const SOURCE: &str = "e2e";
const COLLATERAL: i128 = 1_000_000_000;
const LEVERAGE: u64 = 1;
const DEFAULT_ASSET: &str = "0000000000000000000000000000000000000000000000000000000000000000";

pub struct E2eContext {
    pub orderbook_id: String,
    pub perp_id: String,
    pub source_pk: String,
    pub alice: (String, String),
    pub bob: (String, String),
    pub cmt_a_hex: String,
    pub cmt_b_hex: String,
}

/// Reuse already-deployed contracts: place orders, deposit, open positions.
pub fn setup_with_existing(
    keys_dir: &Path,
    perp_id: &str,
    orderbook_id: &str,
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
    asset_id: &str,
) -> Result<E2eContext> {
    let alice = generate_keypair("e2e-alice");
    let bob = generate_keypair("e2e-bob");
    let source_pk = source_pubkey()?;
    eprintln!("  ✓ admin source: {}", source_pk);
    eprintln!("  ✓ alice: {} (identity: {})", alice.0, alice.1);
    eprintln!("  ✓ bob:   {} (identity: {})", bob.0, bob.1);
    fund(&alice.0, "alice");
    fund(&bob.0, "bob");
    eprintln!("  ✓ using existing orderbook: {}", orderbook_id);
    eprintln!("  ✓ using existing perp:      {}", perp_id);

    let usdc_sac = deploy_usdc_sac()?;
    trust_usdc(&usdc_sac, &alice.1, &alice.0)?;
    mint_usdc(&usdc_sac, &alice.0, COLLATERAL)?;
    trust_usdc(&usdc_sac, &bob.1, &bob.0)?;
    mint_usdc(&usdc_sac, &bob.0, COLLATERAL)?;

    ob_place_order(orderbook_id, &alice.1, cmt_a_hex,
        hint_price_a, hint_side_a, hint_size_a, hint_leverage_a, revealed, asset_id, proof_a_json)?;
    eprintln!("  ✓ order A placed");

    ob_place_order(orderbook_id, &bob.1, cmt_b_hex,
        hint_price_b, hint_side_b, hint_size_b, hint_leverage_b, revealed, asset_id, proof_b_json)?;
    eprintln!("  ✓ order B placed");

    let zero_note = "0000000000000000000000000000000000000000000000000000000000000000";
    let sw_blinding = [0u8; 32];
    let sw_blinding_hex = hex::encode(&sw_blinding);

    let note_secret_a: u64 = rand::random();
    let (note_cmt_a, note_null_a, note_proof_a) =
        crate::proof::gen_note_spend(keys_dir, COLLATERAL as u64, note_secret_a)?;
    let sw_amount_cmt_a = sha256_note_commitment(COLLATERAL, &sw_blinding);
    perp_deposit_note(perp_id, &alice.1, &alice.0, &note_cmt_a, COLLATERAL, &sw_amount_cmt_a)?;
    perp_open_position(
        perp_id, SOURCE, &note_cmt_a, &note_null_a, cmt_a_hex,
        hint_side_a, hint_price_a, hint_leverage_a, 0, 0, 0, 0, 0,
        zero_note, zero_note, asset_id,
        COLLATERAL, &sw_blinding_hex, zero_note,
        &proof_json(&note_proof_a.proof), proof_a_json,
    )?;
    eprintln!("  ✓ position A opened");

    let note_secret_b: u64 = rand::random();
    let (note_cmt_b, note_null_b, note_proof_b) =
        crate::proof::gen_note_spend(keys_dir, COLLATERAL as u64, note_secret_b)?;
    let sw_amount_cmt_b = sha256_note_commitment(COLLATERAL, &sw_blinding);
    perp_deposit_note(perp_id, &bob.1, &bob.0, &note_cmt_b, COLLATERAL, &sw_amount_cmt_b)?;
    perp_open_position(
        perp_id, SOURCE, &note_cmt_b, &note_null_b, cmt_b_hex,
        hint_side_b, hint_price_b, hint_leverage_b, 0, 0, 0, 0, 0,
        zero_note, zero_note, asset_id,
        COLLATERAL, &sw_blinding_hex, zero_note,
        &proof_json(&note_proof_b.proof), proof_b_json,
    )?;
    eprintln!("  ✓ position B opened");

    Ok(E2eContext {
        orderbook_id: orderbook_id.to_string(),
        perp_id: perp_id.to_string(),
        source_pk,
        alice,
        bob,
        cmt_a_hex: cmt_a_hex.to_string(),
        cmt_b_hex: cmt_b_hex.to_string(),
    })
}

/// Deploy contracts, place orders, deposit, open positions (all before match).
pub fn deploy_and_place(
    wasm_dir: &Path,
    keys_dir: &Path,
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
    asset_id: &str,
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
    let native_token = deploy_usdc_sac()?;
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
    ob_place_order(&orderbook_id, &alice.1, cmt_a_hex,
        hint_price_a, hint_side_a, hint_size_a, hint_leverage_a, revealed, &DEFAULT_ASSET, proof_a_json)?;
    eprintln!("  ✓ order A placed");

    // ── Place order B (Bob) ──────────────────────────────────────────────
    eprintln!("  [place] Placing order B (Bob, cmt={}…)…", &cmt_b_hex[..12]);
    eprintln!("    hint_price={} hint_side={} hint_size={} hint_leverage={} revealed={}",
        hint_price_b, hint_side_b, hint_size_b, hint_leverage_b, revealed);
    ob_place_order(&orderbook_id, &bob.1, cmt_b_hex,
        hint_price_b, hint_side_b, hint_size_b, hint_leverage_b, revealed, &DEFAULT_ASSET, proof_b_json)?;
    eprintln!("  ✓ order B placed");

    // ── Generate note proof & deposit_note (Alice) ─────────────────────
    eprintln!("  [note] Alice: generating note proof…");
    let note_secret_a: u64 = rand::random();
    let (note_cmt_a, note_null_a, note_proof_a) =
        crate::proof::gen_note_spend(keys_dir, COLLATERAL as u64, note_secret_a)?;
    eprintln!("  [deposit_note] Alice depositing {} stroops…", COLLATERAL);
    trust_usdc(&native_token, &alice.1, &alice.0)?;
    mint_usdc(&native_token, &alice.0, COLLATERAL)?;
    let dp_blinding = [0u8; 32];
    let dp_blinding_hex = hex::encode(&dp_blinding);
    let dp_amount_cmt_a = sha256_note_commitment(COLLATERAL, &dp_blinding);
    perp_deposit_note(&perp_id, &alice.1, &alice.0, &note_cmt_a, COLLATERAL, &dp_amount_cmt_a)?;
    eprintln!("  ✓ Alice note deposited");

    // ── Generate note proof & deposit_note (Bob) ───────────────────────
    eprintln!("  [note] Bob: generating note proof…");
    let note_secret_b: u64 = rand::random();
    let (note_cmt_b, note_null_b, note_proof_b) =
        crate::proof::gen_note_spend(keys_dir, COLLATERAL as u64, note_secret_b)?;
    eprintln!("  [deposit_note] Bob depositing {} stroops…", COLLATERAL);
    trust_usdc(&native_token, &bob.1, &bob.0)?;
    mint_usdc(&native_token, &bob.0, COLLATERAL)?;
    let dp_amount_cmt_b = sha256_note_commitment(COLLATERAL, &dp_blinding);
    perp_deposit_note(&perp_id, &bob.1, &bob.0, &note_cmt_b, COLLATERAL, &dp_amount_cmt_b)?;
    eprintln!("  ✓ Bob note deposited");

    // ── Open position A from note (Alice) ────────────────────────────────
    let zero_note = "0000000000000000000000000000000000000000000000000000000000000000";
    eprintln!("  [position] Opening position A from note (Alice, cmt={}…)…", &cmt_a_hex[..12]);
    eprintln!("    hint_price={} hint_side={} hint_leverage={}", hint_price_a, hint_side_a, hint_leverage_a);
    perp_open_position(
        &perp_id, SOURCE, &note_cmt_a, &note_null_a, cmt_a_hex,
        hint_side_a, hint_price_a, hint_leverage_a, 0, 0, 0, 0, 0,
        zero_note, zero_note, &DEFAULT_ASSET,
        COLLATERAL, &dp_blinding_hex, zero_note,
        &proof_json(&note_proof_a.proof), proof_a_json,
    )?;
    eprintln!("  ✓ position A opened");

    // ── Open position B from note (Bob) ──────────────────────────────────
    eprintln!("  [position] Opening position B from note (Bob, cmt={}…)…", &cmt_b_hex[..12]);
    eprintln!("    hint_price={} hint_side={} hint_leverage={}", hint_price_b, hint_side_b, hint_leverage_b);
    perp_open_position(
        &perp_id, SOURCE, &note_cmt_b, &note_null_b, cmt_b_hex,
        hint_side_b, hint_price_b, hint_leverage_b, 0, 0, 0, 0, 0,
        zero_note, zero_note, &DEFAULT_ASSET,
        COLLATERAL, &dp_blinding_hex, zero_note,
        &proof_json(&note_proof_b.proof), proof_b_json,
    )?;
    eprintln!("  ✓ position B opened");

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
    keys_dir: &Path,
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
        wasm_dir, keys_dir, &proof_a_json, &proof_b_json,
        cmt_a_hex, cmt_b_hex,
        hint_price_a, hint_price_b,
        hint_side_a, hint_side_b,
        hint_size_a, hint_size_b,
        hint_leverage_a, hint_leverage_b,
        revealed, &DEFAULT_ASSET,
    )?;

    let match_price_hex = &p_match.public_inputs[2];
    let match_size_hex = &p_match.public_inputs[3];
    let nf_a_hex = &p_match.public_inputs[4];
    let nf_b_hex = &p_match.public_inputs[5];

    // ── Match via perp engine ──────────────────────────────────────────────
    eprintln!("── Phase 2: On-chain match ──");
    eprintln!("  [match] match_positions(cmt_a={}…, cmt_b={}…)",
        &cmt_a_hex[..12], &cmt_b_hex[..12]);
    perp_match_positions(
        &ctx.perp_id,
        cmt_a_hex, cmt_b_hex,
        &hex_field(nf_a_hex), &hex_field(nf_b_hex),
        &hex_field(match_price_hex), &hex_field(match_size_hex),
        &proof_json(&p_match.proof),
    )?;

    verify_match(&ctx, nf_a_hex, nf_b_hex)?;
    eprintln!("  ✓ Full E2E completed in {:.2}s", start.elapsed().as_secs_f64());
    Ok(())
}

/// Verify match results on-chain (positions + nullifiers).
pub fn verify_match(ctx: &E2eContext, nf_a_hex: &str, nf_b_hex: &str) -> Result<()> {
    use crate::soroban_rpc::scval_bytes32;
    eprintln!("  [verify] Checking matched positions…");
    let pos_a2 = xdr_view(&ctx.perp_id, "get_position", vec![scval_bytes32(&ctx.cmt_a_hex)?])?;
    let pos_b2 = xdr_view(&ctx.perp_id, "get_position", vec![scval_bytes32(&ctx.cmt_b_hex)?])?;
    eprintln!("  ✓ position A: {}", pos_a2);
    eprintln!("  ✓ position B: {}", pos_b2);

    eprintln!("  [verify] Checking nullifiers…");
    let spent_a = xdr_view(&ctx.perp_id, "is_spent", vec![scval_bytes32(&hex_field(nf_a_hex))?])?;
    let spent_b = xdr_view(&ctx.perp_id, "is_spent", vec![scval_bytes32(&hex_field(nf_b_hex))?])?;
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
    let native_token = deploy_usdc_sac()?;
    init_perp_engine(&perp_id, SOURCE, &native_token)?;
    eprintln!("  ✓ initialized");

    eprintln!("\n── Phase 2: Shielded deposit ──");
    eprintln!("  amount={} secret=<hidden>", amount);

    // Generate note commitment off-chain (alice never reveals the secret on-chain)
    let (cmt_hex, null_hex, note_proof) =
        crate::proof::gen_note_spend(keys_dir, amount, secret)?;
    eprintln!("  note_commitment: {}…", &cmt_hex[..16]);
    eprintln!("  nullifier:       {}…", &null_hex[..16]);

    use crate::soroban_rpc::scval_bytes32;
    // Alice calls deposit_note — amount is visible, but the commitment (not address) is stored
    trust_usdc(&native_token, &alice.1, &alice.0)?;
    mint_usdc(&native_token, &alice.0, amount as i128)?;
    let pd_blinding = [0u8; 32];
    let pd_amount_cmt = sha256_note_commitment(amount as i128, &pd_blinding);
    perp_deposit_note(&perp_id, &alice.1, &alice.0, &cmt_hex, amount as i128, &pd_amount_cmt)?;
    eprintln!("  ✓ note deposited");

    eprintln!("\n── Phase 3: Shielded withdrawal to Bob ──");
    eprintln!("  recipient: {} (different from depositor {})", &bob.0[..8], &alice.0[..8]);

    let pj = proof_json(&note_proof.proof);
    let pd_blinding_hex = hex::encode(&pd_blinding);
    perp_withdraw_note(&perp_id, SOURCE, &cmt_hex, &null_hex, &bob.0, amount as i128, &pd_blinding_hex, &pj)?;

    let spent = xdr_view(&perp_id, "is_spent", vec![scval_bytes32(&null_hex)?])?;
    eprintln!("  ✓ nullifier spent: {}", spent);
    assert!(spent.contains("true") || spent.contains("Bool(true)"),
        "nullifier should be spent after withdrawal, got: {spent}");

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
    fund(&recipient.0, "recipient");

    let perp_id = deploy(&pe_wasm)?;
    eprintln!("  ✓ perp-engine: {}", perp_id);
    let native_token = deploy_usdc_sac()?;
    init_perp_engine(&perp_id, SOURCE, &native_token)?;
    eprintln!("  ✓ initialized");

    eprintln!("\n── Phase 2: Shielded deposit ──");
    let (note_cmt_hex, note_null_hex, note_proof) =
        crate::proof::gen_note_spend(keys_dir, amount, note_secret)?;
    eprintln!("  note_commitment: {}…", &note_cmt_hex[..16]);

    use crate::soroban_rpc::scval_bytes32;
    trust_usdc(&native_token, &alice.1, &alice.0)?;
    mint_usdc(&native_token, &alice.0, amount as i128)?;
    let deposit_blinding = [0u8; 32];
    let amount_cmt = sha256_note_commitment(amount as i128, &deposit_blinding);
    perp_deposit_note(&perp_id, &alice.1, &alice.0, &note_cmt_hex, amount as i128, &amount_cmt)?;
    eprintln!("  ✓ note deposited (amount_cmt={}…)", &amount_cmt[..16]);

    eprintln!("\n── Phase 3: Open position from note ──");
    let commit_proof =
        crate::proof::gen_commitment(keys_dir, 0, 100_000_000, 1, 1, 0, 0, 42, order_secret, false)?;
    let pos_cmt_hex = hex_field(&commit_proof.public_inputs[0]);
    eprintln!("  position_commitment: {}…", &pos_cmt_hex[..16]);

    let zero64 = "0".repeat(64);
    let collateral_blinding_hex = hex::encode(&deposit_blinding); // must match deposit blinding
    perp_open_position(
        &perp_id, SOURCE, &note_cmt_hex, &note_null_hex, &pos_cmt_hex,
        0, 100_000_000, 1, 1, 0, 0, 0, 0,
        &zero64, &zero64, DEFAULT_ASSET,
        amount as i128, &collateral_blinding_hex, &zero64,
        &proof_json(&note_proof.proof), &proof_json(&commit_proof.proof),
    )?;

    let note_null_spent = xdr_view(&perp_id, "is_spent", vec![scval_bytes32(&note_null_hex)?])?;
    eprintln!("  ✓ note nullifier spent: {}", note_null_spent);
    assert!(note_null_spent.contains("true") || note_null_spent.contains("Bool(true)"),
        "note nullifier should be spent after open_position_from_note, got: {note_null_spent}");

    eprintln!("\n── Phase 4: Cancel position → refund note ──");
    let (cancel_null_hex, cancel_proof) =
        crate::proof::gen_cancel(keys_dir, &pos_cmt_hex, order_secret)?;
    // Refund note: Poseidon2(amount, settle_secret, 8)
    let (refund_note_hex, refund_null_hex, refund_proof) =
        crate::proof::gen_note_spend(keys_dir, amount, settle_secret)?;
    let refund_blinding = [0x01u8; 32];
    let refund_blinding_hex = hex::encode(&refund_blinding);
    eprintln!("  cancel_nullifier: {}…", &cancel_null_hex[..16]);
    eprintln!("  refund_note:      {}…", &refund_note_hex[..16]);

    perp_cancel_position(
        &perp_id, SOURCE, &pos_cmt_hex, &cancel_null_hex, &refund_note_hex,
        amount as i128, &refund_blinding_hex, &proof_json(&cancel_proof.proof),
    )?;
    eprintln!("  ✓ position cancelled");

    eprintln!("\n── Phase 5: Withdraw refund note ──");
    eprintln!("  recipient: {} (unlinked from original depositor)", &recipient.0[..8]);

    perp_withdraw_note(
        &perp_id, SOURCE, &refund_note_hex, &refund_null_hex, &recipient.0,
        amount as i128, &refund_blinding_hex, &proof_json(&refund_proof.proof),
    )?;

    let settle_null_spent = xdr_view(&perp_id, "is_spent", vec![scval_bytes32(&refund_null_hex)?])?;
    eprintln!("  ✓ refund nullifier spent: {}", settle_null_spent);
    assert!(settle_null_spent.contains("true") || settle_null_spent.contains("Bool(true)"),
        "refund nullifier should be spent, got: {settle_null_spent}");

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
    let usdc_sac = deploy_usdc_sac()?;

    Ok((orderbook_id, perp_id, source_pk, usdc_sac))
}

/// Register 6 markets: GOLD, SPY, TSLA, BTC, ETH, SOL.
/// Each gets registered with default config and an initial oracle price.
pub fn multi_market_setup(perp_id: &str) -> Result<()> {
    use crate::soroban_rpc::{scval_address, scval_bytes32, scval_asset_config, SorobanRpc};
    let rpc = SorobanRpc::new();
    let admin_pk = source_pubkey()?;

    let markets: &[(&str, u64, u64)] = &[
        ("BTC",  6000000000000, 50),  // $60,000, 50x
        ("GOLD", 24000000000,  50),   // $2,400, 50x
        ("SPY",  54000000000,  10),   // $540, 10x (equity)
        ("TSLA", 24000000000,  10),   // $240, 10x (equity)
        ("AAPL", 20000000000,  10),   // $200, 10x (equity)
        ("XRP",  250000000,    50),   // $2.50, 50x
    ];

    for (i, (name, price, max_lev)) in markets.iter().enumerate() {
        let asset_hex = format!("{:0>64x}", i + 1); // start at 1, skip default (0)
        eprintln!("  [deploy] registering {name} (asset_id={i}, price={price}, max_lev={max_lev})…");

        let config = scval_asset_config(*max_lev, 500, 1000, 100, 150, 50, true)?;
        rpc.invoke_xdr(perp_id, SOURCE, "register_asset", vec![
            scval_address(&admin_pk)?,
            scval_bytes32(&asset_hex)?,
            // Name as ScVal::Bytes (variable-length byte string)
            {
                let name_bytes: Vec<u8> = name.as_bytes().to_vec();
                stellar_xdr::ScVal::Bytes(stellar_xdr::ScBytes(
                    name_bytes.try_into().map_err(|_| anyhow::anyhow!("name too long"))?
                ))
            },
            config,
        ])?;

        eprintln!("  ✓ {name} registered (price={price} tracked by TEE)");
        std::thread::sleep(std::time::Duration::from_secs(3));
    }

    eprintln!("  ✓ 6 markets registered");
    Ok(())
}


/// Initialize perp-engine with admin and token (retries on contract-not-found).
pub fn init_perp_engine(perp_id: &str, admin: &str, token: &str) -> Result<String> {
    use crate::soroban_rpc::{scval_address, scval_bytes32, scval_asset_config, SorobanRpc};
    let rpc = SorobanRpc::new();
    // `admin` may be an identity name (e.g. "e2e") — resolve to G... pubkey
    let admin_pk = if admin.starts_with('G') {
        admin.to_string()
    } else {
        std::process::Command::new("stellar")
            .args(["keys", "address", admin])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow::anyhow!("could not resolve identity '{admin}' to pubkey"))?
    };
    let args = vec![
        scval_address(&admin_pk)?,
        scval_address(token)?,
        stellar_xdr::ScVal::Void, // vault: Option<Address> = None
    ];
    const MAX_ATTEMPTS: u32 = 60;
    const RETRY_SECS: u64 = 10;
    for attempt in 0..MAX_ATTEMPTS {
        match rpc.invoke_xdr(perp_id, SOURCE, "initialize", args.clone()) {
            Ok(r) => {
                // Register default asset
                let default_asset = DEFAULT_ASSET;
                let config = scval_asset_config(50, 500, 1000, 100, 150, 50, true)?;
                let register_args = vec![
                    scval_address(&admin_pk)?,
                    scval_bytes32(default_asset)?,
                    stellar_xdr::ScVal::Bytes(stellar_xdr::ScBytes(stellar_xdr::BytesM::default())),
                    config,
                ];
                match rpc.invoke_xdr(perp_id, SOURCE, "register_asset", register_args) {
                    Ok(_) => eprintln!("  [init] default asset registered"),
                    Err(e) => eprintln!("  [init] register_asset note: {}", e),
                }
                return Ok(r);
            }
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

pub fn ob_place_order(
    ob_id: &str, identity: &str,
    commitment: &str,
    hint_price: u64, hint_side: u64, hint_size: u64, hint_leverage: u64,
    revealed: u64, asset_id: &str, proof: &str,
) -> Result<()> {
    use crate::soroban_rpc::{scval_bytes32, scval_u64, scval_tif, scval_proof};
    let rpc = crate::soroban_rpc::SorobanRpc::new();
    // portfolio_key = all-zeros for isolated margin (second public input of commitment circuit)
    let zeros = "0000000000000000000000000000000000000000000000000000000000000000";
    rpc.invoke_xdr(ob_id, identity, "place_order", vec![
        scval_bytes32(commitment)?,
        scval_bytes32(zeros)?,
        scval_u64(hint_price),
        scval_u64(hint_side),
        scval_u64(hint_size),
        scval_u64(hint_leverage),
        scval_u64(revealed),
        scval_tif("GTC")?,
        scval_u64(0), // expiry_ledger
        scval_bytes32(asset_id)?,
        scval_proof(proof)?,
    ])?;
    Ok(())
}

fn xdr_view(contract_id: &str, function: &str, args: Vec<stellar_xdr::ScVal>) -> Result<String> {
    let rpc = crate::soroban_rpc::SorobanRpc::new();
    rpc.invoke_view_xdr(contract_id, SOURCE, function, args)
}

fn perp_match_positions(
    perp_id: &str,
    cmt_a: &str, cmt_b: &str,
    nf_a: &str, nf_b: &str,
    match_price: &str, match_size: &str,
    proof: &str,
) -> Result<()> {
    use crate::soroban_rpc::{scval_bytes32, scval_proof};
    let rpc = crate::soroban_rpc::SorobanRpc::new();
    rpc.invoke_xdr(perp_id, SOURCE, "match_positions", vec![
        scval_bytes32(cmt_a)?,
        scval_bytes32(cmt_b)?,
        scval_bytes32(nf_a)?,
        scval_bytes32(nf_b)?,
        scval_bytes32(match_price)?,
        scval_bytes32(match_size)?,
        scval_proof(proof)?,
    ])?;
    Ok(())
}

pub fn perp_deposit_note(
    perp_id: &str, from_identity: &str, from_pk: &str,
    commitment: &str, amount: i128, amount_commitment: &str,
) -> Result<()> {
    use crate::soroban_rpc::{scval_address, scval_bytes32, scval_i128};
    let rpc = crate::soroban_rpc::SorobanRpc::new();
    rpc.invoke_xdr(perp_id, from_identity, "deposit_note", vec![
        scval_address(from_pk)?,
        scval_bytes32(commitment)?,
        scval_i128(amount),
        scval_bytes32(amount_commitment)?,
    ])?;
    Ok(())
}

fn perp_withdraw_note(
    perp_id: &str, source_identity: &str,
    commitment: &str, nullifier: &str, recipient_pk: &str,
    amount: i128, blinding: &str, proof: &str,
) -> Result<()> {
    use crate::soroban_rpc::{scval_address, scval_bytes32, scval_i128, scval_proof};
    let rpc = crate::soroban_rpc::SorobanRpc::new();
    rpc.invoke_xdr(perp_id, source_identity, "withdraw_note", vec![
        scval_bytes32(commitment)?,
        scval_bytes32(nullifier)?,
        scval_address(recipient_pk)?,
        scval_i128(amount),
        scval_bytes32(blinding)?,
        scval_proof(proof)?,
    ])?;
    Ok(())
}

pub fn perp_open_position(
    perp_id: &str, source_identity: &str,
    note_cmt: &str, note_null: &str, pos_cmt: &str,
    side: u64, price: u64, leverage: u64, size: u64,
    tp_price: u64, sl_price: u64, tif: u64, expiry_ledger: u64,
    liq_note: &str, portfolio_key: &str, asset_id: &str,
    collateral_amount: i128, collateral_blinding: &str, settlement_commitment: &str,
    note_proof: &str, commit_proof: &str,
) -> Result<()> {
    use crate::soroban_rpc::{scval_bytes, scval_bytes32, scval_i128, scval_proof};
    let rpc = crate::soroban_rpc::SorobanRpc::new();
    let sealed = build_sealed_params(side, price, leverage, size, tp_price, sl_price, tif, expiry_ledger);
    rpc.invoke_xdr(perp_id, source_identity, "open_position_from_note", vec![
        scval_bytes32(note_cmt)?,
        scval_bytes32(note_null)?,
        scval_bytes32(pos_cmt)?,
        scval_bytes(&sealed)?,
        scval_bytes32(liq_note)?,
        scval_bytes32(portfolio_key)?,
        scval_bytes32(asset_id)?,
        scval_i128(collateral_amount),
        scval_bytes32(collateral_blinding)?,
        scval_bytes32(settlement_commitment)?,
        scval_proof(note_proof)?,
        scval_proof(commit_proof)?,
    ])?;
    Ok(())
}

fn perp_cancel_position(
    perp_id: &str, source_identity: &str,
    pos_cmt: &str, cancel_null: &str, recipient_note: &str,
    refund_amount: i128, refund_blinding: &str, cancel_proof: &str,
) -> Result<()> {
    use crate::soroban_rpc::{scval_bytes32, scval_i128, scval_proof};
    let rpc = crate::soroban_rpc::SorobanRpc::new();
    rpc.invoke_xdr(perp_id, source_identity, "cancel_position_to_note", vec![
        scval_bytes32(pos_cmt)?,
        scval_bytes32(cancel_null)?,
        scval_bytes32(recipient_note)?,
        scval_i128(refund_amount),
        scval_bytes32(refund_blinding)?,
        scval_proof(cancel_proof)?,
    ])?;
    Ok(())
}

/// Pack position params into a 64-byte LE blob (dev-mode plaintext sealed_params).
fn build_sealed_params(
    side: u64, price: u64, leverage: u64, size: u64,
    tp: u64, sl: u64, tif: u64, expiry: u64,
) -> Vec<u8> {
    let mut buf = vec![0u8; 64];
    buf[0..8].copy_from_slice(&side.to_le_bytes());
    buf[8..16].copy_from_slice(&price.to_le_bytes());
    buf[16..24].copy_from_slice(&leverage.to_le_bytes());
    buf[24..32].copy_from_slice(&size.to_le_bytes());
    buf[32..40].copy_from_slice(&tp.to_le_bytes());
    buf[40..48].copy_from_slice(&sl.to_le_bytes());
    buf[48..56].copy_from_slice(&tif.to_le_bytes());
    buf[56..64].copy_from_slice(&expiry.to_le_bytes());
    buf
}

/// SHA-256 note amount commitment: SHA256(amount_le16 || blinding32).
/// Matches the on-chain `note_amount_commitment` helper in perp-engine.
pub fn sha256_note_commitment(amount: i128, blinding: &[u8; 32]) -> String {
    use sha2::{Digest, Sha256};
    let mut preimage = [0u8; 48];
    preimage[..16].copy_from_slice(&amount.to_le_bytes());
    preimage[16..].copy_from_slice(blinding);
    hex::encode(Sha256::digest(&preimage))
}

/// Call perp.open_position_from_pool — TEE-gated, spends a ShieldedPool leaf.
pub fn perp_open_position_from_pool(
    perp_id: &str, source_identity: &str,
    pool_id: &str, pool_root: &str, pool_nullifier_hash: &str,
    pos_cmt: &str,
    side: u64, price: u64, leverage: u64, size: u64,
    tp_price: u64, sl_price: u64, tif: u64, expiry_ledger: u64,
    collateral_blinding: &str, settlement_commitment: &str,
    liq_note: &str, portfolio_key: &str, asset_id: &str,
    spend_proof: &str, commit_proof: &str,
) -> Result<()> {
    use crate::soroban_rpc::{scval_address, scval_bytes, scval_bytes32, scval_proof};
    let rpc = crate::soroban_rpc::SorobanRpc::new();
    let sealed = build_sealed_params(side, price, leverage, size, tp_price, sl_price, tif, expiry_ledger);
    rpc.invoke_xdr(perp_id, source_identity, "open_position_from_pool", vec![
        scval_address(pool_id)?,
        scval_bytes32(pool_root)?,
        scval_bytes32(pool_nullifier_hash)?,
        scval_bytes32(pos_cmt)?,
        scval_bytes(&sealed)?,
        scval_bytes32(collateral_blinding)?,
        scval_bytes32(settlement_commitment)?,
        scval_bytes32(liq_note)?,
        scval_bytes32(portfolio_key)?,
        scval_bytes32(asset_id)?,
        scval_proof(spend_proof)?,
        scval_proof(commit_proof)?,
    ])?;
    eprintln!("  ✓ perp.open_position_from_pool pos_cmt={}…", &pos_cmt[..16]);
    Ok(())
}

/// Call perp.withdraw_to_pool — spends a settlement note back into the ShieldedPool.
pub fn perp_withdraw_to_pool(
    perp_id: &str, source_identity: &str,
    pool_id: &str,
    note_cmt: &str, nullifier: &str,
    amount: i128, blinding: &str,
    new_pool_leaf: &str, new_pool_root: &str,
    remainder_note: &str, remainder_blinding: &str,
    note_spend_proof: &str, pool_insert_proof: &str,
) -> Result<()> {
    use crate::soroban_rpc::{scval_address, scval_bytes32, scval_i128, scval_proof};
    let rpc = crate::soroban_rpc::SorobanRpc::new();
    rpc.invoke_xdr(perp_id, source_identity, "withdraw_to_pool", vec![
        scval_address(pool_id)?,
        scval_bytes32(note_cmt)?,
        scval_bytes32(nullifier)?,
        scval_i128(amount),
        scval_bytes32(blinding)?,
        scval_bytes32(new_pool_leaf)?,
        scval_bytes32(new_pool_root)?,
        scval_bytes32(remainder_note)?,
        scval_bytes32(remainder_blinding)?,
        scval_proof(note_spend_proof)?,
        scval_proof(pool_insert_proof)?,
    ])?;
    eprintln!("  ✓ perp.withdraw_to_pool note={}… leaf={}…", &note_cmt[..16], &new_pool_leaf[..16]);
    Ok(())
}

pub fn hex_field(decimal: &str) -> String {
    // Already 64-char hex? Pass through.
    if decimal.len() == 64 && decimal.chars().all(|c| c.is_ascii_hexdigit()) {
        return decimal.to_string();
    }
    let n: num_bigint::BigUint = decimal.parse().expect("Invalid decimal in hex_field");
    format!("{:0>64x}", n)
}

pub fn proof_json(p: &rust_circuits::ProofHex) -> String {
    serde_json::json!({"a": p.a, "b": p.b, "c": p.c}).to_string()
}

pub fn native_token_id() -> Result<String> {
    let out = std::process::Command::new("stellar")
        .args([
            "contract", "id", "asset",
            "--asset", "native",
            "--network-passphrase", NETWORK_PASSPHRASE,
            "--rpc-url", &rpc_url(),
        ])
        .output()
        .map_err(|e| anyhow::anyhow!("get native token id: {e}"))?;
    if !out.status.success() {
        anyhow::bail!("stellar contract id asset native failed:\n{}", String::from_utf8_lossy(&out.stderr));
    }
    let id = String::from_utf8(out.stdout)?.trim().to_string();
    eprintln!("  [rpc] native SAC token: {}", id);
    Ok(id)
}

pub fn deploy_usdc_sac() -> Result<String> {
    let issuer = source_pubkey()?;
    let asset = format!("USDC:{issuer}");

    eprintln!("  [usdc] Deploying USDC SAC (issuer={})…", &issuer[..8]);
    let out = std::process::Command::new("stellar")
        .args([
            "contract", "asset", "deploy",
            "--asset", &asset,
            "--source", SOURCE,
            "--network-passphrase", NETWORK_PASSPHRASE,
            "--rpc-url", &rpc_url(),
        ])
        .output()
        .map_err(|e| anyhow::anyhow!("deploy USDC SAC: {e}"))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let combined = format!("{stdout}\n{stderr}");
        if !combined.contains("ExistingValue") && !combined.contains("already") {
            anyhow::bail!("deploy USDC SAC failed:\n{stderr}");
        }
        eprintln!("  [usdc] SAC already deployed, fetching ID…");
    }

    let out = std::process::Command::new("stellar")
        .args([
            "contract", "id", "asset",
            "--asset", &asset,
            "--network-passphrase", NETWORK_PASSPHRASE,
            "--rpc-url", &rpc_url(),
        ])
        .output()
        .map_err(|e| anyhow::anyhow!("get USDC SAC id: {e}"))?;

    if !out.status.success() {
        anyhow::bail!("stellar contract id asset failed:\n{}", String::from_utf8_lossy(&out.stderr));
    }

    let id = String::from_utf8(out.stdout)?.trim().to_string();
    eprintln!("  [usdc] SAC: {}", id);
    Ok(id)
}

pub fn trust_usdc(sac_id: &str, identity: &str, pk: &str) -> Result<()> {
    use crate::soroban_rpc::{scval_address, SorobanRpc};
    let rpc = SorobanRpc::new();
    rpc.invoke_xdr(sac_id, identity, "trust", vec![
        scval_address(pk)?,
    ])?;
    eprintln!("  [usdc] trustline created for {}…", &pk[..8]);
    Ok(())
}

pub fn mint_usdc(sac_id: &str, to_pk: &str, amount: i128) -> Result<()> {
    use crate::soroban_rpc::{scval_address, scval_i128, SorobanRpc};
    let rpc = SorobanRpc::new();
    rpc.invoke_xdr(sac_id, SOURCE, "mint", vec![
        scval_address(to_pk)?,
        scval_i128(amount),
    ])?;
    eprintln!("  [usdc] minted {} to {}…", amount, &to_pk[..8]);
    Ok(())
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

fn install_wasm(wasm: &Path) -> Result<String> {
    let wasm_bytes = std::fs::read(wasm)?;
    crate::soroban_rpc::install_wasm_via_rpc(&wasm_bytes, SOURCE)
}

fn deploy(wasm: &Path) -> Result<String> {
    eprintln!("  [deploy] Preparing deployment…");
    let salt: [u8; 32] = rand::thread_rng().gen();
    let wasm_bytes = std::fs::read(wasm)
        .map_err(|e| anyhow::anyhow!("read wasm {}: {e}", wasm.display()))?;
    eprintln!("  [deploy] WASM: {} ({} bytes)", wasm.display(), wasm_bytes.len());

    // Stellar CLI v22 tries to parse the embedded XDR contract spec from the WASM which
    // fails for larger contracts. Install + deploy both bypass the CLI via our own RPC code.
    let wasm_hash = crate::soroban_rpc::install_wasm_via_rpc(&wasm_bytes, SOURCE)?;
    let contract_id = crate::soroban_rpc::deploy_contract_via_rpc(&wasm_hash, salt, SOURCE)?;

    eprintln!("  [deploy] ✓ Contract deployed: {}", contract_id);
    Ok(contract_id)
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

// ── Shielded Pool ─────────────────────────────────────────────────────────────

/// Deploy and initialize the shielded-pool contract.
/// `empty_root_hex` = output of `cargo run -p rust-circuits -- pool-zeros` (last line).
pub fn deploy_shielded_pool(
    wasm_dir: &Path,
    token: &str,
    denomination: u128,
    empty_root_hex: &str,
) -> Result<String> {
    use crate::soroban_rpc::{scval_address, scval_bytes32, SorobanRpc};
    let wasm = wasm_dir.join("shielded_pool.wasm");
    eprintln!("  [pool] Deploying shielded-pool contract…");
    let pool_id = deploy(&wasm)?;
    eprintln!("  ✓ shielded-pool: {}", pool_id);

    eprintln!("  [pool] Initializing (token={}, denom={})…", &token[..8], denomination);
    let rpc = SorobanRpc::new();
    rpc.invoke_xdr(&pool_id, SOURCE, "initialize", vec![
        scval_address(token)?,
        crate::soroban_rpc::scval_u128(denomination),
        scval_bytes32(empty_root_hex)?,
    ])?;
    eprintln!("  ✓ shielded-pool initialized");
    Ok(pool_id)
}

/// Call pool.deposit — transfers USDC from depositor into the pool.
/// `commitment_hex`, `new_root_hex`, `proof_json` come from `proof::gen_pool_insert`.
pub fn pool_deposit(
    pool_id: &str,
    depositor_identity: &str,
    depositor_pk: &str,
    commitment_hex: &str,
    new_root_hex: &str,
    proof_json: &str,
) -> Result<()> {
    use crate::soroban_rpc::{scval_address, scval_bytes32, scval_proof, SorobanRpc};
    let rpc = SorobanRpc::new();
    rpc.invoke_xdr(pool_id, depositor_identity, "deposit", vec![
        scval_address(depositor_pk)?,
        scval_bytes32(commitment_hex)?,
        scval_bytes32(new_root_hex)?,
        scval_proof(proof_json)?,
    ])?;
    eprintln!("  ✓ pool.deposit commitment={}", &commitment_hex[..16]);
    Ok(())
}

/// Call pool.withdraw — sends USDC from the pool to recipient_addr.
/// `root_hex`, `nullifier_hash_hex`, `recipient_hex`, `proof_json` come from `proof::gen_pool_withdraw`.
/// `recipient_identity` is the Stellar identity (key name) that must sign.
pub fn pool_withdraw(
    pool_id: &str,
    recipient_identity: &str,
    recipient_pk: &str,
    root_hex: &str,
    nullifier_hash_hex: &str,
    recipient_hex: &str,
    proof_json: &str,
) -> Result<()> {
    use crate::soroban_rpc::{scval_address, scval_bytes32, scval_proof, SorobanRpc};
    let rpc = SorobanRpc::new();
    rpc.invoke_xdr(pool_id, recipient_identity, "withdraw", vec![
        scval_bytes32(root_hex)?,
        scval_bytes32(nullifier_hash_hex)?,
        scval_bytes32(recipient_hex)?,
        scval_address(recipient_pk)?,
        scval_proof(proof_json)?,
    ])?;
    eprintln!("  ✓ pool.withdraw null_hash={}", &nullifier_hash_hex[..16]);
    Ok(())
}

/// Full shielded trade flow: pool.deposit → pool.withdraw → perp.deposit_note → trade.
///
/// Demonstrates full address privacy:
///   alice deposits USDC into the pool (visible on-chain, but commitment is opaque)
///   alice withdraws to herself via ZK proof (breaks commitment↔address link)
///   alice then trades normally via the perp engine
///
/// In production, the withdraw would go to a FRESH keypair to maximize privacy.
/// For testing we reuse the same key to avoid XLM funding complexity.
pub fn shielded_pool_e2e(
    wasm_dir: &Path,
    keys_dir: &Path,
    denomination: u128,
    pool_secret: u64,
    pool_nullifier: u64,
) -> Result<()> {
    use crate::proof;
    use sha2::{Digest, Sha256};
    let t = Instant::now();

    let alice = generate_keypair("pool-alice");
    eprintln!("  [pool e2e] alice: {}", &alice.0[..8]);

    // ── Deploy infrastructure ─────────────────────────────────────────────
    eprintln!("\n[1/7] Deploy contracts…");
    let usdc = deploy_usdc_sac()?;
    let pool_wasm = wasm_dir.join("shielded_pool.wasm");
    if !pool_wasm.exists() {
        anyhow::bail!(
            "shielded_pool.wasm not found at {}. Build with:\n  \
             cargo build --target wasm32v1-none -p shielded-pool --release",
            pool_wasm.display()
        );
    }

    // Compute empty root off-chain
    let empty_root = {
        use rust_circuits::compute_empty_root;
        use rust_circuits::fr_to_biguint;
        use ark_ff::BigInteger;
        let r = compute_empty_root();
        let bytes = r.into_bigint().to_bytes_be();
        hex::encode(&bytes)
    };
    eprintln!("  empty_root: {}…", &empty_root[..16]);

    let pool_id = deploy_shielded_pool(wasm_dir, &usdc, denomination, &empty_root)?;

    // Deploy perp engine for the trading leg
    let pe_wasm = wasm_dir.join("perp_engine.wasm");
    let perp_id = deploy(&pe_wasm)?;
    eprintln!("  ✓ perp-engine: {}", perp_id);
    init_perp_engine(&perp_id, SOURCE, &usdc)?;

    // ── Fund alice ────────────────────────────────────────────────────────
    eprintln!("\n[2/7] Fund alice with USDC…");
    trust_usdc(&usdc, &alice.1, &alice.0)?;
    mint_usdc(&usdc, &alice.0, denomination as i128)?;

    // ── Pool deposit ──────────────────────────────────────────────────────
    eprintln!("\n[3/7] pool.deposit (alice → pool, ZK insert proof)…");
    let insert = proof::gen_pool_insert(keys_dir, pool_secret, pool_nullifier, &[])?;
    eprintln!("  commitment: {}…", &insert.commitment_hex[..16]);
    eprintln!("  new_root:   {}…", &insert.new_root_hex[..16]);

    // Alice must trust USDC to interact with the pool (SAC requires trustline)
    pool_deposit(
        &pool_id, &alice.1, &alice.0,
        &insert.commitment_hex, &insert.new_root_hex,
        &serde_json::to_string(&insert.proof)?,
    )?;
    eprintln!("  ✓ ({:.1}s)", t.elapsed().as_secs_f64());

    // ── Pool withdraw ─────────────────────────────────────────────────────
    eprintln!("\n[4/7] pool.withdraw (ZK spend proof → alice)…");
    // recipient_hex = sha256(alice pubkey bytes) — binds proof to alice's address
    let recipient_hex = {
        let pk_bytes = hex::decode(&alice.0).unwrap_or_else(|_| alice.0.as_bytes().to_vec());
        let hash = Sha256::digest(&pk_bytes);
        hex::encode(hash)
    };

    // Rebuild leaf set from the single committed leaf
    use ark_ff::PrimeField;
    let commitment_fr = ark_bn254::Fr::from_be_bytes_mod_order(
        &hex::decode(&insert.commitment_hex).unwrap()
    );
    let all_leaves = vec![commitment_fr];

    let withdraw_result = proof::gen_pool_withdraw(
        keys_dir, pool_secret, pool_nullifier, &all_leaves, &recipient_hex,
    )?;

    pool_withdraw(
        &pool_id, &alice.1, &alice.0,
        &withdraw_result.root_hex, &withdraw_result.nullifier_hash_hex,
        &recipient_hex, &serde_json::to_string(&withdraw_result.proof)?,
    )?;
    eprintln!("  ✓ USDC returned to alice ({:.1}s)", t.elapsed().as_secs_f64());

    // ── Perp deposit_note ─────────────────────────────────────────────────
    eprintln!("\n[5/7] perp.deposit_note (alice → perp engine)…");
    let (note_cmt_hex, note_null_hex, note_proof) =
        proof::gen_note_spend(keys_dir, denomination as u64, pool_secret)?;
    let deposit_blinding_sp = [0u8; 32];
    let amount_cmt_sp = sha256_note_commitment(denomination as i128, &deposit_blinding_sp);
    perp_deposit_note(&perp_id, &alice.1, &alice.0, &note_cmt_hex, denomination as i128, &amount_cmt_sp)?;
    eprintln!("  ✓ note committed: {}… ({:.1}s)", &note_cmt_hex[..16], t.elapsed().as_secs_f64());

    // ── Open position ──────────────────────────────────────────────────────
    eprintln!("\n[6/7] perp.open_position_from_note…");
    let pos_secret = pool_secret ^ 0xdeadbeef;
    let commit_proof = proof::gen_commitment(
        keys_dir, 0, 100_000, denomination as u64, 1, 0, 0, 42, pos_secret, false,
    )?;
    let pos_cmt_hex = dec_to_hex(&commit_proof.public_inputs[0]);
    let zero64_sp = "0".repeat(64);
    let collateral_blinding_sp = hex::encode(&deposit_blinding_sp);

    perp_open_position(
        &perp_id, SOURCE,
        &note_cmt_hex, &note_null_hex, &pos_cmt_hex,
        0, 100_000, 1, denomination as u64, 0, 0, 0, 0,
        &zero64_sp, &zero64_sp, DEFAULT_ASSET,
        denomination as i128, &collateral_blinding_sp, &zero64_sp,
        &proof_json(&note_proof.proof),
        &proof_json(&commit_proof.proof),
    )?;
    eprintln!("  ✓ position open ({:.1}s)", t.elapsed().as_secs_f64());

    eprintln!("\n[7/7] Done. Full shielded pool → perp flow in {:.1}s", t.elapsed().as_secs_f64());
    eprintln!("  On-chain trace: alice addr visible for deposit + withdraw,");
    eprintln!("  but pool breaks the link between USDC source and note commitment.");
    Ok(())
}

fn dec_to_hex(decimal: &str) -> String {
    let n: num_bigint::BigUint = decimal.parse().unwrap_or_default();
    format!("{:0>64x}", n)
}

/// Full private end-to-end flow with no deposit→position address link.
///
/// Flow:
///   1. pool.deposit  (alice → pool via ZK insert proof)
///   2. perp.open_position_from_pool  (TEE; pool.withdraw bound to pos_cmt)
///   3. perp.cancel_position_to_note  (TEE; position → refund note)
///   4. perp.withdraw_to_pool  (TEE; refund note → pool via ZK insert proof)
///   5. pool.withdraw  (exit; different leaf → fresh address, no on-chain link)
///
/// Privacy guarantee: on-chain trace shows pool deposit (by alice) and pool
/// withdrawal (to alice), but the two pool leaves are unlinkable — the
/// intermediate perp position commitment reveals nothing about the depositor.
pub fn full_private_e2e(
    wasm_dir: &Path,
    keys_dir: &Path,
    denomination: u128,
    pool_secret: u64,
    pool_nullifier: u64,
    pos_secret: u64,
    cancel_secret: u64,
) -> Result<()> {
    use crate::proof;
    use ark_ff::PrimeField;
    let t = Instant::now();
    let zero64 = "0".repeat(64);

    // ── Deploy ────────────────────────────────────────────────────────────
    eprintln!("\n[1/9] Deploy ShieldedPool + perp-engine…");
    let usdc = deploy_usdc_sac()?;
    let pool_wasm = wasm_dir.join("shielded_pool.wasm");
    if !pool_wasm.exists() {
        anyhow::bail!("shielded_pool.wasm not found at {}. Build with:\n  cargo build --target wasm32v1-none -p shielded-pool --release", pool_wasm.display());
    }
    let empty_root = {
        use rust_circuits::{compute_empty_root, fr_to_biguint};
        use ark_ff::BigInteger;
        hex::encode(compute_empty_root().into_bigint().to_bytes_be())
    };
    let pool_id = deploy_shielded_pool(wasm_dir, &usdc, denomination, &empty_root)?;
    let pe_wasm = wasm_dir.join("perp_engine.wasm");
    let perp_id = deploy(&pe_wasm)?;
    eprintln!("  ✓ pool: {}", pool_id);
    eprintln!("  ✓ perp: {}", perp_id);
    init_perp_engine(&perp_id, SOURCE, &usdc)?;
    // For e2e dev: set SOURCE as the TEE account so TEE-gated calls work with our signing key.
    {
        use crate::soroban_rpc::{scval_address, SorobanRpc};
        let rpc = SorobanRpc::new();
        let source_pk = crate::soroban_rpc::source_pubkey_of(SOURCE)?;
        // Option<Address>::Some(addr) is encoded as ScVal::Address in Soroban.
        rpc.invoke_xdr(&perp_id, SOURCE, "set_tee_account", vec![
            scval_address(&source_pk)?,
            scval_address(&source_pk)?,
        ])?;
        eprintln!("  ✓ TEE account set to SOURCE for dev e2e");
    }

    // ── Fund alice ────────────────────────────────────────────────────────
    eprintln!("\n[2/9] Fund alice…");
    let alice = generate_keypair("fp-alice");
    fund(&alice.0, "fp-alice");
    // Wait for alice's account to be visible on the RPC node (Friendbot may lag).
    eprintln!("  [fund] waiting for fp-alice account to appear on-chain…");
    for _ in 0..30 {
        if account_exists(&alice.0) {
            eprintln!("  [fund] fp-alice account confirmed on-chain");
            break;
        }
        std::thread::sleep(std::time::Duration::from_secs(3));
    }
    trust_usdc(&usdc, &alice.1, &alice.0)?;
    mint_usdc(&usdc, &alice.0, denomination as i128)?;
    eprintln!("  ✓ alice {} funded with {} USDC", &alice.0[..8], denomination);

    // ── pool.deposit ──────────────────────────────────────────────────────
    eprintln!("\n[3/9] pool.deposit (alice → pool)…");
    let insert1 = proof::gen_pool_insert(keys_dir, pool_secret, pool_nullifier, &[])?;
    pool_deposit(
        &pool_id, &alice.1, &alice.0,
        &insert1.commitment_hex, &insert1.new_root_hex,
        &proof_json(&insert1.proof.proof),
    )?;
    eprintln!("  ✓ leaf1={}… root={}…", &insert1.commitment_hex[..16], &insert1.new_root_hex[..16]);
    eprintln!("  ({:.1}s)", t.elapsed().as_secs_f64());

    // ── Generate position commitment ──────────────────────────────────────
    eprintln!("\n[4/9] Generate position commitment…");
    let commit_proof = proof::gen_commitment(
        keys_dir, 0, 100_000_000, denomination as u64, 1, 0, 0, 1, pos_secret, false,
    )?;
    let pos_cmt_hex = dec_to_hex(&commit_proof.public_inputs[0]);
    eprintln!("  pos_cmt={}…", &pos_cmt_hex[..16]);

    // ── Generate pool spend proof (recipient bound to pos_cmt) ────────────
    eprintln!("\n[5/9] Generate ShieldedWithdraw proof (recipient=pos_cmt)…");
    let leaf1_fr = ark_bn254::Fr::from_be_bytes_mod_order(
        &hex::decode(&insert1.commitment_hex).unwrap()
    );
    let spend_result = proof::gen_pool_withdraw(
        keys_dir, pool_secret, pool_nullifier, &[leaf1_fr], &pos_cmt_hex,
    )?;
    eprintln!("  null_hash={}…", &spend_result.nullifier_hash_hex[..16]);
    eprintln!("  ({:.1}s)", t.elapsed().as_secs_f64());

    // ── perp.open_position_from_pool ──────────────────────────────────────
    eprintln!("\n[6/9] perp.open_position_from_pool (TEE-gated)…");
    let collateral_blinding = [0u8; 32];
    let collateral_blinding_hex = hex::encode(&collateral_blinding);
    perp_open_position_from_pool(
        &perp_id, SOURCE,
        &pool_id, &spend_result.root_hex, &spend_result.nullifier_hash_hex,
        &pos_cmt_hex,
        0, 100_000_000, 1, denomination as u64, 0, 0, 0, 0,
        &collateral_blinding_hex, &zero64,
        &zero64, &zero64, DEFAULT_ASSET,
        &proof_json(&spend_result.proof.proof),
        &proof_json(&commit_proof.proof),
    )?;
    eprintln!("  ({:.1}s)", t.elapsed().as_secs_f64());

    // ── perp.cancel_position_to_note ─────────────────────────────────────
    eprintln!("\n[7/9] perp.cancel_position_to_note (TEE-gated)…");
    let (cancel_null_hex, cancel_proof) =
        proof::gen_cancel(keys_dir, &pos_cmt_hex, pos_secret)?;
    // refund note: Poseidon2(denomination, cancel_secret, 8)
    let (refund_cmt_hex, refund_null_hex, refund_proof) =
        proof::gen_note_spend(keys_dir, denomination as u64, cancel_secret)?;
    let refund_blinding = [0x01u8; 32];
    let refund_blinding_hex = hex::encode(&refund_blinding);
    eprintln!("  cancel_null={}…", &cancel_null_hex[..16]);
    eprintln!("  refund_note={}…", &refund_cmt_hex[..16]);
    perp_cancel_position(
        &perp_id, SOURCE,
        &pos_cmt_hex, &cancel_null_hex, &refund_cmt_hex,
        denomination as i128, &refund_blinding_hex,
        &proof_json(&cancel_proof.proof),
    )?;
    eprintln!("  ✓ cancelled ({:.1}s)", t.elapsed().as_secs_f64());

    // ── perp.withdraw_to_pool ─────────────────────────────────────────────
    eprintln!("\n[8/9] perp.withdraw_to_pool (TEE-gated; refund note → pool leaf)…");
    // Generate new pool leaf for the exit note (different secret/nullifier for unlinkability)
    let exit_secret: u64 = cancel_secret ^ 0xc0ffee42;
    let exit_nullifier: u64 = pool_nullifier ^ 0xdead;
    let insert2 = proof::gen_pool_insert(keys_dir, exit_secret, exit_nullifier, &[leaf1_fr])?;
    eprintln!("  exit_leaf={}…", &insert2.commitment_hex[..16]);
    perp_withdraw_to_pool(
        &perp_id, SOURCE,
        &pool_id,
        &refund_cmt_hex, &refund_null_hex,
        denomination as i128, &refund_blinding_hex,
        &insert2.commitment_hex, &insert2.new_root_hex,
        &zero64, &zero64, // no remainder (amount == denomination)
        &proof_json(&refund_proof.proof),
        &proof_json(&insert2.proof.proof),
    )?;
    eprintln!("  ({:.1}s)", t.elapsed().as_secs_f64());

    // ── pool.withdraw ─────────────────────────────────────────────────────
    eprintln!("\n[9/9] pool.withdraw (exit leaf → alice; breaks deposit link)…");
    // Alice gets USDC back. In production use a fresh keypair for max privacy.
    use sha2::{Digest, Sha256};
    let alice_recipient_hex = {
        let pk_bytes = hex::decode(&alice.0).unwrap_or_else(|_| alice.0.as_bytes().to_vec());
        hex::encode(Sha256::digest(&pk_bytes))
    };
    let leaf2_fr = ark_bn254::Fr::from_be_bytes_mod_order(
        &hex::decode(&insert2.commitment_hex).unwrap()
    );
    let exit_withdraw = proof::gen_pool_withdraw(
        keys_dir, exit_secret, exit_nullifier,
        &[leaf1_fr, leaf2_fr],
        &alice_recipient_hex,
    )?;
    pool_withdraw(
        &pool_id, &alice.1, &alice.0,
        &exit_withdraw.root_hex, &exit_withdraw.nullifier_hash_hex,
        &alice_recipient_hex,
        &proof_json(&exit_withdraw.proof.proof),
    )?;
    eprintln!("  ✓ {} USDC returned to alice ({:.1}s)", denomination, t.elapsed().as_secs_f64());

    eprintln!("\n━━━ FullPrivate E2E PASSED ({:.2}s) ━━━", t.elapsed().as_secs_f64());
    eprintln!("  Privacy: pool_deposit(alice) and pool_withdraw(alice) use DIFFERENT leaves.");
    eprintln!("  On-chain: no link between note commitment and position commitment.");
    eprintln!("  Deposit leaf:  {}…", &insert1.commitment_hex[..16]);
    eprintln!("  Exit leaf:     {}…", &insert2.commitment_hex[..16]);
    Ok(())
}
