use soroban_sdk::{Bytes, Env};

fn bytes_from_slice(env: &Env, input: &[u8]) -> Bytes {
    let mut b = Bytes::new(env);
    for &byte in input {
        b.push_back(byte);
    }
    b
}

pub fn keccak256(env: &Env, input: &[u8]) -> [u8; 32] {
    let bytes = bytes_from_slice(env, input);
    env.crypto().keccak256(&bytes).to_array()
}

pub fn sha256(env: &Env, input: &[u8]) -> [u8; 32] {
    let bytes = bytes_from_slice(env, input);
    env.crypto().sha256(&bytes).to_array()
}
