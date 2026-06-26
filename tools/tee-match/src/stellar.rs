use crate::proof::MatchProof;
use anyhow::Result;

pub fn submit_match(perp_id: &str, source: &str, cmt_a: &str, cmt_b: &str, proof: &MatchProof) -> Result<()> {
    let hex = |dec: &str| -> String {
        let n: num_bigint::BigUint = dec.parse().expect("Invalid decimal");
        format!("{:0>64x}", n)
    };

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
        "--source-account", source,
        "--network", "testnet",
        "--",
        "match_positions",
        "--cmt_a", cmt_a,
        "--cmt_b", cmt_b,
        "--nullifier_a", &hex(&proof.public_inputs[4]),
        "--nullifier_b", &hex(&proof.public_inputs[5]),
        "--match_price", &hex(&proof.public_inputs[2]),
        "--match_size", &hex(&proof.public_inputs[3]),
        "--proof-file-path", &tmp.to_string_lossy(),
    ]);

    let output = cmd.output().map_err(|e| anyhow::anyhow!("stellar invoke: {e}"))?;
    let _ = std::fs::remove_file(&tmp);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("xdr processing error") || stderr.contains("Transaction hash is") {
            return Ok(());
        }
        anyhow::bail!("match failed:\n{stderr}");
    }
    Ok(())
}
