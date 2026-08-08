# near-poc — KZR / Ultraverse native-NEAR workspace

Native-NEAR (Rust) proof of concept for the KZR EVM→NEAR migration, per
`KZR_EVM_to_NEAR_Migration_Architecture (2).docx`. Route B (native NEAR).

## Crates

| Crate       | Standard | Ports (EVM)         | Status |
|-------------|----------|---------------------|--------|
| `kzr-token` | NEP-141 (+148/145) | `Kruzer.sol` | ✅ built + unit-tested (9) |
| `game-assets` | NEP-245 | `KruzerAssets1155.sol` | ✅ built + unit-tested (13) |
| `ingame-conversion` | — | `InGameToKZR.sol` | ⏳ not started |

## Toolchain

```
rustc 1.93 · wasm32-unknown-unknown · cargo-near 0.22 · near-cli-rs 0.29
```

## kzr-token — Kruzer Coin (KZR)

NEP-141 fungible token, direct port of `Kruzer.sol`. See doc §4 for the mapping.

| Solidity (Kruzer.sol) | kzr-token |
|---|---|
| ERC-20 core | NEP-141 (`ft_transfer`, `ft_transfer_call`, `ft_balance_of`, …) |
| `ERC20Burnable` | `burn` — self-burn only (deflationary) |
| `ERC20Pausable` | `paused` flag guarding the **mint** path |
| `ERC20Permit` / EIP-2612 | dropped (→ `ft_transfer_call` + fn-call keys + meta-tx) |
| `AccessControl` roles | `owner_id` (admin) + `minters` set |
| `MAX_SUPPLY` 1B, 18 dec | `MAX_SUPPLY` const, checked in `mint` |
| immutable | remove account full-access keys post-deploy |

NEAR addition: `mint` auto-registers unregistered recipients from the contract's
storage budget (NEP-145), so rewards/conversion can pay first-time players in one hop.

### Build & test

```bash
# from near-poc/
cargo test -p kzr-token                       # unit tests
cd kzr-token && cargo near build non-reproducible-wasm
# artifact: near-poc/target/near/kzr_token/kzr_token.wasm
```

Use `cargo near build` (reproducible, Docker) for the mainnet/audit artifact.

### Deploy to testnet (kzr-dev.testnet)

Deploy + call the initializer in one transaction. `owner`/`treasury` here are the
dev account; in production `owner` becomes the Sputnik DAO (doc N4) and the token
account is locked by removing full-access keys (doc §3.5).

```bash
near contract deploy kzr-dev.testnet \
  use-file target/near/kzr_token/kzr_token.wasm \
  with-init-call new json-args '{
    "owner_id": "kzr-dev.testnet",
    "treasury_id": "kzr-dev.testnet",
    "initial_supply": "1000000000000000000000"
  }' \
  prepaid-gas '100.0 Tgas' attached-deposit '0 NEAR' \
  network-config testnet sign-with-legacy-keychain send
```

### Interact

```bash
# views
near contract call-function as-read-only kzr-dev.testnet ft_metadata json-args {} network-config testnet now
near contract call-function as-read-only kzr-dev.testnet ft_total_supply json-args {} network-config testnet now
near contract call-function as-read-only kzr-dev.testnet ft_balance_of \
  json-args '{"account_id":"kzr-dev.testnet"}' network-config testnet now

# mint 100 KZR (caller must be a minter; owner is a minter by default)
near contract call-function as-transaction kzr-dev.testnet mint \
  json-args '{"account_id":"alice.testnet","amount":"100000000000000000000"}' \
  prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' \
  network-config testnet sign-with-legacy-keychain send
```

> Decimals = 18, so 1 KZR = `1000000000000000000`. Initial supply above = 1,000 KZR.

## game-assets — NEP-245 multi-token

Native-NEAR port of `KruzerAssets1155.sol`. Hand-rolled (no NEP-245 in
`near-contract-standards`), tightly scoped to the game loop. See doc §4, §6, §12.

| Solidity (KruzerAssets1155.sol) | game-assets |
|---|---|
| ERC-1155 core | NEP-245 (`mt_transfer`, `mt_batch_transfer`, `mt_balance_of`, `mt_supply`) |
| `ERC1155Supply` | per-id `supply` + admin-set `max_supply` (`register_token`) |
| EIP-712 `MintVoucher` | ed25519-signed **Borsh** voucher, verified via `env::ed25519_verify` |
| EIP-712 domain separator | `contract_id` + `chain_id` inside the voucher |
| burn-to-craft | `burn_for_craft` (atomic, same contract) |
| packed token-id | `build_token_id(type,game,category,item)` (doc §12) |

Voucher guards on `mint_with_voucher`: signature · domain (contract+chain) · expiry ·
nonce replay · per-mission dedup · per-id `max_supply` · per-player rolling daily cap.
`max_supply` is contract-governed (not voucher-supplied) to bound damage if the signer
key leaks. Player attaches a storage deposit; the remainder is refunded.

**Scoped out of the POC:** NEP-245 approvals and `mt_transfer_call` (not needed by the
game loop). The production signer (`backend/sign_voucher.mjs`) is not yet written — tests
sign vouchers in-process with `ed25519-dalek`.

### Build & test

```bash
cargo test -p game-assets
cd game-assets && cargo near build non-reproducible-wasm
# artifact: near-poc/target/near/game_assets/game_assets.wasm
```

### Deploy to testnet

`signer_public_key` is the **raw 32-byte ed25519 key, base64-encoded** (not the
`ed25519:` string form). `daily_mint_cap` is in asset units.

```bash
near contract deploy kzr-dev.testnet \
  use-file target/near/game_assets/game_assets.wasm \
  with-init-call new json-args '{
    "owner_id": "kzr-dev.testnet",
    "signer_public_key": "<base64-of-32-byte-ed25519-pubkey>",
    "chain_id": "near:testnet",
    "base_uri": "ipfs://<cid>/",
    "daily_mint_cap": "1000"
  }' \
  prepaid-gas '100.0 Tgas' attached-deposit '0 NEAR' \
  network-config testnet sign-with-legacy-keychain send
```

> Deploy `game-assets` to a **separate** account (e.g. `assets.kzr.near` in prod) —
> `kzr-dev.testnet` above is just for a quick single-account testnet trial.

### Build a token id (doc §12 example — MK-1 Stability Module)

```bash
near contract call-function as-read-only kzr-dev.testnet build_token_id \
  json-args '{"kind":2,"game":1,"category":3,"item_id":17}' network-config testnet now
```
