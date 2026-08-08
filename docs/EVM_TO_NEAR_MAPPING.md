# EVM → NEAR mapping

Derived from the **actually built** artifacts, not only the migration design doc. The one shipped EVM source in this repo is `KruzerEVM/contracts/Kruzer.sol` (an ERC-20). The multi-token assets, in-game coin, and conversion contracts have **no Solidity source in this repo** — they are net-new NEAR designs realised from the migration doc §4, so their NEAR implementation is the canonical spec.

## Contract-level map

| EVM (source) | NEAR (built) | Relationship |
|---|---|---|
| `Kruzer.sol` — ERC-20 + Burnable + Pausable + Permit + AccessControl ("Kruzer Coin" / KZR) | `kzr-token` — NEP-141 (+145/148) | **Direct port** |
| *(none — design doc concept)* | `ingame-coin` — NEP-141 NXC (Nexus Credits) + 24h P2P cap | **Net-new** |
| *(none — design doc ERC-1155 "KruzerAssets")* | `game-assets` — NEP-245 multi-token + ed25519 voucher mint | **Net-new** |
| *(none — design doc "InGameToKZR")* | `ingame-conversion` — `ft_on_transfer` receiver, async NXC→KZR | **Net-new** |

## `Kruzer.sol` → `kzr-token` (function map)

| Solidity | NEAR | Notes |
|---|---|---|
| `ERC20` name/symbol/decimals ("Kruzer Coin", "KZR", 18) | `ft_metadata` (NEP-148), 18 decimals | KZR decimals preserved at 18 |
| `balanceOf` / `totalSupply` | `ft_balance_of` / `ft_total_supply` | |
| `transfer` / `transferFrom` + allowances | `ft_transfer` / `ft_transfer_call` (NEP-141) | NEAR drops ERC-20 allowances; uses transfer-call + NEP-145 storage registration instead |
| `mint(to, amount)` `onlyRole(MINTER_ROLE)` | `mint(account_id, amount)` gated by minter set | `checked_add` against `MAX_SUPPLY` (1B @ 18dp); mint auto-registers recipient storage |
| `ERC20Burnable.burn` | `burn(amount)` (self-burn) | |
| `ERC20Pausable.pause/unpause` `onlyRole(PAUSER_ROLE)` | `pause` / `unpause` (owner-gated) | pause blocks the mint path |
| `AccessControl` MINTER_ROLE / PAUSER_ROLE | `add_minter`/`remove_minter`/`get_minters` + `owner` | roles flattened to owner + minter set |
| `ERC20Permit` (EIP-2612 / EIP-712 sigs) | *(not ported to the token)* — the EIP-712 signature pattern reappears as **ed25519-signed Borsh vouchers** in `game-assets` | see below |
| `_update` hooks | n/a | |

Fixed cap: `MAX_SUPPLY = 1_000_000_000 × 10^18`, enforced with `checked_add` in `mint`. Initial treasury supply minted at `new`.

## Concept mappings (cross-cutting)

| EVM concept | NEAR realisation |
|---|---|
| EIP-712 typed-data signature (Permit) | **ed25519 over Borsh** `MintVoucher`, verified on-chain via `env::ed25519_verify`; domain-bound to `contract_id` + `chain_id` (replaces `verifyingContract` + `chainId`) |
| ERC-1155 `balanceOf`/`safeTransferFrom`/`mint`/`burn` | **NEP-245** `mt_balance_of` / `mt_transfer` / `mint_with_voucher` / `burn_for_craft`; NEP-297 `mt_mint`/`mt_burn`/`mt_transfer` events |
| ERC-1155 `uri(id)` | `token_reference(token_id)` → `{base_uri}{token_id}.json` (IPFS) |
| `msg.sender` | `env::predecessor_account_id()` |
| `require(...)` / revert | `require!` / `env::panic_str` (atomic rollback) |
| gas paid by tx sender | gas can be paid by a **relayer** (voucher-authorised mint) — enables gasless claim/craft |
| synchronous cross-contract call | **async Promises** + `#[callback_result]` — drives the conversion reserve-then-rollback |
| approvals / operator | intentionally **not supported** in `game-assets` (`mt_transfer` rejects `approval`) — scoped out |

## Token-id scheme (NEP-245, design doc §12)

A 64-bit packed id: `(kind<<60) | (game<<48) | (category<<32) | item_id`, exposed on-chain as `build_token_id` / `decode_token_id`.

| Item | (kind, game, cat, item) | token_id | max_supply |
|---|---|---|---|
| Rifle Cell (ammo) | (0,1,1,1) | 281479271677953 | 10,000,000 |
| Nano Medkit (med) | (0,1,4,1) | 281492156579841 | 10,000,000 |
| Weapon-Mod Fragment | (2,1,3,1) | 2306124497075306497 | 10,000,000 |
| MK-1 Stability Module | (2,1,3,17) | 2306124497075306513 | 100,000 |
| Hackclaw (weapon) | (2,1,5,1) | 2306124505665241089 | 100,000 |
| Adaptive Armor Skin | (1,1,…) | 1153202988173492225 | 100,000 |
| First Restoration Badge | (3,1,0,1) | 3459045988797251585 | 1,000,000 |

Category convention: `0` = none/generic, `5` = weapon. `kind`: 0 = consumable, 1 = cosmetic, 2 = equipment/mod, 3 = achievement.

## Standards

NEP-141 (FT), NEP-145 (storage), NEP-148 (FT metadata), NEP-245 (multi-token), NEP-297 (events). Meta-transactions (NEP-366) and message signing (NEP-413) are referenced for future hardening but not on the critical path of the slice.

## Compliance carry-overs (from the migration baseline §10)

Built to the baseline, **not** the stale whitepaper: no staking/APY/reflections, no on-chain randomness, no paid loot boxes; conversion is one-way coin→KZR only; loot is deterministic voucher mint; asset metadata carries "no financial value / no resale".
