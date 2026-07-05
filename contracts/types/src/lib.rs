#![no_std]

use soroban_sdk::{
    contracterror, contracttype,
    crypto::bn254::{Bn254G1Affine, Bn254G2Affine},
    Address, Bytes, BytesN, Env, Vec,
};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Groth16Error {
    InvalidProof = 0,
    MalformedPublicInputs = 1,
    MalformedProof = 2,
}

#[contracttype]
#[derive(Clone)]
pub struct VerificationKeyBytes {
    pub alpha: BytesN<64>,
    pub beta: BytesN<128>,
    pub gamma: BytesN<128>,
    pub delta: BytesN<128>,
    pub ic: Vec<BytesN<64>>,
}

#[derive(Clone)]
#[contracttype]
pub struct Groth16Proof {
    pub a: Bn254G1Affine,
    pub b: Bn254G2Affine,
    pub c: Bn254G1Affine,
}

impl Groth16Proof {
    pub fn is_empty(&self) -> bool {
        self.a.to_bytes().to_array().is_empty()
            || self.b.to_bytes().to_array().is_empty()
            || self.c.to_bytes().to_array().is_empty()
    }
}

pub const FIELD_ELEMENT_SIZE: u32 = 32;
pub const G1_SIZE: u32 = FIELD_ELEMENT_SIZE * 2;
pub const G2_SIZE: u32 = FIELD_ELEMENT_SIZE * 4;
pub const PROOF_SIZE: u32 = G1_SIZE + G2_SIZE + G1_SIZE;

impl TryFrom<Bytes> for Groth16Proof {
    type Error = Groth16Error;

    fn try_from(value: Bytes) -> Result<Self, Self::Error> {
        if value.len() != PROOF_SIZE {
            return Err(Groth16Error::MalformedProof);
        }
        let a = Bn254G1Affine::from_bytes(
            value
                .slice(0..G1_SIZE)
                .try_into()
                .map_err(|_| Groth16Error::MalformedProof)?,
        );
        let b = Bn254G2Affine::from_bytes(
            value
                .slice(G1_SIZE..G1_SIZE + G2_SIZE)
                .try_into()
                .map_err(|_| Groth16Error::MalformedProof)?,
        );
        let c = Bn254G1Affine::from_bytes(
            value
                .slice(G1_SIZE + G2_SIZE..)
                .try_into()
                .map_err(|_| Groth16Error::MalformedProof)?,
        );
        Ok(Self { a, b, c })
    }
}

#[contracttype]
#[derive(Clone)]
pub struct OrderMeta {
    pub encrypted_hints: Bytes,
    pub revealed: u64,
    pub asset_id: BytesN<32>,
    pub status: OrderStatus,
    pub created_at: u64,
    pub tif: TimeInForce,
    pub expiry_ledger: u64, // 0 = no expiry; only meaningful for GTD
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum TimeInForce {
    GTC = 0, // Good Till Cancelled
    IOC = 1, // Immediate or Cancel — full fill now or cancel
    FOK = 2, // Fill or Kill — full fill now or reject
    GTD = 3, // Good Till Date — expires at expiry_ledger
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum OrderStatus {
    Open = 0,
    Cancelled = 1,
    Expired = 2,
}

// ── Asset Configuration for multi-asset / RWA perp support ──────────────
#[contracttype]
#[derive(Clone)]
pub struct AssetConfig {
    pub max_leverage: u64,
    pub maintenance_margin_bps: i128,
    pub initial_margin_bps: i128,
    pub liq_partial_reward_bps: i128,
    pub liq_full_reward_bps: i128,
    pub ins_fund_bps: i128,
    pub active: bool,
}

// ── ShieldedPool cross-contract client ────────────────────────────────
#[soroban_sdk::contractclient(name = "ShieldedPoolClient")]
pub trait IShieldedPool {
    /// Verify a ShieldedWithdraw ZK proof and transfer `denomination` tokens to `recipient_addr`.
    /// `recipient` is bound in the ZK proof (anti-front-running binding field).
    fn withdraw(
        env: Env,
        root: BytesN<32>,
        nullifier_hash: BytesN<32>,
        recipient: BytesN<32>,
        recipient_addr: Address,
        proof: Groth16Proof,
    );
    /// Like `deposit` but called by a contract that has already transferred `denomination` USDC
    /// to the pool address. No depositor auth or token transfer — just ZK verify + state update.
    fn deposit_from_contract(
        env: Env,
        commitment: BytesN<32>,
        new_root: BytesN<32>,
        proof: Groth16Proof,
    );
    /// Fixed denomination for every deposit/withdraw in this pool (in token base units).
    fn denomination(env: Env) -> u128;
}

// ── CollateralVault cross-contract client ──────────────────────────────
#[soroban_sdk::contractclient(name = "CollateralVaultClient")]
pub trait ICollateralVault {
    fn deposit(env: Env, from: Address, amount: i128);
    fn withdraw(env: Env, to: Address, amount: i128);
    fn lock(env: Env, caller: Address, user: Address, amount: i128);
    fn unlock(env: Env, caller: Address, user: Address, amount: i128);
    fn transfer_out(env: Env, caller: Address, user: Address, to: Address, amount: i128);
    fn move_locked_to_free(
        env: Env,
        caller: Address,
        from_user: Address,
        to_user: Address,
        amount: i128,
    );
    fn free_balance(env: Env, who: Address) -> i128;
}
