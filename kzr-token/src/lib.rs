//! # Kruzer Coin (KZR) — NEP-141 fungible token
//!
//! Native-NEAR port of `Kruzer.sol` (ERC-20 + Burnable + Pausable + Permit +
//! role-based minting). Mapping follows the EVM→NEAR migration architecture doc §4:
//!
//! | Solidity (Kruzer.sol)      | NEAR (this contract)                              |
//! |----------------------------|---------------------------------------------------|
//! | ERC-20 core                | NEP-141 via `near-contract-standards`             |
//! | ERC20Burnable              | `burn` — self-burn only (deflationary)            |
//! | ERC20Pausable              | `paused` flag guarding the **mint** path          |
//! | ERC20Permit / EIP-2612     | dropped — replaced by ft_transfer_call + fn-keys  |
//! | AccessControl roles        | `owner_id` (admin) + `minters` set                |
//! | MAX_SUPPLY (1B, 18 dec)    | `MAX_SUPPLY`, checked in `mint`                   |
//! | immutable posture          | remove account full-access keys post-deploy       |
//!
//! NEAR-specific addition: `mint` auto-registers unregistered recipients from the
//! contract's own storage budget (NEP-145), so rewards/conversion can pay a
//! first-time player in a single hop (doc §3.3).

use near_contract_standards::fungible_token::core::FungibleTokenCore;
use near_contract_standards::fungible_token::events::{FtBurn, FtMint};
use near_contract_standards::fungible_token::metadata::{
    FungibleTokenMetadata, FungibleTokenMetadataProvider, FT_METADATA_SPEC,
};
use near_contract_standards::fungible_token::resolver::FungibleTokenResolver;
use near_contract_standards::fungible_token::FungibleToken;
use near_contract_standards::storage_management::{
    StorageBalance, StorageBalanceBounds, StorageManagement,
};
use near_sdk::borsh::BorshSerialize;
use near_sdk::collections::{LazyOption, UnorderedSet};
use near_sdk::json_types::U128;
use near_sdk::{
    assert_one_yocto, env, near, require, AccountId, BorshStorageKey, NearToken, PanicOnDefault,
    PromiseOrValue,
};


/// 1,000,000,000 KZR with 18 decimals (identical value to the ERC-20).
const MAX_SUPPLY: u128 = 1_000_000_000 * 1_000_000_000_000_000_000;
const DECIMALS: u8 = 18;

#[derive(BorshSerialize, BorshStorageKey)]
#[borsh(crate = "near_sdk::borsh")]
enum StorageKey {
    FungibleToken,
    Metadata,
    Minters,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Contract {
    /// NEP-141 core state (balances, total supply, storage bookkeeping).
    token: FungibleToken,
    /// NEP-148 metadata (name / symbol / decimals).
    metadata: LazyOption<FungibleTokenMetadata>,
    /// Admin (`DEFAULT_ADMIN_ROLE`). Intended to be a Sputnik DAO in production.
    owner_id: AccountId,
    /// Accounts allowed to `mint` (`MINTER_ROLE`).
    minters: UnorderedSet<AccountId>,
    /// Emergency pause — guards the mint path (the leak-risk surface).
    paused: bool,
}

#[near]
impl Contract {
    /// Mirrors `Kruzer.sol`'s `constructor(admin, treasury, initialSupply)`.
    ///
    /// * `owner_id`   — admin; also granted the minter role.
    /// * `treasury_id`— receives the initial supply.
    /// * `initial_supply` — must be in `(0, MAX_SUPPLY]`.
    #[init]
    pub fn new(owner_id: AccountId, treasury_id: AccountId, initial_supply: U128) -> Self {
        let initial: u128 = initial_supply.into();
        require!(initial > 0, "Zero amount");
        require!(initial <= MAX_SUPPLY, "Max supply exceeded");

        let metadata = FungibleTokenMetadata {
            spec: FT_METADATA_SPEC.to_string(),
            name: "Kruzer Coin".to_string(),
            symbol: "KZR".to_string(),
            icon: None,
            reference: None,
            reference_hash: None,
            decimals: DECIMALS,
        };
        metadata.assert_valid();

        let mut this = Self {
            token: FungibleToken::new(StorageKey::FungibleToken),
            metadata: LazyOption::new(StorageKey::Metadata, Some(&metadata)),
            owner_id: owner_id.clone(),
            minters: UnorderedSet::new(StorageKey::Minters),
            paused: false,
        };

        this.minters.insert(&owner_id);

        this.token.internal_register_account(&treasury_id);
        this.token.internal_deposit(&treasury_id, initial);
        FtMint {
            owner_id: &treasury_id,
            amount: U128(initial),
            memo: Some("initial supply"),
        }
        .emit();

        this
    }


    /// Mint `amount` KZR to `account_id`. Requires the minter role and an unpaused
    /// contract. Auto-registers `account_id` from the contract's storage budget if
    /// it has no balance record yet (lets conversion/rewards pay first-time players).
    pub fn mint(&mut self, account_id: AccountId, amount: U128) {
        self.assert_minter();
        require!(!self.paused, "Paused");
        let amount: u128 = amount.into();
        require!(amount > 0, "Zero amount");
        require!(
            self.token
                .total_supply
                .checked_add(amount)
                .unwrap_or_else(|| env::panic_str("Overflow"))
                <= MAX_SUPPLY,
            "Max supply exceeded"
        );

        if !self.token.accounts.contains_key(&account_id) {
            self.token.internal_register_account(&account_id);
        }
        self.token.internal_deposit(&account_id, amount);

        FtMint {
            owner_id: &account_id,
            amount: U128(amount),
            memo: None,
        }
        .emit();
    }


    /// Burn `amount` of the caller's own KZR. Deflationary burn-on-use only; there
    /// is no privileged burn-from, matching the Solidity `ERC20Burnable` semantics.
    pub fn burn(&mut self, amount: U128) {
        let account_id = env::predecessor_account_id();
        let amount: u128 = amount.into();
        require!(amount > 0, "Zero amount");

        self.token.internal_withdraw(&account_id, amount);

        FtBurn {
            owner_id: &account_id,
            amount: U128(amount),
            memo: None,
        }
        .emit();
    }


    #[payable]
    pub fn pause(&mut self) {
        assert_one_yocto();
        self.assert_owner();
        self.paused = true;
    }

    #[payable]
    pub fn unpause(&mut self) {
        assert_one_yocto();
        self.assert_owner();
        self.paused = false;
    }


    #[payable]
    pub fn add_minter(&mut self, account_id: AccountId) {
        assert_one_yocto();
        self.assert_owner();
        self.minters.insert(&account_id);
    }

    #[payable]
    pub fn remove_minter(&mut self, account_id: AccountId) {
        assert_one_yocto();
        self.assert_owner();
        self.minters.remove(&account_id);
    }

    /// Transfer admin (e.g. hand off to the Sputnik DAO). Analogous to
    /// transferring `DEFAULT_ADMIN_ROLE`.
    #[payable]
    pub fn set_owner(&mut self, new_owner: AccountId) {
        assert_one_yocto();
        self.assert_owner();
        self.owner_id = new_owner;
    }


    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn get_owner(&self) -> AccountId {
        self.owner_id.clone()
    }

    pub fn get_minters(&self) -> Vec<AccountId> {
        self.minters.to_vec()
    }

    pub fn is_minter(&self, account_id: AccountId) -> bool {
        self.minters.contains(&account_id)
    }

    pub fn max_supply(&self) -> U128 {
        U128(MAX_SUPPLY)
    }


    fn assert_owner(&self) {
        require!(
            env::predecessor_account_id() == self.owner_id,
            "Only owner"
        );
    }

    fn assert_minter(&self) {
        require!(
            self.minters.contains(&env::predecessor_account_id()),
            "Only minter"
        );
    }
}

#[near]
impl FungibleTokenCore for Contract {
    #[payable]
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>) {
        self.token.ft_transfer(receiver_id, amount, memo)
    }

    #[payable]
    fn ft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        amount: U128,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<U128> {
        self.token.ft_transfer_call(receiver_id, amount, memo, msg)
    }

    fn ft_total_supply(&self) -> U128 {
        self.token.ft_total_supply()
    }

    fn ft_balance_of(&self, account_id: AccountId) -> U128 {
        self.token.ft_balance_of(account_id)
    }
}

#[near]
impl FungibleTokenResolver for Contract {
    #[private]
    fn ft_resolve_transfer(
        &mut self,
        sender_id: AccountId,
        receiver_id: AccountId,
        amount: U128,
    ) -> U128 {
        let (used, _burned) =
            self.token
                .internal_ft_resolve_transfer(&sender_id, receiver_id, amount);
        used.into()
    }
}

#[near]
impl StorageManagement for Contract {
    #[payable]
    fn storage_deposit(
        &mut self,
        account_id: Option<AccountId>,
        registration_only: Option<bool>,
    ) -> StorageBalance {
        self.token.storage_deposit(account_id, registration_only)
    }

    #[payable]
    fn storage_withdraw(&mut self, amount: Option<NearToken>) -> StorageBalance {
        self.token.storage_withdraw(amount)
    }

    #[payable]
    fn storage_unregister(&mut self, force: Option<bool>) -> bool {
        self.token.storage_unregister(force)
    }

    fn storage_balance_bounds(&self) -> StorageBalanceBounds {
        self.token.storage_balance_bounds()
    }

    fn storage_balance_of(&self, account_id: AccountId) -> Option<StorageBalance> {
        self.token.storage_balance_of(account_id)
    }
}

#[near]
impl FungibleTokenMetadataProvider for Contract {
    fn ft_metadata(&self) -> FungibleTokenMetadata {
        self.metadata.get().unwrap()
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::{testing_env, NearToken};

    fn setup(predecessor: AccountId, yocto: u128) {
        let mut b = VMContextBuilder::new();
        b.predecessor_account_id(predecessor)
            .attached_deposit(NearToken::from_yoctonear(yocto));
        testing_env!(b.build());
    }

    fn new_contract() -> Contract {
        setup(accounts(0), 0);
        Contract::new(accounts(0), accounts(1), U128(1_000 * 10u128.pow(18)))
    }

    #[test]
    fn constructor_mints_initial_supply_to_treasury() {
        let c = new_contract();
        assert_eq!(c.ft_total_supply().0, 1_000 * 10u128.pow(18));
        assert_eq!(c.ft_balance_of(accounts(1)).0, 1_000 * 10u128.pow(18));
        assert_eq!(c.get_owner(), accounts(0));
        assert!(c.is_minter(accounts(0)));
        assert!(!c.is_paused());
        assert_eq!(c.max_supply().0, MAX_SUPPLY);
    }

    #[test]
    fn minter_can_mint_and_autoregister() {
        let mut c = new_contract();
        setup(accounts(0), 0); // alice is a minter
        c.mint(accounts(2), U128(500 * 10u128.pow(18)));
        assert_eq!(c.ft_balance_of(accounts(2)).0, 500 * 10u128.pow(18));
        assert_eq!(c.ft_total_supply().0, 1_500 * 10u128.pow(18));
    }

    #[test]
    #[should_panic(expected = "Only minter")]
    fn non_minter_cannot_mint() {
        let mut c = new_contract();
        setup(accounts(3), 0); // danny is not a minter
        c.mint(accounts(2), U128(1));
    }

    #[test]
    #[should_panic(expected = "Max supply exceeded")]
    fn mint_beyond_cap_panics() {
        let mut c = new_contract();
        setup(accounts(0), 0);
        c.mint(accounts(2), U128(MAX_SUPPLY)); // 1B + existing 1k > cap
    }

    #[test]
    #[should_panic(expected = "Paused")]
    fn mint_blocked_when_paused() {
        let mut c = new_contract();
        setup(accounts(0), 1); // 1 yocto for payable pause
        c.pause();
        setup(accounts(0), 0);
        c.mint(accounts(2), U128(1));
    }

    #[test]
    fn self_burn_reduces_supply() {
        let mut c = new_contract();
        setup(accounts(1), 0); // bob (treasury) burns his own tokens
        c.burn(U128(400 * 10u128.pow(18)));
        assert_eq!(c.ft_balance_of(accounts(1)).0, 600 * 10u128.pow(18));
        assert_eq!(c.ft_total_supply().0, 600 * 10u128.pow(18));
    }

    #[test]
    fn owner_manages_minters() {
        let mut c = new_contract();
        setup(accounts(0), 1); // alice (owner), 1 yocto
        c.add_minter(accounts(4));
        assert!(c.is_minter(accounts(4)));
        setup(accounts(4), 0);
        c.mint(accounts(5), U128(1));
        assert_eq!(c.ft_balance_of(accounts(5)).0, 1);
        setup(accounts(0), 1);
        c.remove_minter(accounts(4));
        assert!(!c.is_minter(accounts(4)));
    }

    #[test]
    #[should_panic(expected = "Only owner")]
    fn non_owner_cannot_add_minter() {
        let mut c = new_contract();
        setup(accounts(3), 1); // danny, not owner
        c.add_minter(accounts(3));
    }

    #[test]
    fn owner_can_transfer_admin() {
        let mut c = new_contract();
        setup(accounts(0), 1);
        c.set_owner(accounts(3));
        assert_eq!(c.get_owner(), accounts(3));
    }
}
