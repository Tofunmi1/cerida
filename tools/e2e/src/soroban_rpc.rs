use anyhow::{Result, bail, Context};
use serde_json::{json, Value};
use sha2::{Sha256, Digest};
use std::time::{Duration, Instant};
use stellar_xdr::*;

const DEFAULT_RPC_URL: &str = "https://stellar-testnet.g.alchemy.com/v2/FqjaGAy9IMENhdv2i_3UUVDPZnNClYNq";
const MAX_POLL_SECS: u64 = 120;

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

    /// Build an InvokeHostFunction XDR directly, bypassing CLI WASM-size limitations.
    pub fn build_invoke_xdr(
        &self,
        contract_id: &str,
        source: &str,
        function: &str,
        args: Vec<ScVal>,
    ) -> Result<String> {
        let source_pk = source_pubkey(source)?;
        let pk_bytes = pk_to_bytes(&source_pk)?;

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

    /// Sign a TransactionEnvelope XDR using the stellar CLI.
    pub(crate) fn sign_xdr(&self, xdr: &str, source: &str) -> Result<String> {
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
    pub(crate) fn poll_with_retry(
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

/// Deploy a contract by WASM hash, bypassing CLI XDR size check.
/// Returns the contract ID (Stellar C... strkey).
pub fn deploy_contract_via_rpc(wasm_hash_hex: &str, salt_hex: &str, source: &str) -> Result<String> {
    use stellar_strkey::Strkey;
    let rpc = SorobanRpc::new();
    let source_pk = source_pubkey(source)?;
    let start = Instant::now();
    eprintln!("  [rpc] Deploying contract via RPC (wasm_hash={})…", &wasm_hash_hex[..16]);

    let mut wasm_hash = [0u8; 32];
    hex::decode_to_slice(wasm_hash_hex, &mut wasm_hash)
        .map_err(|e| anyhow::anyhow!("invalid wasm hash: {e}"))?;
    let mut salt = [0u8; 32];
    hex::decode_to_slice(salt_hex, &mut salt)
        .map_err(|e| anyhow::anyhow!("invalid salt: {e}"))?;

    let pk_bytes = pk_to_bytes(&source_pk)?;
    let preimage = ContractIdPreimage::Address(ContractIdPreimageFromAddress {
        address: ScAddress::Account(AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(pk_bytes)))),
        salt: Uint256(salt),
    });

    let create_args = CreateContractArgs {
        contract_id_preimage: preimage,
        executable: ContractExecutable::Wasm(Hash(wasm_hash)),
    };
    let host_fn = HostFunction::CreateContract(create_args);
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
    eprintln!("  [rpc] deploy submitted: {}", tx_hash);
    rpc.poll_with_retry(&tx_hash, &signed, source, "deploy_contract", start)?;

    // Derive contract ID strkey
    let contract_id = derive_contract_id(&pk_bytes, &salt)?;
    eprintln!("  [rpc] ✓ contract deployed: {}", contract_id);
    Ok(contract_id)
}

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

fn get_sequence_number(_rpc: &SorobanRpc, account_id: &str) -> Result<i64> {
    let client = reqwest::blocking::Client::new();
    let url = format!("https://horizon-testnet.stellar.org/accounts/{account_id}");
    let v: serde_json::Value = client.get(&url).send()?.json()?;
    let seq: i64 = v["sequence"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("horizon getAccount: no sequence field in response"))?
        .parse()
        .context("parse sequence")?;
    Ok(seq)
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
