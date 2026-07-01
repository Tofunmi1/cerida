use crate::log;
use crate::proof::MatchProof;
use anyhow::Result;
use std::time::Instant;

const DEFAULT_RPC_URL: &str = "https://soroban-testnet.stellar.org";

pub fn rpc_url() -> String {
    std::env::var("SOROBAN_RPC_URL").unwrap_or_else(|_| DEFAULT_RPC_URL.to_string())
}

const NETWORK_PASSPHRASE: &str = "Test SDF Network ; September 2015";

const SOURCE_IDENTITY: &str = "e2e";

pub fn submit_match(perp_id: &str, _source: &str, cmt_a: &str, cmt_b: &str, proof: &MatchProof) -> Result<()> {
    let start = Instant::now();

    let hex = |dec: &str| -> String {
        let n: num_bigint::BigUint = dec.parse().expect("Invalid decimal");
        format!("{:0>64x}", n)
    };

    let nullifier_a_hex = hex(&proof.public_inputs[4]);
    let nullifier_b_hex = hex(&proof.public_inputs[5]);
    let match_price_hex = hex(&proof.public_inputs[2]);
    let match_size_hex = hex(&proof.public_inputs[3]);

    let proof_json = serde_json::json!({
        "a": proof.proof.a,
        "b": proof.proof.b,
        "c": proof.proof.c,
    })
    .to_string();

    let tmp = std::env::temp_dir().join(format!("tee_match_proof_{}.json", std::process::id()));
    std::fs::write(&tmp, &proof_json)?;

    let mut cmd = std::process::Command::new("stellar");
    cmd.args([
        "contract", "invoke",
        "--id", perp_id,
        "--source", SOURCE_IDENTITY,
        "--network-passphrase", NETWORK_PASSPHRASE,
        "--rpc-url", &rpc_url(),
        "--",
        "match_positions",
        "--cmt_a", cmt_a,
        "--cmt_b", cmt_b,
        "--nullifier_a", &nullifier_a_hex,
        "--nullifier_b", &nullifier_b_hex,
        "--match_price", &match_price_hex,
        "--match_size", &match_size_hex,
        "--proof-file-path", &tmp.to_string_lossy(),
    ]);

    log::debug!("Executing stellar CLI command",
        "contract", &perp_id[..8],
        "source", SOURCE_IDENTITY,
        "method", "match_positions",
        "cmt_a", &cmt_a[..16],
        "cmt_b", &cmt_b[..16]
    );

    let exec_start = Instant::now();
    let output = cmd.output().map_err(|e| anyhow::anyhow!("stellar invoke: {e}"))?;
    let exec_duration = exec_start.elapsed();
    let _ = std::fs::remove_file(&tmp);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("xdr processing error") || stderr.contains("Transaction hash is") {
            // Extract tx hash from stderr for logging
            let tx_hash = stderr.lines()
                .find_map(|l| l.strip_prefix("Transaction hash is "))
                .or_else(|| stderr.lines().find_map(|l| l.strip_prefix("Signing transaction: ")))
                .map(|h| h.trim().to_string());
            log::info!("Match transaction submitted",
                "contract", &perp_id[..8],
                "tx_hash", tx_hash.as_deref().unwrap_or("unknown"),
                "exec_time", log::duration_secs(&exec_duration),
                "total_time", log::duration_secs(&start.elapsed()),
                "nullifier_a", &nullifier_a_hex[..16],
                "nullifier_b", &nullifier_b_hex[..16],
                "match_price", &match_price_hex,
                "match_size", &match_size_hex
            );
            return Ok(());
        }
        log::error!("Match transaction failed",
            "contract", &perp_id[..8],
            "stderr", &stderr[..stderr.len().min(500)]
        );
        anyhow::bail!("match failed:\n{stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    log::info!("Match transaction submitted successfully",
        "contract", &perp_id[..8],
        "stdout", stdout.trim(),
        "total_time", log::duration_secs(&start.elapsed())
    );
    Ok(())
}

pub fn submit_cancel(
    orderbook_id: &str,
    perp_id: &str,
    owner: &str,
    commitment: &str,
    nullifier: &str,
    proof: &MatchProof,
    source: &str,
) -> Result<()> {
    let start = Instant::now();
    let proof_json = serde_json::json!({
        "a": proof.proof.a,
        "b": proof.proof.b,
        "c": proof.proof.c,
    })
    .to_string();

    let cancel_order = |contract_id: &str, method: &str| -> Result<()> {
        let start = Instant::now();
        let mut cmd = std::process::Command::new("stellar");
        cmd.args([
            "contract", "invoke",
            "--id", contract_id,
            "--source", source,
            "--network-passphrase", NETWORK_PASSPHRASE,
            "--rpc-url", &rpc_url(),
            "--",
            method, "--owner", owner,
            "--commitment", commitment,
            "--nullifier", nullifier,
            "--proof", &proof_json,
        ]);
        let output = cmd.output().map_err(|e| anyhow::anyhow!("stellar {method}: {e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("xdr processing error") || stderr.contains("Transaction hash is") {
                log::warning!("Cancel {method}: CLI reported error but tx likely submitted",
                    "contract", &contract_id[..8],
                    "stderr", &stderr[..stderr.len().min(300)]
                );
                return Ok(());
            }
            log::error!("Cancel {method} failed",
                "contract", &contract_id[..8],
                "stderr", &stderr[..stderr.len().min(500)]
            );
            anyhow::bail!("{method} on {contract_id} failed:\n{stderr}");
        }
        log::info!("Cancel submitted on-chain",
            "contract", &contract_id[..8],
            "method", method,
            "commitment", &commitment[..16],
            "nullifier", &nullifier[..16],
            "took", log::duration_secs(&start.elapsed())
        );
        Ok(())
    };

    cancel_order(orderbook_id, "cancel_order")?;
    cancel_order(perp_id, "cancel_position")?;

    log::info!("Cancel submitted on-chain",
        "orderbook", &orderbook_id[..8],
        "perp", &perp_id[..8],
        "commitment", &commitment[..16],
        "nullifier", &nullifier[..16],
        "took", log::duration_secs(&start.elapsed())
    );
    Ok(())
}

pub fn submit_mark_price(perp_id: &str, source: &str, price: u64) -> Result<()> {
    let start = Instant::now();
    let mut cmd = std::process::Command::new("stellar");
    cmd.args([
        "contract", "invoke",
        "--id", perp_id,
        "--source", source,
        "--network-passphrase", NETWORK_PASSPHRASE,
        "--rpc-url", &rpc_url(),
        "--",
        "set_mark_price",
        "--price", &price.to_string(),
    ]);

    log::debug!("Executing stellar CLI command",
        "contract", &perp_id[..8],
        "source", source,
        "method", "set_mark_price",
        "price", price
    );

    let output = cmd.output().map_err(|e| anyhow::anyhow!("stellar invoke set_mark_price: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("xdr processing error") || stderr.contains("Transaction hash is") {
            log::info!("Mark price submitted (false-positive XDR error)",
                "contract", &perp_id[..8],
                "price", price,
                "took", log::duration_secs(&start.elapsed())
            );
            return Ok(());
        }
        log::error!("set_mark_price failed",
            "contract", &perp_id[..8],
            "stderr", &stderr[..stderr.len().min(500)]
        );
        anyhow::bail!("set_mark_price failed:\n{stderr}");
    }

    log::info!("Mark price submitted on-chain",
        "contract", &perp_id[..8],
        "price", price,
        "took", log::duration_secs(&start.elapsed())
    );
    Ok(())
}
