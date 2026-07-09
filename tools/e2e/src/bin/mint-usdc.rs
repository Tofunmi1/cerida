fn main() -> anyhow::Result<()> {
    e2e::stellar::mint_usdc(
        "CA6SDT7HE7LMYFYUQZGTVG6QJNKKZC2CZWE6ZK7YBZFZA23UNIGLBKFA",
        "GD4BWKZXC6FKM4I3NXZDAWAPPGXYA3WK7VQIBLLVNERQFWBUXSZSRWZL",
        6_000_000_000_000_i128,
    )?;
    println!("Minted 600,000 USDC");
    Ok(())
}
