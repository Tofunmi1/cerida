use anyhow::Result;
use ark_bn254::{G1Affine, G2Affine};
use ark_ff::PrimeField;
use clap::Parser;
use sha2::{Digest, Sha256};
use sp1_sdk::blocking::{ProveRequest, Prover, ProverClient};
use sp1_sdk::{HashableKey, ProvingKey, SP1Stdin};
use sp1_verifier::{
    load_ark_groth16_verifying_key_from_bytes, load_ark_proof_from_bytes,
    GROTH16_VK_BYTES, VK_ROOT_BYTES,
};

const ELF: &[u8] = include_bytes!("../../elf/private-payment-sp1");

// Proof byte layout (from proof.bytes()):
//   [0..4]   : 4-byte prefix (first 4 bytes of SHA256(GROTH16_VK_BYTES))
//   [4..36]  : exit_code (32 bytes, 0 for success)
//   [36..68] : vk_root (32 bytes)
//   [68..100]: proof_nonce (32 bytes, 0 for local proofs)
//   [100..356]: raw gnark proof = A(64) + B(128) + C(64)
const PROOF_PREFIX_LEN: usize = 4 + 32 + 32 + 32; // 100

#[derive(Parser)]
#[command(about = "SP1 prover for private payment commitments")]
struct Args {
    #[arg(long, default_value_t = 0)]
    amount: u64,

    #[arg(long, default_value_t = 0)]
    secret: u64,

    /// Use real Groth16 prover (slow, ~90s). Default: mock proof (instant).
    #[arg(long, default_value_t = false)]
    real: bool,

    /// Print vkey hash (hex, no 0x) and exit.
    #[arg(long, default_value_t = false)]
    vkey: bool,

    /// Export the SP1 Groth16 circuit VK + vk_root as JSON (for build.rs).
    #[arg(long, default_value_t = false)]
    export_vk: bool,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct GuestInput {
    amount: u64,
    secret: u64,
}

/// VK JSON format consumed by pool contract's build.rs.
/// All byte arrays are hex-encoded, already in Soroban's BN254 byte layout.
#[derive(serde::Serialize)]
struct VkJson {
    /// SP1 program verifying key hash (32 bytes → 64 hex, no 0x prefix)
    vkey_hash: String,
    /// G1 alpha: x_be32 || y_be32 (64 bytes → 128 hex)
    alpha_g1: String,
    /// G2 beta: x.c1_be32 || x.c0_be32 || y.c1_be32 || y.c0_be32 (128 bytes → 256 hex)
    beta_g2: String,
    gamma_g2: String,
    delta_g2: String,
    /// 6 IC points, each in G1 Soroban format (64 bytes → 128 hex)
    ic: Vec<String>,
    /// VK merkle root constant for SP1 v6.x (32 bytes → 64 hex)
    vk_root: String,
}

#[derive(serde::Serialize)]
struct ProofOutput {
    proof: ProofHex,
    /// [vkey_hash_hex, committed_values_digest_hex]
    public_inputs: [String; 2],
    commitment: String,
    nullifier: String,
    vkey_hash: String,
}

#[derive(serde::Serialize)]
struct ProofHex {
    a: String,
    b: String,
    c: String,
}

fn bigint_to_be_32(v: impl ark_ff::biginteger::BigInteger) -> [u8; 32] {
    let bytes = v.to_bytes_be();
    let mut out = [0u8; 32];
    let start = 32usize.saturating_sub(bytes.len());
    out[start..].copy_from_slice(&bytes[..bytes.len().min(32)]);
    out
}

fn g1_to_soroban(p: &G1Affine) -> [u8; 64] {
    let mut out = [0u8; 64];
    out[..32].copy_from_slice(&bigint_to_be_32(p.x.into_bigint()));
    out[32..].copy_from_slice(&bigint_to_be_32(p.y.into_bigint()));
    out
}

fn g2_to_soroban(p: &G2Affine) -> [u8; 128] {
    let mut out = [0u8; 128];
    out[..32].copy_from_slice(&bigint_to_be_32(p.x.c1.into_bigint()));
    out[32..64].copy_from_slice(&bigint_to_be_32(p.x.c0.into_bigint()));
    out[64..96].copy_from_slice(&bigint_to_be_32(p.y.c1.into_bigint()));
    out[96..].copy_from_slice(&bigint_to_be_32(p.y.c0.into_bigint()));
    out
}

fn export_vk(vkey_hash: String) -> Result<VkJson> {
    let vk = load_ark_groth16_verifying_key_from_bytes(&GROTH16_VK_BYTES)
        .map_err(|e| anyhow::anyhow!("Failed to parse Groth16 VK: {e:?}"))?;

    let ic: Vec<String> = vk.gamma_abc_g1.iter().map(|p| hex::encode(g1_to_soroban(p))).collect();
    assert_eq!(ic.len(), 6, "SP1 v6 Groth16 VK must have exactly 6 IC points (got {})", ic.len());

    Ok(VkJson {
        vkey_hash,
        alpha_g1: hex::encode(g1_to_soroban(&vk.alpha_g1)),
        beta_g2: hex::encode(g2_to_soroban(&vk.beta_g2)),
        gamma_g2: hex::encode(g2_to_soroban(&vk.gamma_g2)),
        delta_g2: hex::encode(g2_to_soroban(&vk.delta_g2)),
        ic,
        vk_root: hex::encode(*VK_ROOT_BYTES),
    })
}

fn compute_commitment_nullifier(amount: u64, secret: u64) -> ([u8; 32], [u8; 32]) {
    let commitment: [u8; 32] = Sha256::new()
        .chain_update(amount.to_be_bytes())
        .chain_update(secret.to_be_bytes())
        .chain_update([1u8])
        .finalize()
        .into();
    let nullifier: [u8; 32] = Sha256::new()
        .chain_update(commitment)
        .chain_update(secret.to_be_bytes())
        .chain_update([2u8])
        .finalize()
        .into();
    (commitment, nullifier)
}

/// committed_values_digest = SHA256(commitment || nullifier) with top 3 bits masked.
/// Matches SP1's SP1PublicValues::hash_bn254() which does:
///   sha256(bincode_serialize(PublicOutput)) & 0x1fffffff...
/// bincode for [u8;32],[u8;32] = raw 64 bytes.
fn committed_values_digest(commitment: &[u8; 32], nullifier: &[u8; 32]) -> [u8; 32] {
    let mut hash: [u8; 32] = Sha256::new()
        .chain_update(commitment)
        .chain_update(nullifier)
        .finalize()
        .into();
    hash[0] &= 0b00011111; // mask top 3 bits (matches SP1's hash_bn254())
    hash
}

/// Parse the Gnark raw proof (256 bytes) into Soroban-format hex strings.
/// Gnark decompressed G1: x_le32 || y_le32 → Soroban: x_be32 || y_be32
/// Gnark decompressed G2: (x.a1_le32||x.a0_le32)||(y.a1_le32||y.a0_le32) → Soroban: (x.c1_be32||x.c0_be32)||(y.c1_be32||y.c0_be32)
/// Use sp1-verifier's ark converter for correctness.
fn gnark_proof_to_soroban(raw: &[u8; 256]) -> Result<(String, String, String)> {
    let proof = load_ark_proof_from_bytes(raw)
        .map_err(|e| anyhow::anyhow!("Failed to parse Gnark proof: {e:?}"))?;
    Ok((
        hex::encode(g1_to_soroban(&proof.a)),
        hex::encode(g2_to_soroban(&proof.b)),
        hex::encode(g1_to_soroban(&proof.c)),
    ))
}

fn build_output(vkey_hash_hex: &str, proof_bytes: &[u8], amount: u64, secret: u64) -> Result<ProofOutput> {
    let (commitment, nullifier) = compute_commitment_nullifier(amount, secret);
    let digest = committed_values_digest(&commitment, &nullifier);

    let (a, b, c) = if proof_bytes.len() >= PROOF_PREFIX_LEN + 256 {
        let raw: &[u8; 256] = proof_bytes[PROOF_PREFIX_LEN..PROOF_PREFIX_LEN + 256]
            .try_into().unwrap();
        gnark_proof_to_soroban(raw)?
    } else {
        // Mock: zero-filled
        ("00".repeat(64), "00".repeat(128), "00".repeat(64))
    };

    Ok(ProofOutput {
        proof: ProofHex { a, b, c },
        public_inputs: [vkey_hash_hex.to_string(), hex::encode(digest)],
        commitment: hex::encode(commitment),
        nullifier: hex::encode(nullifier),
        vkey_hash: vkey_hash_hex.to_string(),
    })
}

fn main() -> Result<()> {
    let args = Args::parse();
    sp1_sdk::utils::setup_logger();

    // Always set up prover (mock for export-vk and non-real; real otherwise)
    if !args.real {
        std::env::set_var("SP1_PROVER", "mock");
    }
    let client = ProverClient::from_env();
    let pk = client.setup(ELF.into())?;
    // bytes32() returns "0x<64 hex chars>"; strip the prefix for our hex format
    let vkey_hash_hex = pk.verifying_key().bytes32().trim_start_matches("0x").to_string();

    if args.export_vk {
        let vk = export_vk(vkey_hash_hex)?;
        println!("{}", serde_json::to_string_pretty(&vk)?);
        return Ok(());
    }

    if args.vkey {
        println!("{vkey_hash_hex}");
        return Ok(());
    }

    let proof_bytes = if args.real {
        let mut stdin = SP1Stdin::new();
        stdin.write(&GuestInput { amount: args.amount, secret: args.secret });
        eprintln!("Generating SP1 Groth16 proof (~90s)...");
        let proof = client.prove(&pk, stdin).groth16().run()?;
        proof.bytes()
    } else {
        vec![] // empty = mock, build_output falls through to zero-filled
    };

    let output = build_output(&vkey_hash_hex, &proof_bytes, args.amount, args.secret)?;
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
