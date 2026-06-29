use std::{env, fmt::Write as _, fs, path::PathBuf};

use serde_json::Value;

fn main() {
    println!("cargo:rerun-if-env-changed=SP1_VK_JSON");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));

    let vk_path = env::var("SP1_VK_JSON").unwrap_or_default();

    if vk_path.is_empty() {
        write_placeholder_vk(&out_dir);
        return;
    }

    let path = PathBuf::from(&vk_path);
    println!("cargo:rerun-if-changed={}", path.display());
    let json = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read SP1_VK_JSON `{}`: {e}", path.display()));

    let content = vk_rs_from_json(&json);
    fs::write(out_dir.join("sp1_vk.rs"), content).expect("failed to write sp1_vk.rs");
}

fn write_placeholder_vk(out_dir: &PathBuf) {
    let content = "\
pub const SP1_VK_ALPHA_G1: [u8; 64] = [0u8; 64];
pub const SP1_VK_BETA_G2: [u8; 128] = [0u8; 128];
pub const SP1_VK_GAMMA_G2: [u8; 128] = [0u8; 128];
pub const SP1_VK_DELTA_G2: [u8; 128] = [0u8; 128];
pub const SP1_VK_IC: [[u8; 64]; 6] = [[0u8; 64]; 6];
pub const SP1_VKEY_HASH: [u8; 32] = [0u8; 32];
pub const SP1_VK_ROOT: [u8; 32] = [0u8; 32];
";
    fs::write(out_dir.join("sp1_vk.rs"), content).expect("failed to write placeholder sp1_vk.rs");
}

fn decode_hex_field<const N: usize>(v: &Value, field: &str) -> [u8; N] {
    let s = v[field].as_str().unwrap_or_else(|| panic!("missing or non-string field: {field}"));
    let bytes = hex::decode(s).unwrap_or_else(|e| panic!("invalid hex in `{field}`: {e}"));
    assert_eq!(
        bytes.len(), N,
        "field `{field}` must be {} bytes, got {}",
        N, bytes.len()
    );
    let mut arr = [0u8; N];
    arr.copy_from_slice(&bytes);
    arr
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

fn vk_rs_from_json(json: &str) -> String {
    let v: Value = serde_json::from_str(json).expect("SP1_VK_JSON is not valid JSON");

    let vkey_hash = decode_hex_field::<32>(&v, "vkey_hash");
    let alpha = decode_hex_field::<64>(&v, "alpha_g1");
    let beta = decode_hex_field::<128>(&v, "beta_g2");
    let gamma = decode_hex_field::<128>(&v, "gamma_g2");
    let delta = decode_hex_field::<128>(&v, "delta_g2");
    let vk_root = decode_hex_field::<32>(&v, "vk_root");

    let ic_arr = v["ic"].as_array().expect("ic must be a JSON array");
    assert_eq!(ic_arr.len(), 6, "SP1 Groth16 VK must have exactly 6 IC points, got {}", ic_arr.len());
    let ic_items: Vec<String> = ic_arr.iter().enumerate().map(|(i, pt)| {
        let s = pt.as_str().unwrap_or_else(|| panic!("ic[{i}] must be a string"));
        let bytes = hex::decode(s).unwrap_or_else(|e| panic!("invalid hex in ic[{i}]: {e}"));
        assert_eq!(bytes.len(), 64, "ic[{i}] must be 64 bytes, got {}", bytes.len());
        fmt_bytes(&bytes)
    }).collect();

    let mut out = String::new();
    writeln!(out, "pub const SP1_VK_ALPHA_G1: [u8; 64] = {};", fmt_bytes(&alpha)).unwrap();
    writeln!(out, "pub const SP1_VK_BETA_G2: [u8; 128] = {};", fmt_bytes(&beta)).unwrap();
    writeln!(out, "pub const SP1_VK_GAMMA_G2: [u8; 128] = {};", fmt_bytes(&gamma)).unwrap();
    writeln!(out, "pub const SP1_VK_DELTA_G2: [u8; 128] = {};", fmt_bytes(&delta)).unwrap();
    writeln!(out, "pub const SP1_VK_IC: [[u8; 64]; 6] = [{}];", ic_items.join(",")).unwrap();
    writeln!(out, "pub const SP1_VKEY_HASH: [u8; 32] = {};", fmt_bytes(&vkey_hash)).unwrap();
    writeln!(out, "pub const SP1_VK_ROOT: [u8; 32] = {};", fmt_bytes(&vk_root)).unwrap();
    out
}
