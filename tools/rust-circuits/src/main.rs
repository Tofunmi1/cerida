use std::path::PathBuf;

use anyhow::{Context, Result};
use ark_bn254::Fr;
use ark_ff::{BigInteger, PrimeField};
use ark_serialize::CanonicalSerialize;
use clap::{Parser, Subcommand};
use rust_circuits::circuits::shielded_insert::TREE_DEPTH;
use rust_circuits::{
    compute_empty_root, compute_leaf_hash, compute_merkle_path, compute_new_root,
    compute_pool_nullifier_hash, compute_pool_zeros, compute_root_from_leaves, load_pk,
    prove_cancel, prove_commitment, prove_match, prove_note_spend, prove_shielded_insert,
    prove_shielded_withdraw, setup_pool, vk_to_json,
};

#[derive(Parser)]
#[command(name = "rust-prover")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    OrderCommitment {
        #[arg(long)]
        side: u64,
        #[arg(long)]
        price: u64,
        #[arg(long)]
        size: u64,
        #[arg(long, default_value = "1")]
        leverage: u64,
        #[arg(long, default_value = "0")]
        asset_id: u64,
        #[arg(long)]
        nonce: u64,
        #[arg(long, default_value = "false")]
        cross_margin: bool,
        #[arg(long)]
        secret: u64,
    },
    OrderCancel {
        #[arg(long)]
        commitment: String,
        #[arg(long)]
        secret: u64,
    },
    /// Run setup for all circuits, saving pk.bin and vk.json
    Setup {
        #[arg(long, default_value = "circuits/keys")]
        out_dir: PathBuf,
    },
    /// Run setup for the shielded pool circuits (independent trusted setup)
    SetupPool {
        #[arg(long, default_value = "circuits/keys")]
        out_dir: PathBuf,
    },
    /// Print the 20 Merkle zero subtree hashes as hex (for embedding in the contract)
    PoolZeros,
    /// Generate an insert proof for a shielded pool deposit
    ShieldedInsert {
        /// The commitment to insert (hex string)
        #[arg(long)]
        commitment: String,
        /// The leaf index to insert at
        #[arg(long)]
        leaf_index: u64,
        /// Current leaves in the tree as comma-separated decimal Fr values (empty = blank tree)
        #[arg(long, default_value = "")]
        leaves: String,
        /// Path to the insert proving key
        #[arg(long, default_value = "circuits/keys/shielded_insert.pk.bin")]
        pk: PathBuf,
    },
    /// Generate a withdrawal proof
    ShieldedWithdraw {
        /// secret (decimal)
        #[arg(long)]
        secret: u64,
        /// nullifier (decimal)
        #[arg(long)]
        nullifier: u64,
        /// recipient Fr value as hex (sha256 of recipient address bytes)
        #[arg(long)]
        recipient: String,
        /// All leaves in the tree as comma-separated decimal Fr values
        #[arg(long)]
        leaves: String,
        /// Path to the withdraw proving key
        #[arg(long, default_value = "circuits/keys/shielded_withdraw.pk.bin")]
        pk: PathBuf,
    },
    NoteSpend {
        #[arg(long)]
        amount: u64,
        #[arg(long)]
        secret: u64,
    },
    OrderMatch {
        #[arg(long)]
        side_a: u64,
        #[arg(long)]
        price_a: u64,
        #[arg(long)]
        size_a: u64,
        #[arg(long, default_value = "1")]
        leverage_a: u64,
        #[arg(long, default_value = "0")]
        asset_id_a: u64,
        #[arg(long)]
        nonce_a: u64,
        #[arg(long)]
        secret_a: u64,
        #[arg(long)]
        side_b: u64,
        #[arg(long)]
        price_b: u64,
        #[arg(long)]
        size_b: u64,
        #[arg(long, default_value = "1")]
        leverage_b: u64,
        #[arg(long, default_value = "0")]
        asset_id_b: u64,
        #[arg(long)]
        nonce_b: u64,
        #[arg(long)]
        secret_b: u64,
        #[arg(long)]
        match_price: u64,
        #[arg(long)]
        match_size: u64,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Setup { out_dir } => {
            use rust_circuits::setup_all;
            std::fs::create_dir_all(&out_dir)?;
            let mut rng = rand::thread_rng();

            let results = setup_all(&mut rng)?;
            let names = [
                "order_commitment",
                "order_cancel",
                "order_match",
                "note_spend",
            ];
            for (name, (pk, vk)) in names.iter().zip(results.iter()) {
                eprintln!("Setting up {}…", name);
                let pk_path = out_dir.join(format!("{}.pk.bin", name));
                let mut pk_bytes = Vec::new();
                pk.serialize_compressed(&mut pk_bytes)?;
                std::fs::write(&pk_path, &pk_bytes)
                    .with_context(|| format!("Writing {pk_path:?}"))?;
                eprintln!("  pk: {pk_path:?} ({} bytes)", pk_bytes.len());
                let vk_json = vk_to_json(vk);
                let vk_path = out_dir.join(format!("{}_vk.json", name));
                let vk_json_str = serde_json::to_string_pretty(&vk_json)?;
                std::fs::write(&vk_path, &vk_json_str)
                    .with_context(|| format!("Writing {vk_path:?}"))?;
                eprintln!("  vk: {vk_path:?} ({} bytes)", vk_json_str.len());
            }
        }
        Command::SetupPool { out_dir } => {
            std::fs::create_dir_all(&out_dir)?;
            let mut rng = rand::thread_rng();
            let [(pk_insert, vk_insert), (pk_withdraw, vk_withdraw)] = setup_pool(&mut rng)?;

            for (name, pk, vk) in [
                ("shielded_insert", &pk_insert, &vk_insert),
                ("shielded_withdraw", &pk_withdraw, &vk_withdraw),
            ] {
                eprintln!("Setting up {}…", name);
                let pk_path = out_dir.join(format!("{}.pk.bin", name));
                let mut pk_bytes = Vec::new();
                pk.serialize_compressed(&mut pk_bytes)?;
                std::fs::write(&pk_path, &pk_bytes)
                    .with_context(|| format!("Writing {pk_path:?}"))?;
                eprintln!("  pk: {pk_path:?} ({} bytes)", pk_bytes.len());
                let vk_json = vk_to_json(vk);
                let vk_path = out_dir.join(format!("{}_vk.json", name));
                let vk_json_str = serde_json::to_string_pretty(&vk_json)?;
                std::fs::write(&vk_path, &vk_json_str)
                    .with_context(|| format!("Writing {vk_path:?}"))?;
                eprintln!("  vk: {vk_path:?} ({} bytes)", vk_json_str.len());
            }
        }
        Command::PoolZeros => {
            let zeros = compute_pool_zeros();
            println!("// Zero subtree hashes for TREE_DEPTH={}", TREE_DEPTH);
            println!("// Embed these in the shielded-pool contract as ZEROS constant");
            for (i, z) in zeros.iter().enumerate() {
                let bytes = z.into_bigint().to_bytes_be();
                println!("zeros[{}] = 0x{}", i, hex::encode(&bytes));
            }
            let root = compute_empty_root();
            let root_bytes = root.into_bigint().to_bytes_be();
            println!("empty_root = 0x{}", hex::encode(&root_bytes));
        }
        Command::ShieldedInsert {
            commitment,
            leaf_index,
            leaves,
            pk,
        } => {
            let cmt_bytes = hex::decode(&commitment).with_context(|| "commitment must be hex")?;
            let cmt_fr = Fr::from_be_bytes_mod_order(&cmt_bytes);

            let leaf_frs: Vec<Fr> = if leaves.is_empty() {
                vec![]
            } else {
                leaves
                    .split(',')
                    .map(|s| -> Result<Fr> {
                        let n: num_bigint::BigUint = s
                            .trim()
                            .parse()
                            .with_context(|| format!("bad leaf value: {s}"))?;
                        Ok(Fr::from_be_bytes_mod_order(&n.to_bytes_be()))
                    })
                    .collect::<Result<Vec<_>>>()?
            };

            let old_root = if leaf_frs.is_empty() {
                compute_empty_root()
            } else {
                compute_root_from_leaves(&leaf_frs)
            };

            let path_elements = compute_merkle_path(&leaf_frs, leaf_index as usize);
            let new_root = compute_new_root(cmt_fr, leaf_index, &path_elements);

            let proving_key =
                load_pk(&pk).with_context(|| format!("loading insert pk from {pk:?}"))?;

            let out = prove_shielded_insert(
                &proving_key,
                old_root,
                new_root,
                cmt_fr,
                leaf_index,
                path_elements,
            )?;
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        Command::ShieldedWithdraw {
            secret,
            nullifier,
            recipient,
            leaves,
            pk,
        } => {
            let secret_fr = Fr::from(secret);
            let nullifier_fr = Fr::from(nullifier);

            let leaf = compute_leaf_hash(secret_fr, nullifier_fr);
            let nullifier_hash = compute_pool_nullifier_hash(nullifier_fr);

            let recipient_bytes =
                hex::decode(&recipient).with_context(|| "recipient must be hex")?;
            let recipient_fr = Fr::from_be_bytes_mod_order(&recipient_bytes);

            let leaf_frs: Vec<Fr> = leaves
                .split(',')
                .map(|s| -> Result<Fr> {
                    let n: num_bigint::BigUint = s
                        .trim()
                        .parse()
                        .with_context(|| format!("bad leaf value: {s}"))?;
                    Ok(Fr::from_be_bytes_mod_order(&n.to_bytes_be()))
                })
                .collect::<Result<Vec<_>>>()?;

            let leaf_index = leaf_frs
                .iter()
                .position(|&l| l == leaf)
                .with_context(|| "leaf not found in tree")?;

            let path_elements = compute_merkle_path(&leaf_frs, leaf_index);
            let mut path_indices = [false; TREE_DEPTH];
            for (i, slot) in path_indices.iter_mut().enumerate() {
                *slot = ((leaf_index >> i) & 1) == 1;
            }

            let root = compute_root_from_leaves(&leaf_frs);

            let proving_key =
                load_pk(&pk).with_context(|| format!("loading withdraw pk from {pk:?}"))?;

            let out = prove_shielded_withdraw(
                &proving_key,
                root,
                nullifier_hash,
                recipient_fr,
                secret_fr,
                nullifier_fr,
                path_elements,
                path_indices,
            )?;
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        Command::OrderCommitment {
            side,
            price,
            size,
            leverage,
            asset_id,
            nonce,
            secret,
            cross_margin,
        } => {
            let out = prove_commitment(
                Fr::from(side),
                Fr::from(price),
                Fr::from(size),
                Fr::from(leverage),
                Fr::from(asset_id),
                Fr::from(0),
                Fr::from(nonce),
                Fr::from(secret),
                cross_margin,
            )?;
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        Command::NoteSpend { amount, secret } => {
            let out = prove_note_spend(Fr::from(amount), Fr::from(secret))?;
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        Command::OrderCancel { commitment, secret } => {
            let cmt: num_bigint::BigUint = commitment
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid commitment decimal: {commitment}"))?;
            let cmt_fr = Fr::from_be_bytes_mod_order(&cmt.to_bytes_be());
            let out = prove_cancel(cmt_fr, Fr::from(secret))?;
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        Command::OrderMatch {
            side_a,
            price_a,
            size_a,
            leverage_a,
            asset_id_a,
            nonce_a,
            secret_a,
            side_b,
            price_b,
            size_b,
            leverage_b,
            asset_id_b,
            nonce_b,
            secret_b,
            match_price,
            match_size,
        } => {
            let out = prove_match(
                Fr::from(side_a),
                Fr::from(price_a),
                Fr::from(size_a),
                Fr::from(leverage_a),
                Fr::from(asset_id_a),
                Fr::from(0),
                Fr::from(nonce_a),
                Fr::from(secret_a),
                Fr::from(side_b),
                Fr::from(price_b),
                Fr::from(size_b),
                Fr::from(leverage_b),
                Fr::from(asset_id_b),
                Fr::from(0),
                Fr::from(nonce_b),
                Fr::from(secret_b),
                Fr::from(match_price),
                Fr::from(match_size),
            )?;
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
    }

    Ok(())
}
