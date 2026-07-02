#![no_std]
#![allow(clippy::too_many_arguments)]

use soroban_sdk::{
    contract, contractimpl, contracttype,
    crypto::bn254::{Bn254Fr, Bn254G1Affine as G1Affine, Bn254G2Affine as G2Affine},
    token::TokenClient,
    Address, Bytes, BytesN, Env, Vec,
};
use types::{
    FundingState, Groth16Error, Groth16Proof, MatchRecord, OracleConfig, PriceSample, TimeInForce,
};

include!(concat!(env!("OUT_DIR"), "/vk.rs"));

const FUNDING_INTERVAL: u64 = 5760; // ~8 hours at 5s/ledger; standard perp funding period
const TWAP_WINDOW: u64 = 8; // number of price samples for TWAP (covers ~8 heartbeat periods)
const MAX_FUNDING_RATE_BPS: i64 = 75; // ±0.75% cap per funding interval (industry standard)
const MAX_PRICE_DEVIATION_BPS: u64 = 5000; // new price must be within 50% of TWAP
#[allow(dead_code)]
const MAINTENANCE_MARGIN_BPS: i128 = 500; // 5% of notional
#[allow(dead_code)]
const PARTIAL_REWARD_BPS: i128 = 100; // 1% of freed half-collateral → liquidator
#[allow(dead_code)]
const FULL_REWARD_BPS: i128 = 150; // 1.5% of remaining collateral → liquidator
#[allow(dead_code)]
const INS_FUND_BPS: i128 = 50; // 0.5% of remaining collateral → insurance fund

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

fn field_to_u64(b: &BytesN<32>) -> u64 {
    let arr = b.to_array();
    u64::from_be_bytes([
        arr[24], arr[25], arr[26], arr[27], arr[28], arr[29], arr[30], arr[31],
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
    Note(BytesN<32>),
    MarkPrice,
    InsuranceFund,
    BadDebt,
    TwapSample(u64),            // ring buffer slot 0..TWAP_WINDOW-1 → PriceSample
    TwapHead,                   // next write position in ring buffer
    PortfolioGroup(BytesN<32>), // portfolio_key → Vec<BytesN<32>> of member commitments
    AssetConfig(BytesN<32>),
    AssetOracle(BytesN<32>),
    AssetTwapSample(BytesN<32>, u64),
    AssetTwapHead(BytesN<32>),
    AssetName(BytesN<32>),
    AssetList, // Vec<BytesN<32>> of all registered asset IDs
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum MarginMode {
    Isolated = 0,
    Cross = 1,
}

#[contracttype]
#[derive(Clone)]
pub struct Config {
    pub admin: Address,
    pub token: Address,
    /// When set, collateral is held in a CollateralVault instead of this contract.
    pub vault: Option<Address>,
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
    pub collateral: i128,
    pub entry_price: u64,
    pub matched_price: u64,
    pub side: u64,
    pub leverage: u64,
    pub status: PositionStatus,
    pub created_at: u64,
    pub match_id: u64,
    pub funding_at_open: i128,
    pub hint_size: u64,
    pub tif: TimeInForce,
    pub expiry_ledger: u64,
    pub tp_price: u64,
    pub sl_price: u64,
    pub effective_collateral: i128,
    pub partial_liq_done: bool,
    pub liquidation_recipient_note: BytesN<32>,
    pub asset_id: BytesN<32>,
    pub margin_mode: MarginMode,
    pub portfolio_key: BytesN<32>,
}

#[contract]
pub struct PerpEngine;

#[contractimpl]
impl PerpEngine {
    pub fn initialize(env: Env, admin: Address, token: Address, vault: Option<Address>) {
        if env.storage().instance().has(&DataKey::Config) {
            panic!("PerpEngine: already initialized");
        }
        env.storage().instance().set(
            &DataKey::Config,
            &Config {
                admin: admin.clone(),
                token: token.clone(),
                vault: vault.clone(),
            },
        );

        #[allow(deprecated)]
        env.events()
            .publish((soroban_sdk::symbol_short!("init"),), ());
    }

    fn add_to_portfolio(env: &Env, portfolio_key: &BytesN<32>, commitment: &BytesN<32>) {
        let key = DataKey::PortfolioGroup(portfolio_key.clone());
        let mut group: soroban_sdk::Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| soroban_sdk::Vec::new(env));
        group.push_back(commitment.clone());
        env.storage().persistent().set(&key, &group);
        env.storage().persistent().extend_ttl(&key, 17280, 17280);
    }

    fn remove_from_portfolio(env: &Env, portfolio_key: &BytesN<32>, commitment: &BytesN<32>) {
        let key = DataKey::PortfolioGroup(portfolio_key.clone());
        let group: soroban_sdk::Vec<BytesN<32>> = match env.storage().persistent().get(&key) {
            Some(g) => g,
            None => return,
        };
        let mut new_group: soroban_sdk::Vec<BytesN<32>> = soroban_sdk::Vec::new(env);
        for i in 0..group.len() {
            let cmt = group.get(i).unwrap();
            if cmt != *commitment {
                new_group.push_back(cmt);
            }
        }
        env.storage().persistent().set(&key, &new_group);
        env.storage().persistent().extend_ttl(&key, 17280, 17280);
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
            panic!(
                "PerpEngine: both positions must be open (A={:?}, B={:?})",
                meta_a.status as u32, meta_b.status as u32
            );
        }

        let exec_size = field_to_u64(&match_size);
        let now = env.ledger().sequence() as u64;

        // FOK/IOC: match size must equal the position's requested size
        if (meta_a.tif == TimeInForce::FOK || meta_a.tif == TimeInForce::IOC)
            && exec_size != meta_a.hint_size
        {
            panic!(
                "PerpEngine: FOK/IOC order A requires full fill (wanted={}, got={})",
                meta_a.hint_size, exec_size
            );
        }
        if (meta_b.tif == TimeInForce::FOK || meta_b.tif == TimeInForce::IOC)
            && exec_size != meta_b.hint_size
        {
            panic!(
                "PerpEngine: FOK/IOC order B requires full fill (wanted={}, got={})",
                meta_b.hint_size, exec_size
            );
        }
        // GTD: reject if past expiry
        if meta_a.tif == TimeInForce::GTD && meta_a.expiry_ledger > 0 && now > meta_a.expiry_ledger
        {
            panic!(
                "PerpEngine: order A has expired (expiry={}, now={})",
                meta_a.expiry_ledger, now
            );
        }
        if meta_b.tif == TimeInForce::GTD && meta_b.expiry_ledger > 0 && now > meta_b.expiry_ledger
        {
            panic!(
                "PerpEngine: order B has expired (expiry={}, now={})",
                meta_b.expiry_ledger, now
            );
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

    /// Deposit tokens and record a shielded note commitment (no address stored).
    /// note_commitment = Poseidon2(amount, secret) — computed client-side.
    pub fn deposit_note(env: Env, from: Address, note_commitment: BytesN<32>, amount: i128) {
        from.require_auth();
        if amount <= 0 {
            panic!("PerpEngine: deposit amount must be positive");
        }
        let note_key = DataKey::Note(note_commitment.clone());
        if env.storage().persistent().has(&note_key) {
            panic!("PerpEngine: note commitment already exists");
        }
        let cfg = Self::config(&env);
        TokenClient::new(&env, &cfg.token).transfer(&from, env.current_contract_address(), &amount);
        env.storage().persistent().set(&note_key, &amount);
        env.storage()
            .persistent()
            .extend_ttl(&note_key, 17280, 17280);

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("dep_note"),),
            (note_commitment, amount),
        );
    }

    /// Withdraw a shielded note to any recipient by proving knowledge of the secret.
    /// Proof: NoteSpend — public inputs [note_commitment, nullifier].
    pub fn withdraw_note(
        env: Env,
        note_commitment: BytesN<32>,
        nullifier: BytesN<32>,
        recipient: Address,
        proof: Groth16Proof,
    ) {
        let null_key = DataKey::Nullifier(nullifier.clone());
        if env.storage().persistent().has(&null_key) {
            panic!("PerpEngine: nullifier already spent");
        }
        let note_key = DataKey::Note(note_commitment.clone());
        let amount: i128 = env
            .storage()
            .persistent()
            .get(&note_key)
            .unwrap_or_else(|| panic!("PerpEngine: note not found"));

        let mut pi: Vec<Bn254Fr> = Vec::new(&env);
        pi.push_back(Bn254Fr::from_bytes(note_commitment.clone()));
        pi.push_back(Bn254Fr::from_bytes(nullifier.clone()));
        let vk = load_vk(&env, &VK_NOTE_SPEND_IC);
        match verify_groth16(&env, &vk, &proof, &pi) {
            Ok(true) => {}
            _ => panic!("PerpEngine: invalid note spend proof"),
        }

        env.storage().persistent().set(&null_key, &true);
        env.storage()
            .persistent()
            .extend_ttl(&null_key, 17280, 17280);

        let cfg = Self::config(&env);
        TokenClient::new(&env, &cfg.token).transfer(
            &env.current_contract_address(),
            &recipient,
            &amount,
        );

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("wdraw_n"),),
            (note_commitment, nullifier, amount),
        );
    }

    /// Add margin to a position by spending a shielded note (no address linkage).
    /// Proof: NoteSpend — public inputs [note_commitment, nullifier].
    pub fn add_margin_from_note(
        env: Env,
        note_commitment: BytesN<32>,
        nullifier: BytesN<32>,
        position_commitment: BytesN<32>,
        proof: Groth16Proof,
    ) {
        let null_key = DataKey::Nullifier(nullifier.clone());
        if env.storage().persistent().has(&null_key) {
            panic!("PerpEngine: nullifier already spent");
        }
        let note_key = DataKey::Note(note_commitment.clone());
        let amount: i128 = env
            .storage()
            .persistent()
            .get(&note_key)
            .unwrap_or_else(|| panic!("PerpEngine: note not found"));

        let mut pi: Vec<Bn254Fr> = Vec::new(&env);
        pi.push_back(Bn254Fr::from_bytes(note_commitment.clone()));
        pi.push_back(Bn254Fr::from_bytes(nullifier.clone()));
        let vk = load_vk(&env, &VK_NOTE_SPEND_IC);
        match verify_groth16(&env, &vk, &proof, &pi) {
            Ok(true) => {}
            _ => panic!("PerpEngine: invalid note spend proof"),
        }

        env.storage().persistent().set(&null_key, &true);
        env.storage()
            .persistent()
            .extend_ttl(&null_key, 17280, 17280);

        let pos_key = DataKey::Position(position_commitment.clone());
        let mut meta: PositionMeta = env
            .storage()
            .persistent()
            .get(&pos_key)
            .unwrap_or_else(|| panic!("PerpEngine: position not found"));
        if meta.status != PositionStatus::Open && meta.status != PositionStatus::Matched {
            panic!("PerpEngine: can only add margin to an open or matched position");
        }

        meta.collateral += amount;
        env.storage().persistent().set(&pos_key, &meta);
        env.storage()
            .persistent()
            .extend_ttl(&pos_key, 17280, 17280);

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("mgn_note"),),
            (
                note_commitment,
                nullifier,
                position_commitment,
                amount,
                meta.collateral,
            ),
        );
    }

    pub fn get_note(env: Env, note_commitment: BytesN<32>) -> Option<i128> {
        env.storage()
            .persistent()
            .get::<_, i128>(&DataKey::Note(note_commitment))
    }

    /// Trigger a take-profit close. Callable by anyone — keeper pattern.
    /// Long (side=0): triggers when oracle >= tp_price.
    /// Short (side=1): triggers when oracle <= tp_price.
    pub fn trigger_tp(env: Env, commitment: BytesN<32>) -> i128 {
        let pos_key = DataKey::Position(commitment.clone());
        let mut meta: PositionMeta = env
            .storage()
            .persistent()
            .get(&pos_key)
            .unwrap_or_else(|| panic!("PerpEngine: position not found"));
        if meta.status != PositionStatus::Matched {
            panic!("PerpEngine: can only trigger TP on a matched position");
        }
        if meta.tp_price == 0 {
            panic!("PerpEngine: no TP price set");
        }
        let oracle_price = Self::require_oracle_price(&env);
        let triggered = if meta.side == 0 {
            oracle_price >= meta.tp_price
        } else {
            oracle_price <= meta.tp_price
        };
        if !triggered {
            panic!(
                "PerpEngine: TP not triggered (side={} oracle={} tp={})",
                meta.side, oracle_price, meta.tp_price
            );
        }
        let (settlement, _) = Self::compute_settlement_with_funding(
            &env,
            meta.effective_collateral,
            meta.leverage,
            meta.side,
            meta.matched_price,
            oracle_price,
            meta.funding_at_open,
        );
        meta.status = PositionStatus::Closed;
        meta.matched_price = oracle_price;
        env.storage().persistent().set(&pos_key, &meta);
        env.storage()
            .persistent()
            .extend_ttl(&pos_key, 17280, 17280);
        if settlement > 0 {
            let note_key = DataKey::Note(meta.liquidation_recipient_note.clone());
            let existing: i128 = env.storage().persistent().get(&note_key).unwrap_or(0);
            env.storage()
                .persistent()
                .set(&note_key, &(existing + settlement));
            env.storage()
                .persistent()
                .extend_ttl(&note_key, 17280, 17280);
        }
        if meta.match_id != 0 {
            Self::try_close_match(&env, meta.match_id, &commitment);
        }
        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("trig_tp"),),
            (commitment, oracle_price, meta.tp_price, settlement),
        );
        settlement
    }

    /// Trigger a stop-loss close. Callable by anyone — keeper pattern.
    /// Long (side=0): triggers when oracle <= sl_price.
    /// Short (side=1): triggers when oracle >= sl_price.
    pub fn trigger_sl(env: Env, commitment: BytesN<32>) -> i128 {
        let pos_key = DataKey::Position(commitment.clone());
        let mut meta: PositionMeta = env
            .storage()
            .persistent()
            .get(&pos_key)
            .unwrap_or_else(|| panic!("PerpEngine: position not found"));
        if meta.status != PositionStatus::Matched {
            panic!("PerpEngine: can only trigger SL on a matched position");
        }
        if meta.sl_price == 0 {
            panic!("PerpEngine: no SL price set");
        }
        let oracle_price = Self::require_oracle_price(&env);
        let triggered = if meta.side == 0 {
            oracle_price <= meta.sl_price
        } else {
            oracle_price >= meta.sl_price
        };
        if !triggered {
            panic!(
                "PerpEngine: SL not triggered (side={} oracle={} sl={})",
                meta.side, oracle_price, meta.sl_price
            );
        }
        let (settlement, _) = Self::compute_settlement_with_funding(
            &env,
            meta.effective_collateral,
            meta.leverage,
            meta.side,
            meta.matched_price,
            oracle_price,
            meta.funding_at_open,
        );
        meta.status = PositionStatus::Closed;
        meta.matched_price = oracle_price;
        env.storage().persistent().set(&pos_key, &meta);
        env.storage()
            .persistent()
            .extend_ttl(&pos_key, 17280, 17280);
        if settlement > 0 {
            let note_key = DataKey::Note(meta.liquidation_recipient_note.clone());
            let existing: i128 = env.storage().persistent().get(&note_key).unwrap_or(0);
            env.storage()
                .persistent()
                .set(&note_key, &(existing + settlement));
            env.storage()
                .persistent()
                .extend_ttl(&note_key, 17280, 17280);
        }
        if meta.match_id != 0 {
            Self::try_close_match(&env, meta.match_id, &commitment);
        }
        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("trig_sl"),),
            (commitment, oracle_price, meta.sl_price, settlement),
        );
        settlement
    }

    /// Open a position by spending a shielded note as collateral.
    /// Requires a NoteSpend proof [note_commitment, note_nullifier] and
    /// an OrderCommitment proof [position_commitment]. No address auth — the
    /// ZK proofs are the sole authorization.
    pub fn open_position_from_note(
        env: Env,
        note_commitment: BytesN<32>,
        note_nullifier: BytesN<32>,
        position_commitment: BytesN<32>,
        hint_price: u64,
        hint_side: u64,
        hint_leverage: u64,
        hint_size: u64,
        tif: TimeInForce,
        expiry_ledger: u64,
        tp_price: u64,
        sl_price: u64,
        liquidation_recipient_note: BytesN<32>,
        portfolio_key: BytesN<32>, // zeros=isolated, non-zero=cross (Poseidon2(secret))
        asset_id: BytesN<32>,
        note_proof: Groth16Proof,
        commit_proof: Groth16Proof,
    ) {
        if hint_side > 1 {
            panic!(
                "PerpEngine: side must be 0 (long) or 1 (short), got {}",
                hint_side
            );
        }
        if hint_leverage == 0 {
            panic!("PerpEngine: leverage must be >= 1");
        }
        if tif == TimeInForce::GTD && expiry_ledger == 0 {
            panic!("PerpEngine: GTD requires expiry_ledger > 0");
        }
        if tif != TimeInForce::GTD && expiry_ledger != 0 {
            panic!("PerpEngine: expiry_ledger only valid for GTD");
        }
        if tp_price > 0 && sl_price > 0 {
            if hint_side == 0 && tp_price <= sl_price {
                panic!("PerpEngine: long TP must be above SL");
            }
            if hint_side == 1 && tp_price >= sl_price {
                panic!("PerpEngine: short TP must be below SL");
            }
        }
        // Validate asset is registered and active
        let asset_cfg = env
            .storage()
            .persistent()
            .get::<_, types::AssetConfig>(&DataKey::AssetConfig(asset_id.clone()))
            .unwrap_or_else(|| panic!("PerpEngine: asset not registered"));
        if !asset_cfg.active {
            panic!("PerpEngine: asset is not active");
        }
        if hint_leverage > asset_cfg.max_leverage {
            panic!(
                "PerpEngine: leverage exceeds asset max ({} > {})",
                hint_leverage, asset_cfg.max_leverage
            );
        }

        let note_null_key = DataKey::Nullifier(note_nullifier.clone());
        if env.storage().persistent().has(&note_null_key) {
            panic!("PerpEngine: note nullifier already spent");
        }

        let note_key = DataKey::Note(note_commitment.clone());
        let collateral: i128 = env
            .storage()
            .persistent()
            .get(&note_key)
            .unwrap_or_else(|| panic!("PerpEngine: note not found"));

        let pos_key = DataKey::Position(position_commitment.clone());
        if env.storage().persistent().has(&pos_key) {
            panic!("PerpEngine: commitment already exists");
        }

        let mut note_pi: Vec<Bn254Fr> = Vec::new(&env);
        note_pi.push_back(Bn254Fr::from_bytes(note_commitment.clone()));
        note_pi.push_back(Bn254Fr::from_bytes(note_nullifier.clone()));
        let note_vk = load_vk(&env, &VK_NOTE_SPEND_IC);
        match verify_groth16(&env, &note_vk, &note_proof, &note_pi) {
            Ok(true) => {}
            _ => panic!("PerpEngine: invalid note spend proof"),
        }

        let mut commit_pi: Vec<Bn254Fr> = Vec::new(&env);
        commit_pi.push_back(Bn254Fr::from_bytes(position_commitment.clone()));
        commit_pi.push_back(Bn254Fr::from_bytes(portfolio_key.clone()));
        let commit_vk = load_vk(&env, &VK_COMMIT_IC);
        match verify_groth16(&env, &commit_vk, &commit_proof, &commit_pi) {
            Ok(true) => {}
            _ => panic!("PerpEngine: invalid commitment proof"),
        }

        let zero32 = BytesN::from_array(&env, &[0u8; 32]);
        let margin_mode = if portfolio_key != zero32 {
            MarginMode::Cross
        } else {
            MarginMode::Isolated
        };
        if margin_mode == MarginMode::Cross {
            Self::add_to_portfolio(&env, &portfolio_key, &position_commitment);
        }

        env.storage().persistent().set(&note_null_key, &true);
        env.storage()
            .persistent()
            .extend_ttl(&note_null_key, 17280, 17280);

        let created_at = env.ledger().sequence() as u64;
        let meta = PositionMeta {
            collateral,
            entry_price: hint_price,
            matched_price: 0,
            side: hint_side,
            leverage: hint_leverage,
            status: PositionStatus::Open,
            created_at,
            match_id: 0,
            funding_at_open: Self::read_funding_cumulative(&env),
            hint_size,
            tif,
            expiry_ledger,
            tp_price,
            sl_price,
            effective_collateral: collateral,
            partial_liq_done: false,
            liquidation_recipient_note,
            asset_id,
            margin_mode,
            portfolio_key,
        };
        env.storage().persistent().set(&pos_key, &meta);
        env.storage()
            .persistent()
            .extend_ttl(&pos_key, 17280, 17280);

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("open_n"),),
            (
                note_commitment,
                note_nullifier,
                position_commitment,
                collateral,
                hint_side,
                hint_leverage,
                hint_price,
                created_at,
            ),
        );
    }

    /// Cancel an open position and refund collateral to a shielded note.
    /// No `require_auth` — the cancel ZK proof [cancel_nullifier] is the
    /// sole authorization. recipient_note = Poseidon2(0, note_secret, 8);
    /// withdraw later via `withdraw_note` with `prove_note_spend(0, note_secret)`.
    pub fn cancel_position_to_note(
        env: Env,
        position_commitment: BytesN<32>,
        cancel_nullifier: BytesN<32>,
        recipient_note: BytesN<32>,
        cancel_proof: Groth16Proof,
    ) {
        let null_key = DataKey::Nullifier(cancel_nullifier.clone());
        if env.storage().persistent().has(&null_key) {
            panic!("PerpEngine: nullifier already spent");
        }

        let pos_key = DataKey::Position(position_commitment.clone());
        let mut meta: PositionMeta = env
            .storage()
            .persistent()
            .get(&pos_key)
            .unwrap_or_else(|| panic!("PerpEngine: position not found"));
        if meta.status != PositionStatus::Open {
            panic!(
                "PerpEngine: can only cancel an open position (status={:?})",
                meta.status as u32
            );
        }
        let mut pi: Vec<Bn254Fr> = Vec::new(&env);
        pi.push_back(Bn254Fr::from_bytes(cancel_nullifier.clone()));
        let vk = load_vk(&env, &VK_CANCEL_IC);
        match verify_groth16(&env, &vk, &cancel_proof, &pi) {
            Ok(true) => {}
            _ => panic!("PerpEngine: invalid cancel proof"),
        }

        if meta.margin_mode == MarginMode::Cross {
            Self::remove_from_portfolio(&env, &meta.portfolio_key, &position_commitment);
        }
        meta.status = PositionStatus::Cancelled;
        env.storage().persistent().set(&pos_key, &meta);
        env.storage().persistent().set(&null_key, &true);
        env.storage()
            .persistent()
            .extend_ttl(&pos_key, 17280, 17280);
        env.storage()
            .persistent()
            .extend_ttl(&null_key, 17280, 17280);

        let note_key = DataKey::Note(recipient_note.clone());
        if env.storage().persistent().has(&note_key) {
            panic!("PerpEngine: recipient note already exists");
        }
        let refund = meta.collateral;
        env.storage().persistent().set(&note_key, &refund);
        env.storage()
            .persistent()
            .extend_ttl(&note_key, 17280, 17280);

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("cxl_n"),),
            (
                position_commitment,
                cancel_nullifier,
                recipient_note,
                refund,
            ),
        );
    }

    /// Close a matched position and credit settlement to a shielded note.
    /// No `require_auth` — the close ZK proof [close_nullifier] is the
    /// sole authorization. recipient_note = Poseidon2(0, note_secret, 8);
    /// withdraw later via `withdraw_note` with `prove_note_spend(0, note_secret)`.
    pub fn close_position_to_note(
        env: Env,
        position_commitment: BytesN<32>,
        close_nullifier: BytesN<32>,
        recipient_note: BytesN<32>,
        close_proof: Groth16Proof,
    ) -> i128 {
        let null_key = DataKey::Nullifier(close_nullifier.clone());
        if env.storage().persistent().has(&null_key) {
            panic!("PerpEngine: nullifier already spent");
        }

        let pos_key = DataKey::Position(position_commitment.clone());
        let mut meta: PositionMeta = env
            .storage()
            .persistent()
            .get(&pos_key)
            .unwrap_or_else(|| panic!("PerpEngine: position not found"));
        if meta.status != PositionStatus::Matched {
            panic!(
                "PerpEngine: can only close a matched position (status={:?})",
                meta.status as u32
            );
        }
        let mut pi: Vec<Bn254Fr> = Vec::new(&env);
        pi.push_back(Bn254Fr::from_bytes(close_nullifier.clone()));
        let vk = load_vk(&env, &VK_CANCEL_IC);
        match verify_groth16(&env, &vk, &close_proof, &pi) {
            Ok(true) => {}
            _ => panic!("PerpEngine: invalid close proof"),
        }

        let oracle_price = Self::require_oracle_price(&env);
        let (settlement, _funding) = Self::compute_settlement_with_funding(
            &env,
            meta.effective_collateral,
            meta.leverage,
            meta.side,
            meta.matched_price,
            oracle_price,
            meta.funding_at_open,
        );

        meta.status = PositionStatus::Closed;
        meta.matched_price = oracle_price;
        env.storage().persistent().set(&pos_key, &meta);
        env.storage().persistent().set(&null_key, &true);
        env.storage()
            .persistent()
            .extend_ttl(&pos_key, 17280, 17280);
        env.storage()
            .persistent()
            .extend_ttl(&null_key, 17280, 17280);

        if settlement > 0 {
            let note_key = DataKey::Note(recipient_note.clone());
            if env.storage().persistent().has(&note_key) {
                panic!("PerpEngine: recipient note already exists");
            }
            env.storage().persistent().set(&note_key, &settlement);
            env.storage()
                .persistent()
                .extend_ttl(&note_key, 17280, 17280);
        }

        if meta.match_id != 0 {
            Self::try_close_match(&env, meta.match_id, &position_commitment);
        }

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("close_n"),),
            (
                position_commitment,
                close_nullifier,
                recipient_note,
                settlement,
                oracle_price,
            ),
        );

        settlement
    }

    /// Two-tier liquidation with privacy-native proceeds and insurance fund.
    ///
    /// Tier 1 — Partial (first MM breach, position still has value):
    ///   Close 50% of the position. 1% of freed collateral goes to liquidator.
    ///   Remaining proceeds go to `liquidation_recipient_note`.
    ///   Position stays open at half effective_collateral.
    ///
    /// Tier 2 — Full (second breach, or position at zero on first breach):
    ///   Close 100%. 1.5% reward to liquidator, 0.5% to insurance fund.
    ///   Any shortfall in reward is drawn from insurance fund → bad_debt if exhausted.
    ///   Proceeds to `liquidation_recipient_note`.
    pub fn liquidate(env: Env, commitment: BytesN<32>) -> i128 {
        let pos_key = DataKey::Position(commitment.clone());
        let mut meta: PositionMeta = env
            .storage()
            .persistent()
            .get(&pos_key)
            .unwrap_or_else(|| panic!("PerpEngine: position not found"));

        if meta.status != PositionStatus::Matched {
            panic!(
                "PerpEngine: can only liquidate a matched position (status={:?})",
                meta.status as u32
            );
        }

        let asset_id = meta.asset_id.clone();
        let oracle_price = Self::require_asset_oracle_price(&env, &asset_id);
        let asset_cfg: types::AssetConfig = env
            .storage()
            .persistent()
            .get(&DataKey::AssetConfig(asset_id.clone()))
            .unwrap_or_else(|| panic!("PerpEngine: asset not registered for position"));
        let (settlement, _) = Self::compute_settlement_with_funding(
            &env,
            meta.effective_collateral,
            meta.leverage,
            meta.side,
            meta.matched_price,
            oracle_price,
            meta.funding_at_open,
        );

        let mm = meta.effective_collateral * asset_cfg.maintenance_margin_bps / 10_000;

        // Cross-margin: check aggregate portfolio health instead of per-position.
        // The portfolio is liquidatable only if the WHOLE group is underwater.
        if meta.margin_mode == MarginMode::Cross {
            let group_key = DataKey::PortfolioGroup(meta.portfolio_key.clone());
            let group: soroban_sdk::Vec<BytesN<32>> = env
                .storage()
                .persistent()
                .get(&group_key)
                .unwrap_or_else(|| soroban_sdk::Vec::new(&env));
            let mut total_settlement: i128 = 0;
            let mut total_mm: i128 = 0;
            for i in 0..group.len() {
                let cmt = group.get(i).unwrap();
                let pos: PositionMeta = env
                    .storage()
                    .persistent()
                    .get(&DataKey::Position(cmt))
                    .unwrap_or_else(|| panic!("PerpEngine: portfolio position not found"));
                if pos.status != PositionStatus::Matched {
                    continue;
                }
                let pos_oracle = Self::require_asset_oracle_price(&env, &pos.asset_id);
                let pos_cfg: types::AssetConfig = env
                    .storage()
                    .persistent()
                    .get(&DataKey::AssetConfig(pos.asset_id.clone()))
                    .unwrap_or_else(|| panic!("PerpEngine: asset not registered"));
                let (s, _) = Self::compute_settlement_with_funding(
                    &env,
                    pos.effective_collateral,
                    pos.leverage,
                    pos.side,
                    pos.matched_price,
                    pos_oracle,
                    pos.funding_at_open,
                );
                total_settlement += s;
                total_mm += pos.effective_collateral * pos_cfg.maintenance_margin_bps / 10_000;
            }
            if total_settlement >= total_mm {
                panic!(
                    "PerpEngine: cross-margin portfolio is healthy (total_settlement={}, total_mm={})",
                    total_settlement, total_mm
                );
            }
        } else if settlement >= mm {
            panic!(
                "PerpEngine: position is not under-collateralized (settlement={}, mm={})",
                settlement, mm
            );
        }

        let liq_note = meta.liquidation_recipient_note.clone();
        let is_partial = !meta.partial_liq_done && settlement > 0;

        if is_partial {
            // ── Tier 1: Partial liquidation ───────────────────────────────
            let half_collateral = meta.effective_collateral / 2;
            let half_settlement = settlement / 2;
            let penalty = half_collateral * asset_cfg.liq_partial_reward_bps / 10_000;
            let to_note = (half_settlement - penalty).max(0);

            // Shrink the position to 50%
            meta.effective_collateral -= half_collateral;
            meta.partial_liq_done = true;
            meta.matched_price = oracle_price; // reset entry for remaining half
            env.storage().persistent().set(&pos_key, &meta);
            env.storage()
                .persistent()
                .extend_ttl(&pos_key, 17280, 17280);

            // Pay liquidator reward from protocol funds
            Self::pay_liquidator_reward(&env, penalty);

            // Pay owner proceeds to the liquidation recipient note
            Self::pay_liquidation_proceeds(&env, &liq_note, to_note);

            #[allow(deprecated)]
            env.events().publish(
                (soroban_sdk::symbol_short!("pliq"),),
                (
                    commitment,
                    oracle_price,
                    penalty,
                    to_note,
                    meta.effective_collateral,
                ),
            );

            penalty
        } else {
            // ── Tier 2: Full liquidation ───────────────────────────────────
            let eff = meta.effective_collateral;
            let base_reward = eff * asset_cfg.liq_full_reward_bps / 10_000;
            let ins_fee = eff * asset_cfg.ins_fund_bps / 10_000;
            let total_fees = base_reward + ins_fee;

            let (actual_reward, ins_delta, to_note) = if settlement >= total_fees {
                // Healthy liquidation: pay all fees, owner gets remainder
                (base_reward, ins_fee, settlement - total_fees)
            } else if settlement >= base_reward {
                // Marginal: pay full reward, partial insurance contribution
                (base_reward, settlement - base_reward, 0i128)
            } else {
                // Underwater: draw from insurance fund to top up reward
                let shortfall = base_reward - settlement;
                let ins_fund = Self::read_insurance_fund(&env);
                let draw = shortfall.min(ins_fund);
                let unmet = shortfall - draw;
                if unmet > 0 {
                    Self::accrue_bad_debt(&env, unmet);
                }
                // ins_delta is negative (we draw from fund)
                (settlement + draw, -(draw), 0i128)
            };

            // Apply insurance fund delta
            Self::update_insurance_fund(&env, ins_delta);

            // Pay liquidator reward from protocol funds
            Self::pay_liquidator_reward(&env, actual_reward);

            // Pay owner proceeds to the liquidation recipient note
            Self::pay_liquidation_proceeds(&env, &liq_note, to_note);

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
                (commitment, oracle_price, actual_reward, to_note, ins_delta),
            );

            actual_reward
        }
    }

    /// Top up the insurance fund. Callable by anyone — tokens must already be
    /// in the contract (e.g. sent directly, or left from liquidation fees).
    pub fn fund_insurance(env: Env, amount: i128) {
        if amount <= 0 {
            panic!("PerpEngine: amount must be positive");
        }
        Self::update_insurance_fund(&env, amount);
    }

    pub fn insurance_balance(env: Env) -> i128 {
        Self::read_insurance_fund(&env)
    }

    pub fn bad_debt(env: Env) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::BadDebt)
            .unwrap_or(0i128)
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

    pub fn get_portfolio_group(env: Env, portfolio_key: BytesN<32>) -> Vec<BytesN<32>> {
        env.storage()
            .persistent()
            .get::<_, Vec<BytesN<32>>>(&DataKey::PortfolioGroup(portfolio_key))
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn get_config(env: Env) -> Config {
        Self::config(&env)
    }

    /// Register a new asset. Admin sets its initial config.
    /// Once registered, positions can reference this asset_id.
    pub fn register_asset(
        env: Env,
        admin: Address,
        asset_id: BytesN<32>,
        name: Bytes,
        config: types::AssetConfig,
    ) {
        admin.require_auth();
        let protocol = Self::config(&env);
        if protocol.admin != admin {
            panic!("PerpEngine: only protocol admin can register assets");
        }
        if env
            .storage()
            .persistent()
            .has(&DataKey::AssetConfig(asset_id.clone()))
        {
            panic!("PerpEngine: asset already registered");
        }
        env.storage()
            .persistent()
            .set(&DataKey::AssetConfig(asset_id.clone()), &config);
        env.storage().persistent().extend_ttl(
            &DataKey::AssetConfig(asset_id.clone()),
            17280,
            17280,
        );

        if !name.is_empty() {
            env.storage()
                .persistent()
                .set(&DataKey::AssetName(asset_id.clone()), &name);
            env.storage().persistent().extend_ttl(
                &DataKey::AssetName(asset_id.clone()),
                17280,
                17280,
            );
        }

        // Track in asset list
        let mut list: soroban_sdk::Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&DataKey::AssetList)
            .unwrap_or_else(|| soroban_sdk::Vec::new(&env));
        list.push_back(asset_id.clone());
        env.storage().persistent().set(&DataKey::AssetList, &list);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::AssetList, 17280, 17280);

        #[allow(deprecated)]
        env.events()
            .publish((soroban_sdk::symbol_short!("reg_ast"),), (asset_id,));
    }

    /// Update an existing asset's config. Must be a registered asset.
    pub fn update_asset_config(
        env: Env,
        admin: Address,
        asset_id: BytesN<32>,
        config: types::AssetConfig,
    ) {
        admin.require_auth();
        let protocol = Self::config(&env);
        if protocol.admin != admin {
            panic!("PerpEngine: only protocol admin can update asset config");
        }
        if !env
            .storage()
            .persistent()
            .has(&DataKey::AssetConfig(asset_id.clone()))
        {
            panic!("PerpEngine: asset not registered");
        }
        env.storage()
            .persistent()
            .set(&DataKey::AssetConfig(asset_id.clone()), &config);
        env.storage().persistent().extend_ttl(
            &DataKey::AssetConfig(asset_id.clone()),
            17280,
            17280,
        );

        #[allow(deprecated)]
        env.events()
            .publish((soroban_sdk::symbol_short!("upd_ast"),), (asset_id,));
    }

    /// Get asset config. Returns None if not registered.
    pub fn get_asset_config(env: Env, asset_id: BytesN<32>) -> Option<types::AssetConfig> {
        env.storage()
            .persistent()
            .get(&DataKey::AssetConfig(asset_id))
    }

    /// List all registered asset IDs.
    pub fn list_assets(env: Env) -> soroban_sdk::Vec<BytesN<32>> {
        env.storage()
            .persistent()
            .get::<_, soroban_sdk::Vec<BytesN<32>>>(&DataKey::AssetList)
            .unwrap_or_else(|| soroban_sdk::Vec::new(&env))
    }

    /// Get asset name for display.
    pub fn get_asset_name(env: Env, asset_id: BytesN<32>) -> Option<Bytes> {
        env.storage()
            .persistent()
            .get(&DataKey::AssetName(asset_id))
    }

    /// Set price for a specific asset. Admin must be the oracle admin for that asset.
    pub fn set_asset_price(env: Env, asset_id: BytesN<32>, admin: Address, price: u64) {
        admin.require_auth();
        if price == 0 {
            panic!("PerpEngine: price must be non-zero");
        }
        let mut cfg = Self::read_asset_oracle_config(&env, &asset_id).unwrap_or_else(|| {
            let protocol = Self::config(&env);
            OracleConfig {
                admin: protocol.admin.clone(),
                price: 0,
                last_updated: 0,
                heartbeat: 3600,
                twap: 0,
            }
        });
        if cfg.admin != admin {
            panic!("PerpEngine: unauthorized oracle admin");
        }
        if cfg.twap > 0 {
            let twap = cfg.twap;
            let dev = price.abs_diff(twap);
            if dev * 10_000 / twap > MAX_PRICE_DEVIATION_BPS {
                panic!(
                    "PerpEngine: price deviation too large (price={}, twap={}, max_bps={})",
                    price, twap, MAX_PRICE_DEVIATION_BPS
                );
            }
        }
        let ledger = env.ledger().sequence() as u64;
        let new_twap = Self::push_asset_twap_sample(&env, &asset_id, price, ledger);
        cfg.price = price;
        cfg.last_updated = ledger;
        cfg.twap = new_twap;
        let key = DataKey::AssetOracle(asset_id.clone());
        env.storage().persistent().set(&key, &cfg);
        env.storage().persistent().extend_ttl(&key, 17280, 17280);

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("aset_pr"),),
            (asset_id, price, ledger, new_twap),
        );
    }

    /// Get current price for an asset.
    pub fn get_asset_price(env: Env, asset_id: BytesN<32>) -> Option<u64> {
        Self::read_asset_oracle_config(&env, &asset_id).map(|c| c.price)
    }

    /// Get oracle config for an asset.
    pub fn get_asset_oracle_config(env: Env, asset_id: BytesN<32>) -> Option<OracleConfig> {
        Self::read_asset_oracle_config(&env, &asset_id)
    }

    /// Get TWAP for an asset.
    pub fn get_asset_twap(env: Env, asset_id: BytesN<32>) -> u64 {
        Self::read_asset_oracle_config(&env, &asset_id)
            .map(|c| c.twap)
            .unwrap_or(0)
    }

    /// Initialize oracle by setting the oracle admin (called once after initialize).
    /// Oracle admin is separate from the protocol admin.
    pub fn set_oracle_admin(env: Env, admin: Address, oracle_admin: Address, heartbeat: u64) {
        admin.require_auth();
        let protocol_cfg = Self::config(&env);
        if protocol_cfg.admin != admin {
            panic!("PerpEngine: only protocol admin can set oracle admin");
        }
        let existing = Self::read_oracle_config(&env);
        if let Some(ref cfg) = existing {
            if cfg.admin != admin && cfg.price != 0 {
                panic!("PerpEngine: oracle already has an admin");
            }
        }
        let cfg = OracleConfig {
            admin: oracle_admin.clone(),
            price: existing.as_ref().map(|c| c.price).unwrap_or(0),
            last_updated: existing.as_ref().map(|c| c.last_updated).unwrap_or(0),
            heartbeat,
            twap: existing.as_ref().map(|c| c.twap).unwrap_or(0),
        };
        env.storage().persistent().set(&DataKey::OracleConfig, &cfg);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::OracleConfig, 17280, 17280);

        #[allow(deprecated)]
        env.events()
            .publish((soroban_sdk::symbol_short!("orc_adm"),), (heartbeat,));
    }

    /// Submit a new price observation. Pushes to the TWAP ring buffer and updates spot price.
    /// New price must be within MAX_PRICE_DEVIATION_BPS of the current TWAP (if TWAP exists).
    pub fn set_price(env: Env, admin: Address, price: u64) {
        admin.require_auth();
        if price == 0 {
            panic!("PerpEngine: price must be non-zero");
        }
        let mut cfg = Self::read_oracle_config(&env).unwrap_or_else(|| {
            // First call initialises oracle admin to the protocol admin
            let protocol = Self::config(&env);
            OracleConfig {
                admin: protocol.admin.clone(),
                price: 0,
                last_updated: 0,
                heartbeat: 3600,
                twap: 0,
            }
        });
        if cfg.admin != admin {
            panic!("PerpEngine: unauthorized oracle admin");
        }

        // Validate price deviation against current TWAP (skip if TWAP not yet established)
        if cfg.twap > 0 {
            let twap = cfg.twap;
            let dev = price.abs_diff(twap);
            if dev * 10_000 / twap > MAX_PRICE_DEVIATION_BPS {
                panic!(
                    "PerpEngine: price deviation too large (price={}, twap={}, max_bps={})",
                    price, twap, MAX_PRICE_DEVIATION_BPS
                );
            }
        }

        let ledger = env.ledger().sequence() as u64;
        let new_twap = Self::push_twap_sample(&env, price, ledger);

        cfg.price = price;
        cfg.last_updated = ledger;
        cfg.twap = new_twap;
        env.storage().persistent().set(&DataKey::OracleConfig, &cfg);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::OracleConfig, 17280, 17280);

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("price"),),
            (price, new_twap, ledger),
        );
    }

    pub fn get_price(env: Env) -> Option<u64> {
        Self::read_oracle_config(&env)
            .map(|cfg| cfg.price)
            .filter(|&p| p > 0)
    }

    pub fn get_twap(env: Env) -> u64 {
        Self::read_oracle_config(&env)
            .map(|cfg| cfg.twap)
            .unwrap_or(0)
    }

    pub fn get_oracle_config(env: Env) -> Option<OracleConfig> {
        Self::read_oracle_config(&env)
    }

    /// Set the mark price (CLOB mid-price posted by the TEE keeper).
    /// Only the protocol admin may post to prevent manipulation.
    pub fn set_mark_price(env: Env, keeper: Address, price: u64) {
        keeper.require_auth();
        let cfg = Self::config(&env);
        if cfg.admin != keeper {
            panic!("PerpEngine: only admin can set mark price");
        }
        if price == 0 {
            panic!("PerpEngine: mark price must be non-zero");
        }
        env.storage().persistent().set(&DataKey::MarkPrice, &price);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::MarkPrice, 17280, 17280);
        #[allow(deprecated)]
        env.events()
            .publish((soroban_sdk::symbol_short!("mark_p"),), (price,));
    }

    pub fn get_mark_price(env: Env) -> u64 {
        env.storage()
            .persistent()
            .get::<_, u64>(&DataKey::MarkPrice)
            .unwrap_or(0)
    }

    pub fn get_match_record(env: Env, match_id: u64) -> Option<MatchRecord> {
        env.storage()
            .persistent()
            .get::<_, MatchRecord>(&DataKey::Match(match_id))
    }

    pub fn settle_match(env: Env, match_id: u64) {
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
            panic!(
                "PerpEngine: position A must be matched (status={:?})",
                meta_a.status as u32
            );
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

        // Credit both positions' liquidation recipient notes
        for (meta, settlement) in [(&mut meta_a, settlement_a), (&mut meta_b, settlement_b)] {
            if settlement > 0 {
                let note_key = DataKey::Note(meta.liquidation_recipient_note.clone());
                let existing: i128 = env.storage().persistent().get(&note_key).unwrap_or(0);
                env.storage()
                    .persistent()
                    .set(&note_key, &(existing + settlement));
                env.storage()
                    .persistent()
                    .extend_ttl(&note_key, 17280, 17280);
            }
        }

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

    /// Advance the funding rate accumulator.
    /// Rate = clamp((twap − mark) / twap × 10_000, ±MAX_FUNDING_RATE_BPS) bps per FUNDING_INTERVAL.
    /// Cumulative is scaled by 100 so that: payment = cumulative_delta × collateral / 1_000_000
    /// gives 0.75% of collateral at the cap over one full interval.
    pub fn update_funding(env: Env, keeper: Address) {
        keeper.require_auth();

        let mut state = Self::read_funding_state(&env);
        let now = env.ledger().sequence() as u64;

        // Require at least half an interval between updates
        let delta = now.saturating_sub(state.last_update);
        if delta < FUNDING_INTERVAL / 2 {
            return;
        }

        // Use TWAP (manipulation-resistant) as the oracle price for funding
        let twap = Self::read_oracle_config(&env).map(|c| c.twap).unwrap_or(0);
        if twap == 0 {
            return; // oracle not yet priced
        }
        let mark_price = Self::derive_mark_price(&env);
        if mark_price == 0 {
            return; // mark price not yet posted
        }

        // premium_bps: positive → oracle above mark → longs pay
        //              negative → oracle below mark → shorts pay
        let premium_bps = ((twap as i64) - (mark_price as i64)) * 10_000 / (twap as i64);
        let rate_bps = premium_bps.clamp(-MAX_FUNDING_RATE_BPS, MAX_FUNDING_RATE_BPS);

        // Accumulate: each unit of cumulative = 1/100 bps per interval
        // After one full interval at MAX_FUNDING_RATE_BPS:
        //   payment = (rate_bps * 100) * collateral / 1_000_000 = rate_bps * collateral / 10_000
        //   = 0.0075 * collateral ✓
        let payment = (rate_bps as i128) * 100_i128 * (delta as i128) / (FUNDING_INTERVAL as i128);
        state.cumulative = state.cumulative.wrapping_add(payment);
        state.rate = rate_bps;
        state.last_update = now;

        env.storage()
            .persistent()
            .set(&DataKey::FundingState, &state);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::FundingState, 17280, 17280);

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("funding"),),
            (rate_bps, payment, delta, state.cumulative),
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

    /// Push a new price sample into the TWAP ring buffer and return the updated TWAP.
    fn push_twap_sample(env: &Env, price: u64, ledger: u64) -> u64 {
        let head: u64 = env
            .storage()
            .persistent()
            .get::<_, u64>(&DataKey::TwapHead)
            .unwrap_or(0);

        let slot = head % TWAP_WINDOW;
        env.storage()
            .persistent()
            .set(&DataKey::TwapSample(slot), &PriceSample { price, ledger });
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::TwapSample(slot), 17280, 17280);

        let next_head = head + 1;
        env.storage()
            .persistent()
            .set(&DataKey::TwapHead, &next_head);

        // Arithmetic mean of all filled slots
        let mut sum: u128 = 0;
        let mut count: u64 = 0;
        for i in 0..TWAP_WINDOW {
            if let Some(sample) = env
                .storage()
                .persistent()
                .get::<_, PriceSample>(&DataKey::TwapSample(i))
            {
                sum += sample.price as u128;
                count += 1;
            }
        }
        if count == 0 {
            price
        } else {
            (sum / count as u128) as u64
        }
    }

    fn require_oracle_price(env: &Env) -> u64 {
        let cfg = Self::read_oracle_config(env)
            .unwrap_or_else(|| panic!("PerpEngine: oracle not initialized"));
        if cfg.price == 0 {
            panic!("PerpEngine: oracle price not set");
        }
        let now = env.ledger().sequence() as u64;
        if now.saturating_sub(cfg.last_updated) > cfg.heartbeat {
            panic!(
                "PerpEngine: oracle price stale (last_updated={}, heartbeat={})",
                cfg.last_updated, cfg.heartbeat
            );
        }
        cfg.price
    }

    // ── Per-asset oracle helpers ─────────────────────────────────────
    fn read_asset_oracle_config(env: &Env, asset_id: &BytesN<32>) -> Option<OracleConfig> {
        let key = DataKey::AssetOracle(asset_id.clone());
        env.storage().persistent().get::<_, OracleConfig>(&key)
    }

    fn push_asset_twap_sample(env: &Env, asset_id: &BytesN<32>, price: u64, ledger: u64) -> u64 {
        let head_key = DataKey::AssetTwapHead(asset_id.clone());
        let head: u64 = env
            .storage()
            .persistent()
            .get::<_, u64>(&head_key)
            .unwrap_or(0);

        let slot = head % TWAP_WINDOW;
        let sample_key = DataKey::AssetTwapSample(asset_id.clone(), slot);
        env.storage()
            .persistent()
            .set(&sample_key, &PriceSample { price, ledger });
        env.storage()
            .persistent()
            .extend_ttl(&sample_key, 17280, 17280);

        let next_head = head + 1;
        env.storage().persistent().set(&head_key, &next_head);

        let mut sum: u128 = 0;
        let mut count: u64 = 0;
        for i in 0..TWAP_WINDOW {
            if let Some(sample) = env
                .storage()
                .persistent()
                .get::<_, PriceSample>(&DataKey::AssetTwapSample(asset_id.clone(), i))
            {
                sum += sample.price as u128;
                count += 1;
            }
        }
        if count == 0 {
            price
        } else {
            (sum / count as u128) as u64
        }
    }

    fn require_asset_oracle_price(env: &Env, asset_id: &BytesN<32>) -> u64 {
        // Try per-asset oracle first, fall back to global
        if let Some(cfg) = Self::read_asset_oracle_config(env, asset_id) {
            if cfg.price == 0 {
                panic!("PerpEngine: oracle price not set for asset");
            }
            let now = env.ledger().sequence() as u64;
            if now.saturating_sub(cfg.last_updated) > cfg.heartbeat {
                panic!(
                    "PerpEngine: oracle price stale for asset (last_updated={}, heartbeat={})",
                    cfg.last_updated, cfg.heartbeat
                );
            }
            return cfg.price;
        }
        // Fallback to global oracle
        Self::require_oracle_price(env)
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
            .unwrap_or_else(|| {
                panic!(
                    "PerpEngine: match {} not found in try_close_match",
                    match_id
                )
            });
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
            if other.status != PositionStatus::Closed && other.status != PositionStatus::Liquidated
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
        env.storage()
            .persistent()
            .get::<_, u64>(&DataKey::MarkPrice)
            .unwrap_or(0)
    }

    fn read_insurance_fund(env: &Env) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::InsuranceFund)
            .unwrap_or(0i128)
    }

    fn update_insurance_fund(env: &Env, delta: i128) {
        let current = Self::read_insurance_fund(env);
        let next = (current + delta).max(0);
        env.storage()
            .persistent()
            .set(&DataKey::InsuranceFund, &next);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::InsuranceFund, 17280, 17280);
    }

    fn accrue_bad_debt(env: &Env, amount: i128) {
        let current: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::BadDebt)
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::BadDebt, &(current + amount));
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::BadDebt, 17280, 17280);
    }

    /// Credit liquidation reward to the insurance fund (protocol capture).
    fn pay_liquidator_reward(env: &Env, amount: i128) {
        if amount <= 0 {
            return;
        }
        Self::update_insurance_fund(env, amount);
    }

    /// Credit liquidation proceeds to the shielded note commitment.
    fn pay_liquidation_proceeds(env: &Env, note: &BytesN<32>, amount: i128) {
        if amount <= 0 {
            return;
        }
        let note_key = DataKey::Note(note.clone());
        let existing: i128 = env.storage().persistent().get(&note_key).unwrap_or(0);
        env.storage()
            .persistent()
            .set(&note_key, &(existing + amount));
        env.storage()
            .persistent()
            .extend_ttl(&note_key, 17280, 17280);
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

    /// Load the note_spend PK from circuits/keys and generate a real proof.
    /// Returns (note_commitment_hex, nullifier_hex, proof_json).
    fn gen_note_proof(
        amount: u64,
        secret: u64,
    ) -> (
        std::string::String,
        std::string::String,
        std::string::String,
    ) {
        use ark_bn254::Fr;
        use rust_circuits::{
            compute_note_commitment, compute_note_nullifier, fr_to_biguint, load_pk,
            prove_note_spend_with_pk,
        };
        use std::string::ToString;

        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let pk_path =
            std::path::Path::new(manifest_dir).join("../../circuits/keys/note_spend.pk.bin");

        let pk = load_pk(&pk_path)
            .expect("Failed to load note_spend.pk.bin — run `cargo run --release --manifest-path tools/rust-circuits/Cargo.toml -- setup` first");

        let amount_fr = Fr::from(amount);
        let secret_fr = Fr::from(secret);
        let note_cmt = compute_note_commitment(amount_fr, secret_fr);
        let nullifier = compute_note_nullifier(note_cmt, secret_fr);

        let out = prove_note_spend_with_pk(&pk, amount_fr, secret_fr)
            .expect("prove_note_spend_with_pk failed");

        let cmt_hex = std::format!("{:0>64x}", fr_to_biguint(&note_cmt));
        let null_hex = std::format!("{:0>64x}", fr_to_biguint(&nullifier));
        let proof_json =
            serde_json::json!({"a": out.proof.a, "b": out.proof.b, "c": out.proof.c}).to_string();

        (cmt_hex, null_hex, proof_json)
    }

    /// Generate an OrderCommitment proof (asset=0, is_market=false).
    /// Returns (commitment_hex, proof_json).
    fn gen_commit_proof(
        side: u64,
        price: u64,
        size: u64,
        leverage: u64,
        nonce: u64,
        secret: u64,
    ) -> (std::string::String, std::string::String) {
        let (cmt_hex, _, proof_json) =
            gen_commit_proof_full(side, price, size, leverage, nonce, secret, false);
        (cmt_hex, proof_json)
    }

    /// Generate an OrderCommitment proof with explicit margin mode.
    /// Returns (commitment_hex, portfolio_key_hex, proof_json).
    fn gen_commit_proof_full(
        side: u64,
        price: u64,
        size: u64,
        leverage: u64,
        nonce: u64,
        secret: u64,
        use_cross: bool,
    ) -> (
        std::string::String,
        std::string::String,
        std::string::String,
    ) {
        use ark_bn254::Fr;
        use ark_ff::AdditiveGroup;
        use rust_circuits::{
            compute_commitment, compute_portfolio_key, fr_to_biguint, load_pk,
            prove_commitment_with_pk,
        };
        use std::string::ToString;

        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let pk_path =
            std::path::Path::new(manifest_dir).join("../../circuits/keys/order_commitment.pk.bin");
        let pk = load_pk(&pk_path).expect("Failed to load order_commitment.pk.bin");

        let asset = Fr::from(0u64);
        let is_market = Fr::ZERO;
        let secret_fr = Fr::from(secret);
        let cmt = compute_commitment(
            Fr::from(side),
            Fr::from(price),
            Fr::from(size),
            Fr::from(leverage),
            asset,
            is_market,
            Fr::from(nonce),
            secret_fr,
        );
        let portfolio_key = if use_cross {
            compute_portfolio_key(secret_fr)
        } else {
            Fr::ZERO
        };
        let out = prove_commitment_with_pk(
            &pk,
            Fr::from(side),
            Fr::from(price),
            Fr::from(size),
            Fr::from(leverage),
            asset,
            is_market,
            Fr::from(nonce),
            secret_fr,
            use_cross,
        )
        .expect("prove_commitment_with_pk failed");

        let cmt_hex = std::format!("{:0>64x}", fr_to_biguint(&cmt));
        let pk_hex = std::format!("{:0>64x}", fr_to_biguint(&portfolio_key));
        let proof_json =
            serde_json::json!({"a": out.proof.a, "b": out.proof.b, "c": out.proof.c}).to_string();
        (cmt_hex, pk_hex, proof_json)
    }

    /// Generate an OrderCancel proof for a position commitment.
    /// Returns (nullifier_hex, proof_json).
    fn gen_cancel_proof(
        commitment_hex: &str,
        secret: u64,
    ) -> (std::string::String, std::string::String) {
        use ark_bn254::Fr;
        use ark_ff::PrimeField;
        use rust_circuits::{compute_nullifier, fr_to_biguint, load_pk, prove_cancel_with_pk};
        use std::convert::TryInto;
        use std::string::ToString;

        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let pk_path =
            std::path::Path::new(manifest_dir).join("../../circuits/keys/order_cancel.pk.bin");
        let pk = load_pk(&pk_path).expect("Failed to load order_cancel.pk.bin");

        let cmt_bytes: [u8; 32] = hex::decode(commitment_hex).unwrap().try_into().unwrap();
        let cmt_fr = Fr::from_be_bytes_mod_order(&cmt_bytes);
        let secret_fr = Fr::from(secret);
        let nullifier = compute_nullifier(cmt_fr, secret_fr);

        let out =
            prove_cancel_with_pk(&pk, cmt_fr, secret_fr).expect("prove_cancel_with_pk failed");

        let null_hex = std::format!("{:0>64x}", fr_to_biguint(&nullifier));
        let proof_json =
            serde_json::json!({"a": out.proof.a, "b": out.proof.b, "c": out.proof.c}).to_string();
        (null_hex, proof_json)
    }

    fn make_groth16_proof(env: &Env, proof_json: &str) -> Groth16Proof {
        use std::convert::TryInto;
        let v: serde_json::Value = serde_json::from_str(proof_json).unwrap();
        let a_hex = v["a"].as_str().unwrap();
        let b_hex = v["b"].as_str().unwrap();
        let c_hex = v["c"].as_str().unwrap();

        let a_bytes: [u8; 64] = hex::decode(a_hex).unwrap().try_into().unwrap();
        let b_bytes: [u8; 128] = hex::decode(b_hex).unwrap().try_into().unwrap();
        let c_bytes: [u8; 64] = hex::decode(c_hex).unwrap().try_into().unwrap();

        use soroban_sdk::crypto::bn254::{Bn254G1Affine, Bn254G2Affine};
        Groth16Proof {
            a: Bn254G1Affine::from_bytes(BytesN::from_array(env, &a_bytes)),
            b: Bn254G2Affine::from_bytes(BytesN::from_array(env, &b_bytes)),
            c: Bn254G1Affine::from_bytes(BytesN::from_array(env, &c_bytes)),
        }
    }

    fn hex_to_bytes32(env: &Env, hex_str: &str) -> BytesN<32> {
        use std::convert::TryInto;
        let bytes: [u8; 32] = hex::decode(hex_str).unwrap().try_into().unwrap();
        BytesN::from_array(env, &bytes)
    }

    fn setup() -> (Env, Address, Address) {
        let env = Env::default();
        let admin = Address::generate(&env);
        let contract_id = env.register(PerpEngine, ());
        let token = env.register_stellar_asset_contract_v2(admin.clone());
        env.mock_all_auths();
        let client = PerpEngineClient::new(&env, &contract_id);
        client.initialize(&admin, &token.address(), &None);
        // Register default asset (asset_id = [0u8; 32]) so tests can open positions
        let default_asset = BytesN::from_array(&env, &[0u8; 32]);
        let config = types::AssetConfig {
            max_leverage: 50,
            maintenance_margin_bps: 500,
            initial_margin_bps: 1000,
            liq_partial_reward_bps: 100,
            liq_full_reward_bps: 150,
            ins_fund_bps: 50,
            active: true,
        };
        client.register_asset(&admin, &default_asset, &Bytes::new(&env), &config);
        (env, contract_id, admin)
    }

    fn create_position(
        env: &Env,
        cid: &Address,
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
                collateral,
                entry_price: price,
                matched_price: if status == PositionStatus::Matched {
                    price
                } else {
                    0
                },
                side,
                leverage,
                status,
                created_at: env.ledger().sequence() as u64,
                match_id,
                funding_at_open: 0,
                hint_size: 1_000_000_000,
                tif: TimeInForce::GTC,
                expiry_ledger: 0,
                tp_price: 0,
                sl_price: 0,
                effective_collateral: collateral,
                partial_liq_done: false,
                liquidation_recipient_note: BytesN::from_array(env, &[0u8; 32]),
                asset_id: BytesN::from_array(env, &[0u8; 32]),
                margin_mode: MarginMode::Isolated,
                portfolio_key: BytesN::from_array(env, &[0u8; 32]),
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
        client.initialize(&admin, &token.address(), &None);
        let cfg = client.get_config();
        assert_eq!(cfg.admin, admin);
        assert_eq!(cfg.token, token.address());
    }

    #[test]
    fn test_oracle_set_and_get_price() {
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        assert_eq!(client.get_price(), None);
        assert_eq!(client.get_twap(), 0);

        client.set_price(&admin, &100_000_000);
        assert_eq!(client.get_price(), Some(100_000_000));
        assert_eq!(client.get_twap(), 100_000_000);

        // Second price within ±50% of TWAP (110M is +10% — fine)
        client.set_price(&admin, &110_000_000);
        assert_eq!(client.get_price(), Some(110_000_000));
        // TWAP = mean(100M, 110M) = 105M
        assert_eq!(client.get_twap(), 105_000_000);

        let cfg = client.get_oracle_config().unwrap();
        assert_eq!(cfg.price, 110_000_000);
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
        // get_price still returns the last price even if stale (only require_oracle_price enforces staleness)
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
        // Update within ±50% of TWAP (120M is +20% from 100M)
        client.set_price(&admin, &120_000_000);
        assert_eq!(client.get_price(), Some(120_000_000));
    }

    #[test]
    fn test_oracle_config_defaults() {
        let (env, cid, _admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        assert!(client.get_oracle_config().is_none());
        assert_eq!(client.get_twap(), 0);
    }

    #[test]
    fn test_oracle_twap_accumulates_over_window() {
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);

        // Feed 8 prices (one full TWAP_WINDOW): 100, 102, 98, 101, 99, 103, 97, 100
        let prices: &[u64] = &[
            100_000, 102_000, 98_000, 101_000, 99_000, 103_000, 97_000, 100_000,
        ];
        for &p in prices {
            client.set_price(&admin, &p);
        }
        let twap = client.get_twap();
        let expected_mean: u64 = prices.iter().sum::<u64>() / prices.len() as u64;
        assert_eq!(
            twap, expected_mean,
            "TWAP should be arithmetic mean of window"
        );
    }

    #[test]
    fn test_oracle_twap_ring_buffer_wraps() {
        // After TWAP_WINDOW samples, older ones get evicted
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);

        // Fill window with 100_000 then push one at 110_000 (within ±50%)
        for _ in 0..8 {
            client.set_price(&admin, &100_000);
        }
        assert_eq!(client.get_twap(), 100_000);

        // Push 110_000 — within 50% of 100_000 ✓
        client.set_price(&admin, &110_000);
        // Window now has 7 × 100_000 and 1 × 110_000 → mean = (700_000 + 110_000) / 8 = 101_250
        assert_eq!(client.get_twap(), 101_250);
    }

    #[test]
    #[should_panic(expected = "price deviation too large")]
    fn test_oracle_price_deviation_rejected() {
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        client.set_price(&admin, &100_000);
        // 200_000 is 100% above TWAP of 100_000 — exceeds 50% cap
        client.set_price(&admin, &200_000);
    }

    #[test]
    fn test_oracle_set_admin_and_handoff() {
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let new_oracle_admin = Address::generate(&env);

        // Protocol admin delegates oracle role
        client.set_oracle_admin(&admin, &new_oracle_admin, &3600u64);
        let cfg = client.get_oracle_config().unwrap();
        assert_eq!(cfg.admin, new_oracle_admin);
        assert_eq!(cfg.heartbeat, 3600);

        // New oracle admin can set price
        client.set_price(&new_oracle_admin, &500_000);
        assert_eq!(client.get_price(), Some(500_000));
    }

    #[test]
    #[should_panic(expected = "only admin can set mark price")]
    fn test_mark_price_unauthorized_panics() {
        let (env, cid, _admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let rando = Address::generate(&env);
        client.set_mark_price(&rando, &100_000);
    }

    #[test]
    fn test_mark_price_set_and_get() {
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        assert_eq!(client.get_mark_price(), 0);
        client.set_mark_price(&admin, &500_000);
        assert_eq!(client.get_mark_price(), 500_000);
    }

    #[test]
    fn test_settle_match_flow() {
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);

        let _owner_a = Address::generate(&env);
        let _owner_b = Address::generate(&env);
        let cmt_a = BytesN::from_array(&env, &[1u8; 32]);
        let cmt_b = BytesN::from_array(&env, &[2u8; 32]);
        let match_id = 1;

        client.set_price(&admin, &1_000_000_000);

        create_position(
            &env,
            &cid,
            &cmt_a,
            100_000_000,
            10,
            0,
            1_000_000_000,
            PositionStatus::Matched,
            match_id,
        );
        create_position(
            &env,
            &cid,
            &cmt_b,
            100_000_000,
            10,
            1,
            1_000_000_000,
            PositionStatus::Matched,
            match_id,
        );
        create_match_record(&env, &cid, match_id, &cmt_a, &cmt_b, 1_000_000_000, 100);

        env.mock_all_auths();
        client.settle_match(&match_id);

        let pos_a = client.get_position(&cmt_a).unwrap();
        assert_eq!(pos_a.status, PositionStatus::Closed);
        let pos_b = client.get_position(&cmt_b).unwrap();
        assert_eq!(pos_b.status, PositionStatus::Closed);

        let rec = client.get_match_record(&match_id).unwrap();
        assert!(rec.closed);

        env.mock_all_auths();
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            client.settle_match(&match_id);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_liquidate_underwater_position() {
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);

        let _owner = Address::generate(&env);
        let _liquidator = Address::generate(&env);
        let cmt = BytesN::from_array(&env, &[42u8; 32]);

        // Fund the contract with tokens so it can pay liquidator reward
        let cfg = client.get_config();
        let token_admin = StellarAssetClient::new(&env, &cfg.token);
        token_admin.mint(&cid, &100_000_000);

        client.set_price(&admin, &50_000_000);

        create_position(
            &env,
            &cid,
            &cmt,
            100_000_000,
            10,
            0,
            100_000_000,
            PositionStatus::Matched,
            0,
        );

        env.mock_all_auths();
        let settlement = client.liquidate(&cmt);
        assert_eq!(settlement, 0);

        let pos = client.get_position(&cmt).unwrap();
        assert_eq!(pos.status, PositionStatus::Liquidated);
    }

    #[test]
    fn test_liquidate_solvent_position_reverts() {
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);

        let _owner = Address::generate(&env);
        let _liquidator = Address::generate(&env);
        let cmt = BytesN::from_array(&env, &[42u8; 32]);

        client.set_price(&admin, &150_000_000);

        create_position(
            &env,
            &cid,
            &cmt,
            100_000_000,
            10,
            0,
            100_000_000,
            PositionStatus::Matched,
            0,
        );

        env.mock_all_auths();
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            client.liquidate(&cmt);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_liquidate_open_position_reverts() {
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);

        let _owner = Address::generate(&env);
        let _liquidator = Address::generate(&env);
        let cmt = BytesN::from_array(&env, &[42u8; 32]);

        client.set_price(&admin, &50_000_000);

        create_position(
            &env,
            &cid,
            &cmt,
            100_000_000,
            10,
            0,
            100_000_000,
            PositionStatus::Open,
            0,
        );

        env.mock_all_auths();
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            client.liquidate(&cmt);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_liquidate_no_oracle_reverts() {
        let (env, cid, _admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);

        let _owner = Address::generate(&env);
        let _liquidator = Address::generate(&env);
        let cmt = BytesN::from_array(&env, &[42u8; 32]);

        create_position(
            &env,
            &cid,
            &cmt,
            100_000_000,
            10,
            0,
            100_000_000,
            PositionStatus::Matched,
            0,
        );

        env.mock_all_auths();
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            client.liquidate(&cmt);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_funding_update() {
        // oracle TWAP = 1_000_000, mark = 990_000 (oracle > mark → longs pay)
        // premium_bps = (1_000_000 - 990_000) * 10_000 / 1_000_000 = 100 bps
        // clamped at max 75 bps → rate = 75
        // after 5760 ledgers: payment = 75 * 100 * 5760 / 5760 = 7500
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        client.set_price(&admin, &1_000_000);
        client.set_mark_price(&admin, &990_000);

        let state = client.get_funding_state();
        assert_eq!(state.cumulative, 0);
        assert_eq!(state.rate, 0);

        env.ledger().set(LedgerInfo {
            sequence_number: 5760, // one full interval
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
        assert_eq!(state.rate, 75, "should be clamped at MAX_FUNDING_RATE_BPS");
        // payment = 75 * 100 * 5760 / 5760 = 7500
        assert_eq!(state.cumulative, 7500);
    }

    #[test]
    fn test_funding_negative_rate() {
        // oracle below mark → shorts pay (negative rate)
        // oracle TWAP = 990_000, mark = 1_000_000 → oracle < mark
        // premium_bps = (990_000 - 1_000_000) * 10_000 / 990_000 ≈ -101 bps → clamped to -75
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        client.set_price(&admin, &990_000);
        client.set_mark_price(&admin, &1_000_000);

        env.ledger().set(LedgerInfo {
            sequence_number: 5760,
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
        assert_eq!(state.rate, -75, "negative rate when oracle < mark");
        assert_eq!(state.cumulative, -7500);
    }

    #[test]
    fn test_funding_no_deviation_zero_rate() {
        // oracle == mark → premium = 0 → rate = 0
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        client.set_price(&admin, &1_000_000);
        client.set_mark_price(&admin, &1_000_000);

        env.ledger().set(LedgerInfo {
            sequence_number: 5760,
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
        client.set_price(&admin, &1_000_000);
        client.set_mark_price(&admin, &990_000);

        // sequence_number=10 → delta=10 < FUNDING_INTERVAL/2=2880, should skip
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
        assert_eq!(state.last_update, 0, "should not have updated");
        assert_eq!(state.cumulative, 0);
    }

    #[test]
    fn test_funding_skips_without_mark_price() {
        // If mark price not set, update_funding should return without updating
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        client.set_price(&admin, &1_000_000);
        // no set_mark_price call

        env.ledger().set(LedgerInfo {
            sequence_number: 5760,
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
        assert_eq!(state.cumulative, 0, "no update without mark price");
    }

    #[test]
    fn test_get_match_record_nonexistent() {
        let (env, cid, _admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        assert!(client.get_match_record(&999).is_none());
    }

    #[test]
    fn test_add_margin_open_position() {
        let (env, cid, _admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let depositor = Address::generate(&env);
        let cfg = client.get_config();
        let asset = StellarAssetClient::new(&env, &cfg.token);

        let margin_amount: u64 = 500;
        let secret: u64 = 111_222;
        let (cmt_hex, null_hex, proof_json) = gen_note_proof(margin_amount, secret);
        let note_cmt = hex_to_bytes32(&env, &cmt_hex);
        let nullifier = hex_to_bytes32(&env, &null_hex);

        asset.mint(&depositor, &(margin_amount as i128));
        client.deposit_note(&depositor, &note_cmt, &(margin_amount as i128));

        let pos_cmt = BytesN::from_array(&env, &[1u8; 32]);
        create_position(
            &env,
            &cid,
            &pos_cmt,
            1000,
            5,
            0,
            100,
            PositionStatus::Open,
            0,
        );

        let proof = make_groth16_proof(&env, &proof_json);
        client.add_margin_from_note(&note_cmt, &nullifier, &pos_cmt, &proof);

        let pos = client.get_position(&pos_cmt).unwrap();
        assert_eq!(pos.collateral, 1500);
        assert!(client.is_spent(&nullifier));
    }

    #[test]
    fn test_add_margin_matched_position() {
        let (env, cid, _admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let depositor = Address::generate(&env);
        let cfg = client.get_config();
        let asset = StellarAssetClient::new(&env, &cfg.token);

        let margin_amount: u64 = 300;
        let secret: u64 = 222_333;
        let (cmt_hex, null_hex, proof_json) = gen_note_proof(margin_amount, secret);
        let note_cmt = hex_to_bytes32(&env, &cmt_hex);
        let nullifier = hex_to_bytes32(&env, &null_hex);

        asset.mint(&depositor, &(margin_amount as i128));
        client.deposit_note(&depositor, &note_cmt, &(margin_amount as i128));

        let pos_cmt = BytesN::from_array(&env, &[2u8; 32]);
        create_position(
            &env,
            &cid,
            &pos_cmt,
            1000,
            10,
            1,
            200,
            PositionStatus::Matched,
            1,
        );

        let proof = make_groth16_proof(&env, &proof_json);
        client.add_margin_from_note(&note_cmt, &nullifier, &pos_cmt, &proof);

        let pos = client.get_position(&pos_cmt).unwrap();
        assert_eq!(pos.collateral, 1300);
        assert!(client.is_spent(&nullifier));
    }

    #[test]
    #[should_panic(expected = "note not found")]
    fn test_add_margin_insufficient_balance() {
        let (env, cid, _admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);

        let pos_cmt = BytesN::from_array(&env, &[3u8; 32]);
        create_position(
            &env,
            &cid,
            &pos_cmt,
            1000,
            5,
            0,
            100,
            PositionStatus::Open,
            0,
        );

        // No note deposited — add_margin_from_note should fail
        let fake_note = BytesN::from_array(&env, &[99u8; 32]);
        let fake_null = BytesN::from_array(&env, &[0u8; 32]);
        use soroban_sdk::crypto::bn254::{Bn254G1Affine, Bn254G2Affine};
        let dummy = Groth16Proof {
            a: Bn254G1Affine::from_bytes(BytesN::from_array(&env, &[0u8; 64])),
            b: Bn254G2Affine::from_bytes(BytesN::from_array(&env, &[0u8; 128])),
            c: Bn254G1Affine::from_bytes(BytesN::from_array(&env, &[0u8; 64])),
        };
        client.add_margin_from_note(&fake_note, &fake_null, &pos_cmt, &dummy);
    }

    #[test]
    fn test_withdraw_note_full_proof() {
        let (env, cid, _admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let depositor = Address::generate(&env);
        let recipient = Address::generate(&env);
        let cfg = client.get_config();
        let asset = StellarAssetClient::new(&env, &cfg.token);

        let amount: u64 = 1_000_000;
        let secret: u64 = 777_999;
        let (cmt_hex, null_hex, proof_json) = gen_note_proof(amount, secret);

        let note_cmt = hex_to_bytes32(&env, &cmt_hex);
        let nullifier = hex_to_bytes32(&env, &null_hex);

        // Deposit
        asset.mint(&depositor, &(amount as i128));
        client.deposit_note(&depositor, &note_cmt, &(amount as i128));
        assert_eq!(client.get_note(&note_cmt), Some(amount as i128));

        // Withdraw to a different recipient — the privacy claim
        let proof = make_groth16_proof(&env, &proof_json);
        client.withdraw_note(&note_cmt, &nullifier, &recipient, &proof);

        // Note nullifier is spent, can't withdraw again
        assert!(client.is_spent(&nullifier));
    }

    #[test]
    #[should_panic(expected = "nullifier already spent")]
    fn test_withdraw_note_double_spend_reverts() {
        let (env, cid, _admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let depositor = Address::generate(&env);
        let recipient = Address::generate(&env);
        let cfg = client.get_config();
        let asset = StellarAssetClient::new(&env, &cfg.token);

        let amount: u64 = 500_000;
        let secret: u64 = 123_456;
        let (cmt_hex, null_hex, proof_json) = gen_note_proof(amount, secret);
        let note_cmt = hex_to_bytes32(&env, &cmt_hex);
        let nullifier = hex_to_bytes32(&env, &null_hex);

        asset.mint(&depositor, &(amount as i128));
        client.deposit_note(&depositor, &note_cmt, &(amount as i128));

        let proof = make_groth16_proof(&env, &proof_json);
        client.withdraw_note(&note_cmt, &nullifier, &recipient, &proof.clone());
        // second spend must panic
        client.withdraw_note(&note_cmt, &nullifier, &recipient, &proof);
    }

    #[test]
    #[should_panic(expected = "note not found")]
    fn test_withdraw_note_nonexistent_reverts() {
        let (env, cid, _admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let recipient = Address::generate(&env);

        let amount: u64 = 100_000;
        let secret: u64 = 9999;
        let (cmt_hex, null_hex, proof_json) = gen_note_proof(amount, secret);
        let note_cmt = hex_to_bytes32(&env, &cmt_hex);
        let nullifier = hex_to_bytes32(&env, &null_hex);

        // No deposit — note doesn't exist
        let proof = make_groth16_proof(&env, &proof_json);
        client.withdraw_note(&note_cmt, &nullifier, &recipient, &proof);
    }

    #[test]
    fn test_add_margin_from_note_full_proof() {
        let (env, cid, _admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let depositor = Address::generate(&env);
        let _pos_owner = Address::generate(&env);
        let cfg = client.get_config();
        let asset = StellarAssetClient::new(&env, &cfg.token);

        let margin_amount: u64 = 250_000;
        let secret: u64 = 888_111;
        let (cmt_hex, null_hex, proof_json) = gen_note_proof(margin_amount, secret);
        let note_cmt = hex_to_bytes32(&env, &cmt_hex);
        let nullifier = hex_to_bytes32(&env, &null_hex);

        // Deposit the note
        asset.mint(&depositor, &(margin_amount as i128));
        client.deposit_note(&depositor, &note_cmt, &(margin_amount as i128));

        // Create a position for pos_owner directly
        let pos_cmt = BytesN::from_array(&env, &[55u8; 32]);
        create_position(
            &env,
            &cid,
            &pos_cmt,
            1_000_000,
            5,
            0,
            100,
            PositionStatus::Open,
            0,
        );

        // Add margin from note (no address required — proof authorizes it)
        let proof = make_groth16_proof(&env, &proof_json);
        client.add_margin_from_note(&note_cmt, &nullifier, &pos_cmt, &proof);

        let pos = client.get_position(&pos_cmt).unwrap();
        assert_eq!(pos.collateral, 1_000_000 + margin_amount as i128);
        assert!(client.is_spent(&nullifier));
    }

    #[test]
    #[should_panic(expected = "note not found")]
    fn test_add_margin_from_note_nonexistent_reverts() {
        let (env, cid, _admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let _pos_owner = Address::generate(&env);

        let amount: u64 = 100_000;
        let secret: u64 = 42;
        let (cmt_hex, null_hex, proof_json) = gen_note_proof(amount, secret);
        let note_cmt = hex_to_bytes32(&env, &cmt_hex);
        let nullifier = hex_to_bytes32(&env, &null_hex);
        let pos_cmt = BytesN::from_array(&env, &[66u8; 32]);
        create_position(
            &env,
            &cid,
            &pos_cmt,
            500_000,
            5,
            0,
            100,
            PositionStatus::Open,
            0,
        );

        let proof = make_groth16_proof(&env, &proof_json);
        client.add_margin_from_note(&note_cmt, &nullifier, &pos_cmt, &proof);
    }

    #[test]
    fn test_deposit_note_stores_note() {
        let (env, cid, _admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let depositor = Address::generate(&env);
        let cfg = client.get_config();
        let asset = StellarAssetClient::new(&env, &cfg.token);
        asset.mint(&depositor, &1000);

        let note_cmt = BytesN::from_array(&env, &[10u8; 32]);
        client.deposit_note(&depositor, &note_cmt, &1000);

        assert_eq!(client.get_note(&note_cmt), Some(1000));
    }

    #[test]
    #[should_panic(expected = "deposit amount must be positive")]
    fn test_deposit_note_zero_amount_reverts() {
        let (env, cid, _admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let depositor = Address::generate(&env);
        let note_cmt = BytesN::from_array(&env, &[11u8; 32]);
        client.deposit_note(&depositor, &note_cmt, &0);
    }

    #[test]
    #[should_panic(expected = "note commitment already exists")]
    fn test_deposit_note_duplicate_reverts() {
        let (env, cid, _admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let depositor = Address::generate(&env);
        let cfg = client.get_config();
        let asset = StellarAssetClient::new(&env, &cfg.token);
        asset.mint(&depositor, &2000);

        let note_cmt = BytesN::from_array(&env, &[12u8; 32]);
        client.deposit_note(&depositor, &note_cmt, &1000);
        client.deposit_note(&depositor, &note_cmt, &500);
    }

    #[test]
    fn test_get_note_nonexistent_returns_none() {
        let (env, cid, _admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let note_cmt = BytesN::from_array(&env, &[99u8; 32]);
        assert_eq!(client.get_note(&note_cmt), None);
    }

    #[test]
    fn test_open_position_from_note_full_proof() {
        let (env, cid, _admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let depositor = Address::generate(&env);
        let _liq_recipient = Address::generate(&env);
        let cfg = client.get_config();
        let asset = StellarAssetClient::new(&env, &cfg.token);

        let amount: u64 = 5_000_000;
        let note_secret: u64 = 444_777;
        let (note_cmt_hex, note_null_hex, note_proof_json) = gen_note_proof(amount, note_secret);
        let note_cmt = hex_to_bytes32(&env, &note_cmt_hex);
        let note_null = hex_to_bytes32(&env, &note_null_hex);

        let order_secret: u64 = 12_345_678;
        let (pos_cmt_hex, commit_proof_json) =
            gen_commit_proof(0, 100_000_000, 1, 10, 42, order_secret);
        let pos_cmt = hex_to_bytes32(&env, &pos_cmt_hex);

        asset.mint(&depositor, &(amount as i128));
        client.deposit_note(&depositor, &note_cmt, &(amount as i128));

        let note_proof = make_groth16_proof(&env, &note_proof_json);
        let commit_proof = make_groth16_proof(&env, &commit_proof_json);
        client.open_position_from_note(
            &note_cmt,
            &note_null,
            &pos_cmt,
            &100_000_000,
            &0,
            &10,
            &1_000_000_000u64,
            &TimeInForce::GTC,
            &0u64,
            &0u64,
            &0u64,
            &BytesN::from_array(&env, &[0u8; 32]),
            &BytesN::from_array(&env, &[0u8; 32]), // portfolio_key: isolated
            &BytesN::from_array(&env, &[0u8; 32]), // asset_id: default
            &note_proof,
            &commit_proof,
        );

        assert!(client.is_spent(&note_null));
        let pos = client.get_position(&pos_cmt).unwrap();
        assert_eq!(pos.collateral, amount as i128);
        assert_eq!(pos.status, PositionStatus::Open);
        assert_eq!(pos.side, 0);
        assert_eq!(pos.leverage, 10);
    }

    #[test]
    #[should_panic(expected = "note not found")]
    fn test_open_position_from_note_no_deposit_reverts() {
        let (env, cid, _admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let _liq_recipient = Address::generate(&env);

        let (note_cmt_hex, note_null_hex, note_proof_json) = gen_note_proof(1_000_000, 111);
        let note_cmt = hex_to_bytes32(&env, &note_cmt_hex);
        let note_null = hex_to_bytes32(&env, &note_null_hex);
        let (pos_cmt_hex, commit_proof_json) = gen_commit_proof(0, 100_000_000, 1, 5, 1, 999);
        let pos_cmt = hex_to_bytes32(&env, &pos_cmt_hex);

        // No deposit_note — must panic with "note not found"
        let note_proof = make_groth16_proof(&env, &note_proof_json);
        let commit_proof = make_groth16_proof(&env, &commit_proof_json);
        client.open_position_from_note(
            &note_cmt,
            &note_null,
            &pos_cmt,
            &100_000_000,
            &0,
            &5,
            &1_000_000_000u64,
            &TimeInForce::GTC,
            &0u64,
            &0u64,
            &0u64,
            &BytesN::from_array(&env, &[0u8; 32]),
            &BytesN::from_array(&env, &[0u8; 32]), // portfolio_key: isolated
            &BytesN::from_array(&env, &[0u8; 32]), // asset_id: default
            &note_proof,
            &commit_proof,
        );
    }

    #[test]
    fn test_cancel_position_to_note_full_proof() {
        // Full cycle: deposit_note → open_position_from_note → cancel_position_to_note → withdraw_note
        let (env, cid, _admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let depositor = Address::generate(&env);
        let recipient = Address::generate(&env);
        let _liq_recipient = Address::generate(&env);
        let cfg = client.get_config();
        let asset = StellarAssetClient::new(&env, &cfg.token);

        let amount: u64 = 3_000_000;
        let note_secret: u64 = 55_500;
        let (note_cmt_hex, note_null_hex, note_proof_json) = gen_note_proof(amount, note_secret);
        let note_cmt = hex_to_bytes32(&env, &note_cmt_hex);
        let note_null = hex_to_bytes32(&env, &note_null_hex);

        let order_secret: u64 = 99_887_766;
        let (pos_cmt_hex, commit_proof_json) =
            gen_commit_proof(0, 50_000_000, 1, 5, 7, order_secret);
        let pos_cmt = hex_to_bytes32(&env, &pos_cmt_hex);

        let (cancel_null_hex, cancel_proof_json) = gen_cancel_proof(&pos_cmt_hex, order_secret);
        let cancel_null = hex_to_bytes32(&env, &cancel_null_hex);

        // Settlement note: amount=0 sentinel. The actual refund amount comes from storage.
        let settle_secret: u64 = 123_456_789;
        let (settle_cmt_hex, settle_null_hex, settle_proof_json) = gen_note_proof(0, settle_secret);
        let settle_cmt = hex_to_bytes32(&env, &settle_cmt_hex);
        let settle_null = hex_to_bytes32(&env, &settle_null_hex);

        asset.mint(&depositor, &(amount as i128));
        client.deposit_note(&depositor, &note_cmt, &(amount as i128));

        let note_proof = make_groth16_proof(&env, &note_proof_json);
        let commit_proof = make_groth16_proof(&env, &commit_proof_json);
        client.open_position_from_note(
            &note_cmt,
            &note_null,
            &pos_cmt,
            &50_000_000,
            &0,
            &5,
            &1_000_000_000u64,
            &TimeInForce::GTC,
            &0u64,
            &0u64,
            &0u64,
            &BytesN::from_array(&env, &[0u8; 32]),
            &BytesN::from_array(&env, &[0u8; 32]), // portfolio_key: isolated
            &BytesN::from_array(&env, &[0u8; 32]), // asset_id: default
            &note_proof,
            &commit_proof,
        );

        let cancel_proof = make_groth16_proof(&env, &cancel_proof_json);
        client.cancel_position_to_note(&pos_cmt, &cancel_null, &settle_cmt, &cancel_proof);

        // Settlement note holds the full collateral refund
        assert_eq!(client.get_note(&settle_cmt), Some(amount as i128));
        assert!(client.is_spent(&cancel_null));
        assert_eq!(
            client.get_position(&pos_cmt).unwrap().status,
            PositionStatus::Cancelled
        );

        // Withdraw the settlement note using the amount=0 sentinel proof
        let settle_proof = make_groth16_proof(&env, &settle_proof_json);
        client.withdraw_note(&settle_cmt, &settle_null, &recipient, &settle_proof);
        assert!(client.is_spent(&settle_null));
    }

    #[test]
    fn test_close_position_to_note_full_proof() {
        // Full cycle: deposit_note → open_position_from_note → (match) → close_position_to_note → withdraw_note
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let depositor = Address::generate(&env);
        let recipient = Address::generate(&env);
        let _liq_recipient = Address::generate(&env);
        let cfg = client.get_config();
        let asset = StellarAssetClient::new(&env, &cfg.token);

        let amount: u64 = 10_000_000;
        let note_secret: u64 = 777_001;
        let (note_cmt_hex, note_null_hex, note_proof_json) = gen_note_proof(amount, note_secret);
        let note_cmt = hex_to_bytes32(&env, &note_cmt_hex);
        let note_null = hex_to_bytes32(&env, &note_null_hex);

        let order_secret: u64 = 55_000_001;
        let (pos_cmt_hex, commit_proof_json) =
            gen_commit_proof(0, 100_000_000, 1, 5, 100, order_secret);
        let pos_cmt = hex_to_bytes32(&env, &pos_cmt_hex);

        let (close_null_hex, close_proof_json) = gen_cancel_proof(&pos_cmt_hex, order_secret);
        let close_null = hex_to_bytes32(&env, &close_null_hex);

        let settle_secret: u64 = 999_001;
        let (settle_cmt_hex, settle_null_hex, settle_proof_json) = gen_note_proof(0, settle_secret);
        let settle_cmt = hex_to_bytes32(&env, &settle_cmt_hex);
        let settle_null = hex_to_bytes32(&env, &settle_null_hex);

        client.set_price(&admin, &100_000_000);

        asset.mint(&depositor, &(amount as i128));
        client.deposit_note(&depositor, &note_cmt, &(amount as i128));

        let note_proof = make_groth16_proof(&env, &note_proof_json);
        let commit_proof = make_groth16_proof(&env, &commit_proof_json);
        client.open_position_from_note(
            &note_cmt,
            &note_null,
            &pos_cmt,
            &100_000_000,
            &0,
            &5,
            &1_000_000_000u64,
            &TimeInForce::GTC,
            &0u64,
            &0u64,
            &0u64,
            &BytesN::from_array(&env, &[0u8; 32]),
            &BytesN::from_array(&env, &[0u8; 32]), // portfolio_key: isolated
            &BytesN::from_array(&env, &[0u8; 32]), // asset_id: default
            &note_proof,
            &commit_proof,
        );

        // Simulate match (skip match proof in unit test)
        env.as_contract(&cid, || {
            let key = DataKey::Position(pos_cmt.clone());
            let mut meta: PositionMeta = env.storage().persistent().get(&key).unwrap();
            meta.status = PositionStatus::Matched;
            meta.matched_price = 100_000_000;
            env.storage().persistent().set(&key, &meta);
        });

        // Close to note — oracle price = entry price → settlement = collateral
        let close_proof = make_groth16_proof(&env, &close_proof_json);
        let settlement =
            client.close_position_to_note(&pos_cmt, &close_null, &settle_cmt, &close_proof);

        assert_eq!(settlement, amount as i128);
        assert_eq!(client.get_note(&settle_cmt), Some(settlement));
        assert!(client.is_spent(&close_null));
        assert_eq!(
            client.get_position(&pos_cmt).unwrap().status,
            PositionStatus::Closed
        );

        // Withdraw settlement note with amount=0 sentinel proof — pays out stored settlement
        let settle_proof = make_groth16_proof(&env, &settle_proof_json);
        client.withdraw_note(&settle_cmt, &settle_null, &recipient, &settle_proof);
        assert!(client.is_spent(&settle_null));
    }

    #[test]
    #[should_panic(expected = "can only cancel an open position")]
    fn test_cancel_position_to_note_wrong_status_reverts() {
        let (env, cid, _admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let _owner = Address::generate(&env);
        let pos_cmt = BytesN::from_array(&env, &[77u8; 32]);
        let recipient_note = BytesN::from_array(&env, &[88u8; 32]);
        create_position(
            &env,
            &cid,
            &pos_cmt,
            1_000,
            5,
            0,
            100,
            PositionStatus::Matched,
            1,
        );

        let order_secret: u64 = 54321;
        let (_, cancel_proof_json) = gen_commit_proof(0, 100, 1, 5, 1, order_secret);
        let fake_null = BytesN::from_array(&env, &[0u8; 32]);
        let proof = make_groth16_proof(&env, &cancel_proof_json);
        client.cancel_position_to_note(&pos_cmt, &fake_null, &recipient_note, &proof);
    }

    #[test]
    #[should_panic(expected = "can only close a matched position")]
    fn test_close_position_to_note_wrong_status_reverts() {
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let _owner = Address::generate(&env);
        let pos_cmt = BytesN::from_array(&env, &[78u8; 32]);
        let recipient_note = BytesN::from_array(&env, &[89u8; 32]);
        create_position(
            &env,
            &cid,
            &pos_cmt,
            1_000,
            5,
            0,
            100,
            PositionStatus::Open,
            0,
        );
        client.set_price(&admin, &100);

        let (_, proof_json) = gen_commit_proof(0, 100, 1, 5, 1, 54321);
        let fake_null = BytesN::from_array(&env, &[0u8; 32]);
        let proof = make_groth16_proof(&env, &proof_json);
        client.close_position_to_note(&pos_cmt, &fake_null, &recipient_note, &proof);
    }

    #[test]
    #[should_panic(expected = "can only add margin")]
    fn test_add_margin_closed_position_reverts() {
        let (env, cid, _admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let depositor = Address::generate(&env);
        let cfg = client.get_config();
        let asset = StellarAssetClient::new(&env, &cfg.token);

        let margin_amount: u64 = 500;
        let secret: u64 = 333_444;
        let (cmt_hex, null_hex, proof_json) = gen_note_proof(margin_amount, secret);
        let note_cmt = hex_to_bytes32(&env, &cmt_hex);
        let nullifier = hex_to_bytes32(&env, &null_hex);

        asset.mint(&depositor, &(margin_amount as i128));
        client.deposit_note(&depositor, &note_cmt, &(margin_amount as i128));

        let pos_cmt = BytesN::from_array(&env, &[4u8; 32]);
        create_position(
            &env,
            &cid,
            &pos_cmt,
            1000,
            5,
            0,
            100,
            PositionStatus::Closed,
            0,
        );

        let proof = make_groth16_proof(&env, &proof_json);
        client.add_margin_from_note(&note_cmt, &nullifier, &pos_cmt, &proof);
    }

    // ── FOK / IOC / GTD tests ──────────────────────────────────────────────

    fn create_position_tif(
        env: &Env,
        cid: &Address,
        commitment: &BytesN<32>,
        collateral: i128,
        leverage: u64,
        side: u64,
        price: u64,
        hint_size: u64,
        tif: TimeInForce,
        expiry_ledger: u64,
    ) {
        env.as_contract(cid, || {
            let meta = PositionMeta {
                collateral,
                entry_price: price,
                matched_price: 0,
                side,
                leverage,
                status: PositionStatus::Open,
                created_at: env.ledger().sequence() as u64,
                match_id: 0,
                funding_at_open: 0,
                hint_size,
                tif,
                expiry_ledger,
                tp_price: 0,
                sl_price: 0,
                effective_collateral: collateral,
                partial_liq_done: false,
                liquidation_recipient_note: BytesN::from_array(env, &[0u8; 32]),
                asset_id: BytesN::from_array(env, &[0u8; 32]),
                margin_mode: MarginMode::Isolated,
                portfolio_key: BytesN::from_array(env, &[0u8; 32]),
            };
            let key = DataKey::Position(commitment.clone());
            env.storage().persistent().set(&key, &meta);
            env.storage().persistent().extend_ttl(&key, 17280, 17280);
        });
    }

    #[test]
    #[should_panic(expected = "FOK/IOC order A requires full fill")]
    fn test_fok_wrong_size_reverts() {
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let _owner_a = Address::generate(&env);
        let _owner_b = Address::generate(&env);
        // FOK position A with hint_size=1000, match_size will be 500
        let cmt_a = BytesN::from_array(&env, &[20u8; 32]);
        let cmt_b = BytesN::from_array(&env, &[21u8; 32]);
        create_position_tif(
            &env,
            &cid,
            &cmt_a,
            1_000_000,
            5,
            0,
            100,
            1000,
            TimeInForce::FOK,
            0,
        );
        create_position_tif(
            &env,
            &cid,
            &cmt_b,
            1_000_000,
            5,
            1,
            100,
            500,
            TimeInForce::GTC,
            0,
        );

        // Dummy proof bytes — TIF check fires before proof verification
        use soroban_sdk::crypto::bn254::{Bn254G1Affine, Bn254G2Affine};
        let dummy = Groth16Proof {
            a: Bn254G1Affine::from_bytes(BytesN::from_array(&env, &[0u8; 64])),
            b: Bn254G2Affine::from_bytes(BytesN::from_array(&env, &[0u8; 128])),
            c: Bn254G1Affine::from_bytes(BytesN::from_array(&env, &[0u8; 64])),
        };
        let mp = BytesN::from_array(&env, &{
            let mut b = [0u8; 32];
            b[31] = 100;
            b
        });
        // match_size = 500 (field element with last byte = 500>>8=1, 500&255=244)
        let ms = BytesN::from_array(&env, &{
            let mut b = [0u8; 32];
            b[30] = 1;
            b[31] = 244;
            b
        });
        let nf_a = BytesN::from_array(&env, &[30u8; 32]);
        let nf_b = BytesN::from_array(&env, &[31u8; 32]);
        client.set_price(&admin, &100);
        client.match_positions(&cmt_a, &cmt_b, &nf_a, &nf_b, &mp, &ms, &dummy);
    }

    #[test]
    #[should_panic(expected = "FOK/IOC order A requires full fill")]
    fn test_ioc_wrong_size_reverts() {
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let _owner_a = Address::generate(&env);
        let _owner_b = Address::generate(&env);
        let cmt_a = BytesN::from_array(&env, &[22u8; 32]);
        let cmt_b = BytesN::from_array(&env, &[23u8; 32]);
        // IOC position with hint_size=2000, match only 1000
        create_position_tif(
            &env,
            &cid,
            &cmt_a,
            1_000_000,
            5,
            0,
            100,
            2000,
            TimeInForce::IOC,
            0,
        );
        create_position_tif(
            &env,
            &cid,
            &cmt_b,
            1_000_000,
            5,
            1,
            100,
            1000,
            TimeInForce::GTC,
            0,
        );

        use soroban_sdk::crypto::bn254::{Bn254G1Affine, Bn254G2Affine};
        let dummy = Groth16Proof {
            a: Bn254G1Affine::from_bytes(BytesN::from_array(&env, &[0u8; 64])),
            b: Bn254G2Affine::from_bytes(BytesN::from_array(&env, &[0u8; 128])),
            c: Bn254G1Affine::from_bytes(BytesN::from_array(&env, &[0u8; 64])),
        };
        let mp = BytesN::from_array(&env, &{
            let mut b = [0u8; 32];
            b[31] = 100;
            b
        });
        let ms = BytesN::from_array(&env, &{
            let mut b = [0u8; 32];
            b[30] = 3;
            b[31] = 232;
            b
        }); // 1000
        let nf_a = BytesN::from_array(&env, &[32u8; 32]);
        let nf_b = BytesN::from_array(&env, &[33u8; 32]);
        client.set_price(&admin, &100);
        client.match_positions(&cmt_a, &cmt_b, &nf_a, &nf_b, &mp, &ms, &dummy);
    }

    #[test]
    #[should_panic(expected = "order A has expired")]
    fn test_gtd_expired_match_reverts() {
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        env.ledger().set(LedgerInfo {
            sequence_number: 200,
            timestamp: 0,
            network_id: [0; 32],
            protocol_version: 27,
            base_reserve: 0,
            min_persistent_entry_ttl: 4096,
            min_temp_entry_ttl: 16,
            max_entry_ttl: 6_312_000,
        });
        let _owner_a = Address::generate(&env);
        let _owner_b = Address::generate(&env);
        let cmt_a = BytesN::from_array(&env, &[24u8; 32]);
        let cmt_b = BytesN::from_array(&env, &[25u8; 32]);
        // GTD expiry_ledger=100, current ledger=200 → expired
        create_position_tif(
            &env,
            &cid,
            &cmt_a,
            1_000_000,
            5,
            0,
            100,
            1000,
            TimeInForce::GTD,
            100,
        );
        create_position_tif(
            &env,
            &cid,
            &cmt_b,
            1_000_000,
            5,
            1,
            100,
            1000,
            TimeInForce::GTC,
            0,
        );

        use soroban_sdk::crypto::bn254::{Bn254G1Affine, Bn254G2Affine};
        let dummy = Groth16Proof {
            a: Bn254G1Affine::from_bytes(BytesN::from_array(&env, &[0u8; 64])),
            b: Bn254G2Affine::from_bytes(BytesN::from_array(&env, &[0u8; 128])),
            c: Bn254G1Affine::from_bytes(BytesN::from_array(&env, &[0u8; 64])),
        };
        let mp = BytesN::from_array(&env, &{
            let mut b = [0u8; 32];
            b[31] = 100;
            b
        });
        let ms = BytesN::from_array(&env, &{
            let mut b = [0u8; 32];
            b[30] = 3;
            b[31] = 232;
            b
        });
        let nf_a = BytesN::from_array(&env, &[34u8; 32]);
        let nf_b = BytesN::from_array(&env, &[35u8; 32]);
        client.set_price(&admin, &100);
        client.match_positions(&cmt_a, &cmt_b, &nf_a, &nf_b, &mp, &ms, &dummy);
    }

    // ── TP / SL tests ─────────────────────────────────────────────────────

    fn matched_position_with_tpsl(
        env: &Env,
        cid: &Address,
        commitment: &BytesN<32>,
        collateral: i128,
        leverage: u64,
        side: u64,
        entry_price: u64,
        tp_price: u64,
        sl_price: u64,
    ) {
        env.as_contract(cid, || {
            let meta = PositionMeta {
                collateral,
                entry_price,
                matched_price: entry_price,
                side,
                leverage,
                status: PositionStatus::Matched,
                created_at: env.ledger().sequence() as u64,
                match_id: 0,
                funding_at_open: 0,
                hint_size: 1_000_000_000,
                tif: TimeInForce::GTC,
                expiry_ledger: 0,
                tp_price,
                sl_price,
                effective_collateral: collateral,
                partial_liq_done: false,
                liquidation_recipient_note: BytesN::from_array(env, &[0u8; 32]),
                asset_id: BytesN::from_array(env, &[0u8; 32]),
                margin_mode: MarginMode::Isolated,
                portfolio_key: BytesN::from_array(env, &[0u8; 32]),
            };
            let key = DataKey::Position(commitment.clone());
            env.storage().persistent().set(&key, &meta);
            env.storage().persistent().extend_ttl(&key, 17280, 17280);
        });
    }

    #[test]
    fn test_trigger_tp_long() {
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let _owner = Address::generate(&env);
        let cmt = BytesN::from_array(&env, &[50u8; 32]);
        // Long position: entry=100, tp=120. Oracle moves to 130 → TP triggers.
        matched_position_with_tpsl(&env, &cid, &cmt, 1_000_000, 1, 0, 100, 120, 80);
        client.set_price(&admin, &130);

        let settlement = client.trigger_tp(&cmt);
        // PnL: collateral * lev * (130-100)/100 = 1M*1*(30/100) = 300k; settlement = 1.3M
        assert_eq!(settlement, 1_300_000);
        assert_eq!(
            client.get_position(&cmt).unwrap().status,
            PositionStatus::Closed
        );
    }

    #[test]
    fn test_trigger_tp_short() {
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let _owner = Address::generate(&env);
        let cmt = BytesN::from_array(&env, &[51u8; 32]);
        // Short position: entry=100, tp=80. Oracle drops to 70 → TP triggers.
        matched_position_with_tpsl(&env, &cid, &cmt, 1_000_000, 1, 1, 100, 80, 120);
        client.set_price(&admin, &70);

        let settlement = client.trigger_tp(&cmt);
        // Short PnL: -(close-entry)/entry * collateral * lev = -(70-100)/100 * 1M = +300k
        assert_eq!(settlement, 1_300_000);
        assert_eq!(
            client.get_position(&cmt).unwrap().status,
            PositionStatus::Closed
        );
    }

    #[test]
    #[should_panic(expected = "TP not triggered")]
    fn test_trigger_tp_long_not_reached_reverts() {
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let _owner = Address::generate(&env);
        let cmt = BytesN::from_array(&env, &[52u8; 32]);
        matched_position_with_tpsl(&env, &cid, &cmt, 1_000_000, 1, 0, 100, 120, 80);
        client.set_price(&admin, &110); // below tp=120
        client.trigger_tp(&cmt);
    }

    #[test]
    fn test_trigger_sl_long() {
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let _owner = Address::generate(&env);
        let cmt = BytesN::from_array(&env, &[53u8; 32]);
        // Long position: entry=100, sl=80. Oracle drops to 70 → SL triggers.
        matched_position_with_tpsl(&env, &cid, &cmt, 1_000_000, 1, 0, 100, 130, 80);
        client.set_price(&admin, &70);

        let settlement = client.trigger_sl(&cmt);
        // Long PnL: (70-100)/100 * 1M = -300k; settlement = max(0, 700k)
        assert_eq!(settlement, 700_000);
        assert_eq!(
            client.get_position(&cmt).unwrap().status,
            PositionStatus::Closed
        );
    }

    #[test]
    fn test_trigger_sl_short() {
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let _owner = Address::generate(&env);
        let cmt = BytesN::from_array(&env, &[54u8; 32]);
        // Short position: entry=100, sl=120. Oracle rises to 130 → SL triggers.
        matched_position_with_tpsl(&env, &cid, &cmt, 1_000_000, 1, 1, 100, 80, 120);
        client.set_price(&admin, &130);

        let settlement = client.trigger_sl(&cmt);
        // Short PnL: -(130-100)/100 * 1M = -300k; settlement = 700k
        assert_eq!(settlement, 700_000);
        assert_eq!(
            client.get_position(&cmt).unwrap().status,
            PositionStatus::Closed
        );
    }

    #[test]
    #[should_panic(expected = "SL not triggered")]
    fn test_trigger_sl_long_not_reached_reverts() {
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let _owner = Address::generate(&env);
        let cmt = BytesN::from_array(&env, &[55u8; 32]);
        matched_position_with_tpsl(&env, &cid, &cmt, 1_000_000, 1, 0, 100, 130, 80);
        client.set_price(&admin, &90); // above sl=80
        client.trigger_sl(&cmt);
    }

    #[test]
    #[should_panic(expected = "no TP price set")]
    fn test_trigger_tp_no_price_set_reverts() {
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let _owner = Address::generate(&env);
        let cmt = BytesN::from_array(&env, &[56u8; 32]);
        create_position(
            &env,
            &cid,
            &cmt,
            1_000_000,
            1,
            0,
            100,
            PositionStatus::Matched,
            0,
        );
        client.set_price(&admin, &200);
        client.trigger_tp(&cmt);
    }

    #[test]
    #[should_panic(expected = "no SL price set")]
    fn test_trigger_sl_no_price_set_reverts() {
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let _owner = Address::generate(&env);
        let cmt = BytesN::from_array(&env, &[57u8; 32]);
        create_position(
            &env,
            &cid,
            &cmt,
            1_000_000,
            1,
            0,
            100,
            PositionStatus::Matched,
            0,
        );
        client.set_price(&admin, &50);
        client.trigger_sl(&cmt);
    }

    // ── Tiered liquidation tests ──────────────────────────────────────────

    #[test]
    fn test_partial_liquidation_tier1() {
        // Tier 1 fires when position is below maintenance margin but has positive settlement.
        // Oracle drops to 90% of entry (long, 10x → settlement = collateral - 10% * 10 = 0)
        // We need settlement > 0 but < mm. Use 10x long, entry=100, oracle=94 → pnl=−60%, eff_col=40
        // Actually let's use simpler numbers: collateral=1_000_000, leverage=5, long, entry=100, oracle=91
        // notional = 5_000_000; pnl = (91-100)/100 * 5_000_000 = −450_000
        // settlement = 1_000_000 - 450_000 = 550_000; mm = 1_000_000 * 500/10000 = 50_000
        // settlement(550_000) > mm(50_000) → NOT liquidatable. Need bigger drop.
        // entry=100, oracle=60: pnl = (60-100)/100 * 5_000_000 = −2_000_000
        // settlement = 1_000_000 - 2_000_000 = −1_000_000 < 0 → skip to tier 2 directly
        // For tier 1 we need: 0 < settlement < mm
        // mm = collateral * 500/10000 = collateral/20
        // settlement = collateral + pnl; pnl = (oracle-entry)/entry * collateral * leverage
        // Need: 0 < collateral + pnl < collateral/20
        // pnl ≈ −collateral for tight range. Leverage=1 makes it easier:
        // collateral=1_000_000, leverage=1, long, entry=10_000_000, oracle=9_900_000
        // notional=1_000_000; pnl=(9.9M−10M)/10M * 1_000_000 = −10_000
        // settlement = 1_000_000 - 10_000 = 990_000; mm=50_000 → still solvent
        // Use leverage=20: notional=20_000_000; pnl=(9.9M−10M)/10M*20_000_000=−200_000
        // settlement=800_000; mm=50_000 → still not liq
        // The key: settlement must be < mm = collateral * 5% = 50_000
        // settlement = col + (oracle-entry)/entry * col * lev
        // For col=1_000_000, lev=10, entry=100, oracle=x:
        // settlement = 1_000_000 + (x-100)/100 * 10_000_000 = 1_000_000 + (x-100)*100_000
        // For 0 < settlement < 50_000: 0 < 1_000_000 + (x-100)*100_000 < 50_000
        // (x-100)*100_000 = -950_000 to -1_000_000 → x = 90.5..90 → use x=91 → s=100_000 > mm
        // Need oracle=90.5 (not integer). Use collateral=2_000_000 instead:
        // mm = 100_000; settlement = 2_000_000 + (x-100)*200_000
        // for x=90: settlement = 2_000_000 - 2_000_000 = 0 → NOT > 0
        // for x=91: settlement = 200_000 > mm(100_000) still solvent
        // Use leverage=20: notional=40_000_000; mm=100_000
        // settlement = 2_000_000 + (x-100)*400_000; for x=95: =2_000_000-2_000_000=0; x=95.2 → s=80_000 < mm but need int
        // Simplest approach: use small collateral so mm > settlement in integer arithmetic
        // col=100, lev=10, entry=100, oracle=91: s=100+(91-100)/100*1000=100-90=10; mm=5 → 10>5 solvent!
        // oracle=90: s=0 (not >0, goes to tier2); oracle=90: (90-100)/100*1000=-100; s=100-100=0
        // Use entry=1000, oracle=901: pnl=(901-1000)/1000*col*10=(−99/1000)*1000=−990; s=10; mm=5 → s>mm solvent
        // oracle=900: pnl=−1000; s=0 → tier2
        // oracle=901 with col=100: s=col+(901-1000)/1000*100*10=100-99=1 (if integer div truncates)
        // Actually in our code: (oracle-entry) as i128 * notional / entry as i128
        // Let's just set it up so settlement=30, mm=50 — need to reverse-engineer from the compute fn
        // SIMPLEST: use col=1000, lev=10, entry=10000, oracle=9951
        // pnl=(9951-10000)*1000*10/10000=(−49*10000)/10000=−490; s=1000-490=510; mm=50 → solvent
        // oracle=9500: pnl=(−500)*10000/10000=−5000; s=1000-5000=−4000 < 0 → tier2
        // oracle=9901: pnl=−9900000/10000=−990; s=10; mm=50 → tier1! 10<50

        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let _owner = Address::generate(&env);
        let _liquidator = Address::generate(&env);
        let cmt = BytesN::from_array(&env, &[60u8; 32]);

        let col: i128 = 1_000;
        let lev: u64 = 10;
        let entry: u64 = 10_000;
        // settlement = col + col*lev*(oracle-entry)/entry = 1000 + (oracle-10000)
        // oracle=9010: settlement=10, mm=col*500/10000=50 → 0<10<50 → tier 1
        let oracle: u64 = 9_010;

        let cfg = client.get_config();
        let token_admin = StellarAssetClient::new(&env, &cfg.token);
        token_admin.mint(&cid, &(col * 10));

        client.set_price(&admin, &oracle);
        create_position(
            &env,
            &cid,
            &cmt,
            col,
            lev,
            0, // long
            entry,
            PositionStatus::Matched,
            0,
        );

        env.mock_all_auths();
        let reward = client.liquidate(&cmt);
        assert!(reward > 0, "liquidator should receive partial reward");

        let pos = client.get_position(&cmt).unwrap();
        assert_eq!(
            pos.status,
            PositionStatus::Matched,
            "partial liq leaves position open"
        );
        assert!(pos.partial_liq_done);
        assert_eq!(pos.effective_collateral, col / 2);
    }

    #[test]
    fn test_full_liquidation_after_partial() {
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let _owner = Address::generate(&env);
        let _liquidator = Address::generate(&env);
        let cmt = BytesN::from_array(&env, &[61u8; 32]);

        // entry=10000, col=1000, lev=10: settlement = col + (oracle-entry)
        // Tier 1 at oracle=9010: settlement=10, mm=50 → partial fires
        // After partial: effective_col=500, matched_price=9010
        //   new settlement = 500 + 500*10*(oracle-9010)/9010
        //   new mm = 500*500/10000 = 25
        // Tier 2 at oracle=8000: settlement≈500+5000*(8000-9010)/9010=500-560=-60 < 0 → full

        let col: i128 = 1_000;
        let entry: u64 = 10_000;

        let cfg = client.get_config();
        let token_admin = StellarAssetClient::new(&env, &cfg.token);
        token_admin.mint(&cid, &(col * 20));

        client.set_price(&admin, &9_010u64);
        create_position(
            &env,
            &cid,
            &cmt,
            col,
            10,
            0,
            entry,
            PositionStatus::Matched,
            0,
        );

        env.mock_all_auths();
        // Tier 1
        client.liquidate(&cmt);

        // Drive oracle low enough for tier 2 (settlement < 0 on halved position)
        client.set_price(&admin, &8_000u64);
        let reward = client.liquidate(&cmt);
        let _ = reward;

        let pos = client.get_position(&cmt).unwrap();
        assert_eq!(pos.status, PositionStatus::Liquidated);
    }

    #[test]
    fn test_full_liquidation_direct_no_partial() {
        // When settlement <= 0, skip straight to tier 2 even if partial_liq_done=false
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let _owner = Address::generate(&env);
        let _liquidator = Address::generate(&env);
        let cmt = BytesN::from_array(&env, &[62u8; 32]);

        let col: i128 = 1_000;
        let entry: u64 = 10_000;

        let cfg = client.get_config();
        let token_admin = StellarAssetClient::new(&env, &cfg.token);
        token_admin.mint(&cid, &(col * 10));

        // settlement = col + (oracle-entry) = 1000 + (oracle-10000)
        // oracle=8900: settlement=-100 ≤ 0 → tier2 directly (skips tier1)
        client.set_price(&admin, &8_900u64);
        create_position(
            &env,
            &cid,
            &cmt,
            col,
            10,
            0,
            entry,
            PositionStatus::Matched,
            0,
        );

        env.mock_all_auths();
        let reward = client.liquidate(&cmt);
        // settlement is negative, ins_fund empty → reward ≈ 0
        let _ = reward;

        let pos = client.get_position(&cmt).unwrap();
        assert_eq!(pos.status, PositionStatus::Liquidated);
        assert!(!pos.partial_liq_done, "partial was never triggered");
    }

    #[test]
    fn test_fund_insurance_and_balance() {
        let (env, cid, _admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let funder = Address::generate(&env);

        let cfg = client.get_config();
        let token_admin = StellarAssetClient::new(&env, &cfg.token);
        token_admin.mint(&funder, &1_000_000);

        assert_eq!(client.insurance_balance(), 0);

        env.mock_all_auths();
        client.fund_insurance(&500_000i128);
        assert_eq!(client.insurance_balance(), 500_000);

        client.fund_insurance(&200_000i128);
        assert_eq!(client.insurance_balance(), 700_000);
    }

    #[test]
    fn test_insurance_fund_covers_shortfall() {
        // When settlement < base_reward, insurance fund covers the shortfall.
        // Net effect: insurance fund is credited with final settlement (clamped ≥0).
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let cmt = BytesN::from_array(&env, &[63u8; 32]);

        let col: i128 = 1_000;
        let entry: u64 = 10_000;

        let cfg = client.get_config();
        let token_admin = StellarAssetClient::new(&env, &cfg.token);
        token_admin.mint(&cid, &(col * 10));

        env.mock_all_auths();
        client.fund_insurance(&col);
        assert_eq!(client.insurance_balance(), col);

        // oracle=8900 → settlement=0 (clamped from -100), PnL=0
        // base_reward=15, shortfall=15, draw=min(15,1000)=15, net ins change = 0
        client.set_price(&admin, &8_900u64);
        create_position(
            &env,
            &cid,
            &cmt,
            col,
            10,
            0,
            entry,
            PositionStatus::Matched,
            0,
        );

        let reward = client.liquidate(&cmt);
        assert!(reward > 0, "liquidator reward should be positive");
        // Insurance fund is unchanged when settlement clamps to 0
        assert_eq!(client.insurance_balance(), col);
        assert_eq!(client.bad_debt(), 0);
    }

    #[test]
    fn test_bad_debt_accrues_when_fund_empty() {
        // When settlement is deeply negative and insurance fund is empty, bad debt accumulates
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let _owner = Address::generate(&env);
        let _liquidator = Address::generate(&env);
        let cmt = BytesN::from_array(&env, &[64u8; 32]);

        let col: i128 = 1_000;
        let entry: u64 = 10_000;

        let cfg = client.get_config();
        let token_admin = StellarAssetClient::new(&env, &cfg.token);
        token_admin.mint(&cid, &(col * 10));

        // No insurance fund seeded
        assert_eq!(client.insurance_balance(), 0);
        assert_eq!(client.bad_debt(), 0);

        // oracle=8900: settlement=-100; base_reward=col*150/10000=15; shortfall=15; ins_fund=0; bad_debt=15
        client.set_price(&admin, &8_900u64);
        create_position(
            &env,
            &cid,
            &cmt,
            col,
            10,
            0,
            entry,
            PositionStatus::Matched,
            0,
        );

        env.mock_all_auths();
        client.liquidate(&cmt);

        assert_eq!(client.bad_debt(), 15);
    }

    #[test]
    #[should_panic(expected = "not under-collateralized")]
    fn test_liquidate_solvent_position_panics() {
        let (env, cid, admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let _owner = Address::generate(&env);
        let _liquidator = Address::generate(&env);
        let cmt = BytesN::from_array(&env, &[65u8; 32]);

        // oracle above entry → long position is profitable, not liquidatable
        client.set_price(&admin, &15_000u64);
        create_position(
            &env,
            &cid,
            &cmt,
            1_000,
            10,
            0,
            10_000,
            PositionStatus::Matched,
            0,
        );

        env.mock_all_auths();
        client.liquidate(&cmt);
    }

    #[test]
    fn test_cross_margin_portfolio_group_membership() {
        // Two positions opened via from_note with the same secret share a portfolio group.
        let (env, cid, _admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let depositor = Address::generate(&env);
        let _liq_recipient = Address::generate(&env);
        let cfg = client.get_config();
        let asset = StellarAssetClient::new(&env, &cfg.token);

        let order_secret: u64 = 314_159_265;

        // Position A (long)
        let amount_a: u64 = 4_000_000;
        let note_secret_a: u64 = 111_222;
        let (note_cmt_a_hex, note_null_a_hex, note_proof_a_json) =
            gen_note_proof(amount_a, note_secret_a);
        let (pos_cmt_a_hex, pk_hex, commit_proof_a_json) =
            gen_commit_proof_full(0, 100_000_000, 1, 5, 1, order_secret, true);

        let note_cmt_a = hex_to_bytes32(&env, &note_cmt_a_hex);
        let note_null_a = hex_to_bytes32(&env, &note_null_a_hex);
        let pos_cmt_a = hex_to_bytes32(&env, &pos_cmt_a_hex);
        let portfolio_key = hex_to_bytes32(&env, &pk_hex);

        asset.mint(&depositor, &(amount_a as i128));
        client.deposit_note(&depositor, &note_cmt_a, &(amount_a as i128));
        client.open_position_from_note(
            &note_cmt_a,
            &note_null_a,
            &pos_cmt_a,
            &100_000_000,
            &0,
            &5,
            &1_000_000_000u64,
            &TimeInForce::GTC,
            &0u64,
            &0u64,
            &0u64,
            &BytesN::from_array(&env, &[0u8; 32]),
            &portfolio_key,
            &BytesN::from_array(&env, &[0u8; 32]), // asset_id: default
            &make_groth16_proof(&env, &note_proof_a_json),
            &make_groth16_proof(&env, &commit_proof_a_json),
        );

        // Position B (short) — same order_secret → same portfolio_key
        let amount_b: u64 = 6_000_000;
        let note_secret_b: u64 = 333_444;
        let (note_cmt_b_hex, note_null_b_hex, note_proof_b_json) =
            gen_note_proof(amount_b, note_secret_b);
        let (pos_cmt_b_hex, pk_hex_b, commit_proof_b_json) =
            gen_commit_proof_full(1, 100_000_000, 1, 3, 2, order_secret, true);

        // Both must derive the same portfolio key from the same secret
        assert_eq!(
            pk_hex, pk_hex_b,
            "portfolio key must be deterministic from secret"
        );

        let note_cmt_b = hex_to_bytes32(&env, &note_cmt_b_hex);
        let note_null_b = hex_to_bytes32(&env, &note_null_b_hex);
        let pos_cmt_b = hex_to_bytes32(&env, &pos_cmt_b_hex);

        asset.mint(&depositor, &(amount_b as i128));
        client.deposit_note(&depositor, &note_cmt_b, &(amount_b as i128));
        client.open_position_from_note(
            &note_cmt_b,
            &note_null_b,
            &pos_cmt_b,
            &100_000_000,
            &1,
            &3,
            &1_000_000_000u64,
            &TimeInForce::GTC,
            &0u64,
            &0u64,
            &0u64,
            &BytesN::from_array(&env, &[0u8; 32]),
            &portfolio_key,
            &BytesN::from_array(&env, &[0u8; 32]), // asset_id: default
            &make_groth16_proof(&env, &note_proof_b_json),
            &make_groth16_proof(&env, &commit_proof_b_json),
        );

        // Both positions carry the portfolio_key and cross margin mode
        let meta_a = client.get_position(&pos_cmt_a).unwrap();
        let meta_b = client.get_position(&pos_cmt_b).unwrap();
        assert_eq!(meta_a.margin_mode, MarginMode::Cross);
        assert_eq!(meta_b.margin_mode, MarginMode::Cross);
        assert_eq!(meta_a.portfolio_key, portfolio_key);
        assert_eq!(meta_b.portfolio_key, portfolio_key);

        // Contract's PortfolioGroup contains both commitments
        let group = client.get_portfolio_group(&portfolio_key);
        assert_eq!(group.len(), 2);
        assert!(group.contains(&pos_cmt_a));
        assert!(group.contains(&pos_cmt_b));
    }

    #[test]
    fn test_cross_margin_cancel_removes_from_group() {
        let (env, cid, _admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let depositor = Address::generate(&env);
        let _liq_recipient = Address::generate(&env);
        let cfg = client.get_config();
        let asset = StellarAssetClient::new(&env, &cfg.token);

        let order_secret: u64 = 271_828_182;
        let amount: u64 = 5_000_000;
        let note_secret: u64 = 777_888;

        let (note_cmt_hex, note_null_hex, note_proof_json) = gen_note_proof(amount, note_secret);
        let (pos_cmt_hex, pk_hex, commit_proof_json) =
            gen_commit_proof_full(0, 100_000_000, 1, 10, 99, order_secret, true);
        let (cancel_null_hex, cancel_proof_json) = gen_cancel_proof(&pos_cmt_hex, order_secret);

        let settle_secret: u64 = 555_666;
        let (settle_cmt_hex, _settle_null_hex, _) = gen_note_proof(0, settle_secret);

        let note_cmt = hex_to_bytes32(&env, &note_cmt_hex);
        let note_null = hex_to_bytes32(&env, &note_null_hex);
        let pos_cmt = hex_to_bytes32(&env, &pos_cmt_hex);
        let portfolio_key = hex_to_bytes32(&env, &pk_hex);
        let cancel_null = hex_to_bytes32(&env, &cancel_null_hex);
        let settle_cmt = hex_to_bytes32(&env, &settle_cmt_hex);

        asset.mint(&depositor, &(amount as i128));
        client.deposit_note(&depositor, &note_cmt, &(amount as i128));
        client.open_position_from_note(
            &note_cmt,
            &note_null,
            &pos_cmt,
            &100_000_000,
            &0,
            &10,
            &1_000_000_000u64,
            &TimeInForce::GTC,
            &0u64,
            &0u64,
            &0u64,
            &BytesN::from_array(&env, &[0u8; 32]),
            &portfolio_key,
            &BytesN::from_array(&env, &[0u8; 32]), // asset_id: default
            &make_groth16_proof(&env, &note_proof_json),
            &make_groth16_proof(&env, &commit_proof_json),
        );

        // Group has one member
        assert_eq!(client.get_portfolio_group(&portfolio_key).len(), 1);

        // Cancel removes it from the group
        client.cancel_position_to_note(
            &pos_cmt,
            &cancel_null,
            &settle_cmt,
            &make_groth16_proof(&env, &cancel_proof_json),
        );

        assert_eq!(client.get_portfolio_group(&portfolio_key).len(), 0);
        assert_eq!(
            client.get_position(&pos_cmt).unwrap().status,
            PositionStatus::Cancelled
        );
    }
}
