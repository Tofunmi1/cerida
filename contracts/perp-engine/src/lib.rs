#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype,
    crypto::bn254::{Bn254Fr, Bn254G1Affine as G1Affine, Bn254G2Affine as G2Affine},
    token::TokenClient,
    Address, BytesN, Env, Vec,
};
use types::{Groth16Error, Groth16Proof};

include!(concat!(env!("OUT_DIR"), "/vk.rs"));

struct VerificationKey {
    alpha: G1Affine,
    beta: G2Affine,
    gamma: G2Affine,
    delta: G2Affine,
    ic: Vec<G1Affine>,
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
        let s = public_inputs.get(i).ok_or(Groth16Error::MalformedPublicInputs)?;
        let ic_idx = i.checked_add(1).ok_or(Groth16Error::MalformedPublicInputs)?;
        let v = vk.ic.get(ic_idx).ok_or(Groth16Error::MalformedPublicInputs)?;
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

fn field_to_u64(b: &BytesN<32>) -> u64 {
    let arr = b.to_array();
    u64::from_be_bytes([
        arr[24], arr[25], arr[26], arr[27],
        arr[28], arr[29], arr[30], arr[31],
    ])
}

#[contracttype]
pub enum DataKey {
    Config,
    Position(BytesN<32>),
    Nullifier(BytesN<32>),
    Balance(Address),
}

#[contracttype]
#[derive(Clone)]
pub struct Config {
    pub admin: Address,
    pub token: Address,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum PositionStatus {
    Open = 0,
    Matched = 1,
    Closed = 2,
    Cancelled = 3,
    Liquidated = 4,
}

#[contracttype]
#[derive(Clone)]
pub struct PositionMeta {
    pub owner: Address,
    pub collateral: i128,
    pub entry_price: u64,
    pub matched_price: u64,
    pub side: u64,
    pub leverage: u64,
    pub status: PositionStatus,
    pub created_at: u64,
}

#[contract]
pub struct PerpEngine;

#[contractimpl]
impl PerpEngine {
    pub fn initialize(env: Env, admin: Address, token: Address) {
        if env.storage().instance().has(&DataKey::Config) {
            panic!("already initialized");
        }
        env.storage()
            .instance()
            .set(&DataKey::Config, &Config { admin, token });
    }

    pub fn deposit(env: Env, who: Address, amount: i128) {
        who.require_auth();
        if amount <= 0 {
            panic!("amount must be positive");
        }
        let cfg = Self::config(&env);
        TokenClient::new(&env, &cfg.token)
            .transfer(&who, &env.current_contract_address(), &amount);
        let key = DataKey::Balance(who.clone());
        let bal = Self::read_balance(&env, &who);
        env.storage().persistent().set(&key, &(bal + amount));
    }

    pub fn withdraw(env: Env, who: Address, amount: i128) {
        who.require_auth();
        if amount <= 0 {
            panic!("amount must be positive");
        }
        let key = DataKey::Balance(who.clone());
        let bal = Self::read_balance(&env, &who);
        if bal < amount {
            panic!("insufficient balance");
        }
        env.storage().persistent().set(&key, &(bal - amount));
        let cfg = Self::config(&env);
        TokenClient::new(&env, &cfg.token)
            .transfer(&env.current_contract_address(), &who, &amount);
    }

    pub fn get_balance(env: Env, who: Address) -> i128 {
        Self::read_balance(&env, &who)
    }

    fn read_balance(env: &Env, who: &Address) -> i128 {
        env.storage()
            .persistent()
            .get::<_, i128>(&DataKey::Balance(who.clone()))
            .unwrap_or(0)
    }

    pub fn open_position(
        env: Env,
        owner: Address,
        commitment: BytesN<32>,
        collateral: i128,
        hint_price: u64,
        hint_side: u64,
        hint_leverage: u64,
        proof: Groth16Proof,
    ) {
        owner.require_auth();
        if collateral <= 0 {
            panic!("collateral must be positive");
        }
        if hint_side > 1 {
            panic!("side must be 0 (long) or 1 (short)");
        }
        if hint_leverage == 0 {
            panic!("leverage must be >= 1");
        }

        let pos_key = DataKey::Position(commitment.clone());
        if env.storage().persistent().has(&pos_key) {
            panic!("commitment already exists");
        }

        let mut pi: Vec<Bn254Fr> = Vec::new(&env);
        pi.push_back(Bn254Fr::from_bytes(commitment.clone()));
        let vk = load_vk(&env, &VK_COMMIT_IC);
        match verify_groth16(&env, &vk, &proof, &pi) {
            Ok(true) => {}
            _ => panic!("invalid commitment proof"),
        }

        let bal = Self::read_balance(&env, &owner);
        if bal < collateral {
            panic!("insufficient balance");
        }
        env.storage()
            .persistent()
            .set(&DataKey::Balance(owner.clone()), &(bal - collateral));

        let meta = PositionMeta {
            owner,
            collateral,
            entry_price: hint_price,
            matched_price: 0,
            side: hint_side,
            leverage: hint_leverage,
            status: PositionStatus::Open,
            created_at: env.ledger().sequence() as u64,
        };
        env.storage().persistent().set(&pos_key, &meta);
        env.storage()
            .persistent()
            .extend_ttl(&pos_key, 17280, 17280);
    }

    pub fn cancel_position(
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

        let pos_key = DataKey::Position(commitment.clone());
        let mut meta: PositionMeta = env
            .storage()
            .persistent()
            .get(&pos_key)
            .unwrap_or_else(|| panic!("position not found"));
        if meta.owner != owner {
            panic!("unauthorized");
        }
        if meta.status != PositionStatus::Open {
            panic!("can only cancel an open position");
        }

        let mut pi: Vec<Bn254Fr> = Vec::new(&env);
        pi.push_back(Bn254Fr::from_bytes(nullifier.clone()));
        let vk = load_vk(&env, &VK_CANCEL_IC);
        match verify_groth16(&env, &vk, &proof, &pi) {
            Ok(true) => {}
            _ => panic!("invalid cancel proof"),
        }

        meta.status = PositionStatus::Cancelled;
        env.storage().persistent().set(&pos_key, &meta);
        env.storage().persistent().set(&null_key, &true);

        let bal = Self::read_balance(&env, &meta.owner);
        env.storage()
            .persistent()
            .set(&DataKey::Balance(meta.owner.clone()), &(bal + meta.collateral));
        env.storage()
            .persistent()
            .extend_ttl(&pos_key, 17280, 17280);
        env.storage()
            .persistent()
            .extend_ttl(&null_key, 17280, 17280);
    }

    pub fn match_positions(
        env: Env,
        cmt_a: BytesN<32>,
        cmt_b: BytesN<32>,
        nullifier_a: BytesN<32>,
        nullifier_b: BytesN<32>,
        match_price: BytesN<32>,
        match_size: BytesN<32>,
        proof: Groth16Proof,
    ) {
        let null_key_a = DataKey::Nullifier(nullifier_a.clone());
        let null_key_b = DataKey::Nullifier(nullifier_b.clone());
        if env.storage().persistent().has(&null_key_a) {
            panic!("nullifier A already spent");
        }
        if env.storage().persistent().has(&null_key_b) {
            panic!("nullifier B already spent");
        }

        let pos_key_a = DataKey::Position(cmt_a.clone());
        let pos_key_b = DataKey::Position(cmt_b.clone());
        let mut meta_a: PositionMeta = env
            .storage()
            .persistent()
            .get(&pos_key_a)
            .unwrap_or_else(|| panic!("position A not found"));
        let mut meta_b: PositionMeta = env
            .storage()
            .persistent()
            .get(&pos_key_b)
            .unwrap_or_else(|| panic!("position B not found"));

        if meta_a.status != PositionStatus::Open || meta_b.status != PositionStatus::Open {
            panic!("both positions must be open");
        }

        let mut pi: Vec<Bn254Fr> = Vec::new(&env);
        pi.push_back(Bn254Fr::from_bytes(cmt_a.clone()));
        pi.push_back(Bn254Fr::from_bytes(cmt_b.clone()));
        pi.push_back(Bn254Fr::from_bytes(match_price.clone()));
        pi.push_back(Bn254Fr::from_bytes(match_size.clone()));
        pi.push_back(Bn254Fr::from_bytes(nullifier_a.clone()));
        pi.push_back(Bn254Fr::from_bytes(nullifier_b.clone()));
        let vk = load_vk(&env, &VK_MATCH_IC);
        match verify_groth16(&env, &vk, &proof, &pi) {
            Ok(true) => {}
            _ => panic!("invalid match proof"),
        }

        let exec_price = field_to_u64(&match_price);

        meta_a.matched_price = exec_price;
        meta_a.status = PositionStatus::Matched;
        meta_b.matched_price = exec_price;
        meta_b.status = PositionStatus::Matched;

        env.storage().persistent().set(&pos_key_a, &meta_a);
        env.storage().persistent().set(&pos_key_b, &meta_b);
        env.storage().persistent().set(&null_key_a, &true);
        env.storage().persistent().set(&null_key_b, &true);
        for key in [&pos_key_a, &pos_key_b, &null_key_a, &null_key_b] {
            env.storage().persistent().extend_ttl(key, 17280, 17280);
        }

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("match"),),
            (cmt_a, cmt_b, exec_price),
        );
    }

    pub fn close_position(
        env: Env,
        owner: Address,
        commitment: BytesN<32>,
        nullifier: BytesN<32>,
        close_price: u64,
        proof: Groth16Proof,
    ) -> i128 {
        owner.require_auth();
        if close_price == 0 {
            panic!("close_price must be > 0");
        }

        let null_key = DataKey::Nullifier(nullifier.clone());
        if env.storage().persistent().has(&null_key) {
            panic!("nullifier already spent");
        }

        let pos_key = DataKey::Position(commitment.clone());
        let mut meta: PositionMeta = env
            .storage()
            .persistent()
            .get(&pos_key)
            .unwrap_or_else(|| panic!("position not found"));
        if meta.owner != owner {
            panic!("unauthorized");
        }
        if meta.status != PositionStatus::Matched {
            panic!("can only close a matched position");
        }

        let mut pi: Vec<Bn254Fr> = Vec::new(&env);
        pi.push_back(Bn254Fr::from_bytes(nullifier.clone()));
        let vk = load_vk(&env, &VK_CANCEL_IC);
        match verify_groth16(&env, &vk, &proof, &pi) {
            Ok(true) => {}
            _ => panic!("invalid close proof"),
        }

        let settlement = compute_settlement(
            meta.collateral,
            meta.leverage,
            meta.side,
            meta.matched_price,
            close_price,
        );

        meta.status = PositionStatus::Closed;
        env.storage().persistent().set(&pos_key, &meta);
        env.storage().persistent().set(&null_key, &true);
        env.storage()
            .persistent()
            .extend_ttl(&pos_key, 17280, 17280);
        env.storage()
            .persistent()
            .extend_ttl(&null_key, 17280, 17280);

        let bal = Self::read_balance(&env, &owner);
        env.storage()
            .persistent()
            .set(&DataKey::Balance(owner.clone()), &(bal + settlement));

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("close"),),
            (commitment, settlement, close_price),
        );

        settlement
    }

    pub fn liquidate(
        env: Env,
        commitment: BytesN<32>,
        liquidator: Address,
        current_price: u64,
    ) -> i128 {
        liquidator.require_auth();
        if current_price == 0 {
            panic!("current_price must be > 0");
        }

        let pos_key = DataKey::Position(commitment.clone());
        let mut meta: PositionMeta = env
            .storage()
            .persistent()
            .get(&pos_key)
            .unwrap_or_else(|| panic!("position not found"));

        if meta.status != PositionStatus::Matched {
            panic!("can only liquidate a matched position");
        }

        let settlement = compute_settlement(
            meta.collateral,
            meta.leverage,
            meta.side,
            meta.matched_price,
            current_price,
        );

        let threshold = meta.collateral / 5;
        if settlement > threshold {
            panic!("position is not under-collateralized");
        }

        let reward = meta.collateral / 20;
        let to_owner = settlement.saturating_sub(reward).max(0);

        let bal = Self::read_balance(&env, &meta.owner);
        env.storage()
            .persistent()
            .set(&DataKey::Balance(meta.owner.clone()), &(bal + to_owner));
        let cfg = Self::config(&env);
        TokenClient::new(&env, &cfg.token)
            .transfer(&env.current_contract_address(), &liquidator, &reward);

        meta.status = PositionStatus::Liquidated;
        env.storage().persistent().set(&pos_key, &meta);
        env.storage()
            .persistent()
            .extend_ttl(&pos_key, 17280, 17280);

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("liq"),),
            (commitment, liquidator, current_price, reward, to_owner),
        );

        settlement
    }

    pub fn get_position(env: Env, commitment: BytesN<32>) -> Option<PositionMeta> {
        env.storage()
            .persistent()
            .get::<_, PositionMeta>(&DataKey::Position(commitment))
    }

    pub fn is_spent(env: Env, nullifier: BytesN<32>) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::Nullifier(nullifier))
    }

    pub fn get_config(env: Env) -> Config {
        Self::config(&env)
    }

    fn config(env: &Env) -> Config {
        env.storage()
            .instance()
            .get(&DataKey::Config)
            .unwrap_or_else(|| panic!("not initialized"))
    }
}

fn compute_settlement(
    collateral: i128,
    leverage: u64,
    side: u64,
    entry_price: u64,
    close_price: u64,
) -> i128 {
    if entry_price == 0 {
        return collateral;
    }

    let entry = entry_price as i128;
    let close = close_price as i128;
    let lev = leverage as i128;

    let price_delta = close - entry;
    let raw_pnl = collateral * lev * price_delta / entry;

    let signed_pnl = if side == 1 { -raw_pnl } else { raw_pnl };

    let max_gain = collateral * (lev + 1);
    (collateral + signed_pnl).max(0).min(max_gain)
}
