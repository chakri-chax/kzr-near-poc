# KZR / Ultraverse — NEAR Smart-Contract Audit-Readiness Package

**Scope:** four native-NEAR (Rust) contracts of the KZR EVM→NEAR migration POC (Route B).
**Framework:** Trail of Bits code-maturity categories, evidence-based ratings.
**Status:** internal audit-readiness hand-off (no external auditor engaged yet).
**Prepared:** 2026-08-08. **Nature:** documentation only — no contract code was modified.

Ratings vocabulary: **Strong · Satisfactory · Moderate · Weak · Missing · N/A**.

---

## 1. Overview

| Contract | Crate / path | Standard(s) | Purpose |
|---|---|---|---|
| **KZR — Kruzer Coin** | `kzr-token/src/lib.rs` | NEP-141 (+148, +145) | Capped (1B @ 18 dec) utility FT; owner+minters, mint-path pause, self-burn, mint auto-registers recipients. Port of `Kruzer.sol`. |
| **NXC — Nexus Credits** | `ingame-coin/src/lib.rs` | NEP-141 (+148, +145) | Uncapped in-game currency; minter-controlled, pause, self-burn, **+ 50 NXC / 24h per-account P2P transfer cap** with owner-managed sink exemption. |
| **Game Assets** | `game-assets/src/lib.rs` | NEP-245 (core subset) | Multi-token; ed25519-signed **Borsh voucher** minting (nonce + mission-hash dedup, per-id `max_supply`, per-player daily cap, `contract_id`+`chain_id` domain binding), atomic `burn_for_craft`, packed `u64` token-ids, contract-funded storage. Port of `KruzerAssets1155.sol`. |
| **Conversion** | `ingame-conversion/src/lib.rs` | NEP-141 receiver | **One-way NXC→KZR**; `ft_on_transfer` receiver; async **reserve-then-rollback** mint (daily + lifetime counters); owner-updatable rate/caps; `burn_collected`. |

**Locked spec:** `/home/gaian/gaianC/NEAR/.scratch/kzr-near-slice/decisions.md`.
**Compliance constraints:** `/home/gaian/gaianC/NEAR/.scratch/kzr-near-slice/research/01-resources-review.md` (§ Compliance).

### Build & test evidence

Wasm SHA-256 — **interim host build** (`cargo near build non-reproducible-wasm`, non-reproducible; see §8):

| Artifact | SHA-256 | Bytes |
|---|---|---|
| `kzr_token` | `44088339e825b12659b310be933a75d0011b5a76021aeb30972e7ad5eb3690df` | 171,677 |
| `ingame_coin` | `79f30cc98321fdca4f88298809633cabed042be12945a5638bca8efc543f8d82` | 179,724 |
| `game_assets` | `470505c33966f7f892ee64844af846c2db186f0781ebf2357518fac2b5f5e7fc` | 214,271 |
| `ingame_conversion` | `81e467c64b9f21f3180fe890ffbc993997a8f4a1798d14db28bc6bdd06a9fb2b` | 137,642 |

**Tests:** 43 unit tests pass (kzr-token 9, ingame-coin 9, game-assets 16, ingame-conversion 9) + **1 near-workspaces integration test** (`integration-tests/tests/full_loop.rs` — full mission loop + async conversion rollback against a real sandbox, `near-sandbox 2.13.3`).

**Toolchain:** `near-sdk 5.29`, `near-contract-standards 5.29`, edition 2021, `rust-version 1.86`. Workspace `[profile.release]` sets `overflow-checks = true`, `opt-level = "z"`, `lto = true`, `panic = "abort"`, `codegen-units = 1` (see `near-poc/Cargo.toml`) — **`overflow-checks = true` means arithmetic `+`/`*`/`-` panic (abort → revert) on overflow in the shipped Wasm, not just in debug.**

---

## 2. Per-contract analysis

### 2.1 KZR — `kzr-token/src/lib.rs`

#### State-changing entry points

| Method | Lines | Access control | Payable | Args |
|---|---|---|---|---|
| `new` | 76–115 | `#[init]` (one-shot; `PanicOnDefault`) | — | `owner_id`, `treasury_id`, `initial_supply: U128` |
| `mint` | 124–145 | `assert_minter` **+** `!paused` | no | `account_id`, `amount` |
| `burn` | 153–166 | public — **self-burn** (`predecessor_account_id`) | no | `amount` |
| `pause` / `unpause` | 172–184 | `assert_owner` | **1 yocto** | — |
| `add_minter` / `remove_minter` | 190–202 | `assert_owner` | **1 yocto** | `account_id` |
| `set_owner` | 206–211 | `assert_owner` | **1 yocto** | `new_owner` |
| `ft_transfer` | 260–263 | public (delegates to inner FT) | **1 yocto** (std) | `receiver_id`, `amount`, `memo?` |
| `ft_transfer_call` | 265–274 | public | **1 yocto** (std) | `receiver_id`, `amount`, `memo?`, `msg` |
| `ft_resolve_transfer` | 287–298 | `#[private]` (callback) | no | `sender_id`, `receiver_id`, `amount` |
| `storage_deposit` / `storage_withdraw` / `storage_unregister` | 306–323 | public (NEP-145) | payable / 1 yocto | per NEP-145 |

**View methods (no state change):** `ft_total_supply`, `ft_balance_of`, `is_paused`, `get_owner`, `get_minters`, `is_minter`, `max_supply`, `storage_balance_bounds`, `storage_balance_of`, `ft_metadata`.

#### Invariants / security properties

- **I-KZR-1 — Hard cap:** `total_supply ≤ MAX_SUPPLY` (`1_000_000_000 × 10^18`). Enforced in `new` (line 80) and `mint` (lines 129–132: `total_supply + amount <= MAX_SUPPLY`).
- **I-KZR-2 — Mint is privileged & pausable:** only accounts in `minters` can mint (line 125), and only while `!paused` (line 126).
- **I-KZR-3 — Burn is self-only:** `burn` withdraws exclusively from `predecessor_account_id` (line 154). No privileged `burn_from` exists (matches `ERC20Burnable`).
- **I-KZR-4 — Auto-registration is bounded to mint:** `mint` registers an unregistered recipient from the contract's own storage budget (lines 134–136); no other path spends contract storage on behalf of arbitrary accounts.
- **I-KZR-5 — Admin mutations require a full-access signature:** all owner mutations assert 1 yoctoNEAR (`assert_one_yocto`), blocking function-call-key abuse.

#### Arithmetic / storage / low-level

- Cap check uses a raw `total_supply + amount` (line 130). With `overflow-checks = true` this **panics** rather than wrapping, so the cap cannot be bypassed via a `u128` wrap in the shipped Wasm. It nonetheless depends on the compiler flag — see Finding **F-3** (prefer `checked_add`).
- `burn` underflow is caught by the standard's `internal_withdraw` (balance-checked panic).
- No `unsafe`, no raw storage pokes; NEP-141 core/resolver/storage delegate to audited `near-contract-standards` types. Manual `impl` of core/storage is a deliberate 5.x-compat choice documented at lines 256–257 and 301–303.
- **No cross-contract calls** — no async/reentrancy surface.

---

### 2.2 NXC — `ingame-coin/src/lib.rs`

#### State-changing entry points

| Method | Lines | Access control | Payable | Args |
|---|---|---|---|---|
| `new` | 48–72 | `#[init]` | — | `owner_id` |
| `mint` | 74–89 | `assert_minter` **+** `!paused` | no | `account_id`, `amount` |
| `burn` | 91–102 | public — self-burn | no | `amount` |
| `pause` / `unpause` | 104–116 | `assert_owner` | **1 yocto** | — |
| `add_minter` / `remove_minter` | 118–130 | `assert_owner` | **1 yocto** | `account_id` |
| `set_owner` | 132–137 | `assert_owner` | **1 yocto** | `new_owner` |
| `register_sink` / `unregister_sink` | 139–151 | `assert_owner` | **1 yocto** | `account_id` |
| `set_transfer_cap` | 153–158 | `assert_owner` | **1 yocto** | `cap: U128` |
| `ft_transfer` | 224–228 | public **+ `note_p2p` cap** | **1 yocto** | `receiver_id`, `amount`, `memo?` |
| `ft_transfer_call` | 230–240 | public **+ `note_p2p` cap** | **1 yocto** | `receiver_id`, `amount`, `memo?`, `msg` |
| `ft_resolve_transfer` | 253–264 | `#[private]` | no | `sender_id`, `receiver_id`, `amount` |
| `storage_*` | 268–294 | public (NEP-145) | payable / 1 yocto | per NEP-145 |

**View methods:** `is_paused`, `get_owner`, `get_minters`, `is_minter`, `get_sinks`, `is_sink`, `get_transfer_cap`, `p2p_transferred_of`, `ft_total_supply`, `ft_balance_of`, `storage_balance_bounds`, `storage_balance_of`, `ft_metadata`.

#### Invariants / security properties

- **I-NXC-1 — P2P cap:** for any sender and any 24h bucket, the sum of P2P transfer amounts to **non-sink** receivers is `≤ transfer_cap`. Enforced in `note_p2p` (lines 207–219): sinks short-circuit (line 208), otherwise `used + amount <= self.transfer_cap` (line 214) and the bucket counter is updated (line 218).
- **I-NXC-2 — Sink exemption:** transfers to owner-registered sinks (the conversion contract; later shops) are unrestricted (line 208), governed instead by the conversion caps. Matches decisions §1.
- **I-NXC-3 — Uncapped supply, privileged mint:** no `MAX_SUPPLY`; minting is minter-gated + pausable (lines 75–76) — this is the KZR pattern minus the cap, as specified.
- **I-NXC-4 — Bucket definition:** the 24h window is a **fixed** UTC bucket `block_timestamp / DAY_NS` (line 211), not a rolling window — a deliberate match to the daily-cap pattern used elsewhere (decisions §1). Consequence: up to `2 × cap` can move across a bucket boundary in a short real-time window; **accepted by design.**

#### Arithmetic / storage / low-level

- `used + amount` (line 214) overflow-safe under `overflow-checks = true`; `used ≤ transfer_cap` is bounded.
- `p2p_transferred: LookupMap<(AccountId, u64), u128>` grows one entry per `(sender, day)`; unbounded over time, storage staked by the contract (Finding **F-6**).
- The cap is checked **before** the transfer executes; `ft_transfer` failures revert atomically. `ft_transfer_call` refunds via `ft_resolve_transfer` do **not** decrement the counter — see Finding **F-4**.
- No `unsafe`; no cross-contract calls initiated by this contract.

---

### 2.3 Game Assets — `game-assets/src/lib.rs`

#### State-changing entry points

| Method | Lines | Access control | Payable | Args |
|---|---|---|---|---|
| `new` | 119–141 | `#[init]` | — | `owner_id`, `signer_public_key: Base64VecU8`, `chain_id`, `base_uri`, `daily_mint_cap` |
| `mint_with_voucher` | 178–220 | **public**, gated by ed25519 sig + domain + expiry + nonce + mission + daily-cap + `max_supply`; **+ `!paused`** | no (gasless) | `voucher: MintVoucher`, `signature: Base64VecU8` |
| `burn_for_craft` | 230–247 | public — self-burn (`predecessor`) | no (gasless) | `token_ids[]`, `amounts[]`, `memo?` |
| `mt_transfer` | 253–268 | public; rejects `approval` | **1 yocto** | `receiver_id`, `token_id`, `amount`, `approval?`, `memo?` |
| `mt_batch_transfer` | 270–291 | public; rejects `approvals` | **1 yocto** | `receiver_id`, `token_ids[]`, `amounts[]`, `approvals?`, `memo?` |
| `register_token` | 333–345 | `assert_owner`; single-shot per id | **1 yocto** | `token_id`, `max_supply` |
| `set_signer_public_key` | 347–352 | `assert_owner` | **1 yocto** | `signer_public_key` |
| `set_base_uri` | 354–359 | `assert_owner` | **1 yocto** | `base_uri` |
| `set_daily_mint_cap` | 361–366 | `assert_owner` | **1 yocto** | `daily_mint_cap` |
| `pause` / `unpause` | 368–380 | `assert_owner` | **1 yocto** | — |
| `set_owner` | 382–387 | `assert_owner` | **1 yocto** | `new_owner` |
| `storage_top_up` | 503–509 | **public** (donation) | payable (`> 0`) | — |
| `owner_withdraw` | 511–516 | `assert_owner` | **1 yocto** | `amount` |

**View methods:** `build_token_id`, `decode_token_id` (both `&self`, pure helpers), `mt_balance_of`, `mt_batch_balance_of`, `mt_supply`, `mt_batch_supply`, `token_reference`, `get_owner`, `get_signer_public_key`, `get_chain_id`, `get_base_uri`, `is_paused`, `get_daily_mint_cap`, `max_supply_of`, `is_nonce_used`, `is_mission_claimed`, `daily_minted_of`.

#### Invariants / security properties

- **I-GA-1 — Voucher authenticity & domain:** a mint requires `ed25519_verify(sig, borsh(voucher), signer_pk)` (lines 190–199) **and** `voucher.contract_id == current_account_id` (181–183) **and** `voucher.chain_id == self.chain_id` (184). This binds every voucher to one contract on one chain (the EIP-712 domain-separator analogue).
- **I-GA-2 — Expiry:** `block_timestamp < voucher.expires_at_ns` (185–188).
- **I-GA-3 — Nonce single-use:** each `nonce` redeemable at most once (`used_nonces`, checked line 201, set line 214).
- **I-GA-4 — Mission single-claim:** each `mission_hash` redeemable at most once (`claimed_missions`, checked 202–205, set 215). **Granularity is per-voucher (single `token_id`)** — see Finding **F-1**.
- **I-GA-5 — Per-id supply ceiling:** `supply[token_id] + amount ≤ max_supply[token_id]`, and minting an **unregistered** id panics `"Token not registered"` (`internal_mint`, lines 467–477). `max_supply` is **contract-governed** (`register_token`), never taken from the voucher — bounding damage if the signer key leaks.
- **I-GA-6 — Per-player daily cap:** `daily_minted[(receiver, day)] + amount ≤ daily_mint_cap` (209–212).
- **I-GA-7 — Checks-before-effects:** all guards (paused, domain, expiry, signature, nonce, mission, amount, daily cap) precede the three inserts + `internal_mint` (214–217). No cross-contract call in the mint/burn/transfer paths ⇒ **no reentrancy surface**.
- **I-GA-8 — Burn is self-only & supply-consistent:** `burn_for_craft` burns only `predecessor`'s balances (line 240), asserts `bal ≥ amount` (line 481), and reduces `supply` via `saturating_sub` (line 484).
- **I-GA-9 — Approvals rejected:** `mt_transfer`/`mt_batch_transfer` panic if `approval(s)` is `Some` (263, 280) — the out-of-scope approval surface cannot be exercised.

#### Arithmetic / storage / low-level

- **Bit-packing** `[type:4|game:12|category:16|item_id:32]` (`pack_token_id` 163–172; `decode_token_id` 153–161) validates `kind < 16`, `game < 4096`; `category`/`item_id` occupy full `u16`/`u32`. Round-trip proven by `token_id_packing_round_trips` (incl. full-range `15/4095/65535/u32::MAX`).
- `env::ed25519_verify` is a host function (no in-Wasm crypto). Signature is length-checked to 64 bytes (191–195); the public key to 32 bytes (`to_ed25519_key`, 444–450). **Anti-replay dedups on `nonce`/`mission_hash`, not on signature bytes** — correctly immune to ed25519 signature malleability.
- `supply + amount` (line 473) overflow-safe under the profile flag (Finding **F-3**).
- **Contract-funded storage:** `storage_top_up` (payable, public donation) funds the stake; `owner_withdraw` reclaims. `used_nonces`, `claimed_missions`, `daily_minted`, `balances`, `supply` all grow monotonically (nonces/missions *must* — that is the anti-replay set) — storage runway is an operational concern (Finding **F-6**).
- **NEP-297 events** are hand-rolled as `EVENT_JSON:` with `standard:"nep245"` for `mt_mint`/`mt_burn`/`mt_transfer` (emit helpers 520–577).

---

### 2.4 Conversion — `ingame-conversion/src/lib.rs`

#### State-changing entry points

| Method | Lines | Access control | Payable | Args |
|---|---|---|---|---|
| `new` | 61–84 | `#[init]`; requires `rate_num,rate_den > 0` | — | `owner_id`, `kzr_token`, `coin_token`, `rate_num`, `rate_den`, `daily_cap`, `lifetime_cap` |
| `ft_on_transfer` | 86–130 | **only `coin_token`** (predecessor check, 93–96) **+ `!paused`** | no | `sender_id`, `amount`, `msg` (ignored) |
| `on_mint_complete` | 132–153 | `#[private]` (callback) | no | `account_id`, `kzr_out`, `nxc_in`, `day`, `#[callback_result] result` |
| `burn_collected` | 155–162 | `assert_owner` | **1 yocto** | `amount` |
| `set_rate` | 164–171 | `assert_owner`; `> 0` | **1 yocto** | `rate_num`, `rate_den` |
| `set_caps` | 173–179 | `assert_owner` | **1 yocto** | `daily_cap`, `lifetime_cap` |
| `pause` / `unpause` | 181–193 | `assert_owner` | **1 yocto** | — |
| `set_owner` | 195–200 | `assert_owner` | **1 yocto** | `new_owner` |
| `storage_top_up` | 202–208 | public (donation) | payable (`> 0`) | — |
| `owner_withdraw` | 210–215 | `assert_owner` | **1 yocto** | `amount` |

**View methods:** `quote`, `get_owner`, `get_kzr_token`, `get_coin_token`, `get_rate`, `get_caps`, `is_paused`, `daily_converted_of`, `lifetime_converted_of`.

#### Invariants / security properties

- **I-CV-1 — Caller authenticity:** `ft_on_transfer` accepts calls **only** from the registered `coin_token` (93–96). This is the single most security-critical check: without it, any account could invoke it and mint KZR for free (the contract is a KZR minter). **Enforced.** Direct-call rejection is covered by test `only_coin_can_call`.
- **I-CV-2 — One-way only:** the contract can only *mint* KZR from received NXC; there is **no** KZR→NXC path, no reverse method, no refund-in-KZR path. Structural.
- **I-CV-3 — Conversion is floor-rounded & non-zero:** `kzr_out = nxc.checked_mul(rate_num) / rate_den` (101–104), and `kzr_out > 0` is required (105, `"Below minimum conversion"`) ⇒ dust that rounds to 0 KZR is rejected and the NXC is refunded (the `require!` aborts `ft_on_transfer`, so the NXC-side resolver refunds in full).
- **I-CV-4 — Caps reserved before mint:** `daily_converted[(sender,day)]` and `lifetime_converted[sender]` are checked (110, 112–115) then **incremented before** dispatching the mint (117–119). This optimistic reservation makes concurrent conversions from one account respect the caps while a mint is in flight.
- **I-CV-5 — Failed mint leaves counters unchanged (net):** on mint failure, `on_mint_complete` rolls both counters back by `saturating_sub(kzr_out)` (144–150) and returns `nxc_in` so the NXC-side resolver refunds the sender in full. On success it returns `U128(0)` (all NXC consumed → held by the contract).
- **I-CV-6 — No mint without ≥ equal NXC consumption on success:** success returns `0` unused ⇒ the full `nxc_in` stays with the contract; `kzr_out = floor(nxc_in × num/den) ≤ nxc_in × num/den`. NXC held is later reducible by owner via `burn_collected`.

#### Async correctness — the reserve-then-rollback path (examined closely)

The promise chain (121–129): `ext_kzr::mint(sender, kzr_out).then(ext_self::on_mint_complete(sender, kzr_out, nxc_in, day))`, returned as the `ft_on_transfer` result so the **NXC** token's `ft_resolve_transfer` uses `on_mint_complete`'s return as the "unused" amount.

Correctness observations (all **verified correct**):

1. **Day bucket is passed *through*, not recomputed.** `day` is captured at reserve time (line 107) and threaded into the callback (127, 133). The rollback (144) decrements the **exact** bucket that was reserved even if the callback executes after a UTC midnight rollover. Recomputing `day` in the callback would have been a real bug (decrement the wrong bucket, leaving the old one over-reserved); the code avoids it. **This is done right.**
2. **`kzr_out` and `nxc_in` are likewise threaded** (not recomputed from a possibly-changed `rate`), so a mid-flight `set_rate` cannot desynchronize the rollback from the reservation.
3. **Rollback is order-independent and underflow-safe.** For interleaved conversions that each added their own `kzr_out`, subtracting the same `kzr_out` on failure restores the counter regardless of callback order; `saturating_sub` prevents underflow.
4. **No synchronous reentrancy.** `kzr-token.mint` does not call back into the conversion contract; even if it did, counters are already reserved.
5. **Integration-tested.** `full_loop.rs` step 4 asserts success (5 KZR minted, 500 NXC held, lifetime = 5 KZR); step 5 pauses KZR to force a mint failure and asserts **NXC fully refunded, no KZR minted, lifetime rolled back to 5 KZR** — i.e., the rollback path executes correctly on a real sandbox.

**I found no correctness bug in the async rollback.** The one residual concern is the general NEAR rule that the callback must be infallible — captured as Finding **F-5** (informational; the callback is currently allocation-light and panic-free, and 15 Tgas is statically reserved).

#### Arithmetic / low-level

- `checked_mul(...).unwrap_or_else(panic)` guards the rate multiply in both `ft_on_transfer` (101–103) and `quote` (220–223); `rate_den > 0` enforced at `new`/`set_rate` ⇒ no divide-by-zero. Floor division is intentional.
- `MINT_GAS`/`CALLBACK_GAS` = 15 Tgas each (static) — ample for the simple mint + trivial callback.
- No `unsafe`.

---

## 3. NEP conformance matrix

| NEP | Contract(s) | Conformance | Notes / deviations |
|---|---|---|---|
| **141 (Fungible Token)** | KZR, NXC | **Conformant (core + resolver)** | `ft_transfer` / `ft_transfer_call` / `ft_total_supply` / `ft_balance_of` / `ft_resolve_transfer` present; `ft_transfer*` assert 1 yocto. `mint`/`burn` are non-standard extensions. |
| **141 — NXC P2P cap** | NXC | **Interface-compatible deviation** | `ft_transfer`/`ft_transfer_call` keep NEP-141 signatures and emit standard events, but may **reject** an otherwise-valid transfer that exceeds the 24h P2P cap (`note_p2p`, 207–219). A strict NEP-141 integrator must expect a possible `"P2P 24h transfer cap exceeded"` panic. Documented, intentional (decisions §1). |
| **145 (Storage Management)** | KZR, NXC | **Conformant** | Full `StorageManagement` impl delegating to the inner FT (KZR 304–332, NXC 267–295). |
| **145** | Game Assets, Conversion | **Not implemented (by design)** | Contract-funded storage model instead: `storage_top_up` (payable) + owner `owner_withdraw`. No per-user storage accounting. |
| **148 (FT Metadata)** | KZR, NXC | **Conformant** | `FungibleTokenMetadataProvider::ft_metadata`; `metadata.assert_valid()` at init. |
| **148 / 245 metadata** | Game Assets | **Partial** | No `mt_metadata`/`nep148`-style struct; metadata is an off-chain pointer via `token_reference = {base_uri}{token_id}.json` and `base_uri`. |
| **245 (Multi-Token)** | Game Assets | **Core subset** | Implements `mt_transfer`, `mt_batch_transfer`, `mt_balance_of`, `mt_batch_balance_of`, `mt_supply`, `mt_batch_supply`. **Intentionally out of scope:** `mt_transfer_call` (receiver hook) and the **approval** management interface (`mt_approve`/`mt_revoke`/…) — both are explicitly rejected/absent and documented (module docs 22–24; approval rejection at 263/280). |
| **297 (Events)** | KZR, NXC | **Conformant** | `FtMint`/`FtBurn` (`standard:"nep141"`) on mint/burn; `FtTransfer` emitted by the inner FT on transfers. |
| **297** | Game Assets | **Conformant (hand-rolled)** | `EVENT_JSON:` with `standard:"nep245"`, events `mt_mint`/`mt_burn`/`mt_transfer`, `version 1.0.0` (520–577). |
| **297** | Conversion | **Missing** | Emits **no** NEP-297 events for conversion success or rollback (Finding **F-2**). Observable only indirectly via KZR `FtMint` + NXC transfer/refund events. |
| **297 — admin actions** | all four | **Gap** | Privileged mutations (pause, role/sink changes, `set_rate`/`set_caps`, `register_token`, `set_signer_public_key`, `set_owner`, `owner_withdraw`) emit no events (Finding **F-2**). |
| **366 (Meta-Transactions)** | all four (relevance) | **Compatible, not implemented in-contract** | The relayer (backend §5) sponsors gas for the 3 gasless actions. Contracts are meta-tx-safe because (a) the gasless methods (`mint_with_voucher`, `burn_for_craft`, `ft_transfer_call`) require no attached deposit, and (b) authorization uses either the voucher's `receiver_id`/signature (relayer-agnostic) or `predecessor_account_id`, which under NEP-366 resolves to the delegating **user**, not the relayer — so relayed `burn_for_craft` correctly consumes the user's own assets. |

---

## 4. Compliance matrix (migration §10 + scope PDF)

| # | Constraint | Where enforced / status |
|---|---|---|
| 1 | **One-way conversion only (NXC→KZR, never reverse); no refunds post-conversion; no fiat leg.** | **Enforced structurally** in `ingame-conversion`: only `ft_on_transfer` (NXC in) → `ext_kzr::mint` (KZR out). No reverse method exists. On success NXC is held (not refunded); refund happens only on *failed* mint (a rollback, not a reversal). ✅ |
| 2 | **No yield / APY / reflections / auto-stake.** | **Enforced by absence.** No staking, rewards-accrual, rebase, or reflection logic in any of the four contracts. KZR/NXC are plain NEP-141; conversion is a fixed-rate mint. Burn paths are deflationary *consumption*, not a return. ✅ |
| 3 | **No on-chain randomness; no paid loot boxes.** | **Enforced.** `game-assets` has no randomness source; loot is deterministic and **attested by a signed voucher** (`mint_with_voucher`). No purchase/RNG path. ✅ |
| 4 | **"No financial value / no resale" metadata on cosmetics; no price in URIs.** | **N/A on-chain (off-chain metadata).** `game-assets` stores only a `base_uri` pointer (`token_reference`); it embeds no price and no valuation. The disclaimer language lives in the off-chain JSON at `{base_uri}{token_id}.json` and must be present there (e.g., cosmetic `build_token_id(1,1,2,1)`). **Action: verify the off-chain metadata carries the disclaimer** — not verifiable from code. ⚠️ off-chain |
| 5 | **KZR crosses the in-game/real boundary only at the conversion contract.** | **Enforced by wiring:** KZR is only minted by registered minters; the intended in-game earner is NXC (`ingame-coin`), and `ingame-conversion` is the sole in-game contract registered as a KZR minter (deploy wiring, decisions §7; integration `add_minter(convert)`). In-game vendors transact in NXC (sink-exempt from the P2P cap). ✅ (enforced operationally by minter registration, not by a code constant) |
| 6 | **No PII on-chain.** | **N/A / honored.** No contract stores personal data; `game-assets` uses opaque `mission_hash: [u8;32]` and account ids only. ✅ |

---

## 5. Maturity scorecard

| Category | Rating | One-line evidence |
|---|---|---|
| **Arithmetic** | **Satisfactory** | `checked_mul` on rate math, `saturating_sub` on rollback/burn, balance-checked subtractions; but supply/counter **caps use raw `+`** and rely on `overflow-checks = true` rather than `checked_add` (F-3). |
| **Auditing & Logging (NEP-297)** | **Moderate** | Value-movement events present (FT `nep141`, MT `nep245`); **conversion emits nothing** and **no admin action is logged** (F-2). |
| **Access Controls** | **Strong** | Consistent `assert_owner`/`assert_minter`, 1 yocto on every admin mutation, `#[private]` callbacks, `predecessor == coin_token` gate (I-CV-1), signature+domain voucher gate (I-GA-1). |
| **Complexity Management** | **Strong** | Four small single-file contracts, each tightly scoped; the one genuinely async flow (conversion) is cleanly separated into reserve / dispatch / callback. |
| **Decentralization** | **Weak** | Single plain-account `owner` with broad powers (pause, rate/caps, signer key, `owner_withdraw`); single backend signer & minters. DAO hand-off exists only as `set_owner` (not yet wired); no on-chain timelock/multisig. Damage is **bounded** by contract-governed `max_supply`/caps — an intentional POC posture, not a defect. |
| **Documentation** | **Satisfactory** | Excellent external spec (`decisions.md`) + rich module docs on KZR & game-assets; **NXC and conversion have no module/inline docs**; per-function docs uneven. |
| **Low-Level Manipulation** | **Strong** | No `unsafe`; crypto via host `ed25519_verify`; length-checked key/sig; safe, round-trip-tested bit-packing; storage via typed SDK collections. |
| **Testing & Verification** | **Satisfactory** | 43 unit tests (happy + negative: cap, pause, replay, expiry, wrong-chain, bad-sig, only-coin, dust, cap-exceeded) + 1 real-sandbox integration test covering the async rollback. **No fuzzing / property / invariant tests**; concurrency & partial-refund edges untested. |
| **Front-Running / MEV** | **Satisfactory** | Few surfaces: vouchers bind a fixed `receiver_id` (front-running only delivers to the intended player — no benefit); dedup blocks double-redeem. Note: conversion has **no min-out** vs an owner `set_rate` change (F-7). |

**Narrative.** For a scoped vertical-slice POC this is clean, disciplined code. The three security-critical mechanisms — voucher authentication with contract+chain domain binding, the `predecessor == coin_token` gate on the free-minting `ft_on_transfer`, and the async reserve-then-rollback — are each implemented correctly, and the rollback in particular gets the subtle details right (day-bucket and amount pass-through, order-independent saturating rollback) and is exercised against a real sandbox. The gaps are the ones typical of a POC and are addressable without redesign: **observability** (no conversion/admin events), **centralization** (single-owner/single-signer, DAO only aspirational), and **defense-in-depth arithmetic** (lean on `checked_add`, not a compiler flag). One functional/spec item — the per-voucher granularity of mission-hash dedup vs. the documented multi-item mission loot — should be resolved before the real "Awaken the Nexus" loot table is wired. No critical or high-severity issues were found.

---

## 6. Findings (ranked)

No **Critical** or **High** findings. Ordered by descending priority.

### F-1 · Medium · Per-mission dedup granularity vs. multi-item mission loot
- **Location:** `game-assets` — `MintVoucher` (87–106), `mint_with_voucher` mission check (202–205, 215).
- **Description:** A voucher authorizes exactly **one** `token_id`/`amount`, but the "mission claimed once" guard keys on a single `mission_hash`. The documented "Awaken the Nexus" mission (decisions §3) grants **four** distinct items (30 Rifle Cell, 3 Nano Medkit, 2 Weapon-Mod Fragment, 1 Hackclaw). If the backend issues four vouchers sharing one `mission_hash`, only the first redeems — the other three fail with `"Mission already claimed"`. To grant multi-item loot the backend must derive **distinct** `mission_hash` values per `(mission, item)`, which silently redefines the invariant from "a mission's loot is claimed once" to "a `(mission, item)` is claimed once" and moves the anti-double-claim guarantee onto correct off-chain hash derivation.
- **Impact:** Functional break of the flagship loot table if per-mission hashes are reused; weakened/ambiguous dedup semantics if per-item hashes are used without a documented, collision-resistant scheme.
- **Recommendation:** Either (a) add a **batch voucher** (`Vec<token_id>` + `Vec<amount>`, dedup on one `mission_hash`), or (b) formally specify `mission_hash = H(mission_id ‖ token_id)` in the signer, add a code comment stating on-chain dedup is per-voucher, and cover the multi-item mission in an integration test. Prefer (a) — it keeps the single-claim guarantee on-chain.

### F-2 · Medium · No NEP-297 events for conversions or admin actions
- **Location:** `ingame-conversion` (whole file — no `emit`); admin mutations across all four contracts.
- **Description:** The conversion contract emits **no** event on success or on rollback/refund; the indexer's planned "conversion events" (decisions §5) do not exist, forcing off-chain reconciliation to correlate KZR `FtMint` with NXC transfer/refund receipts. Separately, no privileged state change in any contract (pause, role/sink edits, `set_rate`, `set_caps`, `register_token`, `set_signer_public_key`, `set_owner`, `owner_withdraw`) emits an event, so there is no on-chain audit trail of governance actions.
- **Impact:** Weak auditability/observability; harder incident forensics and indexer correctness.
- **Recommendation:** Emit a structured `EVENT_JSON` (e.g., `standard:"kzr_convert"`) on conversion success and on rollback (with `account`, `nxc_in`, `kzr_out`, `day`, `outcome`), and emit events for every admin mutation.

### F-3 · Low · Supply/counter caps rely on the `overflow-checks` profile flag, not `checked_add`
- **Location:** `kzr-token` `mint` (130); `game-assets` `internal_mint` (473); `ingame-coin` `note_p2p` (214); `ingame-conversion` reservation (110, 113).
- **Description:** Core cap invariants are expressed as raw `a + b <= LIMIT`. They are safe **only because** `[profile.release] overflow-checks = true` turns a `u128` wrap into an abort. If that flag is ever dropped, or a build inherits a different profile, `total_supply + amount` (etc.) could wrap and bypass the cap.
- **Impact:** Latent — a config regression could silently defeat the hard cap / per-id supply / caps. Minters/signer are trusted, limiting practical exploitability today.
- **Recommendation:** Use `checked_add(...).expect("overflow")` for the cap/counter additions so the invariant does not depend on a compiler flag; keep `overflow-checks = true` as belt-and-suspenders.

### F-4 · Low · P2P counter not rolled back on `ft_transfer_call` refund (NXC)
- **Location:** `ingame-coin` `note_p2p` (207–219) called from `ft_transfer_call` (238), vs. `ft_resolve_transfer` (253–264).
- **Description:** `note_p2p` records the full amount **before** the transfer. If a **non-sink** `ft_transfer_call` is (partially or fully) refunded by the receiver, `ft_resolve_transfer` returns the balance but the sender's 24h P2P counter is **not** decremented, over-counting against the sender's own cap. The primary conversion path is unaffected (the conversion contract is a registered **sink**, short-circuited at line 208).
- **Impact:** Minor, self-inflicted accounting imprecision on an edge path; no cross-account effect, no fund loss.
- **Recommendation:** Document the behavior, or move the counter increment to a resolve-time hook so refunded amounts are credited back.

### F-5 · Low (informational) · Conversion callback must remain infallible
- **Location:** `ingame-conversion` `on_mint_complete` (132–153).
- **Description:** The callback is currently panic-free (LookupMap ops + `saturating_sub`) and has 15 Tgas statically reserved. If it ever panicked **after a successful** KZR mint, the NXC-side resolver would treat the whole promise as failed and **refund the NXC while the KZR stays minted** (double credit); a panic **after a failed** mint would leave the daily/lifetime counters permanently over-reserved (lifetime is never time-reset).
- **Impact:** None today; a future edit that introduces an allocation/`unwrap`/heavier logic into the callback could open a double-credit or cap-lockout window.
- **Recommendation:** Keep the callback allocation-light and panic-free; add a code comment stating the infallibility requirement; consider an integration test that asserts counter/balance consistency when the callback is stressed.

### F-6 · Low · Unbounded contract-funded storage growth; unbounded `owner_withdraw`
- **Location:** `game-assets` (`used_nonces`, `claimed_missions`, `daily_minted`, `balances`, `supply`; `owner_withdraw` 511–516); `ingame-conversion` (`daily_converted`, `lifetime_converted`; `owner_withdraw` 210–215); `ingame-coin` (`p2p_transferred`).
- **Description:** These maps grow monotonically (nonces/missions *must* persist for replay protection), with storage staked from the contract's own balance and funded by public `storage_top_up`. `owner_withdraw` transfers an owner-chosen amount with no explicit reserve accounting — it relies on the runtime storage-staking floor to prevent bricking.
- **Impact:** Long-horizon storage runway must be funded/monitored; an owner mis-withdrawal is caught by the runtime (transfer fails) rather than by a contract-level guard.
- **Recommendation:** Track and expose a storage-runway estimate; add a reserve check to `owner_withdraw` (withdraw only above a computed storage buffer); document expected growth rates for capacity planning.

### F-7 · Informational · No min-out / rate-change protection on conversion
- **Location:** `ingame-conversion` `set_rate` (164–171) vs. `ft_on_transfer` (101–104).
- **Description:** The rate is read at execution time; a user's in-flight conversion has no user-specified minimum KZR out, so an owner `set_rate` between decision and execution changes the received amount.
- **Impact:** Minimal today (trusted owner; one-way utility conversion, not a market). Becomes relevant if governance decentralizes or rate changes become frequent/automated.
- **Recommendation:** When decentralizing, consider a min-out parameter (via `msg`) and/or a rate-change timelock.

### F-8 · Informational · `mint` / `burn` / `burn_for_craft` omit the 1-yocto assertion
- **Location:** `kzr-token`/`ingame-coin` `mint` & `burn`; `game-assets` `burn_for_craft`, `mint_with_voucher`.
- **Description:** These omit `assert_one_yocto` (deliberately — role-gated mint, self-only burn, and gasless-by-design voucher/craft). A function-call access key scoped to `burn`/`burn_for_craft` could therefore consume a user's own tokens without a full-access signature.
- **Impact:** Acceptable by design (the gasless UX depends on it), but callers must grant scoped keys deliberately.
- **Recommendation:** No code change; document that scoped keys to `burn`/`burn_for_craft` are token-consuming and should be minted with care by the client.

---

## 7. Recommended fuzz / property-test targets

1. **Conversion counter conservation** (`ingame-conversion`): over random sequences of `ft_on_transfer` + mixed success/failure callbacks (incl. day-boundary crossings), assert `lifetime_converted[u] == Σ successful kzr_out[u]`, `daily_converted[(u,d)] == Σ successful kzr_out[u] on d`, counters never exceed caps, never underflow, and a failed mint is a net no-op on counters.
2. **Conversion rate math:** for random `nxc, rate_num, rate_den (>0)`, `kzr_out == floor(nxc·num/den)`; never mints when `kzr_out == 0`; the only reachable overflow is a genuine `>u128` product (which aborts).
3. **Voucher authorization predicate** (`game-assets`): mint succeeds **iff** valid-sig ∧ contract match ∧ chain match ∧ not-expired ∧ nonce-unused ∧ mission-unclaimed ∧ `amount>0` ∧ within daily cap ∧ `supply+amount ≤ max`. Property: once a nonce/mission is consumed it is never reusable (monotonic dedup).
4. **Per-id supply ceiling:** over arbitrary interleavings of `mint_with_voucher` and `burn_for_craft`, `0 ≤ supply[id] ≤ max_supply[id]` always; supply equals net mints−burns.
5. **Token-id packing bijection:** `decode(pack(k,g,c,i)) == (k,g,c,i)` for all in-range fields; out-of-range `kind ≥ 16` / `game ≥ 4096` always reject.
6. **NXC P2P cap:** over random transfer sequences within/across buckets, `Σ non-sink transfers per (sender,bucket) ≤ cap`; sink transfers unrestricted; sink set changes take effect at the right boundary.
7. **KZR global invariant:** over arbitrary mint/burn/transfer sequences, `total_supply ≤ MAX_SUPPLY`; transfers conserve `total_supply`; burn strictly reduces it.
8. **Arithmetic overflow edge:** a `mint` `amount` near `u128::MAX` must **abort** (not wrap) — directly asserts the cap cannot be bypassed (and is the regression test for F-3 once `checked_add` lands).

---

## 8. Reproducible builds

**Current state.** Docker is available on the build host, but the **contract source is not committed to git**: the repository root is `/home/gaian/gaianC/NEAR`, and `git ls-files` matches **zero** of the contract `src/lib.rs` files (they are untracked). `cargo near build` (the reproducible path) requires a **clean, committed** working tree so it can check out the exact commit inside the pinned Docker image. **Therefore the SHA-256 checksums in §1 are interim, non-reproducible host-build values** produced with `cargo near build non-reproducible-wasm`; they will differ from the eventual reproducible Docker artifacts and must be regenerated.

**Steps to a reproducible build (to perform before external audit / mainnet):**

1. **Commit the source.** Initialize/commit the four crates + workspace + `Cargo.lock` with a clean tree:
   ```bash
   cd /home/gaian/gaianC/NEAR/near-poc
   git init            # if this dir is not already its own repo
   git add kzr-token ingame-coin game-assets ingame-conversion Cargo.toml Cargo.lock
   git commit -m "contracts: source for reproducible build"
   ```
2. **Add reproducible-build metadata to each contract `Cargo.toml`** (`kzr-token`, `ingame-coin`, `game-assets`, `ingame-conversion`):
   ```toml
   [package.metadata.near.reproducible_build]
   image = "sourcescan/cargo-near:0.13.0-rust-1.86.0"   # pin to your cargo-near/rust
   image_digest = "sha256:<pinned digest>"
   passed_env = []
   container_build_command = ["cargo", "near", "build", "non-reproducible-wasm", "--locked"]
   ```
   (Pin `image`/`image_digest` to the exact `sourcescan/cargo-near` tag matching the toolchain in §1.)
3. **Build reproducibly per crate** on a clean tree:
   ```bash
   for c in kzr-token ingame-coin game-assets ingame-conversion; do
     (cd $c && cargo near build)     # reproducible; pulls the pinned Docker image
   done
   ```
4. **Record the new checksums** (`sha256sum target/near/*/*.wasm`) and the source commit hash; these become the canonical audit artifacts. The workspace `[profile.release]` (`overflow-checks`, `opt-level="z"`, `lto`, `panic="abort"`, `codegen-units=1`) is already committed in `Cargo.toml` and carries into the reproducible build.

---

## 9. Auditor onboarding

**Prerequisites:** Rust `1.86`+ with `wasm32-unknown-unknown`, `cargo-near 0.22`+, Docker (for reproducible builds), and a NEAR sandbox binary for integration tests.

**Build each contract (interim, host):**
```bash
cd /home/gaian/gaianC/NEAR/near-poc
for c in kzr-token ingame-coin game-assets ingame-conversion; do
  (cd $c && cargo near build non-reproducible-wasm)
done
# artifacts: target/near/<crate>/<crate>.wasm
```

**Run unit tests (43 total):**
```bash
cargo test                       # whole workspace
cargo test -p kzr-token          # or per crate: kzr-token / ingame-coin / game-assets / ingame-conversion
```

**Run the integration test (full loop + async rollback):** see `integration-tests/README.md`.
```bash
# 1) build the four Wasms first (the test loads them from ../target/near/*/)
# 2) provide a sandbox binary (near-workspaces can auto-download; else:)
cd integration-tests
eval "$(./fetch-sandbox.sh)"     # sets NEAR_SANDBOX_BIN_PATH (near-sandbox 2.13.3)
# 3) run
cargo test full_loop_and_rollback -- --nocapture
```
The integration crate is intentionally excluded from the contract workspace (`Cargo.toml` `exclude = ["integration-tests"]`) so its heavy dependency tree does not touch contract builds.

**Reading order for review:** `decisions.md` (locked spec) → this document → `ingame-conversion/src/lib.rs` (the async path, F-5) → `game-assets/src/lib.rs` (voucher + F-1) → `kzr-token` / `ingame-coin` (F-3, F-4) → `integration-tests/tests/full_loop.rs` (end-to-end evidence).

---

*End of audit-readiness package. Findings F-1…F-8 are candidate follow-up tickets; §7 lists candidate fuzz/property targets. No contract code was modified in preparing this document.*

---

## Remediation (post-audit, applied by the build team)

- **F-1 (batch voucher) — FIXED.** `MintVoucher` now carries `token_ids: Vec<TokenId>` + `amounts: Vec<U128>`; `mint_with_voucher` mints the whole loot table atomically under one `mission_hash`. "Awaken the Nexus" (30 Cell / 3 Medkit / 2 Fragment / 1 Hackclaw) is now one voucher = one signature = one gasless tx. The backend signer (ticket 08) and frontend (ticket 13) build to this format.
- **F-2 (events) — PARTIALLY FIXED.** `ingame-conversion` now emits NEP-297 `EVENT_JSON` `conversion` (success) and `conversion_rollback` (failure) under standard `kzr_conversion` — the indexer's conversion feed now exists. Admin-action events across all contracts remain a follow-up (ticket 20).
- **F-3 (checked_add) — FIXED.** All supply/counter cap checks (kzr-token MAX_SUPPLY, game-assets per-id max_supply + daily cap, ingame-coin P2P cap, ingame-conversion daily+lifetime caps) use `checked_add`; the hard caps no longer depend on the `overflow-checks` profile flag.
- **Re-verification:** 43 unit tests + 1 near-workspaces integration test (full loop + async rollback, batch voucher) all pass.
- **Comment policy:** production "no comments" applied to bodies; `///`/`//!` doc headers retained (aid audit; feed the embedded ABI).

### Final Wasm SHA-256 (host build; deterministic — verified by building twice)
| contract | sha256 |
|---|---|
| kzr_token | `37579a5190c21be2663daf9fb415598629588cfd9d62a3b783cdd0aeb2d1ff15` |
| ingame_coin | `676644807f9597ff5f8fd152a1a914e9f5c5dadaab370718b7d74edf18668891` |
| game_assets | `64d55b76d8929b9ced732ea0a52699942b4ec956ea25925bfdee0942032af970` |
| ingame_conversion | `54a85b2a6cb9d207563d686464b731d10ed6779f1c44e6d80468b07aaf3c3e7e` |

These differ from the interim values above because comment edits change the embedded ABI. Canonical checksums still require the reproducible (Docker + committed source) build in §8.

### Graduated follow-ups
- **Ticket 20** — admin-action NEP-297 events across all contracts (F-2 remainder).
- **Ticket 21** — property/fuzz tests for the audit's recommended targets.
