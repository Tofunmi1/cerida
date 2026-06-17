use soroban_sdk::Env;

pub fn keccak256(env: &Env, input: &[u8]) -> [u8; 32] {
    soroban_sdk::hash::keccak256(env, input)
}

pub fn sha256(env: &Env, input: &[u8]) -> [u8; 32] {
    soroban_sdk::hash::sha256(env, input)
}
