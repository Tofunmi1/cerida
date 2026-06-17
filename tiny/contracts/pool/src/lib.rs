#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env};

#[contracttype]
pub enum DataKey {
    Commitment(BytesN<32>),
    Nullifier(BytesN<32>),
}

#[contracttype]
#[derive(Clone)]
pub struct Note {
    pub amount: i128,
    pub owner: Address,
}

#[contract]
pub struct TinyPool;

#[contractimpl]
impl TinyPool {
    /// Deposit: create a new UTXO commitment.
    pub fn deposit(env: Env, owner: Address, commitment: BytesN<32>, amount: i128) {
        owner.require_auth();
        if amount <= 0 {
            panic!("amount must be positive");
        }
        let key = DataKey::Commitment(commitment.clone());
        if env.storage().persistent().has(&key) {
            panic!("commitment already exists");
        }
        env.storage().persistent().set(&key, &Note { amount, owner });
        env.storage().persistent().extend_ttl(&key, 17280, 17280);
    }

    /// Withdraw: spend a commitment. The caller must have already verified
    /// the ZK proof against the verifier contract externally.
    pub fn withdraw(
        env: Env,
        owner: Address,
        commitment: BytesN<32>,
        nullifier: BytesN<32>,
    ) -> i128 {
        owner.require_auth();

        let null_key = DataKey::Nullifier(nullifier.clone());
        if env.storage().persistent().has(&null_key) {
            panic!("already spent");
        }

        let pos_key = DataKey::Commitment(commitment.clone());
        let note: Note = env
            .storage()
            .persistent()
            .get(&pos_key)
            .unwrap_or_else(|| panic!("commitment not found"));
        if note.owner != owner {
            panic!("unauthorized");
        }

        env.storage().persistent().set(&null_key, &true);
        env.storage().persistent().extend_ttl(&null_key, 17280, 17280);
        env.storage().persistent().remove(&pos_key);

        note.amount
    }

    pub fn balance_of(env: Env, commitment: BytesN<32>) -> Option<i128> {
        env.storage()
            .persistent()
            .get::<_, Note>(&DataKey::Commitment(commitment))
            .map(|n| n.amount)
    }

    pub fn is_spent(env: Env, nullifier: BytesN<32>) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::Nullifier(nullifier))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    fn seed(env: &Env, v: u8) -> BytesN<32> {
        BytesN::from_array(env, &[v; 32])
    }

    #[test]
    fn test_deposit_stores_note() {
        let env = Env::default();
        env.mock_all_auths();
        let id = env.register_contract(None, TinyPool);
        let client = TinyPoolClient::new(&env, &id);

        let owner = Address::generate(&env);
        let c = seed(&env, 1);
        client.deposit(&owner, &c, &1_000_000);
        assert_eq!(client.balance_of(&c), Some(1_000_000));
    }

    #[test]
    #[should_panic(expected = "commitment already exists")]
    fn test_duplicate_deposit_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let id = env.register_contract(None, TinyPool);
        let client = TinyPoolClient::new(&env, &id);

        let owner = Address::generate(&env);
        let c = seed(&env, 1);
        client.deposit(&owner, &c, &1_000_000);
        client.deposit(&owner, &c, &500_000);
    }

    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn test_zero_deposit_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let id = env.register_contract(None, TinyPool);
        let client = TinyPoolClient::new(&env, &id);

        client.deposit(&Address::generate(&env), &seed(&env, 1), &0);
    }
}
