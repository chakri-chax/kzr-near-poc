# game-api (voucher signer)

Backend service that authorises Squad Legacy loot by signing `game-assets` mint vouchers.
TypeScript, run directly with Node 22 type-stripping (no build step).

## Run

```bash
npm run selftest    # local Borsh + ed25519 sign/verify/tamper check
npm run sign -- <receiver.testnet> [mission_id]   # prints mint_with_voucher args JSON
npm start           # HTTP: POST /mission/complete {account_id, mission_id}
```

The signer key is read from `../../.signer.json` (gitignored). `ASSETS_CONTRACT`,
`CHAIN_ID`, `PORT`, `VOUCHER_TTL_MS` are env-overridable.

## What it does

- Serializes `game-assets::MintVoucher` in **Borsh, byte-identical to the Rust contract**
  (verified live: a signed voucher was accepted by `assets.squadlegacy.testnet`).
- Signs it with the KMS/local ed25519 key.
- Emits a **batch voucher** (`token_ids[]` / `amounts[]`) so a mission grants its whole
  loot table atomically under one `mission_hash`.

## u64-in-JSON

`nonce` and `expires_at_ns` are `u64` on-chain (ns timestamps exceed 2^53). The service
emits them as **exact integer literals** in the args JSON — never via `JSON.stringify` of a
JS Number. A frontend MUST submit the returned args as raw bytes to near-api-js
`functionCall` (or use this args string verbatim), not re-`JSON.stringify` the voucher.

## Not yet built (ticket 22)

Server-side mission-state machine (Supabase), NEP-413 ownership proof, NXC minting via the
service, KMS-backed key, Render deploy.
