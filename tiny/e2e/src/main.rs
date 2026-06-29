mod proof;
mod stellar;

use anyhow::Result;
use std::path::PathBuf;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }

    let mode = &args[1];
    let tiny_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("e2e crate must be inside tiny/")
        .to_path_buf();

    match mode.as_str() {
        // ── Circom path ───────────────────────────────────────────────────────
        "proof-gen" | "full" => {
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

            eprintln!("Generating Circom proof for amount={amount}, secret={secret}...");
            let p = proof::generate_proof(&wasm, &r1cs, &zkey, amount, secret)?;
            let cli_json = proof::proof_to_cli_json(&p);

            if mode == "proof-gen" {
                println!("{}", serde_json::to_string_pretty(&cli_json)?);
            } else {
                let target = tiny_root.join("target/wasm32v1-none/release");
                stellar::run_full_e2e(&tiny_root, &target, &cli_json)?;
            }
        }

        // ── SP1 path ──────────────────────────────────────────────────────────
        "sp1-gen" | "sp1" => {
            let amount: u64 = parse_opt(&args, "--amount").expect("Missing --amount");
            let secret: u64 = parse_opt(&args, "--secret").expect("Missing --secret");
            let real = args.iter().any(|a| a == "--real");

            let sp1_proof = if real {
                // Run sp1-host binary for real Groth16 proof (~90s)
                let sp1_host = find_sp1_host(&tiny_root);
                eprintln!("Running sp1-host for real Groth16 proof (~90s)...");
                proof::sp1_run_host(&sp1_host, amount, secret, true)?
            } else {
                // Instant mock proof (placeholder VK mode — development only)
                eprintln!("Generating mock SP1 proof (placeholder VK mode)...");
                proof::sp1_mock_proof(amount, secret)
            };

            if mode == "sp1-gen" {
                println!("{}", serde_json::to_string_pretty(&sp1_proof)?);
            } else {
                let target = tiny_root.join("target/wasm32v1-none/release");
                stellar::run_sp1_e2e(&tiny_root, &target, &sp1_proof)?;
            }
        }

        _ => {
            eprintln!("Unknown mode: {mode}");
            print_usage();
            std::process::exit(1);
        }
    }

    Ok(())
}

fn print_usage() {
    eprintln!("Usage:");
    eprintln!("  Circom path:");
    eprintln!("    e2e proof-gen --amount <N> --secret <N>");
    eprintln!("    e2e full      --amount <N> --secret <N>");
    eprintln!("  SP1 path:");
    eprintln!("    e2e sp1-gen   --amount <N> --secret <N> [--real]");
    eprintln!("    e2e sp1       --amount <N> --secret <N> [--real]");
    eprintln!();
    eprintln!("  --real: use real SP1 Groth16 prover (~90s). Default: mock (instant).");
}

fn parse_opt(args: &[String], name: &str) -> Option<u64> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
}

fn find_sp1_host(tiny_root: &PathBuf) -> PathBuf {
    // Look for pre-built sp1-host binary
    let candidates = [
        tiny_root.join("sp1-prover/target/release/sp1-host"),
        tiny_root.join("sp1-prover/target/debug/sp1-host"),
    ];
    for c in &candidates {
        if c.exists() { return c.clone(); }
    }
    // Fall back to `cargo run` if binary not found
    panic!(
        "sp1-host binary not found. Build it first:\n  cd {}/sp1-prover && cargo build --release",
        tiny_root.display()
    );
}
