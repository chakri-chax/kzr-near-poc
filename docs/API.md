# API contracts

HTTP surfaces of the three backend services, plus the on-chain read/write surface the dApp uses. All HTTP services send `access-control-allow-origin: *` and JSON bodies.

## game-api — voucher signer (`kzr-game-api.onrender.com`)

Signs ed25519 `MintVoucher`s (env `SIGNER_SK_B64` / `SIGNER_PK_B64`), runs the Supabase mission-state machine, verifies NEP-413 ownership proofs, and mints the 500 NXC/mission reward via the `gameapi` key.

### `POST /mission/start`
Request: `{ "account_id", "mission_id"?, "proof" }` where `proof` is a **NEP-413** signed message `{ accountId, publicKey, signature, message, nonce (base64, 32 bytes), recipient }`. Verifies the ed25519 signature over `sha256(Borsh(payload))` **and** that the key is on `account_id` (RPC `view_access_key`); records the mission (`proven=true, step=0`). `401` on a bad proof.

### `POST /mission/objective`
Request: `{ "account_id", "mission_id"?, "step": 1..4 }`. Advances one objective; enforces order (`step == prev+1`), each-once, and bounds — `409` otherwise. Step 4 marks the mission `complete`.

### `POST /mission/complete`
Request: `{ "account_id": "alice.testnet", "mission_id": "awaken-the-nexus" }` (`mission_id` optional, defaults to `awaken-the-nexus`). When the mission is server-confirmed (proven + complete) and past the min-time gate, mints **500 NXC** (idempotent) and returns the loot voucher. With `MISSION_GATING=strict` an unconfirmed mission is rejected (`409`/`401`/`425`); with `lenient` (default) the voucher is still issued so the one-click demo keeps working.
Response: the exact args object for `assets.mint_with_voucher`:
```json
{
  "voucher": {
    "contract_id": "assets.squadlegacy.testnet",
    "chain_id": "near:testnet",
    "receiver_id": "alice.testnet",
    "token_ids": ["281479271677953","281492156579841","2306124497075306497","2306124505665241089"],
    "amounts": ["30","3","2","1"],
    "nonce": 1786180371400650,
    "expires_at_ns": 1786180971400000000,
    "mission_hash": [ 32 bytes ]
  },
  "signature": "base64-ed25519-sig"
}
```
> `nonce` and `expires_at_ns` are u64 — do **not** re-`JSON.stringify` them client-side (float corruption > 2^53). The relayer forwards the raw bytes.

### `POST /craft/complete`
Request: `{ "account_id": "alice.testnet" }`. Response: same shape, minting `["2306124497075306513","3459045988797251585"]` (MK-1 Stability Module + First Restoration Badge), amounts `["1","1"]`, under a per-account craft `mission_hash` (once-per-player).

### `GET /health` → `{ "ok": true }`

## relayer — gasless mint (`kzr-relayer.onrender.com`)

Fetches a voucher from game-api and submits `assets.mint_with_voucher` from `relayer.squadlegacy.testnet`, paying gas. Only ever calls that one method. Per-account rate limit `RATE_PER_MIN` (default 5/min). Submits args as **raw bytes** to preserve u64 exactness.

### `POST /relay/claim`
Request: `{ "account_id": "alice.testnet", "mission_id"?: "awaken-the-nexus" }`.
Response `200`: `{ "ok": true, "tx": "<hash>", "receiver": "alice.testnet" }`.
Errors: `400` missing `account_id` · `429` rate limited · `502` `{error:"game-api", detail}` · `500` `{error}` (e.g. contract panic `"Mission already claimed"`).

### `POST /relay/craft`
Request: `{ "account_id": "alice.testnet" }`. Same response shape; mints MK-1 + Badge. Call **after** the wallet-signed `burn_for_craft` confirms.

### `GET /health` → `{ "ok": true, "relayer": "relayer.squadlegacy.testnet" }`

## indexer — NEP-297 read model (`kzr-indexer.onrender.com`)

Discovers per-account transactions via NearBlocks, enriches authoritative logs via RPC `EXPERIMENTAL_tx_status`, materialises `idx_event` in Supabase. Chain is source of truth; this is for history + fast reads.

### `GET /activity?account=<id>&limit=<n≤100>`
```json
{ "account": "alice.testnet",
  "activity": [
    { "event":"mt_mint","kind":"mint","contract":"assets.squadlegacy.testnet",
      "token_id":"281479271677953","sign":1,"amount":"30","counterparty":"",
      "detail":null,"block_height":"262957099","block_ts":"1786180389822963928",
      "receipt_id":"3gus…" }
  ] }
```
`kind` ∈ `mint | burn | transfer_in | transfer_out | conversion | conversion_rollback`. Conversion rows carry `detail:{nxc_in,kzr_out}` and `sign:0` (feed-only; the real token movement is separate ft rows).

### `GET /inventory?account=<id>` → `{ account, inventory:[{ token_id, balance }] }`
NEP-245 balances materialised as `SUM(sign*amount)` over `contract=assets`, positive only. Reconciles with `assets.mt_batch_balance_of`.

### `GET /balances?account=<id>` → `{ account, kzr:"<yocto>", nxc:"<yocto>" }`

### `GET /health` → `{ ok, events, head, accounts }` (indexed event count, max block height, distinct accounts)

## On-chain surface (dApp)

### Reads (view, via FastNEAR RPC — `lib/near.ts`)
| Call | Returns |
|---|---|
| `assets.mt_batch_balance_of({ account_id, token_ids[] })` | `string[]` balances |
| `token.ft_balance_of({ account_id })` | KZR yocto string |
| `coin.ft_balance_of({ account_id })` | NXC yocto string |
| `convert.get_rate()` | `[num, den]` (e.g. `["1","100"]`) |
| `convert.quote({ nxc_in })` | KZR out (yocto) |
| `assets.mt_supply` / `mt_batch_supply` | circulating per token-id |

### Writes
| Action | Signer | Gas | Path |
|---|---|---|---|
| `assets.mint_with_voucher(voucher, signature)` | relayer | relayer | claim + craft output (gasless) |
| `assets.burn_for_craft(token_ids, amounts, memo)` | player wallet | player | craft inputs |
| `coin.ft_transfer_call(convert, amount, msg)` | player wallet | player | conversion |
| `coin.ft_transfer(receiver, amount)` | player wallet | player | P2P (24h cap applies, sinks exempt) |

Admin (owner-only, `1 yoctoNEAR`): `add_minter` / `register_sink` / `register_token` / `set_signer_public_key` / `set_base_uri` / `set_rate` / `set_caps` / `pause` / `unpause` / `set_owner`. See `RUNBOOK.md`.
