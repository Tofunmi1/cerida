//! Diagnostic: fetch a commitment proof from a remote tee-match server and
//! verify it locally against the local proving/verifying keys.
//!
//! Run: TEE_ADDR=35.255.76.255:9720 cargo test -p e2e --test gcp_proof_check -- --nocapture --ignored

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::str::FromStr;

use ark_bn254::{Bn254, Fq, Fq2, Fr, G1Affine, G2Affine};
use ark_ff::AdditiveGroup;
use ark_groth16::{prepare_verifying_key, Groth16};
use num_bigint::BigUint;
use rust_circuits::{compute_commitment, fr_to_biguint, load_pk};

const SIDE: u64 = 0;
const PRICE: u64 = 100_000;
const SIZE: u64 = 1_000;
const LEVERAGE: u64 = 1;
const ASSET: u64 = 0;

fn send(addr: &str, req: &serde_json::Value) -> serde_json::Value {
    let mut stream = TcpStream::connect(addr).expect("connect to TEE");
    let line = serde_json::to_string(req).unwrap();
    stream.write_all(line.as_bytes()).unwrap();
    stream.write_all(b"\n").unwrap();
    let mut reader = BufReader::new(&stream);
    let mut resp = String::new();
    reader.read_line(&mut resp).unwrap();
    serde_json::from_str(&resp).expect("parse response JSON")
}

fn parse_fq(hex: &str) -> Fq {
    Fq::from_str(&BigUint::parse_bytes(hex.as_bytes(), 16).unwrap().to_string()).unwrap()
}

fn parse_g1(hex: &str) -> G1Affine {
    assert_eq!(hex.len(), 128, "G1 hex must be 128 chars, got {}", hex.len());
    G1Affine::new(parse_fq(&hex[..64]), parse_fq(&hex[64..]))
}

fn parse_g2(hex: &str) -> G2Affine {
    assert_eq!(hex.len(), 256, "G2 hex must be 256 chars, got {}", hex.len());
    // g2_to_hex writes c1, c0, d1, d0
    let x = Fq2::new(parse_fq(&hex[64..128]), parse_fq(&hex[..64]));
    let y = Fq2::new(parse_fq(&hex[192..256]), parse_fq(&hex[128..192]));
    G2Affine::new(x, y)
}

#[test]
#[ignore]
fn gcp_proof_verifies_against_local_vk() {
    let addr = std::env::var("TEE_ADDR").unwrap_or_else(|_| "35.255.76.255:9720".to_string());
    let nonce: u64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let secret: u64 = nonce.wrapping_mul(0x9E3779B97F4A7C15);

    eprintln!("── 1. init on {addr} (nonce={nonce} secret={secret})");
    let resp = send(&addr, &serde_json::json!({
        "cmd": "init",
        "side": SIDE, "price": PRICE, "size": SIZE,
        "leverage": LEVERAGE, "asset": ASSET,
        "nonce": nonce, "secret": secret,
    }));
    assert_eq!(resp["ok"], true, "init failed: {resp}");
    let remote_cmt = resp["commitment"].as_str().expect("commitment in response").to_string();
    eprintln!("   remote commitment: {remote_cmt}");

    eprintln!("── 2. compute commitment locally with same secrets");
    let local_cmt_fr = compute_commitment(
        Fr::from(SIDE), Fr::from(PRICE), Fr::from(SIZE),
        Fr::from(LEVERAGE), Fr::from(ASSET), Fr::ZERO,
        Fr::from(nonce), Fr::from(secret),
    );
    let local_cmt = format!("{:0>64x}", fr_to_biguint(&local_cmt_fr));
    eprintln!("   local  commitment: {local_cmt}");
    assert_eq!(
        remote_cmt, local_cmt,
        "COMMITMENT MISMATCH — remote binary computes a different Poseidon hash \
         (image built from different rust-circuits source than local)"
    );
    eprintln!("   ✓ commitments match — circuit code in image matches local");

    eprintln!("── 3. commit-proof from remote");
    let resp = send(&addr, &serde_json::json!({ "cmd": "commit-proof", "cmt": remote_cmt }));
    assert_eq!(resp["ok"], true, "commit-proof failed: {resp}");
    let proof_json: serde_json::Value =
        serde_json::from_str(resp["proof"].as_str().expect("proof in response")).unwrap();
    eprintln!("   proof received: a={}…", &proof_json["a"].as_str().unwrap()[..16]);

    eprintln!("── 4. verify remote proof against LOCAL pk.vk, public=[cmt, 0]");
    let pk = load_pk("../../circuits/keys/order_commitment.pk.bin").expect("load local pk");
    let pvk = prepare_verifying_key(&pk.vk);
    let proof = ark_groth16::Proof::<Bn254> {
        a: parse_g1(proof_json["a"].as_str().unwrap()),
        b: parse_g2(proof_json["b"].as_str().unwrap()),
        c: parse_g1(proof_json["c"].as_str().unwrap()),
    };
    let ok = Groth16::<Bn254>::verify_proof(&pvk, &proof, &[local_cmt_fr, Fr::ZERO]).unwrap();
    assert!(
        ok,
        "REMOTE PROOF DOES NOT VERIFY against local VK — the tee-match binary or \
         PK in the GCP image differs from local"
    );
    eprintln!("   ✓ remote proof VERIFIES against local VK — proof generation is correct");
    eprintln!();
    eprintln!("CONCLUSION: TEE output is valid. The on-chain failure is in the");
    eprintln!("place_order call itself (e.g. commitment already exists, VK on-chain");
    eprintln!("differs from local, or arg encoding) — not in the TEE proof.");
}

/// Check whether the default-secrets commitment (nonce=111 secret=222) is
/// already stored on the deployed orderbook.
#[test]
#[ignore]
fn default_commitment_already_on_chain() {
    let ob_id = std::env::var("ORDERBOOK_ID")
        .unwrap_or_else(|_| "CCPCKKU2XRVDWK7KQROWQZ62AQ2RESWURKYWIILIBEBRQSXKLU2U5NGV".to_string());
    let cmt = "1a4a22b5fb0b651a7b5f15e1e8188b9a67b4087c5b051bb1346b45253d262853";
    let rpc = e2e::soroban_rpc::SorobanRpc::new();
    let out = rpc
        .invoke_view_xdr(&ob_id, "e2e", "order_meta", vec![
            e2e::soroban_rpc::scval_bytes32(cmt).unwrap(),
        ])
        .expect("order_meta simulation");
    eprintln!("order_meta({}) = {}", &cmt[..16], out);
}

/// Place a fresh, locally-verified GCP proof on the deployed orderbook.
/// If this fails with an invalid-proof trap, the on-chain VK is stale.
#[test]
#[ignore]
fn gcp_proof_places_on_deployed_orderbook() {
    let addr = std::env::var("TEE_ADDR").unwrap_or_else(|_| "35.255.76.255:9720".to_string());
    let ob_id = std::env::var("ORDERBOOK_ID")
        .unwrap_or_else(|_| "CCPCKKU2XRVDWK7KQROWQZ62AQ2RESWURKYWIILIBEBRQSXKLU2U5NGV".to_string());
    let nonce: u64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let secret: u64 = nonce.wrapping_mul(0xD1B54A32D192ED03);

    eprintln!("── 1. init + commit-proof on {addr}");
    let resp = send(&addr, &serde_json::json!({
        "cmd": "init",
        "side": SIDE, "price": PRICE, "size": SIZE,
        "leverage": LEVERAGE, "asset": ASSET,
        "nonce": nonce, "secret": secret,
    }));
    assert_eq!(resp["ok"], true, "init failed: {resp}");
    let cmt = resp["commitment"].as_str().unwrap().to_string();

    let resp = send(&addr, &serde_json::json!({ "cmd": "commit-proof", "cmt": cmt }));
    assert_eq!(resp["ok"], true, "commit-proof failed: {resp}");
    let proof_str = resp["proof"].as_str().unwrap().to_string();

    // Sanity: verify locally first so a failure below can only be on-chain state
    let proof_json: serde_json::Value = serde_json::from_str(&proof_str).unwrap();
    let pk = load_pk("../../circuits/keys/order_commitment.pk.bin").expect("load local pk");
    let pvk = prepare_verifying_key(&pk.vk);
    let proof = ark_groth16::Proof::<Bn254> {
        a: parse_g1(proof_json["a"].as_str().unwrap()),
        b: parse_g2(proof_json["b"].as_str().unwrap()),
        c: parse_g1(proof_json["c"].as_str().unwrap()),
    };
    let cmt_fr = Fr::from_str(&BigUint::parse_bytes(cmt.as_bytes(), 16).unwrap().to_string()).unwrap();
    assert!(Groth16::<Bn254>::verify_proof(&pvk, &proof, &[cmt_fr, Fr::ZERO]).unwrap());
    eprintln!("   ✓ proof verified locally, cmt={}", &cmt[..16]);

    eprintln!("── 2. place_order on {ob_id}");
    let zeros = "0000000000000000000000000000000000000000000000000000000000000000";
    match e2e::stellar::ob_place_order(
        &ob_id, "e2e", &cmt, PRICE, SIDE, SIZE, LEVERAGE, 15, zeros, &proof_str,
    ) {
        Ok(()) => eprintln!("   ✓ place_order SUCCEEDED — on-chain VK matches; the e2e server flow bug is elsewhere"),
        Err(e) => panic!(
            "place_order FAILED with a locally-verified proof and fresh commitment.\n\
             The deployed orderbook's embedded VK does not match the local keys.\n\
             Error: {e}"
        ),
    }
}
