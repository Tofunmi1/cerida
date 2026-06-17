mod proof;
mod stellar;

use anyhow::Result;
use std::path::PathBuf;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage:");
        eprintln!("  e2e proof-gen --amount <N> --secret <N>");
        eprintln!("  e2e full --amount <N> --secret <N>");
        std::process::exit(1);
    }

    let mode = &args[1];
    let tiny_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("e2e crate must be inside tiny/")
        .to_path_buf();

    let amount: u64 = parse_opt(&args, "--amount")
        .or_else(|| args.get(2).and_then(|s| s.parse().ok()))
        .expect("Missing --amount");
    let secret: u64 = parse_opt(&args, "--secret")
        .or_else(|| args.get(3).and_then(|s| s.parse().ok()))
        .expect("Missing --secret");

    let circuit_keys = tiny_root.join("circuit-keys");
    let wasm = circuit_keys.join("main_js").join("main.wasm");
    let r1cs = circuit_keys.join("main.r1cs");
    let zkey = circuit_keys.join("main.zkey");

    if !wasm.exists() || !r1cs.exists() || !zkey.exists() {
        anyhow::bail!(
            "Circuit artifacts not found at {}.\nRun: make circuit setup",
            circuit_keys.display()
        );
    }

    eprintln!("Generating proof for amount={amount}, secret={secret}...");
    let p = proof::generate_proof(&wasm, &r1cs, &zkey, amount, secret)?;
    let cli_json = proof::proof_to_cli_json(&p);

    match mode.as_str() {
        "proof-gen" => {
            println!("{}", serde_json::to_string_pretty(&cli_json)?);
        }
        "full" => {
            let target = tiny_root.join("target/wasm32v1-none/release");
            stellar::run_full_e2e(&tiny_root, &target, &cli_json)?;
        }
        _ => {
            anyhow::bail!("Unknown mode: {mode}. Use proof-gen or full");
        }
    }

    Ok(())
}

fn parse_opt(args: &[String], name: &str) -> Option<u64> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
}
