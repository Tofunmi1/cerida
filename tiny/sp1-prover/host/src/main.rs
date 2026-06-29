use anyhow::Result;
use clap::Parser;
use sha2::{Digest, Sha256};
use sp1_sdk::blocking::{ProveRequest, Prover, ProverClient};
use sp1_sdk::{HashableKey, ProvingKey, SP1Stdin};

const ELF: &[u8] = include_bytes!("../../elf/private-payment-sp1");

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

    /// Only print the vkey hash (hex) and exit (for embedding in contracts).
    #[arg(long, default_value_t = false)]
    vkey: bool,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct GuestInput {
    amount: u64,
    secret: u64,
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

fn proof_bytes_to_hex(bytes: &[u8]) -> (String, String, String) {
    if bytes.len() >= 256 {
        (
            hex::encode(&bytes[0..64]),
            hex::encode(&bytes[64..192]),
            hex::encode(&bytes[192..256]),
        )
    } else {
        ("00".repeat(64), "00".repeat(128), "00".repeat(64))
    }
}

fn build_output(vkey_hash_hex: String, proof_bytes: &[u8], amount: u64, secret: u64) -> ProofOutput {
    let (commitment, nullifier) = compute_commitment_nullifier(amount, secret);
    let committed_bytes: Vec<u8> = [commitment.as_slice(), nullifier.as_slice()].concat();
    let digest_bytes: [u8; 32] = Sha256::digest(&committed_bytes).into();
    let (a, b, c) = proof_bytes_to_hex(proof_bytes);
    ProofOutput {
        proof: ProofHex { a, b, c },
        public_inputs: [vkey_hash_hex.clone(), hex::encode(digest_bytes)],
        commitment: hex::encode(commitment),
        nullifier: hex::encode(nullifier),
        vkey_hash: vkey_hash_hex,
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    sp1_sdk::utils::setup_logger();

    if !args.real {
        // Use mock prover so setup() is instant without real SP1 toolchain.
        std::env::set_var("SP1_PROVER", "mock");
    }

    let client = ProverClient::from_env();
    let pk = client.setup(ELF.into())?;
    let vkey_hash_hex = hex::encode(pk.verifying_key().bytes32());

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
        vec![0u8; 256]
    };

    let output = build_output(vkey_hash_hex, &proof_bytes, args.amount, args.secret);
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
