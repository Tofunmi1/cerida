use crate::field::Bn254Fr;
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::Env;

pub struct Debug;

impl Debug {
    pub fn fmt_hex(env: Env, data: &[u8]) -> soroban_sdk::String {
        let hex_string = data
            .iter()
            .fold(String::new(), |mut acc, byte| {
                let _ = core::write!(&mut acc, "{:02x}", byte);
                acc
            });
        soroban_sdk::String::from_str(&env, &hex_string)
    }

    pub fn fmt_fr(env: Env, fr: &Bn254Fr) -> soroban_sdk::String {
        let bytes = fr.to_bytes_be();
        Self::fmt_hex(env, &bytes)
    }
}
