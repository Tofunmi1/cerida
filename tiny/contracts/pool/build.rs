use std::{env, fmt::Write as _, fs, path::PathBuf};

use ark_bn254::{g1::G1Affine, g2::G2Affine};
use ark_ff::{BigInteger, PrimeField};
use num_bigint::BigUint;
use serde_json::Value;

fn main() {
    println!("cargo:rerun-if-env-changed=SP1_VK_JSON");
    println!("cargo:rerun-if-env-changed=SP1_VKEY_HASH");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));

    let vk_path = env::var("SP1_VK_JSON").unwrap_or_default();
    let vkey_hash_hex = env::var("SP1_VKEY_HASH").unwrap_or_default();

    if vk_path.is_empty() || vkey_hash_hex.is_empty() {
        write_placeholder_vk(&out_dir);
        return;
    }

    let path = PathBuf::from(&vk_path);
    println!("cargo:rerun-if-changed={}", path.display());
    let json = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read SP1_VK_JSON `{}`: {e}", path.display()));

    let vkey_hash_bytes = hex::decode(vkey_hash_hex.trim_start_matches("0x"))
        .unwrap_or_else(|e| panic!("invalid SP1_VKEY_HASH hex: {e}"));
    assert_eq!(vkey_hash_bytes.len(), 32, "SP1_VKEY_HASH must be 32 bytes (64 hex chars)");

    let mut vkey_hash_arr = [0u8; 32];
    vkey_hash_arr.copy_from_slice(&vkey_hash_bytes);

    let content = vk_rs_from_json(&json, &vkey_hash_arr);
    fs::write(out_dir.join("sp1_vk.rs"), content).expect("failed to write sp1_vk.rs");
}

fn write_placeholder_vk(out_dir: &PathBuf) {
    let content = "\
pub const SP1_VK_ALPHA_G1: [u8; 64] = [0u8; 64];
pub const SP1_VK_BETA_G2: [u8; 128] = [0u8; 128];
pub const SP1_VK_GAMMA_G2: [u8; 128] = [0u8; 128];
pub const SP1_VK_DELTA_G2: [u8; 128] = [0u8; 128];
pub const SP1_VK_IC: [[u8; 64]; 3] = [[0u8; 64]; 3];
pub const SP1_VKEY_HASH: [u8; 32] = [0u8; 32];
";
    fs::write(out_dir.join("sp1_vk.rs"), content).expect("failed to write placeholder sp1_vk.rs");
}

fn bigint_to_be_32<B: BigInteger>(value: B) -> [u8; 32] {
    let bytes = value.to_bytes_be();
    let mut out = [0u8; 32];
    let start = 32usize.saturating_sub(bytes.len());
    out[start..].copy_from_slice(&bytes[..bytes.len().min(32)]);
    out
}

fn g1_to_soroban_bytes(p: &G1Affine) -> [u8; 64] {
    let mut out = [0u8; 64];
    out[..32].copy_from_slice(&bigint_to_be_32(p.x.into_bigint()));
    out[32..].copy_from_slice(&bigint_to_be_32(p.y.into_bigint()));
    out
}

fn g2_to_soroban_bytes(p: &G2Affine) -> [u8; 128] {
    let mut out = [0u8; 128];
    out[..32].copy_from_slice(&bigint_to_be_32(p.x.c1.into_bigint()));
    out[32..64].copy_from_slice(&bigint_to_be_32(p.x.c0.into_bigint()));
    out[64..96].copy_from_slice(&bigint_to_be_32(p.y.c1.into_bigint()));
    out[96..].copy_from_slice(&bigint_to_be_32(p.y.c0.into_bigint()));
    out
}

fn parse_fq_decimal(value: &str) -> ark_bn254::Fq {
    let bigint = BigUint::parse_bytes(value.as_bytes(), 10)
        .unwrap_or_else(|| panic!("invalid decimal field element: {value}"));
    ark_bn254::Fq::from_be_bytes_mod_order(&bigint.to_bytes_be())
}

fn fq2_from_decimals(c0: &str, c1: &str) -> ark_bn254::Fq2 {
    ark_bn254::Fq2::new(parse_fq_decimal(c0), parse_fq_decimal(c1))
}

fn g1_bytes(pt: &Value) -> [u8; 64] {
    let arr = pt.as_array().expect("G1 point must be a JSON array");
    let x = parse_fq_decimal(arr[0].as_str().expect("G1.x must be a string"));
    let y = parse_fq_decimal(arr[1].as_str().expect("G1.y must be a string"));
    g1_to_soroban_bytes(&G1Affine::new_unchecked(x, y))
}

fn g2_bytes(pt: &Value) -> [u8; 128] {
    let arr = pt.as_array().expect("G2 point must be a JSON array");
    let x = arr[0].as_array().expect("G2.x must be a JSON array");
    let y = arr[1].as_array().expect("G2.y must be a JSON array");
    let xf = fq2_from_decimals(
        x[0].as_str().expect("G2.x.c0"),
        x[1].as_str().expect("G2.x.c1"),
    );
    let yf = fq2_from_decimals(
        y[0].as_str().expect("G2.y.c0"),
        y[1].as_str().expect("G2.y.c1"),
    );
    g2_to_soroban_bytes(&G2Affine::new_unchecked(xf, yf))
}

fn fmt_bytes(bytes: &[u8]) -> String {
    let mut s = String::from("[");
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 { s.push(','); }
        write!(s, "0x{b:02x}").unwrap();
    }
    s.push(']');
    s
}

fn vk_rs_from_json(json: &str, vkey_hash: &[u8; 32]) -> String {
    let v: Value = serde_json::from_str(json).expect("SP1_VK_JSON is not valid JSON");

    let alpha = g1_bytes(&v["vk_alpha_1"]);
    let beta = g2_bytes(&v["vk_beta_2"]);
    let gamma = g2_bytes(&v["vk_gamma_2"]);
    let delta = g2_bytes(&v["vk_delta_2"]);

    let ic_arr = v["IC"].as_array().expect("IC must be a JSON array");
    assert_eq!(ic_arr.len(), 3, "SP1 Groth16 VK must have exactly 3 IC points");
    let ic_items: Vec<String> = ic_arr.iter().map(|pt| fmt_bytes(&g1_bytes(pt))).collect();

    let mut out = String::new();
    writeln!(out, "pub const SP1_VK_ALPHA_G1: [u8; 64] = {};", fmt_bytes(&alpha)).unwrap();
    writeln!(out, "pub const SP1_VK_BETA_G2: [u8; 128] = {};", fmt_bytes(&beta)).unwrap();
    writeln!(out, "pub const SP1_VK_GAMMA_G2: [u8; 128] = {};", fmt_bytes(&gamma)).unwrap();
    writeln!(out, "pub const SP1_VK_DELTA_G2: [u8; 128] = {};", fmt_bytes(&delta)).unwrap();
    writeln!(out, "pub const SP1_VK_IC: [[u8; 64]; 3] = [{}];", ic_items.join(",")).unwrap();
    writeln!(out, "pub const SP1_VKEY_HASH: [u8; 32] = {};", fmt_bytes(vkey_hash)).unwrap();
    out
}
