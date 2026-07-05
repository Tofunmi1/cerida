#![no_std]
#![allow(clippy::too_many_arguments)]

use soroban_sdk::{
    contract, contractimpl, contracttype,
    crypto::bn254::{Bn254Fr, Bn254G1Affine as G1Affine, Bn254G2Affine as G2Affine},
    token, Address, BytesN, Env, Vec,
};
use types::{Groth16Error, Groth16Proof};

include!(concat!(env!("OUT_DIR"), "/vk.rs"));

// ── Verification key loading ──────────────────────────────────────────────────

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

// ── Storage keys ──────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    /// Initialization guard
    Initialized,
    /// USDC SAC address
    Token,
    /// Fixed deposit/withdraw amount (in token stroops)
    Denomination,
    /// Current Merkle root
    CurrentRoot,
    /// Ring buffer of recent valid roots (for proving with a slightly old root)
    Root(u32),
    /// Head of the root ring buffer (u32)
    RootHead,
    /// Next leaf index to insert into
    NextIndex,
    /// Spent nullifier set
    Nullifier(BytesN<32>),
}

// Ring buffer size: allow proofs generated against any of the last 30 roots.
const ROOT_HISTORY: u32 = 30;

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct ShieldedPool;

#[contractimpl]
impl ShieldedPool {
    /// One-time initializer. `empty_root` is the Merkle root of the all-zeros tree
    /// (compute with `cargo run -p rust-circuits -- pool-zeros`).
    pub fn initialize(env: Env, token: Address, denomination: u128, empty_root: BytesN<32>) {
        if env.storage().instance().has(&DataKey::Initialized) {
            panic!("ShieldedPool: already initialized");
        }
        env.storage().instance().set(&DataKey::Token, &token);
        env.storage()
            .instance()
            .set(&DataKey::Denomination, &denomination);

        // Seed the root ring buffer with the empty tree root
        env.storage()
            .instance()
            .set(&DataKey::CurrentRoot, &empty_root);
        env.storage().instance().set(&DataKey::Root(0), &empty_root);
        env.storage().instance().set(&DataKey::RootHead, &0u32);
        env.storage().instance().set(&DataKey::NextIndex, &0u32);
        env.storage().instance().set(&DataKey::Initialized, &true);
    }

    /// Deposit `denomination` USDC into the pool.
    ///
    /// `commitment`  — Poseidon2(secret, nullifier, 30) computed off-chain
    /// `new_root`    — Merkle root after inserting commitment at next_index
    /// `proof`       — Groth16 proof for ShieldedInsert circuit
    ///                 public inputs: [old_root, new_root, commitment, leaf_index]
    pub fn deposit(
        env: Env,
        depositor: Address,
        commitment: BytesN<32>,
        new_root: BytesN<32>,
        proof: Groth16Proof,
    ) {
        depositor.require_auth();
        Self::assert_initialized(&env);

        let denomination: u128 = env
            .storage()
            .instance()
            .get(&DataKey::Denomination)
            .unwrap();
        let token: Address = env.storage().instance().get(&DataKey::Token).unwrap();
        let current_root: BytesN<32> = env.storage().instance().get(&DataKey::CurrentRoot).unwrap();
        let next_index: u32 = env.storage().instance().get(&DataKey::NextIndex).unwrap();

        // Verify the insert proof: proves old_root → new_root by inserting commitment at next_index
        let vk = load_vk(&env, &VK_INSERT_IC);
        let mut pi: Vec<Bn254Fr> = Vec::new(&env);
        pi.push_back(Bn254Fr::from_bytes(current_root.clone()));
        pi.push_back(Bn254Fr::from_bytes(new_root.clone()));
        pi.push_back(Bn254Fr::from_bytes(commitment.clone()));
        // leaf_index as a 32-byte big-endian BytesN<32>
        let mut idx_bytes = [0u8; 32];
        idx_bytes[28..32].copy_from_slice(&next_index.to_be_bytes());
        pi.push_back(Bn254Fr::from_bytes(BytesN::from_array(&env, &idx_bytes)));

        match verify_groth16(&env, &vk, &proof, &pi) {
            Ok(true) => {}
            _ => panic!("ShieldedPool: invalid insert proof"),
        }

        // Transfer USDC from depositor to this contract
        let token_client = token::Client::new(&env, &token);
        let this = env.current_contract_address();
        token_client.transfer(&depositor, &this, &(denomination as i128));

        // Advance Merkle state
        env.storage()
            .instance()
            .set(&DataKey::CurrentRoot, &new_root);
        let new_head = (next_index + 1) % ROOT_HISTORY;
        env.storage()
            .instance()
            .set(&DataKey::Root(new_head), &new_root);
        env.storage().instance().set(&DataKey::RootHead, &new_head);
        env.storage()
            .instance()
            .set(&DataKey::NextIndex, &(next_index + 1));

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("deposit"),),
            (commitment, new_root, next_index),
        );
    }

    /// Withdraw `denomination` USDC to `recipient` without revealing which deposit you made.
    ///
    /// `root`          — any recent valid Merkle root (from the ring buffer)
    /// `nullifier_hash`— Poseidon2(nullifier, 0, 31) — prevents double-spend
    /// `recipient`     — address that receives the USDC (Fr bytes, matches proof)
    /// `recipient_addr`— the actual Soroban address (must match recipient Fr encoding)
    /// `proof`         — Groth16 proof for ShieldedWithdraw circuit
    ///                   public inputs: [root, nullifier_hash, recipient]
    pub fn withdraw(
        env: Env,
        root: BytesN<32>,
        nullifier_hash: BytesN<32>,
        recipient: BytesN<32>,
        recipient_addr: Address,
        proof: Groth16Proof,
    ) {
        // recipient_addr must sign the tx — prevents frontrunners from swapping the destination
        recipient_addr.require_auth();
        Self::assert_initialized(&env);

        // root must be in the ring buffer
        if !Self::is_known_root(&env, &root) {
            panic!("ShieldedPool: unknown root");
        }

        // nullifier must not already be spent
        let null_key = DataKey::Nullifier(nullifier_hash.clone());
        if env.storage().persistent().has(&null_key) {
            panic!("ShieldedPool: nullifier already spent");
        }

        // Verify the withdrawal proof
        let vk = load_vk(&env, &VK_WITHDRAW_IC);
        let mut pi: Vec<Bn254Fr> = Vec::new(&env);
        pi.push_back(Bn254Fr::from_bytes(root.clone()));
        pi.push_back(Bn254Fr::from_bytes(nullifier_hash.clone()));
        pi.push_back(Bn254Fr::from_bytes(recipient.clone()));

        match verify_groth16(&env, &vk, &proof, &pi) {
            Ok(true) => {}
            _ => panic!("ShieldedPool: invalid withdrawal proof"),
        }

        // Mark nullifier as spent
        env.storage().persistent().set(&null_key, &true);
        env.storage()
            .persistent()
            .extend_ttl(&null_key, 17280, 17280);

        // Transfer USDC to recipient
        let denomination: u128 = env
            .storage()
            .instance()
            .get(&DataKey::Denomination)
            .unwrap();
        let token: Address = env.storage().instance().get(&DataKey::Token).unwrap();
        let token_client = token::Client::new(&env, &token);
        let this = env.current_contract_address();
        token_client.transfer(&this, &recipient_addr, &(denomination as i128));

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("withdraw"),),
            (nullifier_hash, recipient, denomination),
        );
    }

    /// Called by another contract (e.g. perp-engine) that has already transferred
    /// `denomination` USDC to this pool address. Updates the Merkle state and emits
    /// a deposit event — identical to `deposit` but without requiring depositor auth
    /// or performing the token transfer (caller handles it).
    pub fn deposit_from_contract(
        env: Env,
        commitment: BytesN<32>,
        new_root: BytesN<32>,
        proof: Groth16Proof,
    ) {
        Self::assert_initialized(&env);

        let current_root: BytesN<32> = env.storage().instance().get(&DataKey::CurrentRoot).unwrap();
        let next_index: u32 = env.storage().instance().get(&DataKey::NextIndex).unwrap();

        let vk = load_vk(&env, &VK_INSERT_IC);
        let mut pi: Vec<Bn254Fr> = Vec::new(&env);
        pi.push_back(Bn254Fr::from_bytes(current_root.clone()));
        pi.push_back(Bn254Fr::from_bytes(new_root.clone()));
        pi.push_back(Bn254Fr::from_bytes(commitment.clone()));
        let mut idx_bytes = [0u8; 32];
        idx_bytes[28..32].copy_from_slice(&next_index.to_be_bytes());
        pi.push_back(Bn254Fr::from_bytes(BytesN::from_array(&env, &idx_bytes)));

        match verify_groth16(&env, &vk, &proof, &pi) {
            Ok(true) => {}
            _ => panic!("ShieldedPool: invalid insert proof"),
        }

        env.storage()
            .instance()
            .set(&DataKey::CurrentRoot, &new_root);
        let new_head = (next_index + 1) % ROOT_HISTORY;
        env.storage()
            .instance()
            .set(&DataKey::Root(new_head), &new_root);
        env.storage().instance().set(&DataKey::RootHead, &new_head);
        env.storage()
            .instance()
            .set(&DataKey::NextIndex, &(next_index + 1));

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("deposit"),),
            (commitment, new_root, next_index),
        );
    }

    pub fn is_spent(env: Env, nullifier_hash: BytesN<32>) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::Nullifier(nullifier_hash))
    }

    pub fn current_root(env: Env) -> BytesN<32> {
        env.storage()
            .instance()
            .get(&DataKey::CurrentRoot)
            .unwrap_or_else(|| panic!("ShieldedPool: not initialized"))
    }

    pub fn next_index(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::NextIndex)
            .unwrap_or(0)
    }

    pub fn denomination(env: Env) -> u128 {
        env.storage()
            .instance()
            .get(&DataKey::Denomination)
            .unwrap_or_else(|| panic!("ShieldedPool: not initialized"))
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn assert_initialized(env: &Env) {
        if !env.storage().instance().has(&DataKey::Initialized) {
            panic!("ShieldedPool: not initialized");
        }
    }

    fn is_known_root(env: &Env, root: &BytesN<32>) -> bool {
        for i in 0..ROOT_HISTORY {
            if let Some(r) = env
                .storage()
                .instance()
                .get::<_, BytesN<32>>(&DataKey::Root(i))
            {
                if &r == root {
                    return true;
                }
            }
        }
        false
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    fn setup() -> (Env, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let token = Address::generate(&env);
        (env, token)
    }

    fn zero_root(env: &Env) -> BytesN<32> {
        BytesN::from_array(env, &[0u8; 32])
    }

    fn deploy_pool(env: &Env, token: &Address) -> Address {
        let pool_id = env.register(ShieldedPool, ());
        let client = ShieldedPoolClient::new(env, &pool_id);
        client.initialize(token, &1_000_000u128, &zero_root(env));
        pool_id
    }

    fn dummy_proof(env: &Env) -> Groth16Proof {
        Groth16Proof {
            a: G1Affine::from_bytes(BytesN::from_array(env, &[0u8; 64])),
            b: G2Affine::from_bytes(BytesN::from_array(env, &[0u8; 128])),
            c: G1Affine::from_bytes(BytesN::from_array(env, &[0u8; 64])),
        }
    }

    #[test]
    fn test_initialize_sets_state() {
        let (env, token) = setup();
        let pool_id = deploy_pool(&env, &token);
        let client = ShieldedPoolClient::new(&env, &pool_id);

        assert_eq!(client.denomination(), 1_000_000u128);
        assert_eq!(client.next_index(), 0u32);
        assert_eq!(client.current_root(), zero_root(&env));
        assert!(!client.is_spent(&BytesN::from_array(&env, &[0xffu8; 32])));
    }

    #[test]
    #[should_panic(expected = "already initialized")]
    fn test_double_initialize_panics() {
        let (env, token) = setup();
        let pool_id = deploy_pool(&env, &token);
        let client = ShieldedPoolClient::new(&env, &pool_id);
        client.initialize(&token, &500_000u128, &zero_root(&env));
    }

    #[test]
    #[should_panic(expected = "not initialized")]
    fn test_current_root_before_init_panics() {
        let env = Env::default();
        let pool_id = env.register(ShieldedPool, ());
        let client = ShieldedPoolClient::new(&env, &pool_id);
        client.current_root();
    }

    #[test]
    fn test_next_index_starts_at_zero() {
        let (env, token) = setup();
        let pool_id = deploy_pool(&env, &token);
        let client = ShieldedPoolClient::new(&env, &pool_id);
        assert_eq!(client.next_index(), 0u32);
    }

    #[test]
    fn test_is_spent_false_for_fresh_nullifier() {
        let (env, token) = setup();
        let pool_id = deploy_pool(&env, &token);
        let client = ShieldedPoolClient::new(&env, &pool_id);
        let null = BytesN::from_array(&env, &[0xab; 32]);
        assert!(!client.is_spent(&null));
    }

    #[test]
    #[should_panic(expected = "unknown root")]
    fn test_withdraw_unknown_root_panics() {
        let (env, token) = setup();
        let pool_id = deploy_pool(&env, &token);
        let client = ShieldedPoolClient::new(&env, &pool_id);

        let bad_root = BytesN::from_array(&env, &[0xaa; 32]);
        let null_hash = BytesN::from_array(&env, &[0xbb; 32]);
        let recipient = BytesN::from_array(&env, &[0xcc; 32]);
        let recipient_addr = Address::generate(&env);

        client.withdraw(
            &bad_root,
            &null_hash,
            &recipient,
            &recipient_addr,
            &dummy_proof(&env),
        );
    }

    #[test]
    #[should_panic(expected = "nullifier already spent")]
    fn test_withdraw_spent_nullifier_panics() {
        let (env, token) = setup();
        let pool_id = deploy_pool(&env, &token);

        let null_bytes = [0xdd; 32];
        let null_hash: BytesN<32> = BytesN::from_array(&env, &null_bytes);

        // Manually mark nullifier as spent
        env.as_contract(&pool_id, || {
            env.storage()
                .persistent()
                .set(&DataKey::Nullifier(null_hash.clone()), &true);
        });

        let client = ShieldedPoolClient::new(&env, &pool_id);
        let root = client.current_root();
        let recipient = BytesN::from_array(&env, &[0xcc; 32]);
        let recipient_addr = Address::generate(&env);

        client.withdraw(
            &root,
            &null_hash,
            &recipient,
            &recipient_addr,
            &dummy_proof(&env),
        );
    }

    #[test]
    fn test_denomination_getter() {
        let (env, token) = setup();
        let pool_id = deploy_pool(&env, &token);
        let client = ShieldedPoolClient::new(&env, &pool_id);
        assert_eq!(client.denomination(), 1_000_000u128);
    }
}
