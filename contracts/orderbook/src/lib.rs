#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, crypto::bn254::{
        Bn254Fr, Bn254G1Affine as G1Affine, Bn254G2Affine as G2Affine,
    }, Address, BytesN, Env, Vec,
};
use types::{Groth16Error, Groth16Proof, OrderMeta, OrderStatus};

include!(concat!(env!("OUT_DIR"), "/vk.rs"));

#[derive(Clone)]
pub struct VerificationKey {
    pub alpha: G1Affine,
    pub beta: G2Affine,
    pub gamma: G2Affine,
    pub delta: G2Affine,
    pub ic: Vec<G1Affine>,
}

fn load_vk(env: &Env, ic_slice: &[[u8; 64]]) -> VerificationKey {
    let mut ic_vec: Vec<G1Affine> = Vec::new(env);
    for bytes in ic_slice {
        ic_vec.push_back(G1Affine::from_bytes(BytesN::from_array(env, bytes)));
    }
    VerificationKey {
        alpha: G1Affine::from_bytes(BytesN::from_array(env, &VK_ALPHA_G1)),
        beta: G2Affine::from_bytes(BytesN::from_array(env, &VK_BETA_G2)),
        gamma: G2Affine::from_bytes(BytesN::from_array(env, &VK_GAMMA_G2)),
        delta: G2Affine::from_bytes(BytesN::from_array(env, &VK_DELTA_G2)),
        ic: ic_vec,
    }
}

fn verify_groth16(
    env: &Env,
    vk: &VerificationKey,
    proof: &Groth16Proof,
    public_inputs: &Vec<Bn254Fr>,
) -> Result<bool, Groth16Error> {
    let bn = env.crypto().bn254();

    if public_inputs.len().checked_add(1) != Some(vk.ic.len()) {
        return Err(Groth16Error::MalformedPublicInputs);
    }

    let mut vk_x = vk.ic.get(0).ok_or(Groth16Error::MalformedPublicInputs)?;

    for i in 0..public_inputs.len() {
        let s = public_inputs
            .get(i)
            .ok_or(Groth16Error::MalformedPublicInputs)?;
        let ic_idx = i
            .checked_add(1)
            .ok_or(Groth16Error::MalformedPublicInputs)?;
        let v = vk
            .ic
            .get(ic_idx)
            .ok_or(Groth16Error::MalformedPublicInputs)?;
        let prod = bn.g1_mul(&v, &s);
        vk_x = bn.g1_add(&vk_x, &prod);
    }

    let neg_a = -proof.a.clone();

    let g1_points = soroban_sdk::vec![&env, neg_a, vk.alpha.clone(), vk_x, proof.c.clone()];
    let g2_points = soroban_sdk::vec![
        &env,
        proof.b.clone(),
        vk.beta.clone(),
        vk.gamma.clone(),
        vk.delta.clone(),
    ];
    if bn.pairing_check(g1_points, g2_points) {
        Ok(true)
    } else {
        Err(Groth16Error::InvalidProof)
    }
}

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

        let vk = load_vk(&env, &VK_COMMIT_IC);
        match verify_groth16(&env, &vk, &proof, &pi) {
            Ok(true) => {}
            _ => panic!("invalid commitment proof"),
        }

        let meta = OrderMeta {
            owner: owner.clone(),
            hint,
            asset_id: BytesN::from_array(&env, &[0u8; 32]),
            status: OrderStatus::Open,
            created_at: env.ledger().sequence() as u64,
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

        let vk = load_vk(&env, &VK_CANCEL_IC);
        match verify_groth16(&env, &vk, &proof, &pi) {
            Ok(true) => {}
            _ => panic!("invalid cancel proof"),
        }

        meta.status = OrderStatus::Cancelled;
        env.storage().persistent().set(&order_key, &meta);
        env.storage().persistent().set(&null_key, &true);
        env.storage().persistent().extend_ttl(&order_key, 17280, 17280);
        env.storage().persistent().extend_ttl(&null_key, 17280, 17280);
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
