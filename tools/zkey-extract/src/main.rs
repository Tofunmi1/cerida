use std::path::PathBuf;

use ark_bn254::Bn254;
use ark_circom::read_zkey;
use ark_groth16::ProvingKey;
use ark_serialize::CanonicalSerialize;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: zkey-extract <input.zkey> <output_prefix>");
        eprintln!("  Writes <output_prefix>.pk.bin and <output_prefix>.vk.bin");
        std::process::exit(1);
    }

    let zkey_path = PathBuf::from(&args[1]);
    let prefix = &args[2];

    eprintln!("Reading zkey: {}", zkey_path.display());
    let zkey_file = std::fs::File::open(&zkey_path)?;
    let mut reader = std::io::BufReader::new(zkey_file);
    let (pk, _matrices): (ProvingKey<Bn254>, _) = read_zkey(&mut reader)?;

    // Serialize proving key
    let pk_path = format!("{}.pk.bin", prefix);
    eprintln!("Writing proving key: {}", pk_path);
    let mut pk_bytes = Vec::new();
    pk.serialize_compressed(&mut pk_bytes)?;
    std::fs::write(&pk_path, &pk_bytes)?;

    // Serialize verifying key
    let vk_path = format!("{}.vk.bin", prefix);
    eprintln!("Writing verifying key: {}", vk_path);
    let mut vk_bytes = Vec::new();
    pk.vk.serialize_compressed(&mut vk_bytes)?;
    std::fs::write(&vk_path, &vk_bytes)?;

    eprintln!("Done.");
    Ok(())
}
