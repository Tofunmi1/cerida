#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype,
    crypto::bn254::{Bn254Fr, Bn254G1Affine as G1Affine, Bn254G2Affine as G2Affine},
    token::TokenClient,
    Address, BytesN, Env, Vec,
};
use types::{
    Groth16Error, Groth16Proof, FundingState, MatchRecord, OracleConfig,
};

include!(concat!(env!("OUT_DIR"), "/vk.rs"));

const FUNDING_INTERVAL: u64 = 720;  // ledgers (~1 hour at 5s per ledger)
const MAINTENANCE_MARGIN: i128 = 5;  // 5 = 50% of initial margin (0.5 / leverage * 10)
const LIQUIDATOR_REWARD_NUM: i128 = 1;  // 1/20 of collateral
const LIQUIDATOR_REWARD_DEN: i128 = 20;

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
    OracleConfig,
    Match(u64),
    NextMatchId,
    FundingState,
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
    pub match_id: u64,
    pub funding_at_open: i128,
}

#[contract]
pub struct PerpEngine;

#[contractimpl]
impl PerpEngine {
    pub fn initialize(env: Env, admin: Address, token: Address) {
        if env.storage().instance().has(&DataKey::Config) {
            panic!("PerpEngine: already initialized");
        }
        env.storage()
            .instance()
            .set(&DataKey::Config, &Config { admin: admin.clone(), token: token.clone() });

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("init"),),
            (admin, token),
        );
    }

    pub fn deposit(env: Env, who: Address, amount: i128) {
        who.require_auth();
        if amount <= 0 {
            panic!("PerpEngine: deposit amount must be positive");
        }
        let cfg = Self::config(&env);
        TokenClient::new(&env, &cfg.token)
            .transfer(&who, &env.current_contract_address(), &amount);
        let key = DataKey::Balance(who.clone());
        let bal = Self::read_balance(&env, &who);
        let new_bal = bal + amount;
        env.storage().persistent().set(&key, &new_bal);

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("deposit"),),
            (who, amount, new_bal),
        );
    }

    pub fn withdraw(env: Env, who: Address, amount: i128) {
        who.require_auth();
        if amount <= 0 {
            panic!("PerpEngine: withdraw amount must be positive");
        }
        let key = DataKey::Balance(who.clone());
        let bal = Self::read_balance(&env, &who);
        if bal < amount {
            panic!("PerpEngine: insufficient balance (have {}, need {})", bal, amount);
        }
        let new_bal = bal - amount;
        env.storage().persistent().set(&key, &new_bal);
        let cfg = Self::config(&env);
        TokenClient::new(&env, &cfg.token)
            .transfer(&env.current_contract_address(), &who, &amount);

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("withdraw"),),
            (who, amount, new_bal),
        );
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
            panic!("PerpEngine: collateral must be positive");
        }
        if hint_side > 1 {
            panic!("PerpEngine: side must be 0 (long) or 1 (short), got {}", hint_side);
        }
        if hint_leverage == 0 {
            panic!("PerpEngine: leverage must be >= 1");
        }

        let pos_key = DataKey::Position(commitment.clone());
        if env.storage().persistent().has(&pos_key) {
            panic!("PerpEngine: commitment already exists");
        }

        let mut pi: Vec<Bn254Fr> = Vec::new(&env);
        pi.push_back(Bn254Fr::from_bytes(commitment.clone()));
        let vk = load_vk(&env, &VK_COMMIT_IC);
        match verify_groth16(&env, &vk, &proof, &pi) {
            Ok(true) => {}
            _ => panic!("PerpEngine: invalid commitment proof"),
        }

        let bal = Self::read_balance(&env, &owner);
        if bal < collateral {
            panic!("PerpEngine: insufficient balance (have {}, need {})", bal, collateral);
        }
        let new_bal = bal - collateral;
        env.storage()
            .persistent()
            .set(&DataKey::Balance(owner.clone()), &new_bal);

        let created_at = env.ledger().sequence() as u64;
        let meta = PositionMeta {
            owner: owner.clone(),
            collateral,
            entry_price: hint_price,
            matched_price: 0,
            side: hint_side,
            leverage: hint_leverage,
            status: PositionStatus::Open,
            created_at,
            match_id: 0,
            funding_at_open: Self::read_funding_cumulative(&env),
        };
        env.storage().persistent().set(&pos_key, &meta);
        env.storage()
            .persistent()
            .extend_ttl(&pos_key, 17280, 17280);

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("open"),),
            (owner, commitment, collateral, hint_side, hint_leverage, hint_price, created_at),
        );
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
            panic!("PerpEngine: nullifier already spent");
        }

        let pos_key = DataKey::Position(commitment.clone());
        let mut meta: PositionMeta = env
            .storage()
            .persistent()
            .get(&pos_key)
            .unwrap_or_else(|| panic!("PerpEngine: position not found"));
        if meta.owner != owner {
            panic!("PerpEngine: unauthorized caller for cancel_position");
        }
        if meta.status != PositionStatus::Open {
            panic!("PerpEngine: can only cancel an open position (status={:?})", meta.status as u32);
        }

        let mut pi: Vec<Bn254Fr> = Vec::new(&env);
        pi.push_back(Bn254Fr::from_bytes(nullifier.clone()));
        let vk = load_vk(&env, &VK_CANCEL_IC);
        match verify_groth16(&env, &vk, &proof, &pi) {
            Ok(true) => {}
            _ => panic!("PerpEngine: invalid cancel proof"),
        }

        meta.status = PositionStatus::Cancelled;
        env.storage().persistent().set(&pos_key, &meta);
        env.storage().persistent().set(&null_key, &true);

        let bal = Self::read_balance(&env, &meta.owner);
        let returned = meta.collateral;
        let new_bal = bal + returned;
        env.storage()
            .persistent()
            .set(&DataKey::Balance(meta.owner.clone()), &new_bal);
        env.storage()
            .persistent()
            .extend_ttl(&pos_key, 17280, 17280);
        env.storage()
            .persistent()
            .extend_ttl(&null_key, 17280, 17280);

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("cxl_pos"),),
            (owner, commitment, nullifier, returned, new_bal),
        );
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
    ) -> u64 {
        let null_key_a = DataKey::Nullifier(nullifier_a.clone());
        let null_key_b = DataKey::Nullifier(nullifier_b.clone());
        if env.storage().persistent().has(&null_key_a) {
            panic!("PerpEngine: nullifier A already spent");
        }
        if env.storage().persistent().has(&null_key_b) {
            panic!("PerpEngine: nullifier B already spent");
        }

        let pos_key_a = DataKey::Position(cmt_a.clone());
        let pos_key_b = DataKey::Position(cmt_b.clone());
        let mut meta_a: PositionMeta = env
            .storage()
            .persistent()
            .get(&pos_key_a)
            .unwrap_or_else(|| panic!("PerpEngine: position A not found"));
        let mut meta_b: PositionMeta = env
            .storage()
            .persistent()
            .get(&pos_key_b)
            .unwrap_or_else(|| panic!("PerpEngine: position B not found"));

        if meta_a.status != PositionStatus::Open || meta_b.status != PositionStatus::Open {
            panic!("PerpEngine: both positions must be open (A={:?}, B={:?})", meta_a.status as u32, meta_b.status as u32);
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
            _ => panic!("PerpEngine: invalid match proof"),
        }

        let exec_price = field_to_u64(&match_price);
        let exec_size = field_to_u64(&match_size);

        let match_id = Self::next_match_id(&env);
        let now = env.ledger().sequence() as u64;

        let record = MatchRecord {
            cmt_a: cmt_a.clone(),
            cmt_b: cmt_b.clone(),
            match_price: exec_price,
            match_size: exec_size,
            matched_at: now,
            closed: false,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Match(match_id), &record);

        meta_a.matched_price = exec_price;
        meta_a.status = PositionStatus::Matched;
        meta_a.match_id = match_id;
        meta_a.funding_at_open = Self::read_funding_cumulative(&env);
        meta_b.matched_price = exec_price;
        meta_b.status = PositionStatus::Matched;
        meta_b.match_id = match_id;
        meta_b.funding_at_open = Self::read_funding_cumulative(&env);

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
            (cmt_a, cmt_b, exec_price, exec_size, match_id, now),
        );

        match_id
    }

    pub fn close_position(
        env: Env,
        owner: Address,
        commitment: BytesN<32>,
        nullifier: BytesN<32>,
        proof: Groth16Proof,
    ) -> i128 {
        owner.require_auth();

        let null_key = DataKey::Nullifier(nullifier.clone());
        if env.storage().persistent().has(&null_key) {
            panic!("PerpEngine: nullifier already spent");
        }

        let pos_key = DataKey::Position(commitment.clone());
        let mut meta: PositionMeta = env
            .storage()
            .persistent()
            .get(&pos_key)
            .unwrap_or_else(|| panic!("PerpEngine: position not found"));
        if meta.owner != owner {
            panic!("PerpEngine: unauthorized caller for close_position");
        }
        if meta.status != PositionStatus::Matched {
            panic!("PerpEngine: can only close a matched position (status={:?})", meta.status as u32);
        }

        let mut pi: Vec<Bn254Fr> = Vec::new(&env);
        pi.push_back(Bn254Fr::from_bytes(nullifier.clone()));
        let vk = load_vk(&env, &VK_CANCEL_IC);
        match verify_groth16(&env, &vk, &proof, &pi) {
            Ok(true) => {}
            _ => panic!("PerpEngine: invalid close proof"),
        }

        let oracle_price = Self::require_oracle_price(&env);
        let close_price = oracle_price;

        let (settlement, _funding) = Self::compute_settlement_with_funding(
            &env,
            meta.collateral,
            meta.leverage,
            meta.side,
            meta.matched_price,
            close_price,
            meta.funding_at_open,
        );

        meta.status = PositionStatus::Closed;
        meta.matched_price = close_price;
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

        if meta.match_id != 0 {
            Self::try_close_match(&env, meta.match_id, &commitment);
        }

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("close"),),
            (commitment, nullifier, settlement, close_price),
        );

        settlement
    }

    pub fn liquidate(
        env: Env,
        commitment: BytesN<32>,
        liquidator: Address,
    ) -> i128 {
        liquidator.require_auth();

        let pos_key = DataKey::Position(commitment.clone());
        let mut meta: PositionMeta = env
            .storage()
            .persistent()
            .get(&pos_key)
            .unwrap_or_else(|| panic!("PerpEngine: position not found"));

        if meta.status != PositionStatus::Matched {
            panic!("PerpEngine: can only liquidate a matched position (status={:?})", meta.status as u32);
        }

        let oracle_price = Self::require_oracle_price(&env);

        let (settlement, _funding) = Self::compute_settlement_with_funding(
            &env,
            meta.collateral,
            meta.leverage,
            meta.side,
            meta.matched_price,
            oracle_price,
            meta.funding_at_open,
        );

        let mm = meta.collateral * MAINTENANCE_MARGIN as i128 / 10 / meta.leverage as i128;
        if settlement >= mm {
            panic!("PerpEngine: position is not under-collateralized (settlement={}, mm={})", settlement, mm);
        }

        let reward = meta.collateral * LIQUIDATOR_REWARD_NUM / LIQUIDATOR_REWARD_DEN;
        let to_owner = settlement.saturating_sub(reward).max(0);

        let bal = Self::read_balance(&env, &meta.owner);
        env.storage()
            .persistent()
            .set(&DataKey::Balance(meta.owner.clone()), &(bal + to_owner));
        let cfg = Self::config(&env);
        TokenClient::new(&env, &cfg.token)
            .transfer(&env.current_contract_address(), &liquidator, &reward);

        meta.status = PositionStatus::Liquidated;
        meta.matched_price = oracle_price;
        env.storage().persistent().set(&pos_key, &meta);
        env.storage()
            .persistent()
            .extend_ttl(&pos_key, 17280, 17280);

        if meta.match_id != 0 {
            Self::try_close_match(&env, meta.match_id, &commitment);
        }

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("liq"),),
            (commitment, liquidator, oracle_price, reward, to_owner),
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

    pub fn set_price(env: Env, admin: Address, price: u64) {
        admin.require_auth();
        let mut cfg = Self::read_oracle_config(&env).unwrap_or(OracleConfig {
            admin: admin.clone(),
            price: 0,
            last_updated: 0,
            heartbeat: 3600,
        });
        if cfg.admin != admin {
            panic!("PerpEngine: unauthorized oracle admin");
        }
        cfg.price = price;
        cfg.last_updated = env.ledger().sequence() as u64;
        env.storage()
            .persistent()
            .set(&DataKey::OracleConfig, &cfg);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::OracleConfig, 17280, 17280);

        #[allow(deprecated)]
        env.events()
            .publish((soroban_sdk::symbol_short!("price"),), (price, cfg.last_updated));
    }

    pub fn get_price(env: Env) -> Option<u64> {
        Self::read_oracle_config(&env).map(|cfg| cfg.price).filter(|&p| p > 0)
    }

    pub fn get_oracle_config(env: Env) -> Option<OracleConfig> {
        Self::read_oracle_config(&env)
    }

    pub fn get_match_record(env: Env, match_id: u64) -> Option<MatchRecord> {
        env.storage()
            .persistent()
            .get::<_, MatchRecord>(&DataKey::Match(match_id))
    }

    pub fn settle_match(env: Env, admin: Address, match_id: u64) {
        admin.require_auth();
        let mut record: MatchRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Match(match_id))
            .unwrap_or_else(|| panic!("PerpEngine: match {} not found", match_id));
        if record.closed {
            panic!("PerpEngine: match {} already settled", match_id);
        }

        let oracle_price = Self::require_oracle_price(&env);

        let pos_key_a = DataKey::Position(record.cmt_a.clone());
        let pos_key_b = DataKey::Position(record.cmt_b.clone());
        let mut meta_a: PositionMeta = env
            .storage()
            .persistent()
            .get(&pos_key_a)
            .unwrap_or_else(|| panic!("PerpEngine: position A not found for match {}", match_id));
        let mut meta_b: PositionMeta = env
            .storage()
            .persistent()
            .get(&pos_key_b)
            .unwrap_or_else(|| panic!("PerpEngine: position B not found for match {}", match_id));

        if meta_a.status != PositionStatus::Matched {
            panic!("PerpEngine: position A must be matched (status={:?})", meta_a.status as u32);
        }

        let (settlement_a, funding_a) = Self::compute_settlement_with_funding(
            &env,
            meta_a.collateral,
            meta_a.leverage,
            meta_a.side,
            record.match_price,
            oracle_price,
            meta_a.funding_at_open,
        );
        let (settlement_b, funding_b) = Self::compute_settlement_with_funding(
            &env,
            meta_b.collateral,
            meta_b.leverage,
            meta_b.side,
            record.match_price,
            oracle_price,
            meta_b.funding_at_open,
        );

        meta_a.status = PositionStatus::Closed;
        meta_b.status = PositionStatus::Closed;
        meta_a.matched_price = oracle_price;
        meta_b.matched_price = oracle_price;

        env.storage().persistent().set(&pos_key_a, &meta_a);
        env.storage().persistent().set(&pos_key_b, &meta_b);
        env.storage()
            .persistent()
            .extend_ttl(&pos_key_a, 17280, 17280);
        env.storage()
            .persistent()
            .extend_ttl(&pos_key_b, 17280, 17280);

        let bal_a = Self::read_balance(&env, &meta_a.owner);
        env.storage()
            .persistent()
            .set(&DataKey::Balance(meta_a.owner.clone()), &(bal_a + settlement_a));
        let bal_b = Self::read_balance(&env, &meta_b.owner);
        env.storage()
            .persistent()
            .set(&DataKey::Balance(meta_b.owner.clone()), &(bal_b + settlement_b));

        record.closed = true;
        env.storage()
            .persistent()
            .set(&DataKey::Match(match_id), &record);

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("settle"),),
            (
                match_id,
                oracle_price,
                settlement_a,
                settlement_b,
                funding_a,
                funding_b,
            ),
        );
    }

    pub fn update_funding(env: Env, keeper: Address) {
        keeper.require_auth();

        let mut state = Self::read_funding_state(&env);
        let now = env.ledger().sequence() as u64;

        let oracle_price = Self::require_oracle_price(&env);
        let mark_price = Self::derive_mark_price(&env);
        if mark_price == 0 {
            return;
        }

        let premium = (oracle_price as i64) - (mark_price as i64);
        let rate = premium * 100 / (mark_price as i64); // in basis points (0.01%)

        let delta = now.saturating_sub(state.last_update);
        if delta < FUNDING_INTERVAL / 2 {
            return;
        }

        let payment = (rate as i128) * (delta as i128) / (FUNDING_INTERVAL as i128);
        state.cumulative = state.cumulative.wrapping_add(payment);

        let elapsed_ledgers = delta;
        state.last_update = now;
        env.storage().persistent().set(&DataKey::FundingState, &state);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::FundingState, 17280, 17280);

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("funding"),),
            (rate, payment, elapsed_ledgers, state.cumulative),
        );
    }

    pub fn get_funding_state(env: Env) -> FundingState {
        Self::read_funding_state(&env)
    }

    pub fn get_next_match_id(env: Env) -> u64 {
        Self::next_match_id(&env)
    }

    // ---- helper functions ----

    fn config(env: &Env) -> Config {
        env.storage()
            .instance()
            .get(&DataKey::Config)
            .unwrap_or_else(|| panic!("PerpEngine: not initialized"))
    }

    fn next_match_id(env: &Env) -> u64 {
        let id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextMatchId)
            .unwrap_or(1);
        env.storage()
            .instance()
            .set(&DataKey::NextMatchId, &(id + 1));
        id
    }

    fn read_oracle_config(env: &Env) -> Option<OracleConfig> {
        env.storage()
            .persistent()
            .get::<_, OracleConfig>(&DataKey::OracleConfig)
    }

    fn require_oracle_price(env: &Env) -> u64 {
        let cfg = Self::read_oracle_config(env).unwrap_or_else(|| panic!("PerpEngine: oracle not initialized"));
        if cfg.price == 0 {
            panic!("PerpEngine: oracle price not set");
        }
        let now = env.ledger().sequence() as u64;
        if now.saturating_sub(cfg.last_updated) > cfg.heartbeat {
            panic!("PerpEngine: oracle price stale (last_updated={}, heartbeat={})", cfg.last_updated, cfg.heartbeat);
        }
        cfg.price
    }

    fn read_funding_state(env: &Env) -> FundingState {
        env.storage()
            .persistent()
            .get(&DataKey::FundingState)
            .unwrap_or(FundingState {
                last_update: 0,
                cumulative: 0,
                rate: 0,
            })
    }

    fn read_funding_cumulative(env: &Env) -> i128 {
        Self::read_funding_state(env).cumulative
    }

    fn compute_settlement_with_funding(
        env: &Env,
        collateral: i128,
        leverage: u64,
        side: u64,
        entry_price: u64,
        close_price: u64,
        funding_at_open: i128,
    ) -> (i128, i128) {
        if entry_price == 0 {
            return (collateral, 0);
        }

        let entry = entry_price as i128;
        let close = close_price as i128;
        let lev = leverage as i128;

        let price_delta = close - entry;
        let raw_pnl = collateral * lev * price_delta / entry;

        let signed_pnl = if side == 1 { -raw_pnl } else { raw_pnl };

        let funding_now = Self::read_funding_cumulative(env);
        let funding_delta = funding_now.wrapping_sub(funding_at_open);
        let funding_payment = funding_delta * collateral / 1_000_000;

        let scaled_funding = if side == 1 {
            -funding_payment
        } else {
            funding_payment
        };

        let total = collateral + signed_pnl + scaled_funding;
        let max_gain = collateral * (lev + 1);
        (total.max(0).min(max_gain), funding_payment)
    }

    fn try_close_match(env: &Env, match_id: u64, commitment: &BytesN<32>) {
        let mut record: MatchRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Match(match_id))
            .unwrap_or_else(|| panic!("PerpEngine: match {} not found in try_close_match", match_id));
        if record.closed {
            return;
        }

        let other_cmt = if record.cmt_a == *commitment {
            &record.cmt_b
        } else {
            &record.cmt_a
        };

        let other_meta: Option<PositionMeta> = env
            .storage()
            .persistent()
            .get(&DataKey::Position(other_cmt.clone()));

        if let Some(other) = other_meta {
            if other.status != PositionStatus::Closed
                && other.status != PositionStatus::Liquidated
            {
                return;
            }
        }

        record.closed = true;
        env.storage()
            .persistent()
            .set(&DataKey::Match(match_id), &record);
    }

    fn derive_mark_price(env: &Env) -> u64 {
        // Use oracle price as mark price for now
        Self::read_oracle_config(env).map(|c| c.price).unwrap_or(0)
    }
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger, LedgerInfo};
    use soroban_sdk::token::StellarAssetClient;
    use std::panic::{self, AssertUnwindSafe};

    fn setup() -> (Env, Address, Address) {
        let env = Env::default();
        let admin = Address::generate(&env);
        let contract_id = env.register(PerpEngine, ());
        let token = env.register_stellar_asset_contract_v2(admin.clone());
        env.mock_all_auths();
        let client = PerpEngineClient::new(&env, &contract_id);
        client.initialize(&admin, &token.address());
        (env, contract_id, admin)
    }

    fn create_position(
        env: &Env,
        cid: &Address,
        owner: &Address,
        commitment: &BytesN<32>,
        collateral: i128,
        leverage: u64,
        side: u64,
        price: u64,
        status: PositionStatus,
        match_id: u64,
    ) {
        env.as_contract(cid, || {
            let meta = PositionMeta {
                owner: owner.clone(),
                collateral,
                entry_price: price,
                matched_price: if status == PositionStatus::Matched { price } else { 0 },
                side,
                leverage,
                status,
                created_at: env.ledger().sequence() as u64,
                match_id,
                funding_at_open: 0,
            };
            let key = DataKey::Position(commitment.clone());
            env.storage().persistent().set(&key, &meta);
            env.storage().persistent().extend_ttl(&key, 17280, 17280);
        });
    }

    fn create_match_record(
        env: &Env,
        cid: &Address,
        match_id: u64,
        cmt_a: &BytesN<32>,
        cmt_b: &BytesN<32>,
        price: u64,
        size: u64,
    ) {
        env.as_contract(cid, || {
            let record = MatchRecord {
                cmt_a: cmt_a.clone(),
                cmt_b: cmt_b.clone(),
                match_price: price,
                match_size: size,
                matched_at: 0,
                closed: false,
            };
            env.storage()
                .persistent()
                .set(&DataKey::Match(match_id), &record);
            env.storage()
                .persistent()
                .extend_ttl(&DataKey::Match(match_id), 17280, 17280);
            env.storage()
                .instance()
                .set(&DataKey::NextMatchId, &(match_id + 1));
        });
    }

    fn default_ledger_info() -> LedgerInfo {
        LedgerInfo {
            protocol_version: 27,
            sequence_number: 0,
            timestamp: 0,
            network_id: [0; 32],
            base_reserve: 0,
            min_persistent_entry_ttl: 4096,
            min_temp_entry_ttl: 16,
            max_entry_ttl: 6_312_000,
        }
    }

    #[test]
    fn test_initialize_sets_config() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(admin.clone());
        env.mock_all_auths();
        let contract_id = env.register(PerpEngine, ());
        let client = PerpEngineClient::new(&env, &contract_id);
        client.initialize(&admin, &token.address());
        let cfg = client.get_config();
        assert_eq!(cfg.admin, admin);
        assert_eq!(cfg.token, token.address());
    }

    #[test]
    fn test_oracle_set_and_get_price() {
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        assert_eq!(client.get_price(), None);

        client.set_price(&admin, &100_000_000);
        assert_eq!(client.get_price(), Some(100_000_000));

        client.set_price(&admin, &0);
        assert_eq!(client.get_price(), None);

        client.set_price(&admin, &200_000_000);
        assert_eq!(client.get_price(), Some(200_000_000));

        let cfg = client.get_oracle_config().unwrap();
        assert_eq!(cfg.price, 200_000_000);
        assert_eq!(cfg.admin, admin);
        assert_eq!(cfg.heartbeat, 3600);
    }

    #[test]
    fn test_oracle_unauthorized_set_price() {
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let attacker = Address::generate(&env);
        // First set price via admin to establish oracle config
        client.set_price(&admin, &100_000_000);
        // Attacker tries to change — should fail (cfg.admin != attacker)
        env.mock_all_auths();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.set_price(&attacker, &999);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_oracle_stale_price_get_price_still_works() {
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        env.ledger().set(default_ledger_info());
        client.set_price(&admin, &100_000_000);

        env.ledger().set(LedgerInfo {
            sequence_number: 5000,
            timestamp: 0,
            network_id: [0; 32],
            protocol_version: 27,
            base_reserve: 0,
            min_persistent_entry_ttl: 4096,
            min_temp_entry_ttl: 16,
            max_entry_ttl: 6_312_000,
        });

        assert_eq!(client.get_price(), Some(100_000_000));
        client.set_price(&admin, &200_000_000);
        assert_eq!(client.get_price(), Some(200_000_000));
    }

    #[test]
    fn test_oracle_config_defaults() {
        let (env, cid, _admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        assert!(client.get_oracle_config().is_none());
    }

    #[test]
    fn test_settle_match_flow() {
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);

        let owner_a = Address::generate(&env);
        let owner_b = Address::generate(&env);
        let cmt_a = BytesN::from_array(&env, &[1u8; 32]);
        let cmt_b = BytesN::from_array(&env, &[2u8; 32]);
        let match_id = 1;

        client.set_price(&admin, &1_000_000_000);

        create_position(
            &env, &cid, &owner_a, &cmt_a, 100_000_000, 10, 0,
            1_000_000_000, PositionStatus::Matched, match_id,
        );
        create_position(
            &env, &cid, &owner_b, &cmt_b, 100_000_000, 10, 1,
            1_000_000_000, PositionStatus::Matched, match_id,
        );
        create_match_record(&env, &cid, match_id, &cmt_a, &cmt_b, 1_000_000_000, 100);

        env.mock_all_auths();
        client.settle_match(&admin, &match_id);

        let pos_a = client.get_position(&cmt_a).unwrap();
        assert_eq!(pos_a.status, PositionStatus::Closed);
        let pos_b = client.get_position(&cmt_b).unwrap();
        assert_eq!(pos_b.status, PositionStatus::Closed);

        let rec = client.get_match_record(&match_id).unwrap();
        assert!(rec.closed);

        env.mock_all_auths();
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            client.settle_match(&admin, &match_id);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_liquidate_underwater_position() {
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);

        let owner = Address::generate(&env);
        let liquidator = Address::generate(&env);
        let cmt = BytesN::from_array(&env, &[42u8; 32]);

        // Fund the contract with tokens so it can pay liquidator reward
        let cfg = client.get_config();
        let token_admin = StellarAssetClient::new(&env, &cfg.token);
        token_admin.mint(&cid, &100_000_000);

        client.set_price(&admin, &50_000_000);

        create_position(
            &env, &cid, &owner, &cmt, 100_000_000, 10, 0,
            100_000_000, PositionStatus::Matched, 0,
        );

        env.mock_all_auths();
        let settlement = client.liquidate(&cmt, &liquidator);
        assert_eq!(settlement, 0);

        let pos = client.get_position(&cmt).unwrap();
        assert_eq!(pos.status, PositionStatus::Liquidated);
    }

    #[test]
    fn test_liquidate_solvent_position_reverts() {
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);

        let owner = Address::generate(&env);
        let liquidator = Address::generate(&env);
        let cmt = BytesN::from_array(&env, &[42u8; 32]);

        client.set_price(&admin, &150_000_000);

        create_position(
            &env, &cid, &owner, &cmt, 100_000_000, 10, 0,
            100_000_000, PositionStatus::Matched, 0,
        );

        env.mock_all_auths();
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            client.liquidate(&cmt, &liquidator);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_liquidate_open_position_reverts() {
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);

        let owner = Address::generate(&env);
        let liquidator = Address::generate(&env);
        let cmt = BytesN::from_array(&env, &[42u8; 32]);

        client.set_price(&admin, &50_000_000);

        create_position(
            &env, &cid, &owner, &cmt, 100_000_000, 10, 0,
            100_000_000, PositionStatus::Open, 0,
        );

        env.mock_all_auths();
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            client.liquidate(&cmt, &liquidator);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_liquidate_no_oracle_reverts() {
        let (env, cid, _admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);

        let owner = Address::generate(&env);
        let liquidator = Address::generate(&env);
        let cmt = BytesN::from_array(&env, &[42u8; 32]);

        create_position(
            &env, &cid, &owner, &cmt, 100_000_000, 10, 0,
            100_000_000, PositionStatus::Matched, 0,
        );

        env.mock_all_auths();
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            client.liquidate(&cmt, &liquidator);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_funding_update() {
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        client.set_price(&admin, &1_000_000_000);

        let state = client.get_funding_state();
        assert_eq!(state.cumulative, 0);
        assert_eq!(state.rate, 0);

        env.ledger().set(LedgerInfo {
            sequence_number: 1000,
            timestamp: 0,
            network_id: [0; 32],
            protocol_version: 27,
            base_reserve: 0,
            min_persistent_entry_ttl: 4096,
            min_temp_entry_ttl: 16,
            max_entry_ttl: 6_312_000,
        });

        let keeper = Address::generate(&env);
        client.update_funding(&keeper);

        let state = client.get_funding_state();
        assert_eq!(state.rate, 0);
        assert_eq!(state.cumulative, 0);
    }

    #[test]
    fn test_funding_skips_if_too_soon() {
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        client.set_price(&admin, &1_000_000_000);

        env.ledger().set(LedgerInfo {
            sequence_number: 10,
            timestamp: 0,
            network_id: [0; 32],
            protocol_version: 27,
            base_reserve: 0,
            min_persistent_entry_ttl: 4096,
            min_temp_entry_ttl: 16,
            max_entry_ttl: 6_312_000,
        });

        let keeper = Address::generate(&env);
        client.update_funding(&keeper);

        let state = client.get_funding_state();
        assert_eq!(state.last_update, 0);
    }

    #[test]
    fn test_get_match_record_nonexistent() {
        let (env, cid, _admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        assert!(client.get_match_record(&999).is_none());
    }
}
