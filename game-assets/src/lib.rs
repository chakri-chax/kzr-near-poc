//! # Kruzer Game Assets — NEP-245 multi-token
//!
//! Native-NEAR port of `KruzerAssets1155.sol` (ERC-1155 + Supply + EIP-712
//! voucher mint + burn-to-craft). Mapping follows the migration doc §4:
//!
//! | Solidity (KruzerAssets1155.sol) | NEAR (this contract)                        |
//! |---------------------------------|---------------------------------------------|
//! | ERC-1155 core                   | NEP-245 (`mt_transfer`, `mt_balance_of`, …) |
//! | `ERC1155Supply`                 | per-id `supply` + admin-set `max_supply`    |
//! | EIP-712 `MintVoucher`           | ed25519-signed **Borsh** voucher            |
//! | EIP-712 domain separator        | `contract_id` + `chain_id` inside the payload|
//! | burn-to-craft                   | `burn_for_craft` (atomic, same contract)    |
//! | packed token-id                 | `build_token_id` (doc §12)                  |
//!
//! Design notes (see near-poc/README.md):
//! * `max_supply` is **contract-governed** (admin `register_token`), never taken
//!   from the voucher — this bounds damage if the backend signer key leaks (§4).
//! * Loot is computed off-chain and attested by the signed voucher; there is no
//!   on-chain randomness and no paid loot boxes (§10 compliance).
//! * `mint_with_voucher` uses contract-funded storage (the contract pays the
//!   storage stake); the owner funds it via `storage_top_up` and reclaims excess
//!   via `owner_withdraw`. This enables fully gasless (fn-call key / meta-tx) claims.
//! * NEP-245 approvals and `mt_transfer_call` are intentionally out of scope for
//!   the POC (not needed by the game loop); easy to add later.

use near_sdk::borsh::BorshSerialize;
use near_sdk::collections::LookupMap;
use near_sdk::json_types::{Base64VecU8, U128};
use near_sdk::serde_json::json;
use near_sdk::{
    assert_one_yocto, env, near, require, AccountId, BorshStorageKey, NearToken, PanicOnDefault,
    Promise,
};

/// NEP-245 token ids are strings. Ours are the decimal rendering of a packed u64
/// (doc §12): `[ type:4 | game:12 | category:16 | item_id:32 ]`.
pub type TokenId = String;

const NANOSECONDS_PER_DAY: u64 = 86_400_000_000_000;
const EVENT_STANDARD: &str = "nep245";
const EVENT_VERSION: &str = "1.0.0";

#[derive(BorshSerialize, BorshStorageKey)]
#[borsh(crate = "near_sdk::borsh")]
enum StorageKey {
    Balances,
    Supply,
    MaxSupply,
    UsedNonces,
    ClaimedMissions,
    DailyMinted,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Contract {
    /// Admin (intended to be a Sputnik DAO in production).
    owner_id: AccountId,
    /// Raw ed25519 public key (32 bytes) of the backend voucher signer (KMS in production).
    signer_pk: [u8; 32],
    /// Domain binding — e.g. "near:testnet". Prevents cross-chain voucher replay.
    chain_id: String,
    /// Metadata base; token metadata resolves at `{base_uri}{token_id}.json`.
    base_uri: String,
    /// Emergency pause — guards the mint path.
    paused: bool,
    /// Max KZR-asset units mintable per player per UTC day (anti-abuse).
    daily_mint_cap: u128,

    /// (owner, token_id) -> balance.
    balances: LookupMap<(AccountId, TokenId), u128>,
    /// token_id -> circulating supply.
    supply: LookupMap<TokenId, u128>,
    /// token_id -> max supply (must be registered before it can be minted).
    max_supply: LookupMap<TokenId, u128>,
    /// Redeemed voucher nonces (replay protection).
    used_nonces: LookupMap<u64, bool>,
    /// Redeemed mission hashes (per-mission dedup).
    claimed_missions: LookupMap<[u8; 32], bool>,
    /// (player, day_index) -> units minted that day.
    daily_minted: LookupMap<(AccountId, u64), u128>,
}

/// The mint authorization. Signed by the backend key over its **Borsh** bytes.
/// Field order here defines the canonical signing layout — the backend signer
/// (`sign_voucher.mjs`) must serialize identically.
#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct MintVoucher {
    /// Domain: the contract this voucher is valid for.
    pub contract_id: AccountId,
    /// Domain: the chain this voucher is valid for (e.g. "near:testnet").
    pub chain_id: String,
    /// Who receives the minted assets.
    pub receiver_id: AccountId,
    /// Packed token ids (decimal strings) — the mission's loot table.
    pub token_ids: Vec<TokenId>,
    /// Amounts to mint, parallel to `token_ids`.
    pub amounts: Vec<U128>,
    /// Unique per voucher — replay protection.
    pub nonce: u64,
    /// Expiry, nanoseconds since epoch (compared against block timestamp).
    pub expires_at_ns: u64,
    /// Mission identifier hash — a mission's loot can be claimed only once.
    pub mission_hash: [u8; 32],
}

/// Decoded token-id fields (view helper).
#[near(serializers = [json])]
pub struct TokenIdParts {
    pub kind: u8,
    pub game: u16,
    pub category: u16,
    pub item_id: u32,
}

#[near]
impl Contract {
    #[init]
    pub fn new(
        owner_id: AccountId,
        signer_public_key: Base64VecU8,
        chain_id: String,
        base_uri: String,
        daily_mint_cap: U128,
    ) -> Self {
        Self {
            owner_id,
            signer_pk: Self::to_ed25519_key(signer_public_key),
            chain_id,
            base_uri,
            paused: false,
            daily_mint_cap: daily_mint_cap.into(),
            balances: LookupMap::new(StorageKey::Balances),
            supply: LookupMap::new(StorageKey::Supply),
            max_supply: LookupMap::new(StorageKey::MaxSupply),
            used_nonces: LookupMap::new(StorageKey::UsedNonces),
            claimed_missions: LookupMap::new(StorageKey::ClaimedMissions),
            daily_minted: LookupMap::new(StorageKey::DailyMinted),
        }
    }


    /// `[ type:4 | game:12 | category:16 | item_id:32 ] -> u64 -> decimal string`.
    pub fn build_token_id(&self, kind: u8, game: u16, category: u16, item_id: u32) -> TokenId {
        Self::pack_token_id(kind, game, category, item_id)
    }

    /// Inverse of [`build_token_id`].
    pub fn decode_token_id(&self, token_id: TokenId) -> TokenIdParts {
        let packed: u64 = token_id.parse().expect("Invalid token id");
        TokenIdParts {
            kind: ((packed >> 60) & 0xF) as u8,
            game: ((packed >> 48) & 0xFFF) as u16,
            category: ((packed >> 32) & 0xFFFF) as u16,
            item_id: (packed & 0xFFFF_FFFF) as u32,
        }
    }

    fn pack_token_id(kind: u8, game: u16, category: u16, item_id: u32) -> TokenId {
        require!(kind < 16, "type exceeds 4 bits");
        require!(game < 4096, "game exceeds 12 bits");
        let packed: u64 = ((kind as u64) << 60)
            | ((game as u64) << 48)
            | ((category as u64) << 32)
            | (item_id as u64);
        packed.to_string()
    }


    pub fn mint_with_voucher(&mut self, voucher: MintVoucher, signature: Base64VecU8) {
        require!(!self.paused, "Paused");
        require!(
            voucher.contract_id == env::current_account_id(),
            "Wrong contract"
        );
        require!(voucher.chain_id == self.chain_id, "Wrong chain");
        require!(
            env::block_timestamp() < voucher.expires_at_ns,
            "Voucher expired"
        );

        let msg = near_sdk::borsh::to_vec(&voucher).expect("borsh");
        let sig: [u8; 64] = signature
            .0
            .as_slice()
            .try_into()
            .unwrap_or_else(|_| env::panic_str("Signature must be 64 bytes"));
        require!(
            env::ed25519_verify(&sig, &msg, &self.signer_pk),
            "Bad signature"
        );

        require!(!self.used_nonces.contains_key(&voucher.nonce), "Nonce used");
        require!(
            !self.claimed_missions.contains_key(&voucher.mission_hash),
            "Mission already claimed"
        );

        require!(
            !voucher.token_ids.is_empty() && voucher.token_ids.len() == voucher.amounts.len(),
            "token_ids/amounts length mismatch"
        );
        let amounts: Vec<u128> = voucher.amounts.iter().map(|a| a.0).collect();
        let mut total: u128 = 0;
        for amt in &amounts {
            require!(*amt > 0, "Zero amount");
            total = total
                .checked_add(*amt)
                .unwrap_or_else(|| env::panic_str("Overflow"));
        }
        let day = env::block_timestamp() / NANOSECONDS_PER_DAY;
        let day_key = (voucher.receiver_id.clone(), day);
        let today = self.daily_minted.get(&day_key).unwrap_or(0);
        let new_today = today
            .checked_add(total)
            .unwrap_or_else(|| env::panic_str("Overflow"));
        require!(new_today <= self.daily_mint_cap, "Daily cap exceeded");

        self.used_nonces.insert(&voucher.nonce, &true);
        self.claimed_missions.insert(&voucher.mission_hash, &true);
        self.daily_minted.insert(&day_key, &new_today);
        for (token_id, amt) in voucher.token_ids.iter().zip(amounts.iter()) {
            self.internal_mint(&voucher.receiver_id, token_id, *amt);
        }

        self.emit_mint(&voucher.receiver_id, &voucher.token_ids, &amounts);
    }


    /// Burn the caller's own assets as crafting inputs. Deflationary consumption;
    /// the crafted output is delivered by a follow-up voucher mint (loot stays
    /// off-chain, §10). No attached deposit required, so this is callable via a
    /// scoped function-call access key (Phase 4 gasless crafting).
    pub fn burn_for_craft(
        &mut self,
        token_ids: Vec<TokenId>,
        amounts: Vec<U128>,
        memo: Option<String>,
    ) {
        require!(
            !token_ids.is_empty() && token_ids.len() == amounts.len(),
            "token_ids/amounts length mismatch"
        );
        let owner = env::predecessor_account_id();
        let amounts: Vec<u128> = amounts.into_iter().map(|a| a.into()).collect();
        for (token_id, amount) in token_ids.iter().zip(amounts.iter()) {
            require!(*amount > 0, "Zero amount");
            self.internal_burn(&owner, token_id, *amount);
        }
        self.emit_burn(&owner, &token_ids, &amounts, memo);
    }


    #[payable]
    pub fn mt_transfer(
        &mut self,
        receiver_id: AccountId,
        token_id: TokenId,
        amount: U128,
        approval: Option<(AccountId, u64)>,
        memo: Option<String>,
    ) {
        assert_one_yocto();
        require!(approval.is_none(), "Approvals not supported");
        let sender = env::predecessor_account_id();
        let amt: u128 = amount.into();
        self.internal_transfer(&sender, &receiver_id, &token_id, amt);
        self.emit_transfer(&sender, &receiver_id, &[token_id], &[amt], memo);
    }

    #[payable]
    pub fn mt_batch_transfer(
        &mut self,
        receiver_id: AccountId,
        token_ids: Vec<TokenId>,
        amounts: Vec<U128>,
        approvals: Option<Vec<Option<(AccountId, u64)>>>,
        memo: Option<String>,
    ) {
        assert_one_yocto();
        require!(approvals.is_none(), "Approvals not supported");
        require!(
            !token_ids.is_empty() && token_ids.len() == amounts.len(),
            "token_ids/amounts length mismatch"
        );
        let sender = env::predecessor_account_id();
        let amts: Vec<u128> = amounts.into_iter().map(|a| a.into()).collect();
        for (token_id, amount) in token_ids.iter().zip(amts.iter()) {
            self.internal_transfer(&sender, &receiver_id, token_id, *amount);
        }
        self.emit_transfer(&sender, &receiver_id, &token_ids, &amts, memo);
    }


    pub fn mt_balance_of(&self, account_id: AccountId, token_id: TokenId) -> U128 {
        U128(self.balance_of(&account_id, &token_id))
    }

    pub fn mt_batch_balance_of(
        &self,
        account_id: AccountId,
        token_ids: Vec<TokenId>,
    ) -> Vec<U128> {
        token_ids
            .iter()
            .map(|t| U128(self.balance_of(&account_id, t)))
            .collect()
    }

    pub fn mt_supply(&self, token_id: TokenId) -> Option<U128> {
        self.supply.get(&token_id).map(U128)
    }

    pub fn mt_batch_supply(&self, token_ids: Vec<TokenId>) -> Vec<Option<U128>> {
        token_ids
            .iter()
            .map(|t| self.supply.get(t).map(U128))
            .collect()
    }

    /// Off-chain metadata location for a token id (`{base_uri}{token_id}.json`).
    pub fn token_reference(&self, token_id: TokenId) -> String {
        format!("{}{}.json", self.base_uri, token_id)
    }


    /// Register a token id with its immutable max supply before it can be minted.
    #[payable]
    pub fn register_token(&mut self, token_id: TokenId, max_supply: U128) {
        assert_one_yocto();
        self.assert_owner();
        require!(
            self.max_supply.get(&token_id).is_none(),
            "Already registered"
        );
        let _: u64 = token_id.parse().expect("Invalid token id");
        let max: u128 = max_supply.into();
        self.max_supply.insert(&token_id, &max);
        self.supply.insert(&token_id, &0);
        Self::emit_admin("token_registered", json!({ "token_id": token_id, "max_supply": U128(max) }));
    }

    #[payable]
    pub fn set_signer_public_key(&mut self, signer_public_key: Base64VecU8) {
        assert_one_yocto();
        self.assert_owner();
        Self::emit_admin("signer_key_changed", json!({ "signer_public_key": signer_public_key }));
        self.signer_pk = Self::to_ed25519_key(signer_public_key);
    }

    #[payable]
    pub fn set_base_uri(&mut self, base_uri: String) {
        assert_one_yocto();
        self.assert_owner();
        Self::emit_admin("base_uri_changed", json!({ "base_uri": base_uri }));
        self.base_uri = base_uri;
    }

    #[payable]
    pub fn set_daily_mint_cap(&mut self, daily_mint_cap: U128) {
        assert_one_yocto();
        self.assert_owner();
        Self::emit_admin("daily_cap_changed", json!({ "daily_mint_cap": daily_mint_cap }));
        self.daily_mint_cap = daily_mint_cap.into();
    }

    #[payable]
    pub fn pause(&mut self) {
        assert_one_yocto();
        self.assert_owner();
        self.paused = true;
        Self::emit_admin("paused", json!({}));
    }

    #[payable]
    pub fn unpause(&mut self) {
        assert_one_yocto();
        self.assert_owner();
        self.paused = false;
        Self::emit_admin("unpaused", json!({}));
    }

    #[payable]
    pub fn set_owner(&mut self, new_owner: AccountId) {
        assert_one_yocto();
        self.assert_owner();
        let old_owner = self.owner_id.clone();
        Self::emit_admin("owner_changed", json!({ "old_owner": old_owner, "new_owner": new_owner }));
        self.owner_id = new_owner;
    }


    pub fn get_owner(&self) -> AccountId {
        self.owner_id.clone()
    }

    pub fn get_signer_public_key(&self) -> Base64VecU8 {
        Base64VecU8(self.signer_pk.to_vec())
    }

    pub fn get_chain_id(&self) -> String {
        self.chain_id.clone()
    }

    pub fn get_base_uri(&self) -> String {
        self.base_uri.clone()
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn get_daily_mint_cap(&self) -> U128 {
        U128(self.daily_mint_cap)
    }

    pub fn max_supply_of(&self, token_id: TokenId) -> Option<U128> {
        self.max_supply.get(&token_id).map(U128)
    }

    pub fn is_nonce_used(&self, nonce: u64) -> bool {
        self.used_nonces.contains_key(&nonce)
    }

    pub fn is_mission_claimed(&self, mission_hash: [u8; 32]) -> bool {
        self.claimed_missions.contains_key(&mission_hash)
    }

    pub fn daily_minted_of(&self, account_id: AccountId, day_index: u64) -> U128 {
        U128(self.daily_minted.get(&(account_id, day_index)).unwrap_or(0))
    }


    fn assert_owner(&self) {
        require!(
            env::predecessor_account_id() == self.owner_id,
            "Only owner"
        );
    }

    fn to_ed25519_key(bytes: Base64VecU8) -> [u8; 32] {
        let v = bytes.0;
        require!(v.len() == 32, "ed25519 public key must be 32 bytes");
        let mut key = [0u8; 32];
        key.copy_from_slice(&v);
        key
    }

    fn balance_of(&self, account_id: &AccountId, token_id: &TokenId) -> u128 {
        self.balances
            .get(&(account_id.clone(), token_id.clone()))
            .unwrap_or(0)
    }

    fn set_balance(&mut self, account_id: &AccountId, token_id: &TokenId, amount: u128) {
        let key = (account_id.clone(), token_id.clone());
        if amount == 0 {
            self.balances.remove(&key);
        } else {
            self.balances.insert(&key, &amount);
        }
    }

    fn internal_mint(&mut self, receiver: &AccountId, token_id: &TokenId, amount: u128) {
        let max = self
            .max_supply
            .get(token_id)
            .unwrap_or_else(|| env::panic_str("Token not registered"));
        let supply = self.supply.get(token_id).unwrap_or(0);
        let new_supply = supply
            .checked_add(amount)
            .unwrap_or_else(|| env::panic_str("Overflow"));
        require!(new_supply <= max, "Exceeds max supply");
        self.supply.insert(token_id, &new_supply);
        let bal = self.balance_of(receiver, token_id);
        self.set_balance(
            receiver,
            token_id,
            bal.checked_add(amount)
                .unwrap_or_else(|| env::panic_str("Overflow")),
        );
    }

    fn internal_burn(&mut self, owner: &AccountId, token_id: &TokenId, amount: u128) {
        let bal = self.balance_of(owner, token_id);
        require!(bal >= amount, "Insufficient balance");
        self.set_balance(owner, token_id, bal - amount);
        let supply = self.supply.get(token_id).unwrap_or(0);
        self.supply.insert(token_id, &supply.saturating_sub(amount));
    }

    fn internal_transfer(
        &mut self,
        sender: &AccountId,
        receiver: &AccountId,
        token_id: &TokenId,
        amount: u128,
    ) {
        require!(amount > 0, "Zero amount");
        require!(sender != receiver, "Sender and receiver are the same");
        let sb = self.balance_of(sender, token_id);
        require!(sb >= amount, "Insufficient balance");
        self.set_balance(sender, token_id, sb - amount);
        let rb = self.balance_of(receiver, token_id);
        self.set_balance(receiver, token_id, rb + amount);
    }

    #[payable]
    pub fn storage_top_up(&mut self) {
        require!(
            env::attached_deposit().as_yoctonear() > 0,
            "Attach a deposit"
        );
    }

    #[payable]
    pub fn owner_withdraw(&mut self, amount: U128) -> Promise {
        assert_one_yocto();
        self.assert_owner();
        Promise::new(self.owner_id.clone()).transfer(NearToken::from_yoctonear(amount.into()))
    }


    fn emit(event: &str, data: near_sdk::serde_json::Value) {
        let payload = json!({
            "standard": EVENT_STANDARD,
            "version": EVENT_VERSION,
            "event": event,
            "data": [data],
        });
        env::log_str(&format!("EVENT_JSON:{}", payload));
    }

    fn emit_admin(event: &str, mut data: near_sdk::serde_json::Value) {
        if let Some(obj) = data.as_object_mut() {
            obj.insert("by".to_string(), json!(env::predecessor_account_id()));
        }
        let payload = json!({
            "standard": "kzr_admin",
            "version": "1.0.0",
            "event": event,
            "data": [data],
        });
        env::log_str(&format!("EVENT_JSON:{}", payload));
    }

    fn emit_mint(&self, owner_id: &AccountId, token_ids: &[TokenId], amounts: &[u128]) {
        Self::emit(
            "mt_mint",
            json!({
                "owner_id": owner_id,
                "token_ids": token_ids,
                "amounts": amounts.iter().map(|a| U128(*a)).collect::<Vec<_>>(),
            }),
        );
    }

    fn emit_burn(
        &self,
        owner_id: &AccountId,
        token_ids: &[TokenId],
        amounts: &[u128],
        memo: Option<String>,
    ) {
        Self::emit(
            "mt_burn",
            json!({
                "owner_id": owner_id,
                "token_ids": token_ids,
                "amounts": amounts.iter().map(|a| U128(*a)).collect::<Vec<_>>(),
                "memo": memo,
            }),
        );
    }

    fn emit_transfer(
        &self,
        old_owner_id: &AccountId,
        new_owner_id: &AccountId,
        token_ids: &[TokenId],
        amounts: &[u128],
        memo: Option<String>,
    ) {
        Self::emit(
            "mt_transfer",
            json!({
                "old_owner_id": old_owner_id,
                "new_owner_id": new_owner_id,
                "token_ids": token_ids,
                "amounts": amounts.iter().map(|a| U128(*a)).collect::<Vec<_>>(),
                "memo": memo,
            }),
        );
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::{testing_env, NearToken};

    const CHAIN: &str = "near:testnet";
    const SEED: [u8; 32] = [7u8; 32];

    fn signing_key() -> SigningKey {
        SigningKey::from_bytes(&SEED)
    }

    fn signer_public_key() -> Base64VecU8 {
        Base64VecU8(signing_key().verifying_key().to_bytes().to_vec())
    }

    fn set_context(predecessor: AccountId, deposit_yocto: u128, ts_ns: u64) {
        let mut b = VMContextBuilder::new();
        b.current_account_id(contract_id())
            .predecessor_account_id(predecessor)
            .attached_deposit(NearToken::from_yoctonear(deposit_yocto))
            .block_timestamp(ts_ns);
        testing_env!(b.build());
    }

    fn contract_id() -> AccountId {
        accounts(0)
    }

    fn new_contract() -> Contract {
        set_context(accounts(0), 0, 1_000);
        Contract::new(
            accounts(0),                    // owner
            signer_public_key(),
            CHAIN.to_string(),
            "ipfs://base/".to_string(),
            U128(1_000),                    // daily cap
        )
    }

    #[test]
    fn admin_action_emits_kzr_admin_event() {
        let mut c = new_contract();
        set_context(accounts(0), 1, 1_000);
        c.pause();
        let ev = near_sdk::test_utils::get_logs()
            .into_iter()
            .find(|l| l.contains("EVENT_JSON"))
            .expect("admin event not emitted");
        assert!(ev.contains("\"standard\":\"kzr_admin\""));
        assert!(ev.contains("\"event\":\"paused\""));
        assert!(ev.contains("\"by\":"));
    }

    fn register(c: &mut Contract, token_id: &str, max: u128) {
        set_context(accounts(0), 1, 1_000); // owner, 1 yocto
        c.register_token(token_id.to_string(), U128(max));
    }

    fn make_voucher(receiver: AccountId, token_id: &str, amount: u128, nonce: u64) -> MintVoucher {
        MintVoucher {
            contract_id: contract_id(),
            chain_id: CHAIN.to_string(),
            receiver_id: receiver,
            token_ids: vec![token_id.to_string()],
            amounts: vec![U128(amount)],
            nonce,
            expires_at_ns: 10_000_000,
            mission_hash: {
                let mut h = [0u8; 32];
                h[0] = nonce as u8; // unique-ish per nonce for these tests
                h
            },
        }
    }

    fn sign(voucher: &MintVoucher) -> Base64VecU8 {
        let msg = near_sdk::borsh::to_vec(voucher).unwrap();
        let sig = signing_key().sign(&msg);
        Base64VecU8(sig.to_bytes().to_vec())
    }

    fn lcg(seed: &mut u64) -> u64 {
        *seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        *seed
    }

    #[test]
    fn randomized_supply_tracks_and_bounds() {
        let mut c = Contract::new(
            accounts(0),
            signer_public_key(),
            CHAIN.to_string(),
            "ipfs://base/".to_string(),
            U128(u128::MAX),
        );
        let mut seed: u64 = 0x1234_5678_9ABC_DEF0;
        for i in 0u64..64 {
            let tid = c.build_token_id(0, 1, 1, (i as u32) + 1);
            let max = 100u128;
            set_context(accounts(0), 1, 1_000);
            c.register_token(tid.clone(), U128(max));
            let mut total = 0u128;
            let n = (lcg(&mut seed) % 5) + 1;
            for j in 0..n {
                let amt = ((lcg(&mut seed) % 20) + 1) as u128;
                if total + amt > max {
                    break;
                }
                let nonce = i * 1000 + j + 1;
                let mut mh = [0u8; 32];
                mh[0..8].copy_from_slice(&nonce.to_le_bytes());
                let v = MintVoucher {
                    contract_id: contract_id(),
                    chain_id: CHAIN.to_string(),
                    receiver_id: accounts(1),
                    token_ids: vec![tid.clone()],
                    amounts: vec![U128(amt)],
                    nonce,
                    expires_at_ns: 10_000_000,
                    mission_hash: mh,
                };
                let sig = sign(&v);
                set_context(accounts(1), NearToken::from_near(1).as_yoctonear(), 2_000);
                c.mint_with_voucher(v, sig);
                total += amt;
                assert_eq!(c.mt_supply(tid.clone()).unwrap().0, total);
                assert!(total <= max);
                assert!(c.is_nonce_used(nonce));
            }
        }
    }

    #[test]
    fn token_id_packing_round_trips() {
        let c = new_contract();
        let id = c.build_token_id(2, 1, 3, 17);
        let parts = c.decode_token_id(id.clone());
        assert_eq!(parts.kind, 2);
        assert_eq!(parts.game, 1);
        assert_eq!(parts.category, 3);
        assert_eq!(parts.item_id, 17);
        let id2 = c.build_token_id(15, 4095, 65535, u32::MAX);
        let p2 = c.decode_token_id(id2);
        assert_eq!((p2.kind, p2.game, p2.category, p2.item_id), (15, 4095, 65535, u32::MAX));
    }

    #[test]
    fn valid_voucher_mints_and_autoregisters_supply() {
        let mut c = new_contract();
        let tid = c.build_token_id(2, 1, 3, 17);
        register(&mut c, &tid, 100);

        let v = make_voucher(accounts(1), &tid, 5, 1);
        let sig = sign(&v);
        set_context(accounts(1), NearToken::from_near(1).as_yoctonear(), 2_000);
        c.mint_with_voucher(v, sig);

        assert_eq!(c.mt_balance_of(accounts(1), tid.clone()).0, 5);
        assert_eq!(c.mt_supply(tid).unwrap().0, 5);
        assert!(c.is_nonce_used(1));
    }

    #[test]
    #[should_panic(expected = "Bad signature")]
    fn tampered_voucher_rejected() {
        let mut c = new_contract();
        let tid = c.build_token_id(0, 1, 1, 1);
        register(&mut c, &tid, 100);
        let v = make_voucher(accounts(1), &tid, 5, 1);
        let sig = sign(&v);
        let mut tampered = v.clone();
        tampered.amounts = vec![U128(500)];
        set_context(accounts(1), NearToken::from_near(1).as_yoctonear(), 2_000);
        c.mint_with_voucher(tampered, sig);
    }

    #[test]
    #[should_panic(expected = "Nonce used")]
    fn replayed_nonce_rejected() {
        let mut c = new_contract();
        let tid = c.build_token_id(0, 1, 1, 1);
        register(&mut c, &tid, 100);
        let v = make_voucher(accounts(1), &tid, 5, 1);
        let sig = sign(&v);
        set_context(accounts(1), NearToken::from_near(1).as_yoctonear(), 2_000);
        c.mint_with_voucher(v.clone(), sig.clone());
        set_context(accounts(1), NearToken::from_near(1).as_yoctonear(), 3_000);
        c.mint_with_voucher(v, sig);
    }

    #[test]
    #[should_panic(expected = "Mission already claimed")]
    fn duplicate_mission_rejected() {
        let mut c = new_contract();
        let tid = c.build_token_id(0, 1, 1, 1);
        register(&mut c, &tid, 100);

        let v1 = make_voucher(accounts(1), &tid, 5, 1);
        let sig1 = sign(&v1);
        set_context(accounts(1), NearToken::from_near(1).as_yoctonear(), 2_000);
        c.mint_with_voucher(v1, sig1);

        let mut v2 = make_voucher(accounts(1), &tid, 5, 2);
        v2.mission_hash[0] = 1;
        let sig2 = sign(&v2);
        set_context(accounts(1), NearToken::from_near(1).as_yoctonear(), 3_000);
        c.mint_with_voucher(v2, sig2);
    }

    #[test]
    #[should_panic(expected = "Voucher expired")]
    fn expired_voucher_rejected() {
        let mut c = new_contract();
        let tid = c.build_token_id(0, 1, 1, 1);
        register(&mut c, &tid, 100);
        let v = make_voucher(accounts(1), &tid, 5, 1);
        let sig = sign(&v);
        set_context(accounts(1), NearToken::from_near(1).as_yoctonear(), 20_000_000);
        c.mint_with_voucher(v, sig);
    }

    #[test]
    #[should_panic(expected = "Exceeds max supply")]
    fn exceeds_max_supply_rejected() {
        let mut c = new_contract();
        let tid = c.build_token_id(0, 1, 1, 1);
        register(&mut c, &tid, 3);
        let v = make_voucher(accounts(1), &tid, 5, 1); // 5 > max 3
        let sig = sign(&v);
        set_context(accounts(1), NearToken::from_near(1).as_yoctonear(), 2_000);
        c.mint_with_voucher(v, sig);
    }

    #[test]
    #[should_panic(expected = "Daily cap exceeded")]
    fn daily_cap_enforced() {
        let mut c = new_contract();
        let tid = c.build_token_id(0, 1, 1, 1);
        register(&mut c, &tid, 100_000);
        let v = make_voucher(accounts(1), &tid, 1_001, 1);
        let sig = sign(&v);
        set_context(accounts(1), NearToken::from_near(1).as_yoctonear(), 2_000);
        c.mint_with_voucher(v, sig);
    }

    #[test]
    #[should_panic(expected = "Wrong chain")]
    fn wrong_chain_rejected() {
        let mut c = new_contract();
        let tid = c.build_token_id(0, 1, 1, 1);
        register(&mut c, &tid, 100);
        let mut v = make_voucher(accounts(1), &tid, 5, 1);
        v.chain_id = "near:mainnet".to_string();
        let sig = sign(&v);
        set_context(accounts(1), NearToken::from_near(1).as_yoctonear(), 2_000);
        c.mint_with_voucher(v, sig);
    }

    #[test]
    fn burn_for_craft_reduces_balance_and_supply() {
        let mut c = new_contract();
        let tid = c.build_token_id(0, 1, 1, 1);
        register(&mut c, &tid, 100);
        let v = make_voucher(accounts(1), &tid, 10, 1);
        let sig = sign(&v);
        set_context(accounts(1), NearToken::from_near(1).as_yoctonear(), 2_000);
        c.mint_with_voucher(v, sig);

        set_context(accounts(1), 0, 3_000);
        c.burn_for_craft(vec![tid.clone()], vec![U128(4)], Some("craft".into()));
        assert_eq!(c.mt_balance_of(accounts(1), tid.clone()).0, 6);
        assert_eq!(c.mt_supply(tid).unwrap().0, 6);
    }

    #[test]
    fn transfer_moves_balance() {
        let mut c = new_contract();
        let tid = c.build_token_id(0, 1, 1, 1);
        register(&mut c, &tid, 100);
        let v = make_voucher(accounts(1), &tid, 10, 1);
        let sig = sign(&v);
        set_context(accounts(1), NearToken::from_near(1).as_yoctonear(), 2_000);
        c.mint_with_voucher(v, sig);

        set_context(accounts(1), 1, 3_000); // 1 yocto for mt_transfer
        c.mt_transfer(accounts(2), tid.clone(), U128(3), None, None);
        assert_eq!(c.mt_balance_of(accounts(1), tid.clone()).0, 7);
        assert_eq!(c.mt_balance_of(accounts(2), tid).0, 3);
    }

    #[test]
    #[should_panic(expected = "Only owner")]
    fn non_owner_cannot_register() {
        let mut c = new_contract();
        set_context(accounts(3), 1, 1_000);
        c.register_token(c.build_token_id(0, 1, 1, 1), U128(10));
    }

    #[test]
    #[should_panic(expected = "Paused")]
    fn mint_blocked_when_paused() {
        let mut c = new_contract();
        let tid = c.build_token_id(0, 1, 1, 1);
        register(&mut c, &tid, 100);
        set_context(accounts(0), 1, 1_000);
        c.pause();
        let v = make_voucher(accounts(1), &tid, 5, 1);
        let sig = sign(&v);
        set_context(accounts(1), NearToken::from_near(1).as_yoctonear(), 2_000);
        c.mint_with_voucher(v, sig);
    }

    #[test]
    fn mint_needs_no_deposit() {
        let mut c = new_contract();
        let tid = c.build_token_id(0, 1, 1, 1);
        register(&mut c, &tid, 100);
        let v = make_voucher(accounts(1), &tid, 5, 1);
        let sig = sign(&v);
        set_context(accounts(1), 0, 2_000);
        c.mint_with_voucher(v, sig);
        assert_eq!(c.mt_balance_of(accounts(1), tid).0, 5);
    }

    #[test]
    #[should_panic(expected = "Only owner")]
    fn non_owner_cannot_withdraw() {
        let mut c = new_contract();
        set_context(accounts(3), 1, 1_000);
        let _ = c.owner_withdraw(U128(1));
    }

    #[test]
    #[should_panic(expected = "Attach a deposit")]
    fn storage_top_up_requires_deposit() {
        let mut c = new_contract();
        set_context(accounts(0), 0, 1_000);
        c.storage_top_up();
    }
}
