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
use near_sdk::collections::{LazyOption, LookupMap, UnorderedSet};
use near_sdk::json_types::U128;
use near_sdk::{
    assert_one_yocto, env, near, require, AccountId, BorshStorageKey, NearToken, PanicOnDefault,
    PromiseOrValue,
};

const DECIMALS: u8 = 18;
const DAY_NS: u64 = 86_400_000_000_000;
const DEFAULT_TRANSFER_CAP: u128 = 50 * 1_000_000_000_000_000_000;

#[derive(BorshSerialize, BorshStorageKey)]
#[borsh(crate = "near_sdk::borsh")]
enum StorageKey {
    FungibleToken,
    Metadata,
    Minters,
    Sinks,
    P2pTransferred,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Contract {
    token: FungibleToken,
    metadata: LazyOption<FungibleTokenMetadata>,
    owner_id: AccountId,
    minters: UnorderedSet<AccountId>,
    paused: bool,
    transfer_cap: u128,
    sinks: UnorderedSet<AccountId>,
    p2p_transferred: LookupMap<(AccountId, u64), u128>,
}

#[near]
impl Contract {
    #[init]
    pub fn new(owner_id: AccountId) -> Self {
        let metadata = FungibleTokenMetadata {
            spec: FT_METADATA_SPEC.to_string(),
            name: "Nexus Credits".to_string(),
            symbol: "NXC".to_string(),
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
            transfer_cap: DEFAULT_TRANSFER_CAP,
            sinks: UnorderedSet::new(StorageKey::Sinks),
            p2p_transferred: LookupMap::new(StorageKey::P2pTransferred),
        };
        this.minters.insert(&owner_id);
        this
    }

    pub fn mint(&mut self, account_id: AccountId, amount: U128) {
        self.assert_minter();
        require!(!self.paused, "Paused");
        let amount: u128 = amount.into();
        require!(amount > 0, "Zero amount");
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
        Self::emit_admin("paused", near_sdk::serde_json::json!({}));
    }

    #[payable]
    pub fn unpause(&mut self) {
        assert_one_yocto();
        self.assert_owner();
        self.paused = false;
        Self::emit_admin("unpaused", near_sdk::serde_json::json!({}));
    }

    #[payable]
    pub fn add_minter(&mut self, account_id: AccountId) {
        assert_one_yocto();
        self.assert_owner();
        self.minters.insert(&account_id);
        Self::emit_admin("minter_added", near_sdk::serde_json::json!({ "account_id": account_id }));
    }

    #[payable]
    pub fn remove_minter(&mut self, account_id: AccountId) {
        assert_one_yocto();
        self.assert_owner();
        self.minters.remove(&account_id);
        Self::emit_admin("minter_removed", near_sdk::serde_json::json!({ "account_id": account_id }));
    }

    #[payable]
    pub fn set_owner(&mut self, new_owner: AccountId) {
        assert_one_yocto();
        self.assert_owner();
        let old_owner = self.owner_id.clone();
        Self::emit_admin("owner_changed", near_sdk::serde_json::json!({ "old_owner": old_owner, "new_owner": new_owner }));
        self.owner_id = new_owner;
    }

    #[payable]
    pub fn register_sink(&mut self, account_id: AccountId) {
        assert_one_yocto();
        self.assert_owner();
        self.sinks.insert(&account_id);
        Self::emit_admin("sink_registered", near_sdk::serde_json::json!({ "account_id": account_id }));
    }

    #[payable]
    pub fn unregister_sink(&mut self, account_id: AccountId) {
        assert_one_yocto();
        self.assert_owner();
        self.sinks.remove(&account_id);
        Self::emit_admin("sink_unregistered", near_sdk::serde_json::json!({ "account_id": account_id }));
    }

    #[payable]
    pub fn set_transfer_cap(&mut self, cap: U128) {
        assert_one_yocto();
        self.assert_owner();
        self.transfer_cap = cap.into();
        Self::emit_admin("transfer_cap_changed", near_sdk::serde_json::json!({ "cap": cap }));
    }

    fn emit_admin(event: &str, mut data: near_sdk::serde_json::Value) {
        if let Some(obj) = data.as_object_mut() {
            obj.insert(
                "by".to_string(),
                near_sdk::serde_json::json!(near_sdk::env::predecessor_account_id()),
            );
        }
        let payload = near_sdk::serde_json::json!({
            "standard": "kzr_admin",
            "version": "1.0.0",
            "event": event,
            "data": [data],
        });
        near_sdk::env::log_str(&format!("EVENT_JSON:{}", payload));
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

    pub fn get_sinks(&self) -> Vec<AccountId> {
        self.sinks.to_vec()
    }

    pub fn is_sink(&self, account_id: AccountId) -> bool {
        self.sinks.contains(&account_id)
    }

    pub fn get_transfer_cap(&self) -> U128 {
        U128(self.transfer_cap)
    }

    pub fn p2p_transferred_of(&self, account_id: AccountId) -> U128 {
        let bucket = env::block_timestamp() / DAY_NS;
        U128(self.p2p_transferred.get(&(account_id, bucket)).unwrap_or(0))
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

    fn note_p2p(&mut self, sender: &AccountId, receiver: &AccountId, amount: u128) {
        if self.sinks.contains(receiver) {
            return;
        }
        let bucket = env::block_timestamp() / DAY_NS;
        let key = (sender.clone(), bucket);
        let used = self.p2p_transferred.get(&key).unwrap_or(0);
        let new_used = used
            .checked_add(amount)
            .unwrap_or_else(|| env::panic_str("Overflow"));
        require!(
            new_used <= self.transfer_cap,
            "P2P 24h transfer cap exceeded"
        );
        self.p2p_transferred.insert(&key, &new_used);
    }
}

#[near]
impl FungibleTokenCore for Contract {
    #[payable]
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>) {
        self.note_p2p(&env::predecessor_account_id(), &receiver_id, amount.into());
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
        self.note_p2p(&env::predecessor_account_id(), &receiver_id, amount.into());
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

    const ONE: u128 = 1_000_000_000_000_000_000;

    fn ctx(predecessor: AccountId, yocto: u128) {
        let mut b = VMContextBuilder::new();
        b.current_account_id(accounts(0))
            .predecessor_account_id(predecessor)
            .attached_deposit(NearToken::from_yoctonear(yocto))
            .block_timestamp(1_000);
        testing_env!(b.build());
    }

    fn new_contract() -> Contract {
        ctx(accounts(0), 0);
        Contract::new(accounts(0))
    }

    #[test]
    fn admin_action_emits_kzr_admin_event() {
        let mut c = new_contract();
        ctx(accounts(0), 1);
        c.set_transfer_cap(U128(123));
        let ev = near_sdk::test_utils::get_logs()
            .into_iter()
            .find(|l| l.contains("EVENT_JSON"))
            .expect("admin event not emitted");
        assert!(ev.contains("\"standard\":\"kzr_admin\""));
        assert!(ev.contains("\"event\":\"transfer_cap_changed\""));
        assert!(ev.contains("\"cap\":\"123\""));
        assert!(ev.contains("\"by\":"));
    }

    fn lcg(seed: &mut u64) -> u64 {
        *seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        *seed
    }

    #[test]
    fn randomized_p2p_cap_tracks() {
        let mut c = new_contract();
        let mut seed: u64 = 0x0C0F_FEE1_2345_6789;
        for i in 0u64..48 {
            let sender: AccountId = format!("s{}.near", i).parse().unwrap();
            let recv: AccountId = format!("r{}.near", i).parse().unwrap();
            ctx(accounts(0), 0);
            c.mint(sender.clone(), U128(1_000 * ONE));
            c.mint(recv.clone(), U128(ONE));
            let mut used = 0u128;
            let n = (lcg(&mut seed) % 5) + 1;
            for _ in 0..n {
                let a = ((lcg(&mut seed) % 10) + 1) as u128;
                if used + a > 50 {
                    break;
                }
                ctx(sender.clone(), 1);
                c.ft_transfer(recv.clone(), U128(a * ONE), None);
                used += a;
                assert_eq!(c.p2p_transferred_of(sender.clone()).0, used * ONE);
                assert_eq!(c.ft_balance_of(recv.clone()).0, (1 + used) * ONE);
            }
            assert!(used <= 50);
        }
    }

    #[test]
    fn minter_mints_and_autoregisters() {
        let mut c = new_contract();
        ctx(accounts(0), 0);
        c.mint(accounts(1), U128(500 * ONE));
        assert_eq!(c.ft_balance_of(accounts(1)).0, 500 * ONE);
        assert_eq!(c.ft_total_supply().0, 500 * ONE);
        assert_eq!(c.ft_metadata().symbol, "NXC");
        assert_eq!(c.ft_metadata().decimals, 18);
    }

    #[test]
    #[should_panic(expected = "Only minter")]
    fn non_minter_cannot_mint() {
        let mut c = new_contract();
        ctx(accounts(3), 0);
        c.mint(accounts(1), U128(ONE));
    }

    #[test]
    #[should_panic(expected = "Paused")]
    fn mint_blocked_when_paused() {
        let mut c = new_contract();
        ctx(accounts(0), 1);
        c.pause();
        ctx(accounts(0), 0);
        c.mint(accounts(1), U128(ONE));
    }

    #[test]
    fn self_burn_reduces_supply() {
        let mut c = new_contract();
        ctx(accounts(0), 0);
        c.mint(accounts(1), U128(100 * ONE));
        ctx(accounts(1), 0);
        c.burn(U128(40 * ONE));
        assert_eq!(c.ft_balance_of(accounts(1)).0, 60 * ONE);
        assert_eq!(c.ft_total_supply().0, 60 * ONE);
    }

    #[test]
    fn p2p_transfer_under_cap_ok() {
        let mut c = new_contract();
        ctx(accounts(0), 0);
        c.mint(accounts(1), U128(100 * ONE));
        c.mint(accounts(2), U128(0 + ONE));
        ctx(accounts(1), 1);
        c.ft_transfer(accounts(2), U128(30 * ONE), None);
        assert_eq!(c.ft_balance_of(accounts(2)).0, 31 * ONE);
        assert_eq!(c.p2p_transferred_of(accounts(1)).0, 30 * ONE);
    }

    #[test]
    #[should_panic(expected = "P2P 24h transfer cap exceeded")]
    fn p2p_transfer_over_cap_panics() {
        let mut c = new_contract();
        ctx(accounts(0), 0);
        c.mint(accounts(1), U128(100 * ONE));
        c.mint(accounts(2), U128(ONE));
        ctx(accounts(1), 1);
        c.ft_transfer(accounts(2), U128(51 * ONE), None);
    }

    #[test]
    fn transfer_to_sink_is_exempt() {
        let mut c = new_contract();
        ctx(accounts(0), 1);
        c.register_sink(accounts(2));
        ctx(accounts(0), 0);
        c.mint(accounts(1), U128(1_000 * ONE));
        c.mint(accounts(2), U128(ONE));
        ctx(accounts(1), 1);
        c.ft_transfer(accounts(2), U128(500 * ONE), None);
        assert_eq!(c.ft_balance_of(accounts(2)).0, 501 * ONE);
        assert_eq!(c.p2p_transferred_of(accounts(1)).0, 0);
    }

    #[test]
    fn owner_sets_cap_and_manages_sinks() {
        let mut c = new_contract();
        ctx(accounts(0), 1);
        c.set_transfer_cap(U128(10 * ONE));
        assert_eq!(c.get_transfer_cap().0, 10 * ONE);
        c.register_sink(accounts(4));
        assert!(c.is_sink(accounts(4)));
        c.unregister_sink(accounts(4));
        assert!(!c.is_sink(accounts(4)));
    }

    #[test]
    #[should_panic(expected = "Only owner")]
    fn non_owner_cannot_set_cap() {
        let mut c = new_contract();
        ctx(accounts(3), 1);
        c.set_transfer_cap(U128(ONE));
    }
}
