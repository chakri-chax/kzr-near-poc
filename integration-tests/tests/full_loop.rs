use anyhow::Result;
use ed25519_dalek::{Signer, SigningKey};
use near_sdk::borsh::BorshSerialize;
use near_sdk::json_types::{Base64VecU8, U128};
use near_workspaces::types::{Gas, NearToken};
use serde_json::json;

const ONE: u128 = 1_000_000_000_000_000_000;
const SEED: [u8; 32] = [7u8; 32];
const CHAIN: &str = "near:sandbox";
const FAR_FUTURE_NS: u64 = 4_102_444_800_000_000_000;

#[derive(BorshSerialize)]
#[borsh(crate = "near_sdk::borsh")]
struct MintVoucher {
    contract_id: near_sdk::AccountId,
    chain_id: String,
    receiver_id: near_sdk::AccountId,
    token_ids: Vec<String>,
    amounts: Vec<U128>,
    nonce: u64,
    expires_at_ns: u64,
    mission_hash: [u8; 32],
}

fn wasm(name: &str) -> Vec<u8> {
    let path = format!(
        "{}/../target/near/{}/{}.wasm",
        env!("CARGO_MANIFEST_DIR"),
        name,
        name
    );
    std::fs::read(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

fn signing_key() -> SigningKey {
    SigningKey::from_bytes(&SEED)
}

fn signer_public_key_b64() -> Base64VecU8 {
    Base64VecU8(signing_key().verifying_key().to_bytes().to_vec())
}

fn sign_voucher(v: &MintVoucher) -> Base64VecU8 {
    let msg = near_sdk::borsh::to_vec(v).unwrap();
    Base64VecU8(signing_key().sign(&msg).to_bytes().to_vec())
}

#[tokio::test]
async fn full_loop_and_rollback() -> Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    let kzr = worker.dev_deploy(&wasm("kzr_token")).await?;
    let coin = worker.dev_deploy(&wasm("ingame_coin")).await?;
    let assets = worker.dev_deploy(&wasm("game_assets")).await?;
    let convert = worker.dev_deploy(&wasm("ingame_conversion")).await?;

    let player = root
        .create_subaccount("player")
        .initial_balance(NearToken::from_near(10))
        .transact()
        .await?
        .into_result()?;

    let snxc = |n: u128| U128(n * ONE);
    let one_yocto = NearToken::from_yoctonear(1);

    root.call(kzr.id(), "new")
        .args_json(json!({"owner_id": root.id(), "treasury_id": root.id(), "initial_supply": snxc(1_000)}))
        .transact()
        .await?
        .into_result()?;
    root.call(coin.id(), "new")
        .args_json(json!({"owner_id": root.id()}))
        .transact()
        .await?
        .into_result()?;
    root.call(assets.id(), "new")
        .args_json(json!({
            "owner_id": root.id(),
            "signer_public_key": signer_public_key_b64(),
            "chain_id": CHAIN,
            "base_uri": "ipfs://base/",
            "daily_mint_cap": snxc(1_000_000)
        }))
        .transact()
        .await?
        .into_result()?;
    root.call(convert.id(), "new")
        .args_json(json!({
            "owner_id": root.id(),
            "kzr_token": kzr.id(),
            "coin_token": coin.id(),
            "rate_num": U128(1),
            "rate_den": U128(100),
            "daily_cap": snxc(100),
            "lifetime_cap": snxc(1_000)
        }))
        .transact()
        .await?
        .into_result()?;

    root.call(kzr.id(), "add_minter")
        .args_json(json!({"account_id": convert.id()}))
        .deposit(one_yocto)
        .transact()
        .await?
        .into_result()?;
    root.call(coin.id(), "add_minter")
        .args_json(json!({"account_id": root.id()}))
        .deposit(one_yocto)
        .transact()
        .await?
        .into_result()?;
    root.call(coin.id(), "register_sink")
        .args_json(json!({"account_id": convert.id()}))
        .deposit(one_yocto)
        .transact()
        .await?
        .into_result()?;
    coin.call("storage_deposit")
        .args_json(json!({"account_id": convert.id()}))
        .deposit(NearToken::from_millinear(100))
        .transact()
        .await?
        .into_result()?;

    root.call(assets.id(), "storage_top_up")
        .deposit(NearToken::from_near(3))
        .transact()
        .await?
        .into_result()?;
    root.call(convert.id(), "storage_top_up")
        .deposit(NearToken::from_near(1))
        .transact()
        .await?
        .into_result()?;

    let cell: String = assets
        .view("build_token_id")
        .args_json(json!({"kind": 0, "game": 1, "category": 1, "item_id": 1}))
        .await?
        .json()?;
    root.call(assets.id(), "register_token")
        .args_json(json!({"token_id": cell, "max_supply": U128(10_000_000)}))
        .deposit(one_yocto)
        .transact()
        .await?
        .into_result()?;

    // 1. voucher mint
    let mission_hash: Vec<u8> = vec![1u8; 32];
    let voucher = MintVoucher {
        contract_id: assets.id().as_str().parse().unwrap(),
        chain_id: CHAIN.to_string(),
        receiver_id: player.id().as_str().parse().unwrap(),
        token_ids: vec![cell.clone()],
        amounts: vec![U128(30)],
        nonce: 1,
        expires_at_ns: FAR_FUTURE_NS,
        mission_hash: [1u8; 32],
    };
    let sig = sign_voucher(&voucher);
    player
        .call(assets.id(), "mint_with_voucher")
        .args_json(json!({
            "voucher": {
                "contract_id": assets.id(),
                "chain_id": CHAIN,
                "receiver_id": player.id(),
                "token_ids": [cell],
                "amounts": [U128(30)],
                "nonce": 1,
                "expires_at_ns": FAR_FUTURE_NS,
                "mission_hash": mission_hash,
            },
            "signature": sig
        }))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?
        .into_result()?;
    let bal: U128 = assets
        .view("mt_balance_of")
        .args_json(json!({"account_id": player.id(), "token_id": cell}))
        .await?
        .json()?;
    assert_eq!(bal.0, 30, "voucher mint should grant 30 cells");

    // 2. replay guard (on-chain)
    let replay = player
        .call(assets.id(), "mint_with_voucher")
        .args_json(json!({
            "voucher": {
                "contract_id": assets.id(), "chain_id": CHAIN, "receiver_id": player.id(),
                "token_ids": [cell], "amounts": [U128(30)], "nonce": 1,
                "expires_at_ns": FAR_FUTURE_NS, "mission_hash": mission_hash,
            },
            "signature": sig
        }))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert!(replay.is_failure(), "replayed nonce must be rejected");

    // 3. burn-for-craft
    player
        .call(assets.id(), "burn_for_craft")
        .args_json(json!({"token_ids": [cell], "amounts": [U128(10)], "memo": null}))
        .gas(Gas::from_tgas(30))
        .transact()
        .await?
        .into_result()?;
    let bal: U128 = assets
        .view("mt_balance_of")
        .args_json(json!({"account_id": player.id(), "token_id": cell}))
        .await?
        .json()?;
    assert_eq!(bal.0, 20, "burn should leave 20 cells");

    // 4. conversion success
    root.call(coin.id(), "mint")
        .args_json(json!({"account_id": player.id(), "amount": snxc(500)}))
        .transact()
        .await?
        .into_result()?;
    player
        .call(coin.id(), "ft_transfer_call")
        .args_json(json!({"receiver_id": convert.id(), "amount": snxc(500), "memo": null, "msg": ""}))
        .deposit(one_yocto)
        .gas(Gas::from_tgas(120))
        .transact()
        .await?
        .into_result()?;

    let kzr_bal: U128 = kzr
        .view("ft_balance_of")
        .args_json(json!({"account_id": player.id()}))
        .await?
        .json()?;
    assert_eq!(kzr_bal.0, 5 * ONE, "500 NXC should mint 5 KZR");
    let player_nxc: U128 = coin
        .view("ft_balance_of")
        .args_json(json!({"account_id": player.id()}))
        .await?
        .json()?;
    assert_eq!(player_nxc.0, 0, "all NXC consumed on success");
    let convert_nxc: U128 = coin
        .view("ft_balance_of")
        .args_json(json!({"account_id": convert.id()}))
        .await?
        .json()?;
    assert_eq!(convert_nxc.0, 500 * ONE, "convert holds the NXC");
    let life: U128 = convert
        .view("lifetime_converted_of")
        .args_json(json!({"account_id": player.id()}))
        .await?
        .json()?;
    assert_eq!(life.0, 5 * ONE, "lifetime counter reflects 5 KZR");

    // 5. conversion rollback: pause KZR so the mint fails
    root.call(kzr.id(), "pause")
        .deposit(one_yocto)
        .transact()
        .await?
        .into_result()?;
    root.call(coin.id(), "mint")
        .args_json(json!({"account_id": player.id(), "amount": snxc(300)}))
        .transact()
        .await?
        .into_result()?;
    player
        .call(coin.id(), "ft_transfer_call")
        .args_json(json!({"receiver_id": convert.id(), "amount": snxc(300), "memo": null, "msg": ""}))
        .deposit(one_yocto)
        .gas(Gas::from_tgas(120))
        .transact()
        .await?
        .into_result()?;

    let kzr_bal: U128 = kzr
        .view("ft_balance_of")
        .args_json(json!({"account_id": player.id()}))
        .await?
        .json()?;
    assert_eq!(kzr_bal.0, 5 * ONE, "no new KZR minted while paused");
    let player_nxc: U128 = coin
        .view("ft_balance_of")
        .args_json(json!({"account_id": player.id()}))
        .await?
        .json()?;
    assert_eq!(player_nxc.0, 300 * ONE, "NXC refunded on mint failure");
    let convert_nxc: U128 = coin
        .view("ft_balance_of")
        .args_json(json!({"account_id": convert.id()}))
        .await?
        .json()?;
    assert_eq!(convert_nxc.0, 500 * ONE, "convert keeps only the successful conversion");
    let life: U128 = convert
        .view("lifetime_converted_of")
        .args_json(json!({"account_id": player.id()}))
        .await?
        .json()?;
    assert_eq!(life.0, 5 * ONE, "reservation rolled back to 5 KZR");

    Ok(())
}
