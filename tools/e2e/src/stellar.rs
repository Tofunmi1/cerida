use crate::proof::RawProof;
use anyhow::Result;
use rand::Rng;
use std::path::{Path, PathBuf};

const SOURCE: &str = "e2e";

pub fn run_e2e(
    wasm_dir: &Path,
    p_a: &RawProof,
    p_b: &RawProof,
    p_match: &RawProof,
    cmt_a_hex: &str,
    cmt_b_hex: &str,
) -> Result<()> {
    let ob_wasm = wasm_dir.join("orderbook.wasm");

    eprintln!("\n=== Deploy orderbook ===");
    let orderbook_id = deploy(&ob_wasm)?;
    eprintln!("  Orderbook: {orderbook_id}");

    let alice = generate_keypair("e2e-alice");
    let bob = generate_keypair("e2e-bob");
    eprintln!("\n=== Fund traders ===");
    fund(&alice.0);
    fund(&bob.0);
    eprintln!("  Alice: {}", alice.0);
    eprintln!("  Bob: {}", bob.0);

    let hint_a: u64 = 100000;
    let hint_b: u64 = 99000;
    let match_price_hex = &p_match.public_inputs[2];
    let match_size_hex = &p_match.public_inputs[3];
    let nf_a_hex = &p_match.public_inputs[4];
    let nf_b_hex = &p_match.public_inputs[5];

    eprintln!("\n=== Place order A (Alice) ===");
    invoke(
        &orderbook_id,
        &alice.1,
        &[
            "place_order",
            "--owner",
            &alice.0,
            "--commitment",
            cmt_a_hex,
            "--hint",
            &hint_a.to_string(),
            "--proof",
            &proof_json(&p_a.proof),
        ],
    )?;
    let st_a = invoke_view(
        &orderbook_id,
        &alice.0,
        &["status", "--commitment", cmt_a_hex],
    )?;
    eprintln!("  order A status: {st_a}");

    eprintln!("\n=== Place order B (Bob) ===");
    invoke(
        &orderbook_id,
        &bob.1,
        &[
            "place_order",
            "--owner",
            &bob.0,
            "--commitment",
            cmt_b_hex,
            "--hint",
            &hint_b.to_string(),
            "--proof",
            &proof_json(&p_b.proof),
        ],
    )?;
    let st_b = invoke_view(
        &orderbook_id,
        &bob.0,
        &["status", "--commitment", cmt_b_hex],
    )?;
    eprintln!("  order B status: {st_b}");

    eprintln!("\n=== Match orders ===");
    invoke(
        &orderbook_id,
        &SOURCE.to_string(),
        &[
            "match_orders",
            "--cmt_a",
            cmt_a_hex,
            "--cmt_b",
            cmt_b_hex,
            "--nullifier_a",
            &hex_field(nf_a_hex),
            "--nullifier_b",
            &hex_field(nf_b_hex),
            "--match_price",
            &hex_field(match_price_hex),
            "--match_size",
            &hex_field(match_size_hex),
            "--proof",
            &proof_json(&p_match.proof),
        ],
    )?;

    eprintln!("\n=== Verify filled ===");
    let st_a2 = invoke_view(
        &orderbook_id,
        &alice.0,
        &["status", "--commitment", cmt_a_hex],
    )?;
    let st_b2 = invoke_view(
        &orderbook_id,
        &bob.0,
        &["status", "--commitment", cmt_b_hex],
    )?;
    eprintln!("  order A status: {st_a2}");
    eprintln!("  order B status: {st_b2}");

    eprintln!("\n=== Verify nullifiers spent ===");
    let spent_a = invoke_view(
        &orderbook_id,
        &alice.0,
        &["is_spent", "--nullifier", &hex_field(nf_a_hex)],
    )?;
    let spent_b = invoke_view(
        &orderbook_id,
        &bob.0,
        &["is_spent", "--nullifier", &hex_field(nf_b_hex)],
    )?;
    eprintln!("  nullifier A spent: {spent_a}");
    eprintln!("  nullifier B spent: {spent_b}");

    let out = serde_json::json!({
        "orderbook": orderbook_id,
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
    (addr.clone(), addr)
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
    let _ = std::process::Command::new("stellar")
        .args([
            "contract",
            "deploy",
            "--wasm",
            &wasm.to_string_lossy(),
            "--source",
            SOURCE,
            "--network",
            "testnet",
            "--salt",
            &salt_hex,
        ])
        .output();
    Ok(id)
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

fn precompute_id(salt_hex: &str, source_pk: &str) -> Result<String> {
    let out = std::process::Command::new("stellar")
        .args([
            "contract",
            "id",
            "wasm",
            "--salt",
            salt_hex,
            "--source-account",
            source_pk,
            "--network",
            "testnet",
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

fn invoke(contract_id: &str, source: &str, args: &[&str]) -> Result<String> {
    let mut cmd = std::process::Command::new("stellar");
    cmd.args([
        "contract",
        "invoke",
        "--id",
        contract_id,
        "--source-account",
        source,
        "--network",
        "testnet",
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
            "contract",
            "invoke",
            "--id",
            contract_id,
            "--source-account",
            source,
            "--network",
            "testnet",
            "--is-view",
            "--",
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
        "contract",
        "invoke",
        "--id",
        contract_id,
        "--source-account",
        source,
        "--network",
        "testnet",
        "--is-view",
        "--",
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
            "-s",
            "-X",
            "POST",
            "-H",
            "Content-Type: application/json",
            "-d",
            &body.to_string(),
            "https://soroban-testnet.stellar.org",
        ])
        .output()
        .map_err(|e| anyhow::anyhow!("curl getTransaction: {e}"))?;
    let out: serde_json::Value =
        serde_json::from_slice(&resp.stdout).map_err(|e| anyhow::anyhow!("invalid JSON: {e}"))?;
    match out["result"]["status"].as_str() {
        Some("SUCCESS") => {
            let result_xdr = out["result"]["resultXdr"].as_str().unwrap_or("");
            Ok(Some(format!("\"{result_xdr}\"")))
        }
        Some("FAILED") => anyhow::bail!("Transaction FAILED: {tx_hash}"),
        _ => Ok(None),
    }
}
