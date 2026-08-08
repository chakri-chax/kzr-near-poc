# relayer

Pays gas for player actions so claiming loot is gasless. TypeScript, Node 22 type-stripping.

## Gasless claim

`POST /relay/claim { account_id, mission_id? }` →
1. fetches a backend-signed voucher from `game-api` (`/mission/complete`),
2. submits `assets.mint_with_voucher(voucher, signature)` from the **relayer** account (pays gas),
3. loot mints to the voucher's `receiver_id` (the player).

`mint_with_voucher` authorizes on the ed25519 **voucher** signature, not the caller — so the
relayer can submit on the player's behalf with **no player key and no player gas**. The args are
sent as **raw bytes** (near-api-js accepts a `Uint8Array`), preserving the voucher's `u64`
`nonce`/`expires_at_ns` integer literals that a JSON re-encode would corrupt.

Per-account rate limit (in-memory). Uses the `relayer.<root>.testnet` key from
`~/.near-credentials/testnet/`.

## Run

```bash
npm install
GAME_API_URL=http://localhost:8080 npm start   # :8081
```

## Remaining (NEP-366 meta-tx)

Craft (`burn_for_craft`) and convert burn the *player's* own tokens, so they need the player's
authorization. Gasless versions use a NEP-366 `SignedDelegate`: the client signs a DelegateAction
with a scoped function-call key, POSTs it here, and the relayer wraps + submits it (paying gas).
The DelegateAction is Borsh-serialized, so it also avoids the u64-in-JSON limit. Not yet built.
