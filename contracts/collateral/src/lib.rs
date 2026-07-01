#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, token::TokenClient, Address, Env};

#[contracttype]
pub enum DataKey {
    Config,
    Free(Address),
    Locked(Address),
    Authorized(Address),
}

#[contracttype]
#[derive(Clone)]
pub struct Config {
    pub admin: Address,
    pub token: Address,
}

#[contract]
pub struct CollateralVault;

#[contractimpl]
impl CollateralVault {
    pub fn initialize(env: Env, admin: Address, token: Address) {
        if env.storage().instance().has(&DataKey::Config) {
            panic!("CollateralVault: already initialized");
        }
        env.storage()
            .instance()
            .set(&DataKey::Config, &Config { admin, token });
    }

    /// Grant `contract` permission to call lock / unlock / transfer_out.
    pub fn authorize(env: Env, contract: Address) {
        Self::require_admin(&env);
        env.storage()
            .persistent()
            .set(&DataKey::Authorized(contract), &true);
    }

    /// Revoke permission from `contract`.
    pub fn deauthorize(env: Env, contract: Address) {
        Self::require_admin(&env);
        env.storage()
            .persistent()
            .remove(&DataKey::Authorized(contract));
    }

    // ── User-facing ──────────────────────────────────────────────────────────

    /// Transfer `amount` of the vault's token from `from` into the vault, crediting free balance.
    pub fn deposit(env: Env, from: Address, amount: i128) {
        from.require_auth();
        if amount <= 0 {
            panic!("CollateralVault: amount must be positive");
        }
        let cfg = Self::config(&env);
        TokenClient::new(&env, &cfg.token).transfer(
            &from,
            &env.current_contract_address(),
            &amount,
        );
        let key = DataKey::Free(from.clone());
        let bal = Self::read_free(&env, &from);
        env.storage().persistent().set(&key, &(bal + amount));
        env.storage().persistent().extend_ttl(&key, 17280, 17280);

        #[allow(deprecated)]
        env.events()
            .publish((soroban_sdk::symbol_short!("deposit"),), (from, amount));
    }

    /// Withdraw `amount` from the caller's free balance back to their wallet.
    pub fn withdraw(env: Env, to: Address, amount: i128) {
        to.require_auth();
        if amount <= 0 {
            panic!("CollateralVault: amount must be positive");
        }
        let free = Self::read_free(&env, &to);
        if free < amount {
            panic!(
                "CollateralVault: insufficient free balance (have {}, need {})",
                free, amount
            );
        }
        let key = DataKey::Free(to.clone());
        env.storage().persistent().set(&key, &(free - amount));
        env.storage().persistent().extend_ttl(&key, 17280, 17280);
        let cfg = Self::config(&env);
        TokenClient::new(&env, &cfg.token).transfer(&env.current_contract_address(), &to, &amount);

        #[allow(deprecated)]
        env.events()
            .publish((soroban_sdk::symbol_short!("withdraw"),), (to, amount));
    }

    // ── Authorized-contract operations ───────────────────────────────────────

    /// Move `amount` from `user`'s free balance into locked balance.
    /// Caller must be an authorized contract (e.g. the perp-engine).
    pub fn lock(env: Env, caller: Address, user: Address, amount: i128) {
        caller.require_auth();
        Self::require_authorized(&env, &caller);
        if amount <= 0 {
            panic!("CollateralVault: amount must be positive");
        }
        let free = Self::read_free(&env, &user);
        if free < amount {
            panic!(
                "CollateralVault: insufficient free balance to lock (have {}, need {})",
                free, amount
            );
        }
        let locked = Self::read_locked(&env, &user);
        Self::set_free(&env, &user, free - amount);
        Self::set_locked(&env, &user, locked + amount);

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("lock"),),
            (caller, user, amount),
        );
    }

    /// Move `amount` from `user`'s locked balance back to free balance.
    pub fn unlock(env: Env, caller: Address, user: Address, amount: i128) {
        caller.require_auth();
        Self::require_authorized(&env, &caller);
        if amount <= 0 {
            panic!("CollateralVault: amount must be positive");
        }
        let locked = Self::read_locked(&env, &user);
        if locked < amount {
            panic!(
                "CollateralVault: insufficient locked balance to unlock (have {}, need {})",
                locked, amount
            );
        }
        let free = Self::read_free(&env, &user);
        Self::set_locked(&env, &user, locked - amount);
        Self::set_free(&env, &user, free + amount);

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("unlock"),),
            (caller, user, amount),
        );
    }

    /// Send `amount` of the vault's token to `to`, deducting from `user`'s locked balance.
    /// Used by the perp-engine to pay out settlement, liquidation rewards, etc.
    pub fn transfer_out(env: Env, caller: Address, user: Address, to: Address, amount: i128) {
        caller.require_auth();
        Self::require_authorized(&env, &caller);
        if amount <= 0 {
            return;
        }
        let locked = Self::read_locked(&env, &user);
        if locked < amount {
            panic!(
                "CollateralVault: insufficient locked balance for transfer_out (have {}, need {})",
                locked, amount
            );
        }
        Self::set_locked(&env, &user, locked - amount);
        let cfg = Self::config(&env);
        TokenClient::new(&env, &cfg.token).transfer(&env.current_contract_address(), &to, &amount);

        #[allow(deprecated)]
        env.events().publish(
            (soroban_sdk::symbol_short!("xfer_out"),),
            (caller, user, to, amount),
        );
    }

    /// Move `amount` from `user`'s locked balance to their free balance AND credit to `recipient`'s
    /// free balance — used by the engine to redistribute PnL without moving tokens out of the vault.
    /// Caller must be authorized. `from_user`'s locked must cover `amount`.
    pub fn move_locked_to_free(
        env: Env,
        caller: Address,
        from_user: Address,
        to_user: Address,
        amount: i128,
    ) {
        caller.require_auth();
        Self::require_authorized(&env, &caller);
        if amount <= 0 {
            return;
        }
        let locked = Self::read_locked(&env, &from_user);
        if locked < amount {
            panic!(
                "CollateralVault: insufficient locked balance for redistribution (have {}, need {})",
                locked, amount
            );
        }
        Self::set_locked(&env, &from_user, locked - amount);
        let to_free = Self::read_free(&env, &to_user);
        Self::set_free(&env, &to_user, to_free + amount);
    }

    // ── Queries ───────────────────────────────────────────────────────────────

    pub fn free_balance(env: Env, who: Address) -> i128 {
        Self::read_free(&env, &who)
    }

    pub fn locked_balance(env: Env, who: Address) -> i128 {
        Self::read_locked(&env, &who)
    }

    pub fn total_balance(env: Env, who: Address) -> i128 {
        Self::read_free(&env, &who) + Self::read_locked(&env, &who)
    }

    pub fn is_authorized(env: Env, contract: Address) -> bool {
        env.storage()
            .persistent()
            .get::<_, bool>(&DataKey::Authorized(contract))
            .unwrap_or(false)
    }

    pub fn get_config(env: Env) -> Config {
        Self::config(&env)
    }

    // ── Internals ─────────────────────────────────────────────────────────────

    fn config(env: &Env) -> Config {
        env.storage()
            .instance()
            .get(&DataKey::Config)
            .unwrap_or_else(|| panic!("CollateralVault: not initialized"))
    }

    fn require_admin(env: &Env) {
        let cfg = Self::config(env);
        cfg.admin.require_auth();
    }

    fn require_authorized(env: &Env, contract: &Address) {
        if !env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Authorized(contract.clone()))
            .unwrap_or(false)
        {
            panic!("CollateralVault: caller is not authorized");
        }
    }

    fn read_free(env: &Env, who: &Address) -> i128 {
        env.storage()
            .persistent()
            .get::<_, i128>(&DataKey::Free(who.clone()))
            .unwrap_or(0)
    }

    fn read_locked(env: &Env, who: &Address) -> i128 {
        env.storage()
            .persistent()
            .get::<_, i128>(&DataKey::Locked(who.clone()))
            .unwrap_or(0)
    }

    fn set_free(env: &Env, who: &Address, val: i128) {
        let key = DataKey::Free(who.clone());
        env.storage().persistent().set(&key, &val);
        env.storage().persistent().extend_ttl(&key, 17280, 17280);
    }

    fn set_locked(env: &Env, who: &Address, val: i128) {
        let key = DataKey::Locked(who.clone());
        env.storage().persistent().set(&key, &val);
        env.storage().persistent().extend_ttl(&key, 17280, 17280);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{
        testutils::Address as _,
        token::{StellarAssetClient, TokenClient},
        Address, Env,
    };

    fn setup() -> (Env, Address, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault_id = env.register(CollateralVault, ());
        let token_admin = Address::generate(&env);
        let token_id = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let client = CollateralVaultClient::new(&env, &vault_id);
        client.initialize(&admin, &token_id);
        (env, vault_id, token_id, admin)
    }

    fn mint(env: &Env, token_id: &Address, to: &Address, amount: i128) {
        StellarAssetClient::new(env, token_id).mint(to, &amount);
    }

    #[test]
    fn test_deposit_and_free_balance() {
        let (env, vault_id, token_id, _admin) = setup();
        let client = CollateralVaultClient::new(&env, &vault_id);
        let user = Address::generate(&env);
        mint(&env, &token_id, &user, 1_000_000);

        client.deposit(&user, &500_000i128);
        assert_eq!(client.free_balance(&user), 500_000);
        assert_eq!(client.locked_balance(&user), 0);
        assert_eq!(client.total_balance(&user), 500_000);
    }

    #[test]
    fn test_withdraw_reduces_free_balance() {
        let (env, vault_id, token_id, _admin) = setup();
        let client = CollateralVaultClient::new(&env, &vault_id);
        let user = Address::generate(&env);
        mint(&env, &token_id, &user, 1_000_000);

        client.deposit(&user, &1_000_000i128);
        client.withdraw(&user, &300_000i128);
        assert_eq!(client.free_balance(&user), 700_000);
        assert_eq!(TokenClient::new(&env, &token_id).balance(&user), 300_000);
    }

    #[test]
    #[should_panic(expected = "insufficient free balance")]
    fn test_withdraw_overdraft_panics() {
        let (env, vault_id, token_id, _admin) = setup();
        let client = CollateralVaultClient::new(&env, &vault_id);
        let user = Address::generate(&env);
        mint(&env, &token_id, &user, 500_000);
        client.deposit(&user, &500_000i128);
        client.withdraw(&user, &600_000i128);
    }

    #[test]
    fn test_lock_moves_free_to_locked() {
        let (env, vault_id, token_id, admin) = setup();
        let client = CollateralVaultClient::new(&env, &vault_id);
        let user = Address::generate(&env);
        let engine = Address::generate(&env);
        mint(&env, &token_id, &user, 1_000_000);
        client.deposit(&user, &1_000_000i128);
        client.authorize(&engine);

        client.lock(&engine, &user, &400_000i128);
        assert_eq!(client.free_balance(&user), 600_000);
        assert_eq!(client.locked_balance(&user), 400_000);
        let _ = admin; // suppress unused warning
    }

    #[test]
    fn test_unlock_moves_locked_to_free() {
        let (env, vault_id, token_id, _admin) = setup();
        let client = CollateralVaultClient::new(&env, &vault_id);
        let user = Address::generate(&env);
        let engine = Address::generate(&env);
        mint(&env, &token_id, &user, 1_000_000);
        client.deposit(&user, &1_000_000i128);
        client.authorize(&engine);
        client.lock(&engine, &user, &400_000i128);

        client.unlock(&engine, &user, &400_000i128);
        assert_eq!(client.free_balance(&user), 1_000_000);
        assert_eq!(client.locked_balance(&user), 0);
    }

    #[test]
    fn test_transfer_out_deducts_locked_and_sends_tokens() {
        let (env, vault_id, token_id, _admin) = setup();
        let client = CollateralVaultClient::new(&env, &vault_id);
        let user = Address::generate(&env);
        let engine = Address::generate(&env);
        let recipient = Address::generate(&env);
        mint(&env, &token_id, &user, 1_000_000);
        client.deposit(&user, &1_000_000i128);
        client.authorize(&engine);
        client.lock(&engine, &user, &1_000_000i128);

        client.transfer_out(&engine, &user, &recipient, &150_000i128);
        assert_eq!(client.locked_balance(&user), 850_000);
        assert_eq!(
            TokenClient::new(&env, &token_id).balance(&recipient),
            150_000
        );
    }

    #[test]
    #[should_panic(expected = "caller is not authorized")]
    fn test_lock_unauthorized_panics() {
        let (env, vault_id, token_id, _admin) = setup();
        let client = CollateralVaultClient::new(&env, &vault_id);
        let user = Address::generate(&env);
        let rando = Address::generate(&env);
        mint(&env, &token_id, &user, 1_000_000);
        client.deposit(&user, &1_000_000i128);
        client.lock(&rando, &user, &100_000i128);
    }

    #[test]
    fn test_authorize_deauthorize() {
        let (env, vault_id, _token_id, _admin) = setup();
        let client = CollateralVaultClient::new(&env, &vault_id);
        let engine = Address::generate(&env);

        assert!(!client.is_authorized(&engine));
        client.authorize(&engine);
        assert!(client.is_authorized(&engine));
        client.deauthorize(&engine);
        assert!(!client.is_authorized(&engine));
    }

    #[test]
    fn test_multi_user_isolated_balances() {
        let (env, vault_id, token_id, _admin) = setup();
        let client = CollateralVaultClient::new(&env, &vault_id);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        mint(&env, &token_id, &alice, 2_000_000);
        mint(&env, &token_id, &bob, 1_000_000);

        client.deposit(&alice, &2_000_000i128);
        client.deposit(&bob, &1_000_000i128);

        assert_eq!(client.free_balance(&alice), 2_000_000);
        assert_eq!(client.free_balance(&bob), 1_000_000);
        assert_eq!(client.total_balance(&alice), 2_000_000);
    }

    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn test_deposit_zero_panics() {
        let (env, vault_id, _token_id, _admin) = setup();
        let client = CollateralVaultClient::new(&env, &vault_id);
        let user = Address::generate(&env);
        client.deposit(&user, &0i128);
    }
}
