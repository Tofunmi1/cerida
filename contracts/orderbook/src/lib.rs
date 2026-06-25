#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, crypto::bn254::Bn254Fr,
    Address, BytesN, Env, Symbol, Vec,
};
use types::{Groth16Proof, OrderMeta, OrderStatus};
use verifier_groth16::VerifierGroth16;

#[contracttype]
pub enum DataKey {
    Order(BytesN<32>),
    Nullifier(BytesN<32>),
}

#[contract]
pub struct Orderbook;

#[contractimpl]
impl Orderbook {
    pub fn place_order(
        env: Env,
        owner: Address,
        commitment: BytesN<32>,
        hint: u64,
        proof: Groth16Proof,
    ) {
        owner.require_auth();

        let order_key = DataKey::Order(commitment.clone());
        if env.storage().persistent().has(&order_key) {
            panic!("commitment already exists");
        }

        let mut pi: Vec<Bn254Fr> = Vec::new(&env);
        pi.push_back(Bn254Fr::from_bytes(commitment.clone()));

        match VerifierGroth16::verify(env.clone(), proof, pi) {
            Ok(true) => {}
            _ => panic!("invalid commitment proof"),
        }

        let meta = OrderMeta {
            owner: owner.clone(),
            hint,
            asset_id: BytesN::from_array(&env, &[0u8; 32]),
            status: OrderStatus::Open,
            created_at: env.ledger().sequence(),
        };

        env.storage().persistent().set(&order_key, &meta);
        env.storage().persistent().extend_ttl(&order_key, 17280, 17280);
    }

    pub fn cancel_order(
        env: Env,
        owner: Address,
        commitment: BytesN<32>,
        nullifier: BytesN<32>,
        proof: Groth16Proof,
    ) {
        owner.require_auth();

        let null_key = DataKey::Nullifier(nullifier.clone());
        if env.storage().persistent().has(&null_key) {
            panic!("nullifier already spent");
        }

        let order_key = DataKey::Order(commitment.clone());
        let mut meta: OrderMeta = env
            .storage()
            .persistent()
            .get(&order_key)
            .unwrap_or_else(|| panic!("commitment not found"));

        if meta.owner != owner {
            panic!("unauthorized");
        }
        if meta.status != OrderStatus::Open {
            panic!("order is not open");
        }

        let mut pi: Vec<Bn254Fr> = Vec::new(&env);
        pi.push_back(Bn254Fr::from_bytes(nullifier.clone()));

        match VerifierGroth16::verify(env.clone(), proof, pi) {
            Ok(true) => {}
            _ => panic!("invalid cancel proof"),
        }

        meta.status = OrderStatus::Cancelled;
        env.storage().persistent().set(&order_key, &meta);
        env.storage().persistent().set(&null_key, &true);
        env.storage().persistent().extend_ttl(&null_key, 17280, 17280);
        env.storage().persistent().extend_ttl(&order_key, 17280, 17280);
    }

    pub fn match_orders(
        env: Env,
        cmt_a: BytesN<32>,
        cmt_b: BytesN<32>,
        nullifier_a: BytesN<32>,
        nullifier_b: BytesN<32>,
        match_price: BytesN<32>,
        match_size: BytesN<32>,
        proof: Groth16Proof,
    ) {
        let order_key_a = DataKey::Order(cmt_a.clone());
        let order_key_b = DataKey::Order(cmt_b.clone());

        let meta_a: OrderMeta = env
            .storage()
            .persistent()
            .get(&order_key_a)
            .unwrap_or_else(|| panic!("commitment A not found"));
        let meta_b: OrderMeta = env
            .storage()
            .persistent()
            .get(&order_key_b)
            .unwrap_or_else(|| panic!("commitment B not found"));

        if meta_a.status != OrderStatus::Open || meta_b.status != OrderStatus::Open {
            panic!("one or both orders not open");
        }

        let null_key_a = DataKey::Nullifier(nullifier_a.clone());
        let null_key_b = DataKey::Nullifier(nullifier_b.clone());
        if env.storage().persistent().has(&null_key_a) {
            panic!("nullifier A already spent");
        }
        if env.storage().persistent().has(&null_key_b) {
            panic!("nullifier B already spent");
        }

        let mut pi: Vec<Bn254Fr> = Vec::new(&env);
        pi.push_back(Bn254Fr::from_bytes(cmt_a));
        pi.push_back(Bn254Fr::from_bytes(cmt_b));
        pi.push_back(Bn254Fr::from_bytes(match_price));
        pi.push_back(Bn254Fr::from_bytes(match_size));
        pi.push_back(Bn254Fr::from_bytes(nullifier_a.clone()));
        pi.push_back(Bn254Fr::from_bytes(nullifier_b.clone()));

        match VerifierGroth16::verify(env.clone(), proof, pi) {
            Ok(true) => {}
            _ => panic!("invalid match proof"),
        }

        meta_a.status = OrderStatus::Filled;
        meta_b.status = OrderStatus::Filled;
        env.storage().persistent().set(&order_key_a, &meta_a);
        env.storage().persistent().set(&order_key_b, &meta_b);
        env.storage().persistent().set(&null_key_a, &true);
        env.storage().persistent().set(&null_key_b, &true);

        env.storage().persistent().extend_ttl(&order_key_a, 17280, 17280);
        env.storage().persistent().extend_ttl(&order_key_b, 17280, 17280);
        env.storage().persistent().extend_ttl(&null_key_a, 17280, 17280);
        env.storage().persistent().extend_ttl(&null_key_b, 17280, 17280);

        env.events().publish(
            (Symbol::short("match"),),
            (cmt_a, cmt_b, nullifier_a, nullifier_b),
        );
    }

    pub fn status(env: Env, commitment: BytesN<32>) -> Option<OrderStatus> {
        env.storage()
            .persistent()
            .get::<_, OrderMeta>(&DataKey::Order(commitment))
            .map(|m| m.status)
    }

    pub fn is_spent(env: Env, nullifier: BytesN<32>) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::Nullifier(nullifier))
    }

    pub fn order_meta(env: Env, commitment: BytesN<32>) -> Option<OrderMeta> {
        env.storage()
            .persistent()
            .get::<_, OrderMeta>(&DataKey::Order(commitment))
    }
}
