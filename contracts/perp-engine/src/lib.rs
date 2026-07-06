#![no_std]
#![allow(clippy::too_many_arguments, dead_code)]

use soroban_sdk::{
    contract, contractimpl, contracttype,
    crypto::bn254::{Bn254Fr, Bn254G1Affine as G1Affine, Bn254G2Affine as G2Affine},
    token::TokenClient,
    Address, Bytes, BytesN, Env, Vec,
};
use types::{Groth16Error, Groth16Proof, ShieldedPoolClient};

include!(concat!(env!("OUT_DIR"), "/vk.rs"));

#[allow(dead_code)]
const FUNDING_INTERVAL: u64 = 5760;
#[allow(dead_code)]
const TWAP_WINDOW: u64 = 8;
#[allow(dead_code)]
const MAX_FUNDING_RATE_BPS: i64 = 75;
#[allow(dead_code)]
const MAX_PRICE_DEVIATION_BPS: u64 = 5000;
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

#[allow(dead_code)]
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
    Note(BytesN<32>),
    InsuranceFund,
    BadDebt,
    PortfolioGroup(BytesN<32>), // portfolio_key → Vec<BytesN<32>> of member commitments
    AssetConfig(BytesN<32>),
    AssetName(BytesN<32>),
    AssetList, // Vec<BytesN<32>> of all registered asset IDs
    TeeAccount,
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
    pub status: PositionStatus,
    pub created_at: u64,
    pub partial_liq_done: bool,
    pub liquidation_recipient_note: BytesN<32>,
    pub asset_id: BytesN<32>,
    pub margin_mode: MarginMode,
    pub portfolio_key: BytesN<32>,
    pub sealed_params: Bytes,
    pub settlement_commitment: BytesN<32>,
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

    pub fn set_tee_account(env: Env, admin: Address, tee: Option<Address>) {
        admin.require_auth();
        let cfg = Self::config(&env);
        if cfg.admin != admin {
            panic!("PerpEngine: only protocol admin can set TEE account");
        }
        env.storage().instance().set(&DataKey::TeeAccount, &tee);
    }

    fn require_tee_auth(env: &Env) {
        let tee: Option<Address> = env
            .storage()
            .instance()
            .get(&DataKey::TeeAccount)
            .unwrap_or(None);
        match tee {
            Some(addr) => addr.require_auth(),
            None => panic!("PerpEngine: TEE account not set"),
        }
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

    /// Compute note amount commitment: SHA-256(amount_le_16 || blinding_32).
    /// This is the canonical on-chain scheme for all note commitments.
    fn note_amount_commitment(env: &Env, amount: i128, blinding: &BytesN<32>) -> BytesN<32> {
        let amount_le: [u8; 16] = amount.to_le_bytes();
        let blinding_arr: [u8; 32] = blinding.to_array();
        let mut preimage = [0u8; 48];
        preimage[..16].copy_from_slice(&amount_le);
        preimage[16..].copy_from_slice(&blinding_arr);
        env.crypto()
            .sha256(&Bytes::from_slice(env, &preimage))
            .into()
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

    /// Deposit tokens and record a shielded note commitment (no address stored).
    /// note_commitment = Poseidon2(amount, secret) — computed client-side.
    pub fn deposit_note(
        env: Env,
        from: Address,
        note_commitment: BytesN<32>,
        amount: i128,
        amount_commitment: BytesN<32>,
    ) {
        from.require_auth();
        let note_key = DataKey::Note(note_commitment.clone());
        if env.storage().persistent().has(&note_key) {
            panic!("PerpEngine: note commitment already exists");
        }
        let cfg = Self::config(&env);
        // Transfer tokens into contract (no privacy leak — just moving value)
        TokenClient::new(&env, &cfg.token).transfer(&from, env.current_contract_address(), &amount);
        // Store only the commitment hash, NOT the plain amount
        env.storage()
            .persistent()
            .set(&note_key, &amount_commitment);
        env.storage()
            .persistent()
            .extend_ttl(&note_key, 17280, 17280);

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("dep_note"),),
            (note_commitment,),
        );
    }

    /// Withdraw a shielded note to any recipient by proving knowledge of the secret.
    /// Proof: NoteSpend — public inputs [note_commitment, nullifier].
    pub fn withdraw_note(
        env: Env,
        note_commitment: BytesN<32>,
        nullifier: BytesN<32>,
        recipient: Address,
        amount: i128,
        blinding: BytesN<32>,
        proof: Groth16Proof,
    ) {
        let null_key = DataKey::Nullifier(nullifier.clone());
        if env.storage().persistent().has(&null_key) {
            panic!("PerpEngine: nullifier already spent");
        }
        let note_key = DataKey::Note(note_commitment.clone());
        let stored_commitment: BytesN<32> = env
            .storage()
            .persistent()
            .get(&note_key)
            .unwrap_or_else(|| panic!("PerpEngine: note not found"));

        let expected = Self::note_amount_commitment(&env, amount, &blinding);
        if expected != stored_commitment {
            panic!("PerpEngine: amount/blinding does not match stored note commitment");
        }

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
            (note_commitment, nullifier),
        );
    }

    /// Spend a settlement note and re-deposit proceeds into a ShieldedPool — fully private exit.
    /// If settlement >= pool denomination: transfers `denomination` USDC to pool, updates pool
    /// merkle state via `deposit_from_contract`, and creates a remainder note for any leftover.
    /// If settlement < denomination: creates a plain perp note (user exits via `withdraw_note`).
    pub fn withdraw_to_pool(
        env: Env,
        pool_id: Address,
        note_commitment: BytesN<32>,
        nullifier: BytesN<32>,
        amount: i128,
        blinding: BytesN<32>,
        new_pool_leaf: BytesN<32>,
        new_pool_root: BytesN<32>,
        remainder_note: BytesN<32>,
        remainder_blinding: BytesN<32>,
        note_spend_proof: Groth16Proof,
        pool_insert_proof: Groth16Proof,
    ) {
        Self::require_tee_auth(&env);

        let null_key = DataKey::Nullifier(nullifier.clone());
        if env.storage().persistent().has(&null_key) {
            panic!("PerpEngine: nullifier already spent");
        }
        let note_key = DataKey::Note(note_commitment.clone());
        let stored: BytesN<32> = env
            .storage()
            .persistent()
            .get(&note_key)
            .unwrap_or_else(|| panic!("PerpEngine: note not found"));

        let expected = Self::note_amount_commitment(&env, amount, &blinding);
        if expected != stored {
            panic!("PerpEngine: amount/blinding mismatch");
        }

        let mut pi: Vec<Bn254Fr> = Vec::new(&env);
        pi.push_back(Bn254Fr::from_bytes(note_commitment.clone()));
        pi.push_back(Bn254Fr::from_bytes(nullifier.clone()));
        let vk = load_vk(&env, &VK_NOTE_SPEND_IC);
        match verify_groth16(&env, &vk, &note_spend_proof, &pi) {
            Ok(true) => {}
            _ => panic!("PerpEngine: invalid note spend proof"),
        }

        env.storage().persistent().set(&null_key, &true);
        env.storage()
            .persistent()
            .extend_ttl(&null_key, 17280, 17280);

        let pool = ShieldedPoolClient::new(&env, &pool_id);
        let denomination = pool.denomination() as i128;

        if amount >= denomination {
            // Transfer denomination USDC from perp-engine to pool, then update pool state.
            let cfg = Self::config(&env);
            TokenClient::new(&env, &cfg.token).transfer(
                &env.current_contract_address(),
                &pool_id,
                &denomination,
            );
            pool.deposit_from_contract(&new_pool_leaf, &new_pool_root, &pool_insert_proof);

            // Create a remainder note for leftover (if any)
            let leftover = amount - denomination;
            if leftover > 0 {
                let rem_key = DataKey::Note(remainder_note.clone());
                if !env.storage().persistent().has(&rem_key) {
                    let rem_commitment =
                        Self::note_amount_commitment(&env, leftover, &remainder_blinding);
                    env.storage().persistent().set(&rem_key, &rem_commitment);
                    env.storage()
                        .persistent()
                        .extend_ttl(&rem_key, 17280, 17280);
                }
            }

            #[allow(deprecated)]
            env.events().publish(
                (soroban_sdk::symbol_short!("wd_pool"),),
                (note_commitment, nullifier, new_pool_leaf),
            );
        } else {
            // Settlement too small for pool denomination — create a plain note for full amount.
            let rem_key = DataKey::Note(remainder_note.clone());
            if env.storage().persistent().has(&rem_key) {
                panic!("PerpEngine: remainder note already exists");
            }
            let rem_commitment = Self::note_amount_commitment(&env, amount, &remainder_blinding);
            env.storage().persistent().set(&rem_key, &rem_commitment);
            env.storage()
                .persistent()
                .extend_ttl(&rem_key, 17280, 17280);

            #[allow(deprecated)]
            env.events().publish(
                (soroban_sdk::symbol_short!("wd_small"),),
                (note_commitment, nullifier),
            );
        }
    }

    /// Add margin to a position by spending a shielded note (no address linkage).
    /// Proof: NoteSpend — public inputs [note_commitment, nullifier].
    pub fn add_margin_from_note(
        env: Env,
        note_commitment: BytesN<32>,
        nullifier: BytesN<32>,
        position_commitment: BytesN<32>,
        amount: i128,
        blinding: BytesN<32>,
        new_settlement_commitment: BytesN<32>,
        proof: Groth16Proof,
    ) {
        Self::require_tee_auth(&env);
        let null_key = DataKey::Nullifier(nullifier.clone());
        if env.storage().persistent().has(&null_key) {
            panic!("PerpEngine: nullifier already spent");
        }
        let note_key = DataKey::Note(note_commitment.clone());
        let stored_commitment: BytesN<32> = env
            .storage()
            .persistent()
            .get(&note_key)
            .unwrap_or_else(|| panic!("PerpEngine: note not found"));

        let expected = Self::note_amount_commitment(&env, amount, &blinding);
        if expected != stored_commitment {
            panic!("PerpEngine: amount/blinding does not match stored note commitment");
        }

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

        meta.settlement_commitment = new_settlement_commitment;
        env.storage().persistent().set(&pos_key, &meta);
        env.storage()
            .persistent()
            .extend_ttl(&pos_key, 17280, 17280);

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("mgn_note"),),
            (note_commitment, nullifier, position_commitment),
        );
    }

    pub fn get_note(env: Env, note_commitment: BytesN<32>) -> Option<BytesN<32>> {
        env.storage()
            .persistent()
            .get::<_, BytesN<32>>(&DataKey::Note(note_commitment))
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
        sealed_params: Bytes,
        liquidation_recipient_note: BytesN<32>,
        portfolio_key: BytesN<32>,
        asset_id: BytesN<32>,
        settlement_commitment: BytesN<32>,
        note_proof: Groth16Proof,
        commit_proof: Groth16Proof,
    ) {
        // Validate asset is registered and active
        let asset_cfg = env
            .storage()
            .persistent()
            .get::<_, types::AssetConfig>(&DataKey::AssetConfig(asset_id.clone()))
            .unwrap_or_else(|| panic!("PerpEngine: asset not registered"));
        if !asset_cfg.active {
            panic!("PerpEngine: asset is not active");
        }

        let note_null_key = DataKey::Nullifier(note_nullifier.clone());
        if env.storage().persistent().has(&note_null_key) {
            panic!("PerpEngine: note nullifier already spent");
        }

        let note_key = DataKey::Note(note_commitment.clone());
        if !env.storage().persistent().has(&note_key) {
            panic!("PerpEngine: note not found");
        }

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
            status: PositionStatus::Open,
            created_at,
            partial_liq_done: false,
            liquidation_recipient_note,
            asset_id,
            margin_mode,
            portfolio_key,
            sealed_params,
            settlement_commitment,
        };
        env.storage().persistent().set(&pos_key, &meta);
        env.storage()
            .persistent()
            .extend_ttl(&pos_key, 17280, 17280);

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("open_n"),),
            (position_commitment,),
        );
    }

    /// Open a position by spending a ShieldedPool leaf — fully private entry point.
    /// The pool's ZK ShieldedWithdraw proof proves membership in the merkle tree
    /// WITHOUT revealing which leaf is being spent, breaking the deposit→position link.
    /// Collateral = pool.denomination() (fixed per-pool amount).
    pub fn open_position_from_pool(
        env: Env,
        pool_id: Address,
        pool_root: BytesN<32>,
        pool_nullifier_hash: BytesN<32>,
        position_commitment: BytesN<32>,
        sealed_params: Bytes,
        settlement_commitment: BytesN<32>,
        liquidation_recipient_note: BytesN<32>,
        portfolio_key: BytesN<32>,
        asset_id: BytesN<32>,
        spend_proof: Groth16Proof,
        commit_proof: Groth16Proof,
    ) {
        Self::require_tee_auth(&env);

        let asset_cfg = env
            .storage()
            .persistent()
            .get::<_, types::AssetConfig>(&DataKey::AssetConfig(asset_id.clone()))
            .unwrap_or_else(|| panic!("PerpEngine: asset not registered"));
        if !asset_cfg.active {
            panic!("PerpEngine: asset not active");
        }

        let pos_key = DataKey::Position(position_commitment.clone());
        if env.storage().persistent().has(&pos_key) {
            panic!("PerpEngine: commitment already exists");
        }

        // Verify position commitment proof (OrderCommitment circuit)
        let mut commit_pi: Vec<Bn254Fr> = Vec::new(&env);
        commit_pi.push_back(Bn254Fr::from_bytes(position_commitment.clone()));
        commit_pi.push_back(Bn254Fr::from_bytes(portfolio_key.clone()));
        let commit_vk = load_vk(&env, &VK_COMMIT_IC);
        match verify_groth16(&env, &commit_vk, &commit_proof, &commit_pi) {
            Ok(true) => {}
            _ => panic!("PerpEngine: invalid commitment proof"),
        }

        // Cross-call pool: verify ShieldedWithdraw proof and pull denomination USDC
        // into this contract. The spend_proof binds recipient=position_commitment
        // so only the proof owner can open this specific position.
        let pool = ShieldedPoolClient::new(&env, &pool_id);
        let _denomination = pool.denomination() as i128;
        pool.withdraw(
            &pool_root,
            &pool_nullifier_hash,
            &position_commitment, // binding field in ZK proof
            &env.current_contract_address(),
            &spend_proof,
        );

        let zero32 = BytesN::from_array(&env, &[0u8; 32]);
        let margin_mode = if portfolio_key != zero32 {
            MarginMode::Cross
        } else {
            MarginMode::Isolated
        };
        if margin_mode == MarginMode::Cross {
            Self::add_to_portfolio(&env, &portfolio_key, &position_commitment);
        }

        let created_at = env.ledger().sequence() as u64;
        let meta = PositionMeta {
            status: PositionStatus::Open,
            created_at,
            partial_liq_done: false,
            liquidation_recipient_note,
            asset_id,
            margin_mode,
            portfolio_key,
            sealed_params,
            settlement_commitment,
        };
        env.storage().persistent().set(&pos_key, &meta);
        env.storage()
            .persistent()
            .extend_ttl(&pos_key, 17280, 17280);

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("open_pool"),),
            (position_commitment,), // only the commitment, never the pool leaf
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
        refund_amount: i128,
        refund_blinding: BytesN<32>,
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
        let refund_commitment = Self::note_amount_commitment(&env, refund_amount, &refund_blinding);
        env.storage()
            .persistent()
            .set(&note_key, &refund_commitment);
        env.storage()
            .persistent()
            .extend_ttl(&note_key, 17280, 17280);

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("cxl_n"),),
            (position_commitment, cancel_nullifier, recipient_note),
        );
    }

    /// Generic settlement function — replaces trigger_tp/sl/liquidate/close/settle_match.
    /// Callable by TEE only. TEE provides all computed values.
    pub fn settle_position(
        env: Env,
        commitment: BytesN<32>,
        status: u32,
        settlement_note: BytesN<32>,
        settlement_amount: i128,
        settlement_blinding: BytesN<32>,
        _reward_amount: i128,
        ins_delta: i128,
        bad_debt: i128,
    ) {
        Self::require_tee_auth(&env);
        let pos_key = DataKey::Position(commitment.clone());
        let mut meta: PositionMeta = env
            .storage()
            .persistent()
            .get(&pos_key)
            .unwrap_or_else(|| panic!("PerpEngine: position not found"));

        if meta.status != PositionStatus::Matched && meta.status != PositionStatus::Open {
            panic!(
                "PerpEngine: can only settle a matched or open position (status={:?})",
                meta.status as u32
            );
        }

        // Create settlement note with commitment
        let note_key = DataKey::Note(settlement_note.clone());
        if env.storage().persistent().has(&note_key) {
            panic!("PerpEngine: settlement note already exists");
        }
        let settlement_commitment =
            Self::note_amount_commitment(&env, settlement_amount, &settlement_blinding);
        env.storage()
            .persistent()
            .set(&note_key, &settlement_commitment);
        env.storage()
            .persistent()
            .extend_ttl(&note_key, 17280, 17280);

        // Update insurance fund
        let current_ins: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::InsuranceFund)
            .unwrap_or(0);
        let next_ins = (current_ins + ins_delta).max(0);
        env.storage()
            .persistent()
            .set(&DataKey::InsuranceFund, &next_ins);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::InsuranceFund, 17280, 17280);

        // Accrue bad debt
        if bad_debt > 0 {
            let current_bad: i128 = env
                .storage()
                .persistent()
                .get(&DataKey::BadDebt)
                .unwrap_or(0);
            env.storage()
                .persistent()
                .set(&DataKey::BadDebt, &(current_bad + bad_debt));
            env.storage()
                .persistent()
                .extend_ttl(&DataKey::BadDebt, 17280, 17280);
        }

        // Update position status
        let new_status = if status == 2 {
            PositionStatus::Closed
        } else if status == 4 {
            PositionStatus::Liquidated
        } else {
            panic!("PerpEngine: invalid settle status");
        };
        meta.status = new_status;
        env.storage().persistent().set(&pos_key, &meta);
        env.storage()
            .persistent()
            .extend_ttl(&pos_key, 17280, 17280);

        if meta.margin_mode == MarginMode::Cross {
            Self::remove_from_portfolio(&env, &meta.portfolio_key, &commitment);
        }

        #[allow(deprecated)]
        env.events()
            .publish((soroban_sdk::symbol_short!("settle"),), (commitment,));
    }

    /// Partial liquidation — TEE reduces position collateral and pays liquidator reward.
    /// Position stays alive (status unchanged). Can only be called once per position.
    pub fn settle_partial(
        env: Env,
        commitment: BytesN<32>,
        new_settlement_commitment: BytesN<32>,
        reward_note: BytesN<32>,
        reward_amount: i128,
        reward_blinding: BytesN<32>,
    ) {
        Self::require_tee_auth(&env);
        let pos_key = DataKey::Position(commitment.clone());
        let mut meta: PositionMeta = env
            .storage()
            .persistent()
            .get(&pos_key)
            .unwrap_or_else(|| panic!("PerpEngine: position not found"));

        if meta.status != PositionStatus::Open && meta.status != PositionStatus::Matched {
            panic!("PerpEngine: can only partial-liq an open or matched position");
        }
        if meta.partial_liq_done {
            panic!("PerpEngine: partial liquidation already done for this position");
        }

        let reward_key = DataKey::Note(reward_note.clone());
        if env.storage().persistent().has(&reward_key) {
            panic!("PerpEngine: reward note already exists");
        }
        let reward_commitment = Self::note_amount_commitment(&env, reward_amount, &reward_blinding);
        env.storage()
            .persistent()
            .set(&reward_key, &reward_commitment);
        env.storage()
            .persistent()
            .extend_ttl(&reward_key, 17280, 17280);

        meta.partial_liq_done = true;
        meta.settlement_commitment = new_settlement_commitment;
        env.storage().persistent().set(&pos_key, &meta);
        env.storage()
            .persistent()
            .extend_ttl(&pos_key, 17280, 17280);

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("part_liq"),),
            (commitment, reward_note),
        );
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

    /// Upgrade the contract WASM in-place (protocol admin only).
    /// Preserves all contract storage — positions, commitments, oracle configs.
    pub fn upgrade(env: Env, admin: Address, new_wasm_hash: BytesN<32>) {
        admin.require_auth();
        let cfg = Self::config(&env);
        if cfg.admin != admin {
            panic!("PerpEngine: only protocol admin can upgrade");
        }
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    // ---- helper functions ----

    fn config(env: &Env) -> Config {
        env.storage()
            .instance()
            .get(&DataKey::Config)
            .unwrap_or_else(|| panic!("PerpEngine: not initialized"))
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

    #[allow(dead_code)]
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
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
#[allow(unused_variables, dead_code)]
mod test {
    use super::*;
    use soroban_sdk::testutils::{Address as _, LedgerInfo};
    use soroban_sdk::token::StellarAssetClient;
    #[allow(unused_imports)]
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
        let client = PerpEngineClient::new(&env, &contract_id);
        client.initialize(&admin, &token.address(), &None);
        let tee = Address::generate(&env);
        env.mock_all_auths();
        client.set_tee_account(&admin, &Some(tee));
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

    fn create_position(env: &Env, cid: &Address, commitment: &BytesN<32>, status: PositionStatus) {
        env.as_contract(cid, || {
            let meta = PositionMeta {
                status,
                created_at: env.ledger().sequence() as u64,
                partial_liq_done: false,
                liquidation_recipient_note: BytesN::from_array(env, &[0u8; 32]),
                asset_id: BytesN::from_array(env, &[0u8; 32]),
                margin_mode: MarginMode::Isolated,
                portfolio_key: BytesN::from_array(env, &[0u8; 32]),
                sealed_params: Bytes::new(env),
                settlement_commitment: BytesN::from_array(env, &[0u8; 32]),
            };
            let key = DataKey::Position(commitment.clone());
            env.storage().persistent().set(&key, &meta);
            env.storage().persistent().extend_ttl(&key, 17280, 17280);
        });
    }

    #[allow(dead_code)]
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
        client.deposit_note(
            &depositor,
            &note_cmt,
            &(margin_amount as i128),
            &BytesN::from_array(&env, &[0u8; 32]),
        );

        let pos_cmt = BytesN::from_array(&env, &[1u8; 32]);
        create_position(&env, &cid, &pos_cmt, PositionStatus::Open);

        let proof = make_groth16_proof(&env, &proof_json);
        env.mock_all_auths();
        client.add_margin_from_note(
            &note_cmt,
            &nullifier,
            &pos_cmt,
            &0i128,
            &BytesN::from_array(&env, &[0u8; 32]),
            &BytesN::from_array(&env, &[0u8; 32]),
            &proof,
        );

        let pos = client.get_position(&pos_cmt).unwrap();
        assert!(client.is_spent(&nullifier));
        assert_eq!(pos.status, PositionStatus::Open);
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
        client.deposit_note(
            &depositor,
            &note_cmt,
            &(margin_amount as i128),
            &BytesN::from_array(&env, &[0u8; 32]),
        );

        let pos_cmt = BytesN::from_array(&env, &[2u8; 32]);
        create_position(&env, &cid, &pos_cmt, PositionStatus::Matched);

        let proof = make_groth16_proof(&env, &proof_json);
        env.mock_all_auths();
        client.add_margin_from_note(
            &note_cmt,
            &nullifier,
            &pos_cmt,
            &0i128,
            &BytesN::from_array(&env, &[0u8; 32]),
            &BytesN::from_array(&env, &[0u8; 32]),
            &proof,
        );

        let pos = client.get_position(&pos_cmt).unwrap();
        assert!(client.is_spent(&nullifier));
        assert_eq!(pos.status, PositionStatus::Matched);
    }

    #[test]
    #[should_panic(expected = "note not found")]
    fn test_add_margin_insufficient_balance() {
        let (env, cid, _admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);

        let pos_cmt = BytesN::from_array(&env, &[3u8; 32]);
        create_position(&env, &cid, &pos_cmt, PositionStatus::Open);

        // No note deposited — add_margin_from_note should fail
        let fake_note = BytesN::from_array(&env, &[99u8; 32]);
        let fake_null = BytesN::from_array(&env, &[0u8; 32]);
        use soroban_sdk::crypto::bn254::{Bn254G1Affine, Bn254G2Affine};
        let dummy = Groth16Proof {
            a: Bn254G1Affine::from_bytes(BytesN::from_array(&env, &[0u8; 64])),
            b: Bn254G2Affine::from_bytes(BytesN::from_array(&env, &[0u8; 128])),
            c: Bn254G1Affine::from_bytes(BytesN::from_array(&env, &[0u8; 64])),
        };
        env.mock_all_auths();
        client.add_margin_from_note(
            &fake_note,
            &fake_null,
            &pos_cmt,
            &0i128,
            &BytesN::from_array(&env, &[0u8; 32]),
            &BytesN::from_array(&env, &[0u8; 32]),
            &dummy,
        );
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
        client.deposit_note(
            &depositor,
            &note_cmt,
            &(amount as i128),
            &BytesN::from_array(&env, &[0u8; 32]),
        );

        // Withdraw to a different recipient — the privacy claim
        let proof = make_groth16_proof(&env, &proof_json);
        env.mock_all_auths();
        client.withdraw_note(
            &note_cmt,
            &nullifier,
            &recipient,
            &(amount as i128),
            &BytesN::from_array(&env, &[0u8; 32]),
            &proof,
        );

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
        client.deposit_note(
            &depositor,
            &note_cmt,
            &(amount as i128),
            &BytesN::from_array(&env, &[0u8; 32]),
        );

        let proof = make_groth16_proof(&env, &proof_json);
        env.mock_all_auths();
        client.withdraw_note(
            &note_cmt,
            &nullifier,
            &recipient,
            &(amount as i128),
            &BytesN::from_array(&env, &[0u8; 32]),
            &proof,
        );
        // second spend must panic
        env.mock_all_auths();
        client.withdraw_note(
            &note_cmt,
            &nullifier,
            &recipient,
            &(amount as i128),
            &BytesN::from_array(&env, &[0u8; 32]),
            &proof,
        );
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
        env.mock_all_auths();
        client.withdraw_note(
            &note_cmt,
            &nullifier,
            &recipient,
            &(amount as i128),
            &BytesN::from_array(&env, &[0u8; 32]),
            &proof,
        );
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
        client.deposit_note(
            &depositor,
            &note_cmt,
            &(margin_amount as i128),
            &BytesN::from_array(&env, &[0u8; 32]),
        );

        // Create a position for pos_owner directly
        let pos_cmt = BytesN::from_array(&env, &[55u8; 32]);
        create_position(&env, &cid, &pos_cmt, PositionStatus::Open);

        // Add margin from note (no address required — proof authorizes it)
        let proof = make_groth16_proof(&env, &proof_json);
        env.mock_all_auths();
        client.add_margin_from_note(
            &note_cmt,
            &nullifier,
            &pos_cmt,
            &0i128,
            &BytesN::from_array(&env, &[0u8; 32]),
            &BytesN::from_array(&env, &[0u8; 32]),
            &proof,
        );

        let _pos = client.get_position(&pos_cmt).unwrap();
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
        create_position(&env, &cid, &pos_cmt, PositionStatus::Open);

        let proof = make_groth16_proof(&env, &proof_json);
        env.mock_all_auths();
        client.add_margin_from_note(
            &note_cmt,
            &nullifier,
            &pos_cmt,
            &0i128,
            &BytesN::from_array(&env, &[0u8; 32]),
            &BytesN::from_array(&env, &[0u8; 32]),
            &proof,
        );
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
        client.deposit_note(
            &depositor,
            &note_cmt,
            &1000,
            &BytesN::from_array(&env, &[0u8; 32]),
        );

        let stored = client.get_note(&note_cmt).unwrap();
        assert_eq!(stored, BytesN::from_array(&env, &[0u8; 32]));
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
        client.deposit_note(
            &depositor,
            &note_cmt,
            &2000,
            &BytesN::from_array(&env, &[0u8; 32]),
        );
        client.deposit_note(
            &depositor,
            &note_cmt,
            &2000,
            &BytesN::from_array(&env, &[0u8; 32]),
        );
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
        client.deposit_note(
            &depositor,
            &note_cmt,
            &(amount as i128),
            &BytesN::from_array(&env, &[0u8; 32]),
        );

        let note_proof = make_groth16_proof(&env, &note_proof_json);
        let commit_proof = make_groth16_proof(&env, &commit_proof_json);
        client.open_position_from_note(
            &note_cmt,
            &note_null,
            &pos_cmt,
            &Bytes::new(&env),
            &BytesN::from_array(&env, &[0u8; 32]),
            &BytesN::from_array(&env, &[0u8; 32]), // portfolio_key: isolated
            &BytesN::from_array(&env, &[0u8; 32]), // asset_id: default
            &BytesN::from_array(&env, &[0u8; 32]),
            &note_proof,
            &commit_proof,
        );

        assert!(client.is_spent(&note_null));
        let pos = client.get_position(&pos_cmt).unwrap();
        assert_eq!(pos.status, PositionStatus::Open);
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
            &Bytes::new(&env),
            &BytesN::from_array(&env, &[0u8; 32]),
            &BytesN::from_array(&env, &[0u8; 32]), // portfolio_key: isolated
            &BytesN::from_array(&env, &[0u8; 32]), // asset_id: default
            &BytesN::from_array(&env, &[0u8; 32]),
            &note_proof,
            &commit_proof,
        );
    }

    #[test]
    fn test_cancel_position_to_note_full_proof() {
        // Full cycle: deposit_note → open_position_from_note → cancel_position_to_note
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

        asset.mint(&depositor, &(amount as i128));
        client.deposit_note(
            &depositor,
            &note_cmt,
            &(amount as i128),
            &BytesN::from_array(&env, &[0u8; 32]),
        );

        let note_proof = make_groth16_proof(&env, &note_proof_json);
        let commit_proof = make_groth16_proof(&env, &commit_proof_json);
        client.open_position_from_note(
            &note_cmt,
            &note_null,
            &pos_cmt,
            &Bytes::new(&env),
            &BytesN::from_array(&env, &[0u8; 32]),
            &BytesN::from_array(&env, &[0u8; 32]), // portfolio_key: isolated
            &BytesN::from_array(&env, &[0u8; 32]), // asset_id: default
            &BytesN::from_array(&env, &[0u8; 32]),
            &note_proof,
            &commit_proof,
        );

        let cancel_proof = make_groth16_proof(&env, &cancel_proof_json);
        env.mock_all_auths();
        client.cancel_position_to_note(
            &pos_cmt,
            &cancel_null,
            &BytesN::from_array(&env, &[0u8; 32]),
            &(amount as i128),
            &BytesN::from_array(&env, &[0u8; 32]),
            &cancel_proof,
        );

        let pos = client.get_position(&pos_cmt).unwrap();
        assert!(client.is_spent(&cancel_null));
        assert_eq!(pos.status, PositionStatus::Cancelled);
    }

    #[test]
    #[should_panic(expected = "can only cancel an open position")]
    fn test_cancel_position_to_note_wrong_status_reverts() {
        let (env, cid, _admin) = setup();
        let client = PerpEngineClient::new(&env, &cid);
        let _owner = Address::generate(&env);
        let pos_cmt = BytesN::from_array(&env, &[77u8; 32]);
        let recipient_note = BytesN::from_array(&env, &[88u8; 32]);
        create_position(&env, &cid, &pos_cmt, PositionStatus::Matched);

        let order_secret: u64 = 54321;
        let (_, cancel_proof_json) = gen_commit_proof(0, 100, 1, 5, 1, order_secret);
        let fake_null = BytesN::from_array(&env, &[0u8; 32]);
        let proof = make_groth16_proof(&env, &cancel_proof_json);
        client.cancel_position_to_note(
            &pos_cmt,
            &fake_null,
            &recipient_note,
            &0i128,
            &BytesN::from_array(&env, &[0u8; 32]),
            &proof,
        );
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
        client.deposit_note(
            &depositor,
            &note_cmt,
            &(margin_amount as i128),
            &BytesN::from_array(&env, &[0u8; 32]),
        );

        let pos_cmt = BytesN::from_array(&env, &[4u8; 32]);
        create_position(&env, &cid, &pos_cmt, PositionStatus::Closed);

        let proof = make_groth16_proof(&env, &proof_json);
        client.add_margin_from_note(
            &note_cmt,
            &nullifier,
            &pos_cmt,
            &0i128,
            &BytesN::from_array(&env, &[0u8; 32]),
            &BytesN::from_array(&env, &[0u8; 32]),
            &proof,
        );
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
        client.deposit_note(
            &depositor,
            &note_cmt_a,
            &(amount_a as i128),
            &BytesN::from_array(&env, &[0u8; 32]),
        );
        client.open_position_from_note(
            &note_cmt_a,
            &note_null_a,
            &pos_cmt_a,
            &Bytes::new(&env),
            &BytesN::from_array(&env, &[0u8; 32]),
            &portfolio_key,
            &BytesN::from_array(&env, &[0u8; 32]), // asset_id: default
            &BytesN::from_array(&env, &[0u8; 32]),
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
        client.deposit_note(
            &depositor,
            &note_cmt_b,
            &(amount_b as i128),
            &BytesN::from_array(&env, &[0u8; 32]),
        );
        client.open_position_from_note(
            &note_cmt_b,
            &note_null_b,
            &pos_cmt_b,
            &Bytes::new(&env),
            &BytesN::from_array(&env, &[0u8; 32]),
            &portfolio_key,
            &BytesN::from_array(&env, &[0u8; 32]), // asset_id: default
            &BytesN::from_array(&env, &[0u8; 32]),
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

        let note_cmt = hex_to_bytes32(&env, &note_cmt_hex);
        let note_null = hex_to_bytes32(&env, &note_null_hex);
        let pos_cmt = hex_to_bytes32(&env, &pos_cmt_hex);
        let portfolio_key = hex_to_bytes32(&env, &pk_hex);
        let cancel_null = hex_to_bytes32(&env, &cancel_null_hex);

        asset.mint(&depositor, &(amount as i128));
        client.deposit_note(
            &depositor,
            &note_cmt,
            &(amount as i128),
            &BytesN::from_array(&env, &[0u8; 32]),
        );
        client.open_position_from_note(
            &note_cmt,
            &note_null,
            &pos_cmt,
            &Bytes::new(&env),
            &BytesN::from_array(&env, &[0u8; 32]),
            &portfolio_key,
            &BytesN::from_array(&env, &[0u8; 32]), // asset_id: default
            &BytesN::from_array(&env, &[0u8; 32]),
            &make_groth16_proof(&env, &note_proof_json),
            &make_groth16_proof(&env, &commit_proof_json),
        );

        // Group has one member
        assert_eq!(client.get_portfolio_group(&portfolio_key).len(), 1);

        // Cancel removes it from the group
        env.mock_all_auths();
        client.cancel_position_to_note(
            &pos_cmt,
            &cancel_null,
            &BytesN::from_array(&env, &[0u8; 32]),
            &(amount as i128),
            &BytesN::from_array(&env, &[0u8; 32]),
            &make_groth16_proof(&env, &cancel_proof_json),
        );

        assert_eq!(client.get_portfolio_group(&portfolio_key).len(), 0);
        assert_eq!(
            client.get_position(&pos_cmt).unwrap().status,
            PositionStatus::Cancelled
        );
    }
}
