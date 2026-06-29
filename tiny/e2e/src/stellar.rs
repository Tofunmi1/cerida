use crate::proof::{CliProofOutput, Sp1ProofOutput};
use anyhow::Result;
use rand::Rng;
use std::path::PathBuf;

const SOURCE_IDENTITY: &str = "e2e";

// ─── Circom e2e ───────────────────────────────────────────────────────────────

pub fn run_full_e2e(tiny_root: &PathBuf, target: &PathBuf, cli: &CliProofOutput) -> Result<()> {
    let v_wasm = target.join("tiny_verifier.wasm");
    let p_wasm = target.join("tiny_pool.wasm");

    eprintln!("\n=== Deploy verifier ===");
    let verifier_id = stellar_deploy(&v_wasm)?;
    eprintln!("  Verifier: {verifier_id}");

    eprintln!("\n=== Deploy pool ===");
    let pool_id = stellar_deploy(&p_wasm)?;
    eprintln!("  Pool: {pool_id}");

    let trader = generate_keypair();
    eprintln!("\n=== Fund trader ===");
    fund(&trader.0);
    eprintln!("  Trader: {}", trader.0);

    eprintln!("\n=== Deposit ===");
    let cmt = &cli.commitment;
    let hex_cmt = decimal_to_hex32(cmt);
    stellar_invoke(
        &pool_id,
        &trader.1,
        &["deposit", "--owner", &trader.0, "--commitment", &hex_cmt, "--amount", "1000000"],
    )?;
    let bal = stellar_invoke_view(
        &pool_id,
        &trader.0,
        &["balance_of", "--commitment", &hex_cmt],
    )?;
    eprintln!("  balance: {bal}");

    eprintln!("\n=== Verify proof (circom) ===");
    let proof_json = serde_json::json!({
        "a": cli.proof.a,
        "b": cli.proof.b,
        "c": cli.proof.c,
    });
    let pub_ins = serde_json::json!([cli.commitment, cli.nullifier]);
    let verify = stellar_invoke_view(
        &verifier_id,
        &trader.0,
        &[
            "verify",
            "--proof", &proof_json.to_string(),
            "--public_inputs", &pub_ins.to_string(),
        ],
    )?;
    eprintln!("  verify result: {verify}");
    if verify != "true" {
        anyhow::bail!("Verification failed");
    }

    eprintln!("\n=== Withdraw ===");
    let hex_nf = decimal_to_hex32(&cli.nullifier);
    let withdrawn = stellar_invoke(
        &pool_id,
        &trader.1,
        &["withdraw", "--owner", &trader.0, "--commitment", &hex_cmt, "--nullifier", &hex_nf],
    )?;
    eprintln!("  withdrawn: {withdrawn}");

    eprintln!("\n=== Check nullifier spent ===");
    let spent = stellar_invoke_view(&pool_id, &trader.0, &["is_spent", "--nullifier", &hex_nf])?;
    eprintln!("  is_spent: {spent}");

    write_output(tiny_root, &verifier_id, &pool_id, &trader.0, &cli.commitment, &cli.nullifier)
}

// ─── SP1 e2e ─────────────────────────────────────────────────────────────────

pub fn run_sp1_e2e(tiny_root: &PathBuf, target: &PathBuf, sp1: &Sp1ProofOutput) -> Result<()> {
    let p_wasm = target.join("tiny_pool.wasm");

    eprintln!("\n=== Deploy pool (SP1 build) ===");
    let pool_id = stellar_deploy(&p_wasm)?;
    eprintln!("  Pool: {pool_id}");

    let trader = generate_keypair();
    eprintln!("\n=== Fund trader ===");
    fund(&trader.0);
    eprintln!("  Trader: {}", trader.0);

    // commitment from SP1 host (hex already)
    let hex_cmt = &sp1.commitment;
    let hex_nf = &sp1.nullifier;

    eprintln!("\n=== Deposit (SP1 commitment) ===");
    stellar_invoke(
        &pool_id,
        &trader.1,
        &["deposit", "--owner", &trader.0, "--commitment", hex_cmt, "--amount", "1000000"],
    )?;
    let bal = stellar_invoke_view(&pool_id, &trader.0, &["balance_of", "--commitment", hex_cmt])?;
    eprintln!("  balance: {bal}");

    eprintln!("\n=== Withdraw with SP1 Groth16 proof ===");
    let proof_json = serde_json::json!({
        "a": sp1.proof.a,
        "b": sp1.proof.b,
        "c": sp1.proof.c,
    });
    let withdrawn = stellar_invoke(
        &pool_id,
        &trader.1,
        &[
            "withdraw_sp1",
            "--owner", &trader.0,
            "--commitment", hex_cmt,
            "--nullifier", hex_nf,
            "--proof", &proof_json.to_string(),
        ],
    )?;
    eprintln!("  withdrawn: {withdrawn}");

    eprintln!("\n=== Check nullifier spent ===");
    let spent = stellar_invoke_view(&pool_id, &trader.0, &["is_spent", "--nullifier", hex_nf])?;
    eprintln!("  is_spent: {spent}");

    write_output(tiny_root, "n/a", &pool_id, &trader.0, hex_cmt, hex_nf)
}

// ─── Shared helpers ───────────────────────────────────────────────────────────

fn write_output(
    tiny_root: &PathBuf,
    verifier: &str,
    pool: &str,
    trader: &str,
    commitment: &str,
    nullifier: &str,
) -> Result<()> {
    let output = serde_json::json!({
        "verifier": verifier,
        "pool": pool,
        "trader": trader,
        "commitment": commitment,
        "nullifier": nullifier,
    });
    let out_path = tiny_root.join("deployments/testnet/e2e_output.json");
    std::fs::create_dir_all(out_path.parent().unwrap())?;
    std::fs::write(&out_path, serde_json::to_string_pretty(&output)?)?;
    eprintln!("\n=== ✓ E2E PASSED ===");
    Ok(())
}

/// Convert decimal string (Poseidon2 output) to 0-padded 64-char hex.
fn decimal_to_hex32(s: &str) -> String {
    let n: num_bigint::BigUint = s.parse().expect("Invalid decimal in decimal_to_hex32");
    format!("{:0>64x}", n)
}

fn generate_keypair() -> (String, String) {
    let _ = std::process::Command::new("stellar")
        .args(["keys", "generate", "e2e-trader", "--network", "testnet", "--fund"])
        .output()
        .ok();
    let addr = std::process::Command::new("stellar")
        .args(["keys", "address", "e2e-trader"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    let addr = addr.trim().to_string();
    let sk = if addr.is_empty() { "S".to_string() } else { "e2e-trader".to_string() };
    (addr, sk)
}

fn fund(pk: &str) {
    let url = format!("https://friendbot.stellar.org/?addr={pk}");
    let _ = std::process::Command::new("curl")
        .args(["-s", "-o", "/dev/null", "-w", "%{http_code}", &url])
        .output()
        .ok();
}

fn stellar_deploy(wasm: &PathBuf) -> Result<String> {
    let salt: [u8; 32] = rand::thread_rng().gen();
    let salt_hex = hex::encode(salt);
    let source_pk = get_source_public_key()?;
    let contract_id = precompute_contract_id(&salt_hex, &source_pk)?;
    eprintln!("  Precomputed ID: {contract_id}");

    let output = std::process::Command::new("stellar")
        .args([
            "contract", "deploy",
            "--wasm", &wasm.to_string_lossy(),
            "--source", SOURCE_IDENTITY,
            "--network", "testnet",
            "--salt", &salt_hex,
        ])
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to run stellar deploy: {e}"))?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Wait for the deploy TX to be confirmed on-chain before returning
    if let Some(tx_hash) = extract_tx_hash(&stderr).or_else(|| extract_tx_hash(&stdout)) {
        eprintln!("  Deploy TX: {tx_hash}");
        for _ in 0..60 {
            std::thread::sleep(std::time::Duration::from_secs(2));
            if let Some(_) = poll_tx_status(&tx_hash)? {
                eprintln!("  Deployed: {contract_id}");
                return Ok(contract_id);
            }
        }
        anyhow::bail!("Deploy TX {tx_hash} not confirmed after 120s");
    }

    if !output.status.success() {
        anyhow::bail!("stellar deploy failed:\n{stderr}");
    }

    Ok(contract_id)
}

fn get_source_public_key() -> Result<String> {
    let output = std::process::Command::new("stellar")
        .args(["keys", "address", SOURCE_IDENTITY])
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to get source public key: {e}"))?;
    if !output.status.success() {
        anyhow::bail!("Identity '{SOURCE_IDENTITY}' not found. Create it with: stellar keys generate {SOURCE_IDENTITY} --network testnet --fund");
    }
    Ok(String::from_utf8(output.stdout)
        .map_err(|_| anyhow::anyhow!("Invalid UTF-8 from keys address"))?
        .trim()
        .to_string())
}

fn precompute_contract_id(salt_hex: &str, source_pk: &str) -> Result<String> {
    let output = std::process::Command::new("stellar")
        .args([
            "contract", "id", "wasm",
            "--salt", salt_hex,
            "--source-account", source_pk,
            "--network", "testnet",
        ])
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to precompute contract ID: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("stellar contract id failed:\n{stderr}");
    }
    Ok(String::from_utf8(output.stdout)
        .map_err(|_| anyhow::anyhow!("Invalid UTF-8 from contract id"))?
        .trim()
        .to_string())
}

fn stellar_invoke(contract_id: &str, source: &str, args: &[&str]) -> Result<String> {
    let mut cmd = std::process::Command::new("stellar");
    cmd.args(["contract", "invoke", "--id", contract_id, "--source-account", source, "--network", "testnet", "--"]);
    cmd.args(args);
    let output = cmd.output()
        .map_err(|e| anyhow::anyhow!("Failed to run stellar invoke: {e}"))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    if let Some(tx_hash) = extract_tx_hash(&stderr).or_else(|| extract_tx_hash(&stdout)) {
        eprintln!("  TX: {tx_hash}");
        for _ in 0..60 {
            std::thread::sleep(std::time::Duration::from_secs(2));
            if let Some(result) = poll_tx_status(&tx_hash)? {
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

fn extract_tx_hash(output: &str) -> Option<String> {
    output.lines().find_map(|l| {
        let t = l.trim();
        let stripped = t
            .strip_prefix("\u{2139}\u{fe0f}  ")
            .unwrap_or(t);
        stripped.strip_prefix("Signing transaction: ")
            .or_else(|| stripped.strip_prefix("Transaction hash is "))
            .map(|h| h.trim().to_string())
    })
}

fn poll_tx_status(tx_hash: &str) -> Result<Option<String>> {
    let body = serde_json::json!({
        "jsonrpc": "2.0", "id": 1,
        "method": "getTransaction",
        "params": { "hash": tx_hash }
    });
    let resp = std::process::Command::new("curl")
        .args(["-s", "-X", "POST", "-H", "Content-Type: application/json",
               "-d", &body.to_string(), "https://soroban-testnet.stellar.org"])
        .output()
        .map_err(|e| anyhow::anyhow!("curl getTransaction failed: {e}"))?;
    let out: serde_json::Value = serde_json::from_slice(&resp.stdout)
        .map_err(|e| anyhow::anyhow!("invalid JSON: {e}"))?;
    match out["result"]["status"].as_str() {
        Some("SUCCESS") => {
            let result_xdr = out["result"]["resultXdr"].as_str().unwrap_or("");
            Ok(Some(format!("\"{result_xdr}\"")))
        }
        Some("FAILED") => anyhow::bail!("Transaction FAILED: {tx_hash}"),
        _ => Ok(None),
    }
}

fn stellar_invoke_view(contract_id: &str, source: &str, args: &[&str]) -> Result<String> {
    for attempt in 0..3 {
        let mut cmd = std::process::Command::new("stellar");
        cmd.args(["contract", "invoke", "--id", contract_id, "--source-account", source,
                  "--network", "testnet", "--is-view", "--"]);
        cmd.args(args);
        let output = cmd.output()
            .map_err(|e| anyhow::anyhow!("Failed to run stellar invoke (view): {e}"))?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
        }
        if attempt < 2 { std::thread::sleep(std::time::Duration::from_secs(1)); }
    }
    let mut cmd = std::process::Command::new("stellar");
    cmd.args(["contract", "invoke", "--id", contract_id, "--source-account", source,
              "--network", "testnet", "--is-view", "--"]);
    cmd.args(args);
    let output = cmd.output().map_err(|e| anyhow::anyhow!("invoke view: {e}"))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    anyhow::bail!("stellar invoke (view) failed:\n{stderr}");
}
