use anyhow::{Result, bail, Context};
use serde_json::{json, Value};
use sha2::{Sha256, Digest};
use std::time::{Duration, Instant};
use stellar_xdr::*;

const DEFAULT_RPC_URL: &str = "https://stellar-testnet.g.alchemy.com/v2/lT6Z7-nwZ3J20d6_LC7dz";
const MAX_POLL_SECS: u64 = 180;

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

    pub(crate) fn rpc_call(&self, method: &str, params: Value) -> Result<Value> {
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
                // Try to extract a readable reason from resultXdr / resultMetaXdr
                let detail = extract_tx_failure_detail(&result);
                Ok(Some(TxStatus::Failed(detail)))
            }
            _ => Ok(None),
        }
    }

    pub fn send_transaction(&self, tx_xdr: &str) -> Result<String> {
        let mut last_hash = String::new();
        // Track whether any previous attempt got TRY_AGAIN_LATER.
        // If yes and the next attempt returns ERROR, it means the seq was consumed by the
        // original TX (txBAD_SEQ) — hand the hash to the poller rather than bailing.
        let mut had_try_again = false;
        for attempt in 0..5 {
            let result = self.rpc_call("sendTransaction", json!({"transaction": tx_xdr}))?;
            let hash = result["hash"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("sendTransaction: no hash: {result:?}"))?
                .to_string();
            last_hash = hash.clone();
            match result["status"].as_str() {
                Some("PENDING") => return Ok(hash),
                // TRY_AGAIN_LATER: the TX *may* have been submitted per Stellar spec.
                // Wait one ledger (~6s) then check if it landed. If NOT_FOUND, re-submit.
                Some("TRY_AGAIN_LATER") => {
                    had_try_again = true;
                    eprintln!("  [rpc] sendTransaction TRY_AGAIN_LATER (attempt {}), checking after 8s…", attempt + 1);
                    std::thread::sleep(Duration::from_secs(8));
                    // Check if the TX landed despite TRY_AGAIN_LATER
                    match self.get_transaction(&hash) {
                        Ok(Some(_)) => {
                            eprintln!("  [rpc] TX {} was ingested despite TRY_AGAIN_LATER", &hash[..12]);
                            return Ok(hash);
                        }
                        Ok(None) => {
                            // NOT_FOUND — TX was not ingested, safe to re-submit
                            eprintln!("  [rpc] TX not found, re-submitting (attempt {})…", attempt + 2);
                            continue;
                        }
                        Err(e) => eprintln!("  [rpc] getTransaction check failed: {e}, re-submitting…"),
                    }
                    continue;
                }
                Some("ERROR") => {
                    let err = result["errorResultXdr"].as_str().unwrap_or("unknown");
                    if had_try_again {
                        // ERROR after TRY_AGAIN_LATER means the original TX was ingested
                        // (seq consumed). Hand the hash off to poll_with_retry.
                        eprintln!("  [rpc] sendTransaction ERROR after TRY_AGAIN_LATER (seq consumed) — handing to poller");
                        return Ok(last_hash);
                    }
                    bail!("sendTransaction: ERROR: {err}");
                }
                Some(status) => {
                    let err = result["errorResultXdr"].as_str().unwrap_or("unknown");
                    bail!("sendTransaction: {status}: {err}");
                }
                None => return Ok(hash),
            }
        }
        // Return last hash so poll_with_retry can handle via fee-bump if needed
        eprintln!("  [rpc] sendTransaction: TRY_AGAIN_LATER after 5 attempts, handing off to poller");
        Ok(last_hash)
    }

    /// Build an InvokeHostFunction XDR directly, bypassing CLI WASM-size limitations.
    pub fn build_invoke_xdr(
        &self,
        contract_id: &str,
        source: &str,
        function: &str,
        args: Vec<ScVal>,
    ) -> Result<String> {
        let (source_pk, pk_bytes) = resolve_source(source)?;

        let contract_bytes = stellar_strkey::Contract::from_string(contract_id)
            .map_err(|e| anyhow::anyhow!("invalid contract id: {e}"))?
            .0;
        let contract_addr = ScAddress::Contract(ContractId(Hash(contract_bytes)));

        let args_vec: VecM<ScVal> = args.try_into()
            .map_err(|_| anyhow::anyhow!("too many args"))?;
        let invoke_args = InvokeContractArgs {
            contract_address: contract_addr,
            function_name: ScSymbol(function.to_string().try_into()
                .map_err(|_| anyhow::anyhow!("function name too long"))?),
            args: args_vec,
        };

        let seq_num = get_sequence_number(self, &source_pk)?;
        let tx = Transaction {
            source_account: MuxedAccount::Ed25519(Uint256(pk_bytes)),
            fee: 10_000_000,
            seq_num: SequenceNumber(seq_num + 1),
            cond: Preconditions::None,
            memo: Memo::None,
            operations: VecM::try_from(vec![Operation {
                source_account: None,
                body: OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
                    host_function: HostFunction::InvokeContract(invoke_args),
                    auth: VecM::default(),
                }),
            }]).unwrap(),
            ext: TransactionExt::V0,
        };
        let env = TransactionEnvelope::Tx(TransactionV1Envelope {
            tx,
            signatures: VecM::default(),
        });
        Ok(env.to_xdr_base64(Limits::none())?)
    }

    /// Full invoke using direct XDR (no CLI contract-fetch, bypasses WASM size limit).
    pub fn invoke_xdr(
        &self,
        contract_id: &str,
        source: &str,
        function: &str,
        args: Vec<ScVal>,
    ) -> Result<String> {
        let method = function;
        eprintln!("  [rpc] Calling {}({})…", method, &contract_id[..8]);
        let start = Instant::now();

        let unsigned_xdr = self.build_invoke_xdr(contract_id, source, function, args)?;
        let (auth_entries, soroban_data_xdr, min_fee) = self.simulate_transaction(&unsigned_xdr)?;
        let assembled = self.assemble_envelope(&unsigned_xdr, &auth_entries, &soroban_data_xdr, min_fee)?;
        let signed = self.sign_xdr(&assembled, source)?;
        let tx_hash = self.send_transaction(&signed)?;
        eprintln!("  [rpc] {} submitted: {}", method, tx_hash);
        self.poll_with_retry(&tx_hash, &signed, source, method, start)
    }

    /// Simulate a view call and return the result as a debug string (no TX submitted).
    pub fn invoke_view_xdr(
        &self,
        contract_id: &str,
        source: &str,
        function: &str,
        args: Vec<ScVal>,
    ) -> Result<String> {
        eprintln!("  [view] {}({})…", function, &contract_id[..8]);
        let unsigned_xdr = self.build_invoke_xdr(contract_id, source, function, args)?;
        let result = self.rpc_call("simulateTransaction", json!({"transaction": unsigned_xdr}))?;
        let xdr = result["results"][0]["xdr"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("simulateTransaction view: no result xdr: {result:?}"))?;
        let val = ScVal::from_xdr_base64(xdr, Limits::none())
            .map_err(|e| anyhow::anyhow!("decode view result: {e}"))?;
        Ok(format!("{val:?}"))
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
    pub(crate) fn simulate_transaction(&self, tx_xdr: &str) -> Result<(Vec<String>, String, u64)> {
        let result = self.rpc_call("simulateTransaction", json!({"transaction": tx_xdr}))?;

        // Check for simulation-level error (e.g. contract not found, execution failure)
        if let Some(err) = result["error"].as_str() {
            bail!("simulateTransaction failed: {err}");
        }

        // Extract auth entries from results[0].auth
        let auth_entries: Vec<String> = result["results"][0]["auth"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        // Extract SorobanTransactionData
        let tx_data = result["transactionData"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("simulateTransaction: no transactionData (full response: {result:?})"))?
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
    pub(crate) fn assemble_envelope(
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

    /// Sign a TransactionEnvelope XDR natively (no stellar CLI required).
    /// If source starts with 'S' it's treated as a raw secret key strkey.
    /// Otherwise falls back to the stellar CLI (dev machines only).
    pub(crate) fn sign_xdr(&self, xdr: &str, source: &str) -> Result<String> {
        if source.starts_with('S') {
            return sign_xdr_native(xdr, source);
        }
        // Fallback: stellar CLI (only available on dev machines)
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

    /// Poll for transaction confirmation, with re-submit retry on NOT_FOUND timeout.
    pub(crate) fn poll_with_retry(
        &self,
        tx_hash: &str,
        envelope_xdr: &str,
        source: &str,
        method: &str,
        start: Instant,
    ) -> Result<String> {
        let mut current_hash = tx_hash.to_string();
        let original_xdr = envelope_xdr.to_string();
        // Set when re-submit returns ERROR (seq already consumed → TX was ingested).
        // When known ingested, stop re-submitting and give the indexer up to INGESTED_ASSUME_OK_SECS
        // from ingestion_detected_at before assuming success.
        let mut tx_known_ingested = false;
        let mut ingestion_detected_at: Option<Instant> = None;
        // If the indexer won't confirm for this long after we know the TX is ingested, assume OK.
        const INGESTED_ASSUME_OK_SECS: u64 = 120;

        for attempt in 0..5 {
            for _ in 0..(MAX_POLL_SECS / 2) {
                std::thread::sleep(POLL_INTERVAL);
                let elapsed = start.elapsed().as_secs_f64();
                if elapsed as u64 % 30 == 0 && elapsed > 1.0 {
                    eprintln!("  [tx]   still waiting… ({}s elapsed)", elapsed as u64);
                }

                // Once we know the TX is ingested and the indexer has had INGESTED_ASSUME_OK_SECS
                // to catch up, stop waiting and assume success.
                if let Some(det) = ingestion_detected_at {
                    if det.elapsed().as_secs() >= INGESTED_ASSUME_OK_SECS {
                        eprintln!(
                            "  [tx] ✓ {} assuming success (TX ingested, indexer lag {}s, hash={})",
                            method, det.elapsed().as_secs(), current_hash
                        );
                        return Ok(current_hash);
                    }
                }

                match self.get_transaction(&current_hash)? {
                    Some(TxStatus::Success(_result)) => {
                        eprintln!("  [tx] ✓ {} confirmed hash={} ({:.2}s)", method, current_hash, start.elapsed().as_secs_f64());
                        return Ok(current_hash);
                    }
                    Some(TxStatus::Failed(err)) => {
                        eprintln!("  [tx] ✗ {} failed: {} ({:.2}s)", method, err, start.elapsed().as_secs_f64());
                        bail!("TX {current_hash} failed: {err}");
                    }
                    None => {}
                }
            }

            if attempt >= 4 {
                bail!("TX {current_hash} not confirmed after {}s", start.elapsed().as_secs_f64());
            }

            if tx_known_ingested {
                // TX is ingested but indexer is lagging — just keep polling, no re-submit.
                eprintln!("  [tx]   indexer still lagging (TX known ingested), polling more…");
                continue;
            }

            // Re-submit the original signed TX. If the TX was truly NOT ingested
            // (NOT_FOUND for 180s), re-submitting with the same seq is valid.
            eprintln!("  [tx]   timeout — re-submitting original TX (attempt {})…", attempt + 2);
            match self.send_transaction(&original_xdr) {
                Ok(hash) => {
                    eprintln!("  [tx] re-submitted: {}", hash);
                    current_hash = hash;
                }
                Err(e) => {
                    // "sendTransaction: ERROR:" means the TX hit a ledger (seq consumed).
                    // The RPC indexer is just lagging. Switch to polling-only + assume-ok mode.
                    if e.to_string().contains("sendTransaction: ERROR:") {
                        eprintln!("  [tx] re-submit ERROR (seq consumed, TX ingested) — will assume OK in {}s…", INGESTED_ASSUME_OK_SECS);
                        tx_known_ingested = true;
                        ingestion_detected_at = Some(Instant::now());
                    } else {
                        eprintln!("  [tx] re-submit failed: {e} — continuing to poll original");
                    }
                }
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

    fn build_and_sign_fee_bump(&self, inner_signed_xdr: &str, source: &str, fee: i64) -> Result<String> {
        use ed25519_dalek::{SigningKey, Signer};
        use stellar_strkey::Strkey;

        // Get secret key from CLI
        let sk_out = std::process::Command::new("stellar")
            .args(["keys", "show", source])
            .output()
            .map_err(|e| anyhow::anyhow!("stellar keys show: {e}"))?;
        if !sk_out.status.success() {
            bail!("stellar keys show failed: {}", String::from_utf8_lossy(&sk_out.stderr));
        }
        let sk_str = String::from_utf8(sk_out.stdout)?.trim().to_string();
        let sk_bytes = match Strkey::from_string(&sk_str)? {
            Strkey::PrivateKeyEd25519(sk) => sk.0,
            _ => bail!("expected private key strkey"),
        };
        let signing_key = SigningKey::from_bytes(&sk_bytes);

        // Build the inner envelope
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

        // Sign natively
        let tagged = TransactionSignaturePayloadTaggedTransaction::TxFeeBump(fb_tx.clone());
        let payload = TransactionSignaturePayload {
            network_id: Hash(*NETWORK_ID),
            tagged_transaction: tagged,
        };
        let payload_xdr = payload.to_xdr(Limits::none())?;
        let hash = Sha256::digest(&payload_xdr);
        let sig = signing_key.sign(&hash);
        let pk_bytes = signing_key.verifying_key().to_bytes();

        let decorated_sig = DecoratedSignature {
            hint: SignatureHint(pk_bytes[28..].try_into().unwrap()),
            signature: Signature(sig.to_bytes().to_vec().try_into().unwrap()),
        };
        let sigs = VecM::try_from(vec![decorated_sig]).map_err(|e| anyhow::anyhow!("sigs VecM: {e}"))?;
        let fb_env = TransactionEnvelope::TxFeeBump(FeeBumpTransactionEnvelope {
            tx: fb_tx,
            signatures: sigs,
        });
        Ok(fb_env.to_xdr_base64(Limits::none())?)
    }
}

/// Deploy a contract by WASM hash, bypassing CLI XDR size check.
/// Returns the contract ID (Stellar C... strkey).
// ── ScVal encoding helpers ────────────────────────────────────────────────────

pub fn scval_address(strkey: &str) -> Result<ScVal> {
    if let Ok(pk) = stellar_strkey::ed25519::PublicKey::from_string(strkey) {
        let addr = ScAddress::Account(AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(pk.0))));
        return Ok(ScVal::Address(addr));
    }
    if let Ok(c) = stellar_strkey::Contract::from_string(strkey) {
        return Ok(ScVal::Address(ScAddress::Contract(ContractId(Hash(c.0)))));
    }
    anyhow::bail!("scval_address: not a valid strkey: {strkey}")
}

pub fn scval_bytes32(hex: &str) -> Result<ScVal> {
    let hex = hex.trim_start_matches("0x");
    let padded = format!("{:0>64}", hex);
    let mut b = [0u8; 32];
    hex::decode_to_slice(&padded, &mut b).map_err(|e| anyhow::anyhow!("scval_bytes32: {e}"))?;
    Ok(ScVal::Bytes(ScBytes(b.to_vec().try_into().map_err(|_| anyhow::anyhow!("bytes32 too long"))?)))
}

pub fn scval_u64(n: u64) -> ScVal { ScVal::U64(n) }
pub fn scval_u32(n: u32) -> ScVal { ScVal::U32(n) }

pub fn scval_bytes(bytes: &[u8]) -> Result<ScVal> {
    Ok(ScVal::Bytes(ScBytes(bytes.to_vec().try_into().map_err(|_| anyhow::anyhow!("scval_bytes: vec too large"))?)))
}

pub fn scval_u128(n: u128) -> ScVal {
    ScVal::U128(UInt128Parts {
        hi: (n >> 64) as u64,
        lo: n as u64,
    })
}

pub fn scval_tif(tif: &str) -> Result<ScVal> {
    let n = match tif {
        "GTC" => 0u32,
        "IOC" => 1,
        "FOK" => 2,
        "GTD" => 3,
        _ => anyhow::bail!("unknown TimeInForce: {tif}"),
    };
    Ok(ScVal::U32(n))
}

pub fn scval_i128(n: i128) -> ScVal {
    ScVal::I128(Int128Parts {
        hi: (n >> 64) as i64,
        lo: n as u64,
    })
}

/// Encode a Groth16Proof JSON {"a":"hex","b":"hex","c":"hex"} as ScVal::Map.
pub fn scval_proof(proof_json: &str) -> Result<ScVal> {
    let v: serde_json::Value = serde_json::from_str(proof_json)
        .map_err(|e| anyhow::anyhow!("proof JSON parse: {e}"))?;
    let decode_hex = |field: &str| -> Result<Vec<u8>> {
        let s = v[field].as_str().ok_or_else(|| anyhow::anyhow!("proof.{field} missing"))?;
        hex::decode(s).map_err(|e| anyhow::anyhow!("proof.{field} hex: {e}"))
    };
    let a_bytes = decode_hex("a")?;
    let b_bytes = decode_hex("b")?;
    let c_bytes = decode_hex("c")?;

    let mk_entry = |k: &str, bytes: Vec<u8>| -> Result<ScMapEntry> {
        Ok(ScMapEntry {
            key: ScVal::Symbol(ScSymbol(k.to_string().try_into()
                .map_err(|_| anyhow::anyhow!("symbol too long"))?)),
            val: ScVal::Bytes(ScBytes(bytes.try_into()
                .map_err(|_| anyhow::anyhow!("bytes too long"))?)),
        })
    };

    let entries = vec![mk_entry("a", a_bytes)?, mk_entry("b", b_bytes)?, mk_entry("c", c_bytes)?];
    Ok(ScVal::Map(Some(ScMap(entries.try_into().map_err(|_| anyhow::anyhow!("map too large"))?))))
}

/// Encode an AssetConfig struct as ScVal::Map for register_asset.
pub fn scval_asset_config(
    max_leverage: u64,
    maintenance_margin_bps: i128,
    initial_margin_bps: i128,
    liq_partial_reward_bps: i128,
    liq_full_reward_bps: i128,
    ins_fund_bps: i128,
    active: bool,
) -> Result<ScVal> {
    let mk_entry = |k: &str, v: ScVal| -> Result<ScMapEntry> {
        Ok(ScMapEntry {
            key: ScVal::Symbol(ScSymbol(k.to_string().try_into()
                .map_err(|_| anyhow::anyhow!("symbol too long"))?)),
            val: v,
        })
    };
    // Keys must be alphabetically sorted for Soroban ScMap deserialization.
    let entries = vec![
        mk_entry("active", ScVal::Bool(active))?,
        mk_entry("initial_margin_bps", scval_i128(initial_margin_bps))?,
        mk_entry("ins_fund_bps", scval_i128(ins_fund_bps))?,
        mk_entry("liq_full_reward_bps", scval_i128(liq_full_reward_bps))?,
        mk_entry("liq_partial_reward_bps", scval_i128(liq_partial_reward_bps))?,
        mk_entry("maintenance_margin_bps", scval_i128(maintenance_margin_bps))?,
        mk_entry("max_leverage", ScVal::U64(max_leverage))?,
    ];
    Ok(ScVal::Map(Some(ScMap(entries.try_into().map_err(|_| anyhow::anyhow!("map too large"))?))))
}

fn derive_contract_id(source_pk: &[u8; 32], salt: &[u8; 32]) -> Result<String> {
    use sha2::{Sha256, Digest};
    // Contract ID = SHA256( network_id || SHA256( "ContractID" || preimage ) )
    // Simpler: use stellar CLI to compute it
    let source_kp = stellar_strkey::Strkey::PublicKeyEd25519(stellar_strkey::ed25519::PublicKey(*source_pk));
    let source_str = source_kp.to_string();
    let salt_hex = hex::encode(salt);
    let out = std::process::Command::new("stellar")
        .args([
            "contract", "id", "wasm",
            "--salt", &salt_hex,
            "--source-account", &source_str,
            "--network-passphrase", NETWORK_PASSPHRASE,
            "--rpc-url", &rpc_url(),
        ])
        .output()
        .map_err(|e| anyhow::anyhow!("contract id cmd: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let id = stdout.trim().to_string();
    if id.is_empty() {
        anyhow::bail!("derive_contract_id: empty output:\n{}", String::from_utf8_lossy(&out.stderr));
    }
    Ok(id)
}

/// Upload WASM directly via RPC, bypassing the stellar CLI's client-side size check.
/// Returns the WASM hash hex string.
pub fn install_wasm_via_rpc(wasm_bytes: &[u8], source: &str) -> Result<String> {
    let rpc = SorobanRpc::new();
    let source_pk = source_pubkey(source)?;
    let start = Instant::now();
    eprintln!("  [rpc] Uploading WASM via RPC ({} bytes)…", wasm_bytes.len());

    // Compute WASM hash
    let wasm_hash_bytes: [u8; 32] = Sha256::digest(wasm_bytes).into();
    let wasm_hash_hex = hex::encode(wasm_hash_bytes);

    // Build InvokeHostFunction: UploadContractWasm
    let wasm_bytesm: BytesM = wasm_bytes.to_vec().try_into()
        .map_err(|_| anyhow::anyhow!("wasm too large for BytesM"))?;
    let host_fn = HostFunction::UploadContractWasm(wasm_bytesm);
    let op = Operation {
        source_account: None,
        body: OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
            host_function: host_fn,
            auth: VecM::default(),
        }),
    };

    // Get account sequence number
    let account_id = source_pubkey(source)?;
    let seq_num = get_sequence_number(&rpc, &account_id)?;

    let tx = Transaction {
        source_account: MuxedAccount::Ed25519(Uint256(pk_to_bytes(&source_pk)?)),
        fee: 10_000_000,
        seq_num: SequenceNumber(seq_num + 1),
        cond: Preconditions::None,
        memo: Memo::None,
        operations: VecM::try_from(vec![op]).unwrap(),
        ext: TransactionExt::V0,
    };
    let unsigned_env = TransactionEnvelope::Tx(TransactionV1Envelope {
        tx,
        signatures: VecM::default(),
    });
    let unsigned_xdr = unsigned_env.to_xdr_base64(Limits::none())?;

    // Simulate
    let (auth_entries, soroban_data_xdr, min_fee) = rpc.simulate_transaction(&unsigned_xdr)?;

    // Assemble
    let assembled = rpc.assemble_envelope(&unsigned_xdr, &auth_entries, &soroban_data_xdr, min_fee)?;

    // Sign
    let signed = rpc.sign_xdr(&assembled, source)?;

    // Submit
    let tx_hash = rpc.send_transaction(&signed)?;
    eprintln!("  [rpc] WASM upload submitted: {}", tx_hash);

    rpc.poll_with_retry(&tx_hash, &signed, source, "upload_wasm", start)?;
    eprintln!("  [rpc] ✓ WASM installed, hash: {}", &wasm_hash_hex[..16]);
    Ok(wasm_hash_hex)
}

/// Deploy a contract from an already-installed WASM hash, bypassing the Stellar CLI entirely.
/// Returns the contract ID (Stellar C... strkey).
pub fn deploy_contract_via_rpc(wasm_hash_hex: &str, salt: [u8; 32], source: &str) -> Result<String> {
    let rpc = SorobanRpc::new();
    let source_pk = source_pubkey(source)?;
    let pk_bytes = pk_to_bytes(&source_pk)?;
    let start = Instant::now();

    let wasm_hash_bytes: [u8; 32] = hex::decode(wasm_hash_hex)
        .map_err(|e| anyhow::anyhow!("invalid wasm hash hex: {e}"))?
        .try_into()
        .map_err(|_| anyhow::anyhow!("wasm hash must be 32 bytes"))?;

    // Preimage used for both contract ID computation and the host function
    let preimage = ContractIdPreimage::Address(ContractIdPreimageFromAddress {
        address: ScAddress::Account(AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(pk_bytes)))),
        salt: Uint256(salt),
    });

    // Compute the contract ID deterministically
    let hash_id_preimage = HashIdPreimage::ContractId(HashIdPreimageContractId {
        network_id: Hash(*NETWORK_ID),
        contract_id_preimage: preimage.clone(),
    });
    let preimage_xdr = hash_id_preimage.to_xdr(Limits::none())
        .map_err(|e| anyhow::anyhow!("encode HashIdPreimage: {e}"))?;
    let contract_id_bytes: [u8; 32] = Sha256::digest(&preimage_xdr).into();
    let contract_strkey = stellar_strkey::Strkey::Contract(
        stellar_strkey::Contract(contract_id_bytes),
    ).to_string();

    eprintln!("  [rpc] Deploying contract from hash {} (salt {}…)", &wasm_hash_hex[..16], hex::encode(&salt[..4]));
    eprintln!("  [rpc] Precomputed contract ID: {}", contract_strkey);

    let host_fn = HostFunction::CreateContractV2(CreateContractArgsV2 {
        contract_id_preimage: preimage,
        executable: ContractExecutable::Wasm(Hash(wasm_hash_bytes)),
        constructor_args: VecM::default(),
    });
    let op = Operation {
        source_account: None,
        body: OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
            host_function: host_fn,
            auth: VecM::default(),
        }),
    };

    let seq_num = get_sequence_number(&rpc, &source_pk)?;
    let tx = Transaction {
        source_account: MuxedAccount::Ed25519(Uint256(pk_bytes)),
        fee: 10_000_000,
        seq_num: SequenceNumber(seq_num + 1),
        cond: Preconditions::None,
        memo: Memo::None,
        operations: VecM::try_from(vec![op]).unwrap(),
        ext: TransactionExt::V0,
    };
    let unsigned_env = TransactionEnvelope::Tx(TransactionV1Envelope {
        tx,
        signatures: VecM::default(),
    });
    let unsigned_xdr = unsigned_env.to_xdr_base64(Limits::none())?;

    let (auth_entries, soroban_data_xdr, min_fee) = rpc.simulate_transaction(&unsigned_xdr)?;
    let assembled = rpc.assemble_envelope(&unsigned_xdr, &auth_entries, &soroban_data_xdr, min_fee)?;
    let signed = rpc.sign_xdr(&assembled, source)?;
    let tx_hash = rpc.send_transaction(&signed)?;
    eprintln!("  [rpc] Contract deploy TX submitted: {}", tx_hash);

    rpc.poll_with_retry(&tx_hash, &signed, source, "deploy_contract", start)?;
    eprintln!("  [rpc] ✓ Contract deployed: {}", contract_strkey);
    Ok(contract_strkey)
}

fn get_sequence_number(rpc: &SorobanRpc, account_id: &str) -> Result<i64> {
    // Use the same RPC node as submission to avoid Horizon lag causing TxBadSeq.
    let pk_bytes = pk_to_bytes(account_id)?;
    let ledger_key = LedgerKey::Account(LedgerKeyAccount {
        account_id: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(pk_bytes))),
    });
    let key_xdr = ledger_key
        .to_xdr_base64(Limits::none())
        .map_err(|e| anyhow::anyhow!("encode ledger key: {e}"))?;
    let result = rpc.rpc_call("getLedgerEntries", serde_json::json!({ "keys": [key_xdr] }))?;
    let entry_xdr = result["entries"][0]["xdr"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("getLedgerEntries: no xdr for account {account_id}: {result:?}"))?
        .to_string();
    let entry = LedgerEntryData::from_xdr_base64(&entry_xdr, Limits::none())
        .map_err(|e| anyhow::anyhow!("decode ledger entry xdr: {e}"))?;
    match entry {
        LedgerEntryData::Account(acc) => {
            eprintln!("  [seq] {} seqNum={}", &account_id[..8], acc.seq_num.0);
            Ok(acc.seq_num.0.into())
        }
        _ => anyhow::bail!("getLedgerEntries: not an account entry for {account_id}"),
    }
}

/// Resolve a source to (public_key_string, public_key_bytes).
/// If `source` starts with "S", treat it as a secret key and derive the public key directly.
/// Otherwise, treat it as a stellar CLI identity name and look it up.
fn resolve_source(source: &str) -> Result<(String, [u8; 32])> {
    if source.starts_with('S') {
        use ed25519_dalek::SigningKey;
        let key = stellar_strkey::Strkey::from_string(source)
            .map_err(|e| anyhow::anyhow!("invalid secret key: {e}"))?;
        match key {
            stellar_strkey::Strkey::PrivateKeyEd25519(seed) => {
                let signing_key = SigningKey::from_bytes(&seed.0);
                let pk_bytes = signing_key.verifying_key().to_bytes();
                let pk_strkey = stellar_strkey::ed25519::PublicKey(pk_bytes);
                let pk_string = stellar_strkey::Strkey::PublicKeyEd25519(pk_strkey).to_string();
                Ok((pk_string, pk_bytes))
            }
            _ => anyhow::bail!("expected Ed25519 private key strkey, got {key:?}"),
        }
    } else {
        let pk_string = source_pubkey(source)?;
        let pk_bytes = pk_to_bytes(&pk_string)?;
        Ok((pk_string, pk_bytes))
    }
}

pub fn source_pubkey_of(identity: &str) -> Result<String> {
    source_pubkey(identity)
}

/// Derive the public key from a raw Stellar secret key (S...) string.
pub fn pubkey_from_secret(secret: &str) -> Result<String> {
    use ed25519_dalek::SigningKey;
    let key = stellar_strkey::Strkey::from_string(secret)
        .map_err(|e| anyhow::anyhow!("invalid secret key: {e}"))?;
    match key {
        stellar_strkey::Strkey::PrivateKeyEd25519(seed) => {
            let pk_bytes = SigningKey::from_bytes(&seed.0).verifying_key().to_bytes();
            let pk_strkey = stellar_strkey::ed25519::PublicKey(pk_bytes);
            Ok(stellar_strkey::Strkey::PublicKeyEd25519(pk_strkey).to_string())
        }
        _ => anyhow::bail!("expected Ed25519 private key strkey"),
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

/// Extract a human-readable failure reason from a getTransaction FAILED response.
/// Logs the raw result and meta XDR fields so the actual contract panic is visible.
fn extract_tx_failure_detail(result: &Value) -> String {
    // Try resultMetaXdr diagnostic events for contract panic strings
    if let Some(meta_b64) = result["resultMetaXdr"].as_str() {
        if let Ok(meta) = TransactionMeta::from_xdr_base64(meta_b64, Limits::none()) {
            if let TransactionMeta::V3(v3) = meta {
                for ev in v3.soroban_meta.as_ref().map(|m| m.diagnostic_events.as_slice()).unwrap_or(&[]) {
                    if let ContractEventBody::V0(body) = &ev.event.body {
                        if let ScVal::String(s) = &body.data {
                            let msg = s.to_utf8_string_lossy();
                            if !msg.is_empty() {
                                return msg;
                            }
                        }
                        for t in body.topics.as_slice() {
                            if let ScVal::String(s) = t {
                                let msg = s.to_utf8_string_lossy();
                                if !msg.is_empty() {
                                    return msg;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Fall back to resultXdr raw string and the JSON error field
    let result_xdr = result["resultXdr"].as_str().unwrap_or("");
    let json_error = result["error"].as_str().unwrap_or("unknown");
    if !result_xdr.is_empty() {
        eprintln!("  [tx] FAILED resultXdr={}", result_xdr);
    }
    json_error.to_string()
}

fn pk_to_bytes(pk: &str) -> Result<[u8; 32]> {
    use stellar_strkey::Strkey;
    let key = Strkey::from_string(pk).map_err(|e| anyhow::anyhow!("invalid strkey: {e}"))?;
    match key {
        Strkey::PublicKeyEd25519(pk) => Ok(pk.0),
        _ => bail!("expected Ed25519 public key, got {key:?}"),
    }
}

/// Sign a TransactionEnvelope XDR base64 natively using ed25519, no CLI required.
/// `source` must be a Stellar secret key strkey (starts with 'S').
fn sign_xdr_native(xdr: &str, source: &str) -> Result<String> {
    use ed25519_dalek::{SigningKey, Signer};
    use stellar_strkey::Strkey;

    // Decode secret key
    let sk_strkey = Strkey::from_string(source)
        .map_err(|e| anyhow::anyhow!("invalid secret key strkey: {e}"))?;
    let seed = match sk_strkey {
        Strkey::PrivateKeyEd25519(s) => s.0,
        _ => bail!("expected Ed25519 private key strkey"),
    };
    let signing_key = SigningKey::from_bytes(&seed);
    let pk_bytes = signing_key.verifying_key().to_bytes();

    // Decode envelope
    let mut envelope = TransactionEnvelope::from_xdr_base64(xdr, Limits::none())
        .map_err(|e| anyhow::anyhow!("decode envelope: {e}"))?;

    // Compute signature payload: SHA256(network_id || TransactionSignaturePayload.to_xdr())
    let sig_payload = TransactionSignaturePayload {
        network_id: Hash(*NETWORK_ID),
        tagged_transaction: match &envelope {
            TransactionEnvelope::Tx(e) => TransactionSignaturePayloadTaggedTransaction::Tx(e.tx.clone()),
            _ => bail!("only TransactionEnvelope::Tx supported"),
        },
    };
    let payload_xdr = sig_payload.to_xdr(Limits::none())
        .map_err(|e| anyhow::anyhow!("encode sig payload: {e}"))?;
    let hash: [u8; 32] = Sha256::digest(&payload_xdr).into();

    // Sign
    let sig_bytes: [u8; 64] = signing_key.sign(&hash).to_bytes();
    let decorated = DecoratedSignature {
        hint: SignatureHint(pk_bytes[28..32].try_into().unwrap()),
        signature: Signature::try_from(sig_bytes.to_vec())
            .map_err(|e| anyhow::anyhow!("signature encode: {e}"))?,
    };

    // Attach signature to envelope
    match &mut envelope {
        TransactionEnvelope::Tx(e) => {
            let mut sigs: Vec<DecoratedSignature> = e.signatures.to_vec();
            sigs.push(decorated);
            e.signatures = sigs.try_into().map_err(|_| anyhow::anyhow!("too many signatures"))?;
        }
        _ => bail!("only TransactionEnvelope::Tx supported"),
    }

    envelope.to_xdr_base64(Limits::none())
        .map_err(|e| anyhow::anyhow!("encode signed envelope: {e}"))
}

pub enum TxStatus {
    Success(String),
    Failed(String),
}
