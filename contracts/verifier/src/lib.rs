#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, crypto::bn254::Bn254Fr,
    Address, Bytes, BytesN, Env, Vec,
};
use ultrahonk_soroban_verifier::{
    UltraHonkVerifier, VerificationKey, Proof,
    parse_vk, parse_proof,
};

#[contracttype]
pub enum DataKey {
    Position(BytesN<32>),
    Nullifier(BytesN<32>),
}

#[contracttype]
#[derive(Clone)]
pub struct Position {
    pub collateral: i128,
    pub trader: Address,
}

#[contract]
pub struct StellarVerifier;

#[contractimpl]
impl StellarVerifier {
    pub fn __constructor(env: Env, vk_bytes: Bytes) {
        env.storage().instance().set(&DataKey::Position(BytesN::from_array(&env, &[0u8; 32])), &vk_bytes);
    }

    pub fn open(
        env: Env,
        trader: Address,
        commitment: BytesN<32>,
        collateral: i128,
    ) {
        trader.require_auth();
        if collateral <= 0 {
            panic!("collateral must be positive");
        }
        let key = DataKey::Position(commitment);
        if env.storage().persistent().has(&key) {
            panic!("commitment already exists");
        }
        env.storage()
            .persistent()
            .set(&key, &Position { collateral, trader });
        env.storage().persistent().extend_ttl(&key, 17280, 17280);
    }

    pub fn close(
        env: Env,
        trader: Address,
        commitment: BytesN<32>,
        nullifier: BytesN<32>,
        proof_bytes: Bytes,
        public_inputs: Vec<BytesN<32>>,
    ) -> i128 {
        trader.require_auth();

        let null_key = DataKey::Nullifier(nullifier.clone());
        if env.storage().persistent().has(&null_key) {
            panic!("position already closed");
        }

        let pos_key = DataKey::Position(commitment.clone());
        let pos: Position = env
            .storage()
            .persistent()
            .get(&pos_key)
            .unwrap_or_else(|| panic!("position not found"));
        if pos.trader != trader {
            panic!("unauthorized");
        }

        let vk_stored: Bytes = env
            .storage()
            .instance()
            .get(&DataKey::Position(BytesN::from_array(&env, &[0u8; 32])))
            .unwrap_or_else(|| panic!("VK not set"));

        let pi_fr: Vec<Bn254Fr> = public_inputs.iter()
            .map(|b| Bn254Fr::from_bytes(b.to_array()))
            .collect();

        let parsed_proof = parse_proof(&proof_bytes.to_array());

        let vk = parse_vk(&vk_stored.to_array());

        if !UltraHonkVerifier::verify(&env, &vk, &parsed_proof, &pi_fr) {
            panic!("invalid ZK proof");
        }

        env.storage().persistent().set(&null_key, &true);
        env.storage().persistent().extend_ttl(&null_key, 17280, 17280);
        env.storage().persistent().remove(&pos_key);

        pos.collateral
    }

    pub fn collateral_of(env: Env, commitment: BytesN<32>) -> Option<i128> {
        env.storage()
            .persistent()
            .get::<_, Position>(&DataKey::Position(commitment))
            .map(|p| p.collateral)
    }

    pub fn is_spent(env: Env, nullifier: BytesN<32>) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::Nullifier(nullifier))
    }
}
