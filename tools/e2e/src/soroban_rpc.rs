use anyhow::{Result, bail, Context};
use serde_json::{json, Value};
use sha2::{Sha256, Digest};
use std::time::{Duration, Instant};
use stellar_xdr::*;

const DEFAULT_RPC_URL: &str = "https://stellar-testnet.g.alchemy.com/v2/FqjaGAy9IMENhdv2i_3UUVDPZnNClYNq";
const MAX_POLL_SECS: u64 = 360;

pub fn rpc_url() -> String {
    std::env::var("SOROBAN_RPC_URL").unwrap_or_else(|_| DEFAULT_RPC_URL.to_string())
}

const NETWORK_PASSPHRASE: &str = "Test SDF Network ; September 2015";
const POLL_INTERVAL: Duration = Duration::from_secs(2);
const BASE_FEE: u64 = 10_000_000;
const FEE_BUMP_MULTIPLIER: u64 = 5;

lazy_static::lazy_static! {
    static ref NETWORK_ID: [u8; 32] = {
        let h = Sha256::digest(b"Test SDF Network ; September 2015");
        let mut id = [0u8; 32];
        id.copy_from_slice(&h);
        id
    };
}

pub struct SorobanRpc {
    client: reqwest::blocking::Client,
}

impl SorobanRpc {
    pub fn new() -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("reqwest client");
        Self { client }
    }

    fn rpc_call(&self, method: &str, params: Value) -> Result<Value> {
        for attempt in 0..3 {
            let body = json!({"jsonrpc":"2.0","id":1,"method":method,"params":params});
            match self.client.post(&rpc_url()).json(&body).send() {
                Ok(resp) => match resp.json::<Value>() {
                    Ok(v) => {
                        if let Some(err) = v["error"].as_object() {
                            if attempt < 2 { std::thread::sleep(Duration::from_secs(1)); continue; }
                            bail!("RPC {method} error: {err:?}");
                        }
                        return Ok(v["result"].clone());
                    }
                    Err(e) => {
                        if attempt < 2 { std::thread::sleep(Duration::from_secs(1)); continue; }
                        bail!("RPC {method} decode: {e}");
                    }
                },
                Err(e) => {
                    if attempt < 2 {
                        eprintln!("  [rpc] {method} attempt {}/3 failed, retrying…", attempt + 1);
                        std::thread::sleep(Duration::from_secs(2u64.pow(attempt)));
                        continue;
                    }
                    bail!("RPC {method}: {e}");
                }
            }
        }
        unreachable!()
    }

    pub fn get_transaction(&self, hash: &str) -> Result<Option<TxStatus>> {
        let result = self.rpc_call("getTransaction", json!({"hash": hash}))?;
        match result["status"].as_str() {
            Some("SUCCESS") => Ok(Some(TxStatus::Success(
                result["resultXdr"].as_str().unwrap_or("").to_string(),
            ))),
            Some("FAILED") => {
                let error = result["error"].as_str().unwrap_or("unknown");
                Ok(Some(TxStatus::Failed(error.to_string())))
            }
            _ => Ok(None),
        }
    }

    pub fn send_transaction(&self, tx_xdr: &str) -> Result<String> {
        let result = self.rpc_call("sendTransaction", json!({"transaction": tx_xdr}))?;
        let hash = result["hash"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("sendTransaction: no hash: {result:?}"))?
            .to_string();
        match result["status"].as_str() {
            Some("PENDING") => Ok(hash),
            Some(status) => {
                let err = result["errorResultXdr"].as_str().unwrap_or("unknown");
                bail!("sendTransaction: {status}: {err}");
            }
            None => Ok(hash),
        }
    }

    /// Build unsigned TransactionEnvelope via CLI --build-only.
    fn build_unsigned_xdr(&self, contract_id: &str, source: &str, args: &[&str]) -> Result<String> {
        let mut cmd = std::process::Command::new("stellar");
        cmd.args([
            "contract", "invoke",
            "--id", contract_id,
            "--source-account", source,
            "--network-passphrase", NETWORK_PASSPHRASE,
            "--rpc-url", &rpc_url(),
            "--fee", &BASE_FEE.to_string(),
            "--build-only",
            "--",
        ]);
        cmd.args(args);
        let output = cmd.output().map_err(|e| anyhow::anyhow!("build-only cmd: {e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("build-only failed:\n{stderr}");
        }
        let xdr = String::from_utf8(output.stdout)
            .map_err(|_| anyhow::anyhow!("build-only: invalid UTF-8"))?
            .trim()
            .to_string();
        if xdr.is_empty() {
            bail!("build-only: empty XDR");
        }
        Ok(xdr)
    }

    /// Call simulateTransaction RPC. Returns (auth_entries, soroban_tx_data_xdr, min_resource_fee).
    fn simulate_transaction(&self, tx_xdr: &str) -> Result<(Vec<String>, String, u64)> {
        let result = self.rpc_call("simulateTransaction", json!({"transaction": tx_xdr}))?;

        // Extract auth entries from results[0].auth
        let auth_entries: Vec<String> = result["results"][0]["auth"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        // Extract SorobanTransactionData
        let tx_data = result["transactionData"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("simulateTransaction: no transactionData"))?
            .to_string();

        let min_fee: u64 = result["minResourceFee"]
            .as_str()
            .unwrap_or("0")
            .parse()
            .unwrap_or(0);

        Ok((auth_entries, tx_data, min_fee))
    }

    /// Assemble a proper TransactionEnvelope:
    ///  1. Parse the unsigned envelope (from --build-only)
    ///  2. Replace auth in the InvokeHostFunctionOp with simulation auth entries
    ///  3. Set SorobanTransactionData on the envelope ext
    ///  4. Update fee to account for minResourceFee
    fn assemble_envelope(
        &self,
        unsigned_xdr: &str,
        auth_entries: &[String],
        soroban_data_xdr: &str,
        min_resource_fee: u64,
    ) -> Result<String> {
        // Parse the unsigned envelope
        let env = TransactionEnvelope::from_xdr_base64(unsigned_xdr, Limits::none())
            .context("parse unsigned envelope")?;

        let v1 = match &env {
            TransactionEnvelope::Tx(v1) => v1.clone(),
            _ => bail!("expected Tx envelope"),
        };

        let tx = &v1.tx;

        // Parse SorobanTransactionData from simulation
        let soroban_data: SorobanTransactionData =
            SorobanTransactionData::from_xdr_base64(soroban_data_xdr, Limits::none())
                .context("parse SorobanTransactionData")?;

        // Parse auth entries
        let auth: VecM<SorobanAuthorizationEntry> = {
            let entries: Vec<SorobanAuthorizationEntry> = auth_entries
                .iter()
                .map(|b64| SorobanAuthorizationEntry::from_xdr_base64(b64, Limits::none()))
                .collect::<std::result::Result<Vec<_>, _>>()
                .context("parse auth entries")?;
            VecM::try_from(entries).context("VecM auth too large")?
        };

        // Build a new InvokeHostFunctionOp with auth entries
        let op = match &tx.operations[0].body {
            OperationBody::InvokeHostFunction(op) => InvokeHostFunctionOp {
                host_function: op.host_function.clone(),
                auth,
            },
            _ => bail!("expected InvokeHostFunction operation"),
        };

        // Calculate total fee
        let total_fee = tx.fee.saturating_add(min_resource_fee as u32);

        // Build new Transaction with SorobanTransactionData in ext
        let new_tx = Transaction {
            source_account: tx.source_account.clone(),
            fee: total_fee,
            seq_num: tx.seq_num.clone(),
            cond: tx.cond.clone(),
            memo: tx.memo.clone(),
            operations: VecM::try_from(vec![Operation {
                source_account: None,
                body: OperationBody::InvokeHostFunction(op),
            }]).unwrap(),
            ext: TransactionExt::V1(soroban_data),
        };

        let new_env = TransactionEnvelope::Tx(TransactionV1Envelope {
            tx: new_tx,
            signatures: VecM::default(),
        });

        let b64 = new_env
            .to_xdr_base64(Limits::none())
            .context("encode assembled envelope")?;
        Ok(b64)
    }

    /// Sign a TransactionEnvelope XDR using the stellar CLI.
    fn sign_xdr(&self, xdr: &str, source: &str) -> Result<String> {
        let mut cmd = std::process::Command::new("stellar");
        cmd.args(["tx", "sign", "--sign-with-key", source, "--network-passphrase", NETWORK_PASSPHRASE, "--rpc-url", &rpc_url()]);
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| anyhow::anyhow!("tx sign spawn: {e}"))?;
        use std::io::Write;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(xdr.as_bytes())
            .map_err(|e| anyhow::anyhow!("tx sign write stdin: {e}"))?;
        let output = child.wait_with_output().map_err(|e| anyhow::anyhow!("tx sign wait: {e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("tx sign failed:\n{stderr}");
        }
        Ok(String::from_utf8(output.stdout)
            .map_err(|_| anyhow::anyhow!("tx sign: invalid UTF-8"))?
            .trim()
            .to_string())
    }

    /// Poll for transaction confirmation, with fee-bump retry on timeout.
    fn poll_with_retry(
        &self,
        tx_hash: &str,
        envelope_xdr: &str,
        source: &str,
        method: &str,
        start: Instant,
    ) -> Result<String> {
        let mut current_hash = tx_hash.to_string();
        let mut current_xdr = envelope_xdr.to_string();

        for attempt in 0..3 {
            for _ in 0..(MAX_POLL_SECS / 2) {
                std::thread::sleep(POLL_INTERVAL);
                let elapsed = start.elapsed().as_secs_f64();
                if elapsed as u64 % 30 == 0 && elapsed > 1.0 {
                    eprintln!("  [tx]   still waiting… ({}s elapsed)", elapsed as u64);
                }
                match self.get_transaction(&current_hash)? {
                    Some(TxStatus::Success(result)) => {
                        eprintln!("  [tx] ✓ {} confirmed hash={} ({:.2}s)", method, current_hash, start.elapsed().as_secs_f64());
                        return Ok(result);
                    }
                    Some(TxStatus::Failed(err)) => {
                        eprintln!("  [tx] ✗ {} failed: {} ({:.2}s)", method, err, start.elapsed().as_secs_f64());
                        bail!("TX {current_hash} failed: {err}");
                    }
                    None => {}
                }
            }

            if attempt >= 2 {
                bail!("TX {current_hash} not confirmed after {}s", start.elapsed().as_secs_f64());
            }

            let bump_fee = BASE_FEE * FEE_BUMP_MULTIPLIER.pow(attempt as u32 + 1);
            eprintln!(
                "  [tx]   timeout, resubmitting fee-bump {}x ({} stroops)…",
                FEE_BUMP_MULTIPLIER.pow(attempt as u32 + 1),
                bump_fee
            );

            match self.build_fee_bump(&current_xdr, source, bump_fee as i64) {
                Ok(fb_xdr) => match self.sign_xdr(&fb_xdr, source) {
                    Ok(fb_signed) => match self.send_transaction(&fb_signed) {
                        Ok(hash) => {
                            eprintln!("  [tx] fee-bump submitted: {}", hash);
                            current_hash = hash;
                            current_xdr = fb_signed;
                        }
                        Err(e) => eprintln!("  [tx] fee-bump send failed: {e}"),
                    },
                    Err(e) => eprintln!("  [tx] fee-bump sign failed: {e}"),
                },
                Err(e) => eprintln!("  [tx] fee-bump build failed: {e}"),
            }
        }
        unreachable!()
    }

    /// Full invoke flow:
    ///  1. Build unsigned XDR via --build-only
    ///  2. Simulate via RPC → get auth entries + SorobanTransactionData + minFee
    ///  3. Assemble proper envelope with auth entries and SorobanTransactionData
    ///  4. Sign
    ///  5. Submit via sendTransaction
    ///  6. Poll with fee-bump retry
    pub fn invoke(
        &self,
        contract_id: &str,
        source: &str,
        args: &[&str],
    ) -> Result<String> {
        let method = args.first().unwrap_or(&"unknown");
        eprintln!("  [rpc] Calling {}({})…", method, &contract_id[..8]);
        let start = Instant::now();

        // 1. Build unsigned XDR
        let unsigned_xdr = self.build_unsigned_xdr(contract_id, source, args)?;

        // 2. Simulate to get auth entries + SorobanTransactionData
        let (auth_entries, soroban_data_xdr, min_fee) =
            self.simulate_transaction(&unsigned_xdr)?;

        // 3. Assemble full envelope
        let assembled_xdr = self.assemble_envelope(
            &unsigned_xdr,
            &auth_entries,
            &soroban_data_xdr,
            min_fee,
        )?;

        // 4. Sign
        let signed_xdr = self.sign_xdr(&assembled_xdr, source)?;

        // 5. Submit
        let tx_hash = self.send_transaction(&signed_xdr)?;
        eprintln!("  [rpc] {} submitted: {}", method, tx_hash);

        // 6. Poll with retry
        self.poll_with_retry(&tx_hash, &signed_xdr, source, method, start)
    }

    fn build_fee_bump(&self, inner_signed_xdr: &str, source: &str, fee: i64) -> Result<String> {
        let source_pk = source_pubkey(source)?;
        let inner_env = TransactionEnvelope::from_xdr_base64(inner_signed_xdr, Limits::none())
            .context("parse inner envelope for fee-bump")?;
        let inner_v1 = match &inner_env {
            TransactionEnvelope::Tx(v1) => v1.clone(),
            _ => bail!("Expected Tx envelope for fee-bump, got {:?}", inner_env.name()),
        };

        let fb_tx = FeeBumpTransaction {
            fee_source: MuxedAccount::Ed25519(Uint256(pk_to_bytes(&source_pk)?)),
            fee,
            inner_tx: FeeBumpTransactionInnerTx::Tx(inner_v1),
            ext: FeeBumpTransactionExt::V0,
        };
        let fb_env = FeeBumpTransactionEnvelope {
            tx: fb_tx,
            signatures: VecM::default(),
        };
        Ok(fb_env.to_xdr_base64(Limits::none())?)
    }
}

fn source_pubkey(identity: &str) -> Result<String> {
    let out = std::process::Command::new("stellar")
        .args(["keys", "address", identity])
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to get key address: {e}"))?;
    if !out.status.success() {
        bail!("Identity '{identity}' not found");
    }
    Ok(String::from_utf8(out.stdout)
        .map_err(|_| anyhow::anyhow!("invalid UTF-8"))?
        .trim()
        .to_string())
}

fn pk_to_bytes(pk: &str) -> Result<[u8; 32]> {
    use stellar_strkey::Strkey;
    let key = Strkey::from_string(pk).map_err(|e| anyhow::anyhow!("invalid strkey: {e}"))?;
    match key {
        Strkey::PublicKeyEd25519(pk) => Ok(pk.0),
        _ => bail!("expected Ed25519 public key, got {key:?}"),
    }
}

pub enum TxStatus {
    Success(String),
    Failed(String),
}
