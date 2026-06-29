#![no_main]
sp1_zkvm::entrypoint!(main);

use sha2::{Digest, Sha256};

#[derive(serde::Serialize, serde::Deserialize)]
pub struct PrivateInput {
    pub amount: u64,
    pub secret: u64,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct PublicOutput {
    pub commitment: [u8; 32],
    pub nullifier: [u8; 32],
}

pub fn main() {
    let input: PrivateInput = sp1_zkvm::io::read::<PrivateInput>();

    // commitment = SHA256(amount_be || secret_be || 0x01)
    let commitment: [u8; 32] = {
        let mut h = Sha256::new();
        h.update(input.amount.to_be_bytes());
        h.update(input.secret.to_be_bytes());
        h.update([1u8]);
        h.finalize().into()
    };

    // nullifier = SHA256(commitment || secret_be || 0x02)
    let nullifier: [u8; 32] = {
        let mut h = Sha256::new();
        h.update(commitment);
        h.update(input.secret.to_be_bytes());
        h.update([2u8]);
        h.finalize().into()
    };

    sp1_zkvm::io::commit(&PublicOutput { commitment, nullifier });
}
