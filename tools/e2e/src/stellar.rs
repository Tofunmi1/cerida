use crate::proof::RawProof;
use anyhow::Result;
use rand::Rng;
use std::path::{Path, PathBuf};

const SOURCE: &str = "e2e";
const COLLATERAL: i128 = 1_000_000_000;
const LEVERAGE: u64 = 1;

pub fn run_e2e(
    wasm_dir: &Path,
    p_a: &RawProof,
    p_b: &RawProof,
    p_match: &RawProof,
    cmt_a_hex: &str,
    cmt_b_hex: &str,
) -> Result<()> {
    let ob_wasm = wasm_dir.join("orderbook.wasm");
    let pe_wasm = wasm_dir.join("perp_engine.wasm");

    eprintln!("\n=== Deploy orderbook ===");
    let orderbook_id = deploy(&ob_wasm)?;
    eprintln!("  Orderbook: {orderbook_id}");

    eprintln!("\n=== Generate identities ===");
    let alice = generate_keypair("e2e-alice");
    let bob = generate_keypair("e2e-bob");
    let source_pk = source_pubkey()?;
    eprintln!("  Admin: {source_pk}");
    eprintln!("  Alice: {}", alice.0);
    eprintln!("  Bob: {}", bob.0);

    eprintln!("\n=== Fund traders ===");
    fund(&alice.0);
    fund(&bob.0);

    eprintln!("\n=== Deploy perp engine ===");
    let perp_id = deploy(&pe_wasm)?;
    eprintln!("  PerpEngine: {perp_id}");

    eprintln!("\n=== Get native SAC token ID ===");
    let native_token = native_token_id()?;
    eprintln!("  Native token: {native_token}");

    eprintln!("\n=== Initialize perp engine ===");
    invoke(
        &perp_id,
        SOURCE,
        &["initialize", "--admin", &source_pk, "--token", &native_token],
    )?;

    let hint_a: u64 = 100000;
    let hint_b: u64 = 99000;
    let side_a = "0";
    let side_b = "1";
    let match_price_hex = &p_match.public_inputs[2];
    let match_size_hex = &p_match.public_inputs[3];
    let nf_a_hex = &p_match.public_inputs[4];
    let nf_b_hex = &p_match.public_inputs[5];

    // ── Orderbook: hint board ──────────────────────────────────────────────
    eprintln!("\n=== Place order A (Alice) ===");
    invoke(
        &orderbook_id,
        &alice.1,
        &[
            "place_order", "--owner", &alice.0, "--commitment", cmt_a_hex,
            "--hint", &hint_a.to_string(),
            "--proof", &proof_json(&p_a.proof),
        ],
    )?;
    let st_a = invoke_view(
        &orderbook_id, &alice.0,
        &["status", "--commitment", cmt_a_hex],
    )?;
    eprintln!("  order A status: {st_a}");

    eprintln!("\n=== Place order B (Bob) ===");
    invoke(
        &orderbook_id,
        &bob.1,
        &[
            "place_order", "--owner", &bob.0, "--commitment", cmt_b_hex,
            "--hint", &hint_b.to_string(),
            "--proof", &proof_json(&p_b.proof),
        ],
    )?;
    let st_b = invoke_view(
        &orderbook_id, &bob.0,
        &["status", "--commitment", cmt_b_hex],
    )?;
    eprintln!("  order B status: {st_b}");

    // ── Perp engine: open positions ────────────────────────────────────────
    eprintln!("\n=== Open position A (Alice) ===");
    invoke(
        &perp_id,
        &alice.1,
        &[
            "open_position", "--owner", &alice.0, "--commitment", cmt_a_hex,
            "--collateral", &COLLATERAL.to_string(),
            "--hint_price", &hint_a.to_string(),
            "--hint_side", side_a,
            "--hint_leverage", &LEVERAGE.to_string(),
            "--proof", &proof_json(&p_a.proof),
        ],
    )?;
    let pos_a = invoke_view(
        &perp_id, &alice.0,
        &["get_position", "--commitment", cmt_a_hex],
    )?;
    eprintln!("  position A: {pos_a}");

    eprintln!("\n=== Open position B (Bob) ===");
    invoke(
        &perp_id,
        &bob.1,
        &[
            "open_position", "--owner", &bob.0, "--commitment", cmt_b_hex,
            "--collateral", &COLLATERAL.to_string(),
            "--hint_price", &hint_b.to_string(),
            "--hint_side", side_b,
            "--hint_leverage", &LEVERAGE.to_string(),
            "--proof", &proof_json(&p_b.proof),
        ],
    )?;
    let pos_b = invoke_view(
        &perp_id, &bob.0,
        &["get_position", "--commitment", cmt_b_hex],
    )?;
    eprintln!("  position B: {pos_b}");

    // ── Match via perp engine ──────────────────────────────────────────────
    eprintln!("\n=== Match positions ===");
    invoke(
        &perp_id,
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

    eprintln!("\n=== Verify matched status (perp engine) ===");
    let pos_a2 = invoke_view(
        &perp_id, &alice.0,
        &["get_position", "--commitment", cmt_a_hex],
    )?;
    let pos_b2 = invoke_view(
        &perp_id, &bob.0,
        &["get_position", "--commitment", cmt_b_hex],
    )?;
    eprintln!("  position A: {pos_a2}");
    eprintln!("  position B: {pos_b2}");

    eprintln!("\n=== Verify nullifiers spent (perp engine) ===");
    let spent_a = invoke_view(
        &perp_id, &alice.0,
        &["is_spent", "--nullifier", &hex_field(nf_a_hex)],
    )?;
    let spent_b = invoke_view(
        &perp_id, &bob.0,
        &["is_spent", "--nullifier", &hex_field(nf_b_hex)],
    )?;
    eprintln!("  nullifier A spent: {spent_a}");
    eprintln!("  nullifier B spent: {spent_b}");

    let out = serde_json::json!({
        "orderbook": orderbook_id,
        "perp_engine": perp_id,
        "admin": source_pk,
        "alice": alice.0,
        "bob": bob.0,
        "commitment_a": cmt_a_hex,
        "commitment_b": cmt_b_hex,
    });
    let out_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../deployments/testnet")
        .join("e2e_output.json");
    std::fs::create_dir_all(out_path.parent().unwrap())?;
    std::fs::write(&out_path, serde_json::to_string_pretty(&out)?)?;
    eprintln!("\n=== E2E PASSED ===");
    Ok(())
}

fn hex_field(decimal: &str) -> String {
    let n: num_bigint::BigUint = decimal.parse().expect("Invalid decimal in hex_field");
    format!("{:0>64x}", n)
}

fn proof_json(p: &crate::proof::ProofHex) -> String {
    serde_json::json!({"a": p.a, "b": p.b, "c": p.c}).to_string()
}

fn native_token_id() -> Result<String> {
    let out = std::process::Command::new("stellar")
        .args(["contract", "id", "asset", "--asset", "native", "--network", "testnet"])
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to get native token id: {e}"))?;
    if !out.status.success() {
        anyhow::bail!("stellar contract id asset failed:\n{}", String::from_utf8_lossy(&out.stderr));
    }
    Ok(String::from_utf8(out.stdout)
        .map_err(|_| anyhow::anyhow!("Invalid UTF-8"))?
        .trim()
        .to_string())
}

fn generate_keypair(name: &str) -> (String, String) {
    let _ = std::process::Command::new("stellar")
        .args(["keys", "generate", name, "--network", "testnet", "--fund"])
        .output()
        .ok();
    let addr = std::process::Command::new("stellar")
        .args(["keys", "address", name])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    let addr = addr.trim().to_string();
    let sk = if addr.is_empty() { name.to_string() } else { name.to_string() };
    (addr, sk)
}

fn fund(pk: &str) {
    let url = format!("https://friendbot.stellar.org/?addr={pk}");
    let _ = std::process::Command::new("curl")
        .args(["-s", "-o", "/dev/null", "-w", "%{http_code}", &url])
        .output()
        .ok();
}

fn deploy(wasm: &Path) -> Result<String> {
    let salt: [u8; 32] = rand::thread_rng().gen();
    let salt_hex = hex::encode(salt);
    let source_pk = source_pubkey()?;
    let id = precompute_id(&salt_hex, &source_pk)?;
    eprintln!("  Precomputed: {id}");
    let output = std::process::Command::new("stellar")
        .args([
            "contract", "deploy",
            "--wasm", &wasm.to_string_lossy(),
            "--source", SOURCE,
            "--network", "testnet",
            "--salt", &salt_hex,
        ])
        .output()
        .map_err(|e| anyhow::anyhow!("deploy cmd: {e}"))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if let Some(tx_hash) = extract_tx_hash(&stderr) {
        eprintln!("  deploy TX: {tx_hash}");
        for _ in 0..120 {
            std::thread::sleep(std::time::Duration::from_secs(2));
            if let Some(_result) = poll_tx(&tx_hash)? {
                // Verify the contract actually exists
                for attempt in 0..10 {
                    if contract_exists(&id)? {
                        return Ok(id);
                    }
                    eprintln!("  waiting for contract to appear (attempt {})...", attempt + 1);
                    std::thread::sleep(std::time::Duration::from_secs(3));
                }
                anyhow::bail!("contract {} not found after deploy confirmed", id);
            }
        }
        anyhow::bail!("deploy TX {tx_hash} not confirmed after 240s");
    }
    anyhow::bail!("deploy failed: could not extract tx hash:\n{stderr}");
}

fn contract_exists(id: &str) -> Result<bool> {
    let out = std::process::Command::new("stellar")
        .args([
            "contract", "invoke",
            "--id", id,
            "--source-account", SOURCE,
            "--network", "testnet",
            "--is-view", "--",
            "get_config",
        ])
        .output()
        .ok();
    match out {
        Some(o) if o.status.success() => Ok(true),
        Some(o) => {
            let s = String::from_utf8_lossy(&o.stderr);
            // "contract not found" (lowercase) means the ID doesn't exist
            Ok(!s.to_lowercase().contains("contract not found"))
        }
        None => Ok(false),
    }
}

fn precompute_id(salt_hex: &str, source_pk: &str) -> Result<String> {
    let out = std::process::Command::new("stellar")
        .args([
            "contract", "id", "wasm",
            "--salt", salt_hex,
            "--source-account", source_pk,
            "--network", "testnet",
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

fn source_pubkey() -> Result<String> {
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

fn invoke(contract_id: &str, source: &str, args: &[&str]) -> Result<String> {
    let mut cmd = std::process::Command::new("stellar");
    cmd.args([
        "contract", "invoke",
        "--id", contract_id,
        "--source-account", source,
        "--network", "testnet",
        "--",
    ]);
    cmd.args(args);
    let output = cmd.output().map_err(|e| anyhow::anyhow!("stellar invoke: {e}"))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    if let Some(tx_hash) = extract_tx_hash(&stderr).or_else(|| extract_tx_hash(&stdout)) {
        eprintln!("  TX: {tx_hash}");
        for _ in 0..60 {
            std::thread::sleep(std::time::Duration::from_secs(2));
            if let Some(result) = poll_tx(&tx_hash)? {
                return Ok(result);
            }
        }
        anyhow::bail!("TX {tx_hash} not confirmed after 120s");
    }
    if !output.status.success() {
        anyhow::bail!("stellar invoke failed:\n{stderr}");
    }
    Ok(stdout.trim().to_string())
}

fn invoke_view(contract_id: &str, source: &str, args: &[&str]) -> Result<String> {
    for attempt in 0..3 {
        let mut cmd = std::process::Command::new("stellar");
        cmd.args([
            "contract", "invoke",
            "--id", contract_id,
            "--source-account", source,
            "--network", "testnet",
            "--is-view", "--",
        ]);
        cmd.args(args);
        let output = cmd.output().map_err(|e| anyhow::anyhow!("invoke view: {e}"))?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
        }
        if attempt < 2 {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
    let mut cmd = std::process::Command::new("stellar");
    cmd.args([
        "contract", "invoke",
        "--id", contract_id,
        "--source-account", source,
        "--network", "testnet",
        "--is-view", "--",
    ]);
    cmd.args(args);
    let output = cmd.output().map_err(|e| anyhow::anyhow!("invoke view: {e}"))?;
    anyhow::bail!("stellar invoke view failed:\n{}", String::from_utf8_lossy(&output.stderr));
}

fn extract_tx_hash(output: &str) -> Option<String> {
    output.lines().find_map(|l| {
        let t = l.trim();
        let stripped = t.strip_prefix("\u{2139}\u{fe0f}  ").unwrap_or(t);
        stripped
            .strip_prefix("Signing transaction: ")
            .or_else(|| stripped.strip_prefix("Transaction hash is "))
            .map(|h| h.trim().to_string())
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
            "https://soroban-testnet.stellar.org",
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
