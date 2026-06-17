use crate::constants::{bn256_modulus, get_round_constants_t2, get_round_constants_t3};
use soroban_sdk::{Bytes, Env, U256, Vec, symbol_short, vec};

pub fn poseidon2_compress(env: &Env, left: U256, right: U256) -> U256 {
    let bn256_mod = bn256_modulus(env);
    if left >= bn256_mod || right >= bn256_mod {
        panic!("Hash inputs must be within the BN256 range [0.p-1)");
    }
    let round_constants = get_round_constants_t2(env);
    let crypto_hazmat = env.crypto_hazmat();
    let out = crypto_hazmat.poseidon2_permutation(
        &vec![env, left.clone(), right.clone()],
        symbol_short!("BN254"),
        2,
        5,
        8,
        56,
        &vec![env, U256::from_u32(env, 1u32), U256::from_u32(env, 2u32)],
        &round_constants,
    );
    let out_0 = out.get(0).unwrap();
    let mut compressed_0 = out_0.add(&left);
    if compressed_0 >= bn256_mod {
        compressed_0 = compressed_0.rem_euclid(&bn256_mod);
    }
    compressed_0
}

pub fn poseidon2_hash2(env: &Env, a: U256, b: U256, sep: Option<U256>) -> U256 {
    let bn256_mod = bn256_modulus(env);
    let sep = sep.unwrap_or_else(|| U256::from_u32(env, 0u32));
    if a >= bn256_mod || b >= bn256_mod || sep >= bn256_mod {
        panic!("Hash inputs must be within the BN256 range [0.p-1)");
    }
    let round_constants = get_round_constants_t3(env);
    let crypto_hazmat = env.crypto_hazmat();
    let out = crypto_hazmat.poseidon2_permutation(
        &vec![env, a.clone(), b.clone(), sep.clone()],
        symbol_short!("BN254"),
        3,
        5,
        8,
        56,
        &vec![env, U256::from_u32(env, 1u32), U256::from_u32(env, 1u32), U256::from_u32(env, 2u32)],
        &round_constants,
    );
    out.get(0).unwrap()
}
