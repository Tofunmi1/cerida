#![no_std]
#![allow(clippy::too_many_arguments)]

use soroban_sdk::{
    contract, contractimpl, contracttype,
    crypto::bn254::{Bn254Fr, Bn254G1Affine as G1Affine, Bn254G2Affine as G2Affine},
    BytesN, Env, Vec,
};
use types::{Groth16Error, Groth16Proof, OrderMeta, OrderStatus, TimeInForce};

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
        commitment: BytesN<32>,
        portfolio_key: BytesN<32>,
        hint_price: u64,
        hint_side: u64,
        hint_size: u64,
        hint_leverage: u64,
        revealed: u64,
        tif: TimeInForce,
        expiry_ledger: u64,
        asset_id: BytesN<32>,
        proof: Groth16Proof,
    ) {
        if tif == TimeInForce::GTD && expiry_ledger == 0 {
            panic!("Orderbook: GTD order requires expiry_ledger > 0");
        }
        if tif != TimeInForce::GTD && expiry_ledger != 0 {
            panic!("Orderbook: expiry_ledger only valid for GTD orders");
        }

        let order_key = DataKey::Order(commitment.clone());
        if env.storage().persistent().has(&order_key) {
            panic!("Orderbook: commitment already exists");
        }

        let mut pi: Vec<Bn254Fr> = Vec::new(&env);
        pi.push_back(Bn254Fr::from_bytes(commitment.clone()));
        pi.push_back(Bn254Fr::from_bytes(portfolio_key));

        let vk = load_vk(&env, &VK_COMMIT_IC);
        match verify_groth16(&env, &vk, &proof, &pi) {
            Ok(true) => {}
            _ => panic!("Orderbook: invalid commitment proof"),
        }

        let meta = OrderMeta {
            hint_price,
            hint_side,
            hint_size,
            hint_leverage,
            revealed,
            asset_id,
            status: OrderStatus::Open,
            created_at: env.ledger().sequence() as u64,
            tif,
            expiry_ledger,
        };

        env.storage().persistent().set(&order_key, &meta);
        env.storage()
            .persistent()
            .extend_ttl(&order_key, 17280, 17280);

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("place"),),
            (
                commitment,
                hint_price,
                hint_side,
                hint_size,
                hint_leverage,
                revealed,
                meta.created_at,
            ),
        );
    }

    pub fn cancel_order(
        env: Env,
        commitment: BytesN<32>,
        nullifier: BytesN<32>,
        proof: Groth16Proof,
    ) {
        let null_key = DataKey::Nullifier(nullifier.clone());
        if env.storage().persistent().has(&null_key) {
            panic!("Orderbook: nullifier already spent");
        }

        let order_key = DataKey::Order(commitment.clone());
        let mut meta: OrderMeta = env
            .storage()
            .persistent()
            .get(&order_key)
            .unwrap_or_else(|| panic!("Orderbook: commitment not found"));

        if meta.status != OrderStatus::Open {
            panic!(
                "Orderbook: order is not open (status={:?})",
                meta.status as u32
            );
        }
        if meta.status == OrderStatus::Expired {
            panic!("Orderbook: order has expired");
        }

        let mut pi: Vec<Bn254Fr> = Vec::new(&env);
        pi.push_back(Bn254Fr::from_bytes(nullifier.clone()));

        let vk = load_vk(&env, &VK_CANCEL_IC);
        match verify_groth16(&env, &vk, &proof, &pi) {
            Ok(true) => {}
            _ => panic!("Orderbook: invalid cancel proof"),
        }

        meta.status = OrderStatus::Cancelled;
        env.storage().persistent().set(&order_key, &meta);
        env.storage().persistent().set(&null_key, &true);
        env.storage()
            .persistent()
            .extend_ttl(&order_key, 17280, 17280);
        env.storage()
            .persistent()
            .extend_ttl(&null_key, 17280, 17280);

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("cancel"),),
            (commitment, nullifier, meta.created_at),
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

    pub fn get_tif(env: Env, commitment: BytesN<32>) -> Option<TimeInForce> {
        env.storage()
            .persistent()
            .get::<_, OrderMeta>(&DataKey::Order(commitment))
            .map(|m| m.tif)
    }

    /// Mark a GTD order as expired. Callable by anyone once past expiry_ledger.
    pub fn expire_order(env: Env, commitment: BytesN<32>) {
        let order_key = DataKey::Order(commitment.clone());
        let mut meta: OrderMeta = env
            .storage()
            .persistent()
            .get(&order_key)
            .unwrap_or_else(|| panic!("Orderbook: commitment not found"));
        if meta.status != OrderStatus::Open {
            panic!(
                "Orderbook: order is not open (status={:?})",
                meta.status as u32
            );
        }
        if meta.tif != TimeInForce::GTD {
            panic!("Orderbook: only GTD orders can expire");
        }
        let now = env.ledger().sequence() as u64;
        if meta.expiry_ledger == 0 || now <= meta.expiry_ledger {
            panic!(
                "Orderbook: order not yet expired (expiry={}, now={})",
                meta.expiry_ledger, now
            );
        }
        meta.status = OrderStatus::Expired;
        env.storage().persistent().set(&order_key, &meta);
        env.storage()
            .persistent()
            .extend_ttl(&order_key, 17280, 17280);

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("expire"),),
            (commitment, meta.expiry_ledger, now),
        );
    }
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{
        testutils::{Ledger, LedgerInfo},
        Address,
    };

    fn default_ledger() -> LedgerInfo {
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

    fn setup() -> (Env, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let cid = env.register(Orderbook, ());
        (env, cid)
    }

    fn insert_order(
        env: &Env,
        cid: &Address,
        commitment: &BytesN<32>,
        tif: TimeInForce,
        expiry: u64,
    ) {
        env.as_contract(cid, || {
            let meta = OrderMeta {
                hint_price: 100,
                hint_side: 0,
                hint_size: 1000,
                hint_leverage: 5,
                revealed: 15,
                asset_id: BytesN::from_array(env, &[0u8; 32]),
                status: OrderStatus::Open,
                created_at: 0,
                tif,
                expiry_ledger: expiry,
            };
            env.storage()
                .persistent()
                .set(&DataKey::Order(commitment.clone()), &meta);
        });
    }

    #[test]
    fn test_gtd_expire_order() {
        let (env, cid) = setup();
        let client = OrderbookClient::new(&env, &cid);
        let cmt = BytesN::from_array(&env, &[1u8; 32]);
        insert_order(&env, &cid, &cmt, TimeInForce::GTD, 10);

        env.ledger().set(LedgerInfo {
            sequence_number: 11,
            ..default_ledger()
        });
        client.expire_order(&cmt);
        assert_eq!(client.status(&cmt), Some(OrderStatus::Expired));
    }

    #[test]
    #[should_panic(expected = "order not yet expired")]
    fn test_expire_order_before_expiry_reverts() {
        let (env, cid) = setup();
        let client = OrderbookClient::new(&env, &cid);
        let cmt = BytesN::from_array(&env, &[2u8; 32]);
        insert_order(&env, &cid, &cmt, TimeInForce::GTD, 100);

        env.ledger().set(LedgerInfo {
            sequence_number: 50,
            ..default_ledger()
        });
        client.expire_order(&cmt);
    }

    #[test]
    #[should_panic(expected = "only GTD orders can expire")]
    fn test_expire_non_gtd_order_reverts() {
        let (env, cid) = setup();
        let client = OrderbookClient::new(&env, &cid);
        let cmt = BytesN::from_array(&env, &[3u8; 32]);
        insert_order(&env, &cid, &cmt, TimeInForce::GTC, 0);

        env.ledger().set(LedgerInfo {
            sequence_number: 999,
            ..default_ledger()
        });
        client.expire_order(&cmt);
    }

    #[test]
    #[should_panic(expected = "order is not open")]
    fn test_expire_already_expired_order_reverts() {
        let (env, cid) = setup();
        let client = OrderbookClient::new(&env, &cid);
        let cmt = BytesN::from_array(&env, &[4u8; 32]);
        insert_order(&env, &cid, &cmt, TimeInForce::GTD, 5);

        env.ledger().set(LedgerInfo {
            sequence_number: 10,
            ..default_ledger()
        });
        client.expire_order(&cmt);
        client.expire_order(&cmt); // second call should panic
    }

    #[test]
    fn test_status_nonexistent_returns_none() {
        let (env, cid) = setup();
        let client = OrderbookClient::new(&env, &cid);
        let cmt = BytesN::from_array(&env, &[99u8; 32]);
        assert_eq!(client.status(&cmt), None);
    }

    #[test]
    fn test_get_tif_returns_correct_variant() {
        let (env, cid) = setup();
        let client = OrderbookClient::new(&env, &cid);

        let cmt_gtc = BytesN::from_array(&env, &[10u8; 32]);
        let cmt_fok = BytesN::from_array(&env, &[11u8; 32]);
        let cmt_ioc = BytesN::from_array(&env, &[12u8; 32]);
        let cmt_gtd = BytesN::from_array(&env, &[13u8; 32]);

        insert_order(&env, &cid, &cmt_gtc, TimeInForce::GTC, 0);
        insert_order(&env, &cid, &cmt_fok, TimeInForce::FOK, 0);
        insert_order(&env, &cid, &cmt_ioc, TimeInForce::IOC, 0);
        insert_order(&env, &cid, &cmt_gtd, TimeInForce::GTD, 50);

        assert_eq!(client.get_tif(&cmt_gtc), Some(TimeInForce::GTC));
        assert_eq!(client.get_tif(&cmt_fok), Some(TimeInForce::FOK));
        assert_eq!(client.get_tif(&cmt_ioc), Some(TimeInForce::IOC));
        assert_eq!(client.get_tif(&cmt_gtd), Some(TimeInForce::GTD));
    }
}
