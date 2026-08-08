use near_sdk::borsh::BorshSerialize;
use near_sdk::collections::LookupMap;
use near_sdk::json_types::U128;
use near_sdk::serde_json::json;
use near_sdk::{
    assert_one_yocto, env, ext_contract, near, require, AccountId, BorshStorageKey, Gas, NearToken,
    PanicOnDefault, Promise, PromiseError, PromiseOrValue,
};

const DAY_NS: u64 = 86_400_000_000_000;
const MINT_GAS: Gas = Gas::from_tgas(15);
const CALLBACK_GAS: Gas = Gas::from_tgas(15);

#[derive(BorshSerialize, BorshStorageKey)]
#[borsh(crate = "near_sdk::borsh")]
enum StorageKey {
    DailyConverted,
    LifetimeConverted,
}

#[allow(dead_code)]
#[ext_contract(ext_kzr)]
trait ExtKzr {
    fn mint(&mut self, account_id: AccountId, amount: U128);
}

#[allow(dead_code)]
#[ext_contract(ext_coin)]
trait ExtCoin {
    fn burn(&mut self, amount: U128);
}

#[allow(dead_code)]
#[ext_contract(ext_self)]
trait ExtSelf {
    fn on_mint_complete(
        &mut self,
        account_id: AccountId,
        kzr_out: U128,
        nxc_in: U128,
        day: u64,
    ) -> U128;
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Contract {
    owner_id: AccountId,
    kzr_token: AccountId,
    coin_token: AccountId,
    rate_num: u128,
    rate_den: u128,
    daily_cap: u128,
    lifetime_cap: u128,
    paused: bool,
    daily_converted: LookupMap<(AccountId, u64), u128>,
    lifetime_converted: LookupMap<AccountId, u128>,
}

#[near]
impl Contract {
    #[init]
    pub fn new(
        owner_id: AccountId,
        kzr_token: AccountId,
        coin_token: AccountId,
        rate_num: U128,
        rate_den: U128,
        daily_cap: U128,
        lifetime_cap: U128,
    ) -> Self {
        require!(rate_num.0 > 0 && rate_den.0 > 0, "Invalid rate");
        Self {
            owner_id,
            kzr_token,
            coin_token,
            rate_num: rate_num.into(),
            rate_den: rate_den.into(),
            daily_cap: daily_cap.into(),
            lifetime_cap: lifetime_cap.into(),
            paused: false,
            daily_converted: LookupMap::new(StorageKey::DailyConverted),
            lifetime_converted: LookupMap::new(StorageKey::LifetimeConverted),
        }
    }

    pub fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        let _ = msg;
        require!(
            env::predecessor_account_id() == self.coin_token,
            "Only ingame-coin"
        );
        require!(!self.paused, "Paused");

        let nxc: u128 = amount.into();
        require!(nxc > 0, "Zero amount");
        let kzr_out = nxc
            .checked_mul(self.rate_num)
            .unwrap_or_else(|| env::panic_str("Overflow"))
            / self.rate_den;
        require!(kzr_out > 0, "Below minimum conversion");

        let day = env::block_timestamp() / DAY_NS;
        let day_key = (sender_id.clone(), day);
        let today = self.daily_converted.get(&day_key).unwrap_or(0);
        let new_today = today
            .checked_add(kzr_out)
            .unwrap_or_else(|| env::panic_str("Overflow"));
        require!(new_today <= self.daily_cap, "Daily cap exceeded");
        let life = self.lifetime_converted.get(&sender_id).unwrap_or(0);
        let new_life = life
            .checked_add(kzr_out)
            .unwrap_or_else(|| env::panic_str("Overflow"));
        require!(new_life <= self.lifetime_cap, "Lifetime cap exceeded");

        self.daily_converted.insert(&day_key, &new_today);
        self.lifetime_converted.insert(&sender_id, &new_life);

        ext_kzr::ext(self.kzr_token.clone())
            .with_static_gas(MINT_GAS)
            .mint(sender_id.clone(), U128(kzr_out))
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(CALLBACK_GAS)
                    .on_mint_complete(sender_id, U128(kzr_out), amount, day),
            )
            .into()
    }

    #[private]
    pub fn on_mint_complete(
        &mut self,
        account_id: AccountId,
        kzr_out: U128,
        nxc_in: U128,
        day: u64,
        #[callback_result] result: Result<(), PromiseError>,
    ) -> U128 {
        if result.is_ok() {
            Self::emit(
                "conversion",
                json!({"account_id": account_id, "nxc_in": nxc_in, "kzr_out": kzr_out}),
            );
            U128(0)
        } else {
            let day_key = (account_id.clone(), day);
            let today = self.daily_converted.get(&day_key).unwrap_or(0);
            self.daily_converted
                .insert(&day_key, &today.saturating_sub(kzr_out.0));
            let life = self.lifetime_converted.get(&account_id).unwrap_or(0);
            self.lifetime_converted
                .insert(&account_id, &life.saturating_sub(kzr_out.0));
            Self::emit(
                "conversion_rollback",
                json!({"account_id": account_id, "nxc_refunded": nxc_in, "kzr_out": kzr_out}),
            );
            nxc_in
        }
    }

    fn emit(event: &str, data: near_sdk::serde_json::Value) {
        let payload = json!({
            "standard": "kzr_conversion",
            "version": "1.0.0",
            "event": event,
            "data": [data],
        });
        env::log_str(&format!("EVENT_JSON:{}", payload));
    }

    #[payable]
    pub fn burn_collected(&mut self, amount: U128) -> Promise {
        assert_one_yocto();
        self.assert_owner();
        ext_coin::ext(self.coin_token.clone())
            .with_static_gas(MINT_GAS)
            .burn(amount)
    }

    #[payable]
    pub fn set_rate(&mut self, rate_num: U128, rate_den: U128) {
        assert_one_yocto();
        self.assert_owner();
        require!(rate_num.0 > 0 && rate_den.0 > 0, "Invalid rate");
        self.rate_num = rate_num.into();
        self.rate_den = rate_den.into();
    }

    #[payable]
    pub fn set_caps(&mut self, daily_cap: U128, lifetime_cap: U128) {
        assert_one_yocto();
        self.assert_owner();
        self.daily_cap = daily_cap.into();
        self.lifetime_cap = lifetime_cap.into();
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
    pub fn set_owner(&mut self, new_owner: AccountId) {
        assert_one_yocto();
        self.assert_owner();
        self.owner_id = new_owner;
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

    pub fn quote(&self, nxc_in: U128) -> U128 {
        U128(
            nxc_in
                .0
                .checked_mul(self.rate_num)
                .unwrap_or_else(|| env::panic_str("Overflow"))
                / self.rate_den,
        )
    }

    pub fn get_owner(&self) -> AccountId {
        self.owner_id.clone()
    }

    pub fn get_kzr_token(&self) -> AccountId {
        self.kzr_token.clone()
    }

    pub fn get_coin_token(&self) -> AccountId {
        self.coin_token.clone()
    }

    pub fn get_rate(&self) -> (U128, U128) {
        (U128(self.rate_num), U128(self.rate_den))
    }

    pub fn get_caps(&self) -> (U128, U128) {
        (U128(self.daily_cap), U128(self.lifetime_cap))
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn daily_converted_of(&self, account_id: AccountId, day_index: u64) -> U128 {
        U128(
            self.daily_converted
                .get(&(account_id, day_index))
                .unwrap_or(0),
        )
    }

    pub fn lifetime_converted_of(&self, account_id: AccountId) -> U128 {
        U128(self.lifetime_converted.get(&account_id).unwrap_or(0))
    }

    fn assert_owner(&self) {
        require!(
            env::predecessor_account_id() == self.owner_id,
            "Only owner"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::{testing_env, NearToken};

    const ONE: u128 = 1_000_000_000_000_000_000;

    fn ctx(predecessor: AccountId, yocto: u128, ts_ns: u64) {
        let mut b = VMContextBuilder::new();
        b.current_account_id(accounts(0))
            .predecessor_account_id(predecessor)
            .attached_deposit(NearToken::from_yoctonear(yocto))
            .block_timestamp(ts_ns);
        testing_env!(b.build());
    }

    fn new_contract() -> Contract {
        ctx(accounts(0), 0, 1_000);
        Contract::new(
            accounts(0),
            accounts(5),
            accounts(4),
            U128(1),
            U128(100),
            U128(100 * ONE),
            U128(1_000 * ONE),
        )
    }

    #[test]
    fn quote_math() {
        let c = new_contract();
        assert_eq!(c.quote(U128(100 * ONE)).0, ONE);
        assert_eq!(c.quote(U128(250 * ONE)).0, 2 * ONE + ONE / 2);
    }

    #[test]
    fn success_path_reserves_before_mint() {
        let mut c = new_contract();
        ctx(accounts(4), 0, 2_000);
        let _ = c.ft_on_transfer(accounts(1), U128(100 * ONE), String::new());
        assert_eq!(c.daily_converted_of(accounts(1), 2_000 / DAY_NS).0, ONE);
        assert_eq!(c.lifetime_converted_of(accounts(1)).0, ONE);
    }

    #[test]
    #[should_panic(expected = "Only ingame-coin")]
    fn only_coin_can_call() {
        let mut c = new_contract();
        ctx(accounts(3), 0, 2_000);
        let _ = c.ft_on_transfer(accounts(1), U128(100 * ONE), String::new());
    }

    #[test]
    #[should_panic(expected = "Paused")]
    fn paused_rejected() {
        let mut c = new_contract();
        ctx(accounts(0), 1, 1_000);
        c.pause();
        ctx(accounts(4), 0, 2_000);
        let _ = c.ft_on_transfer(accounts(1), U128(100 * ONE), String::new());
    }

    #[test]
    #[should_panic(expected = "Below minimum conversion")]
    fn dust_rejected() {
        let mut c = new_contract();
        ctx(accounts(4), 0, 2_000);
        let _ = c.ft_on_transfer(accounts(1), U128(50), String::new());
    }

    #[test]
    #[should_panic(expected = "Daily cap exceeded")]
    fn daily_cap_enforced() {
        let mut c = new_contract();
        ctx(accounts(4), 0, 2_000);
        let _ = c.ft_on_transfer(accounts(1), U128(10_100 * ONE), String::new());
    }

    #[test]
    #[should_panic(expected = "Lifetime cap exceeded")]
    fn lifetime_cap_enforced() {
        let mut c = new_contract();
        ctx(accounts(0), 1, 1_000);
        c.set_caps(U128(1_000_000 * ONE), U128(1_000 * ONE));
        ctx(accounts(4), 0, 2_000);
        let _ = c.ft_on_transfer(accounts(1), U128(100_100 * ONE), String::new());
    }

    #[test]
    fn owner_updates_rate_and_caps() {
        let mut c = new_contract();
        ctx(accounts(0), 1, 1_000);
        c.set_rate(U128(1), U128(50));
        c.set_caps(U128(200 * ONE), U128(2_000 * ONE));
        assert_eq!(c.get_rate().1 .0, 50);
        assert_eq!(c.get_caps().0 .0, 200 * ONE);
        assert_eq!(c.quote(U128(50 * ONE)).0, ONE);
    }

    #[test]
    #[should_panic(expected = "Only owner")]
    fn non_owner_cannot_set_rate() {
        let mut c = new_contract();
        ctx(accounts(3), 1, 1_000);
        c.set_rate(U128(1), U128(10));
    }
}
