# Architecture — KZR / Ultraverse NEAR vertical slice

This describes the system **as built and deployed**, not the original design doc. It is a living document; update it as the build changes.

## Components

| Layer | Component | Tech | Where it runs |
|---|---|---|---|
| Contracts | `kzr-token` (KZR) | NEP-141, near-sdk 5.29 | `token.squadlegacy.testnet` |
| Contracts | `ingame-coin` (NXC) | NEP-141 + 24h P2P cap | `coin.squadlegacy.testnet` |
| Contracts | `game-assets` | NEP-245 multi-token + ed25519 voucher mint | `assets.squadlegacy.testnet` |
| Contracts | `ingame-conversion` | `ft_on_transfer` receiver, async NXC→KZR | `convert.squadlegacy.testnet` |
| Backend | `game-api` (voucher signer) | TS / Node (`tsx`) | Render → `kzr-game-api.onrender.com` |
| Backend | `relayer` (gasless mint) | TS / Node (`tsx`) + near-api-js 5 | Render → `kzr-relayer.onrender.com` |
| Backend | `indexer` (NEP-297 read model) | TS / Node (`tsx`) + `pg` | Render → `kzr-indexer.onrender.com` |
| Data | read model | Supabase Postgres (`aws-0-ap-southeast-2` pooler) | Supabase |
| Frontend | dApp | Next.js 14 App Router, wallet-selector 8.9 | Vercel → `app-ten-murex-87.vercel.app` |
| Infra accounts | signer / relayer signing keys | ed25519 | `gameapi.` / `relayer.squadlegacy.testnet` |
| External | RPC | FastNEAR (`test.rpc.fastnear.com`) | public |
| External | tx discovery | NearBlocks testnet API | public |

Design intent: **chain state is the source of truth** (frontend reads balances/inventory directly via RPC view calls); the indexer materialises the NEP-297 event history for the activity feed and fast reads. Secrets (signer key, relayer key, DB URL) live only in service env vars, never in the repo.

## Topology

```mermaid
graph TD
  subgraph Client
    UI["Next.js dApp<br/>(Vercel)"]
    W["Wallet<br/>MyNearWallet / Meteor"]
  end
  subgraph Render["Backend (Render)"]
    GA["game-api<br/>ed25519 voucher signer"]
    RL["relayer<br/>gasless mint, pays gas"]
    IX["indexer<br/>NEP-297 → read model"]
  end
  DB[("Supabase<br/>Postgres")]
  subgraph NEAR["NEAR testnet — squadlegacy.testnet"]
    T["token · KZR<br/>NEP-141"]
    C["coin · NXC<br/>NEP-141"]
    A["assets<br/>NEP-245"]
    V["convert<br/>ft_on_transfer"]
  end
  RPC["FastNEAR RPC"]
  NB["NearBlocks API"]

  UI -->|view calls| RPC
  UI -->|activity/inventory| IX
  UI -->|"POST /relay/claim, /relay/craft"| RL
  UI -->|"sign burn_for_craft / ft_transfer_call"| W
  W -->|signed tx| RPC
  RL -->|"fetch voucher"| GA
  RL -->|"mint_with_voucher (gas-paid)"| A
  RPC --> T & C & A & V
  V -->|"cross-contract mint"| T
  V -->|"reserve/rollback"| C
  IX -->|"discover txns"| NB
  IX -->|"tx_status logs"| RPC
  IX --> DB
```

## Flow 1 — Gasless claim (mission loot)

The mint is authorised by the **signer's voucher**, not the player, so the relayer submits it and pays gas. The player needs no key and no NEAR.

```mermaid
sequenceDiagram
  participant UI as dApp
  participant RL as relayer
  participant GA as game-api
  participant A as assets contract
  UI->>RL: POST /relay/claim {account_id}
  RL->>GA: POST /mission/complete {account_id}
  GA-->>RL: {voucher, signature}  (ed25519 over Borsh)
  RL->>A: mint_with_voucher(voucher, sig)  [relayer signs+pays]
  A->>A: verify sig · domain · expiry · nonce · mission_hash dedup · daily cap
  A-->>RL: mt_mint event (loot batch)
  RL-->>UI: {ok, tx}
  UI->>UI: refresh inventory (RPC) + activity (indexer)
```

## Flow 2 — Craft (burn → gasless mint)

The burn spends the player's own tokens, so it is **wallet-signed**; the crafted output is then minted gaslessly like a claim. On MyNearWallet the burn redirects away and back, so a `localStorage` flag + `transactionHashes` on return **resumes** the mint (a burn can never strand the output).

```mermaid
sequenceDiagram
  participant UI as dApp
  participant W as wallet
  participant A as assets contract
  participant RL as relayer
  participant GA as game-api
  UI->>W: sign burn_for_craft([cell×20, frag×2])
  W->>A: burn_for_craft   [player signs+pays gas]
  A-->>W: mt_burn event
  UI->>RL: POST /relay/craft {account_id}
  RL->>GA: POST /craft/complete {account_id}
  GA-->>RL: {voucher: MK-1 + Badge, signature}
  RL->>A: mint_with_voucher  [relayer pays gas]
  A-->>RL: mt_mint event (MK-1 + first-craft Badge)
  RL-->>UI: {ok, tx}
```

## Flow 3 — Conversion (async reserve-then-rollback)

Conversion is a one-way NXC→KZR burn+mint. The player calls `ft_transfer_call` on the coin; the coin invokes `convert.ft_on_transfer`, which reserves against the caps, cross-contract-mints KZR, and **rolls the reservation back** if the mint promise fails (funds returned via the `ft_on_transfer` unused-amount contract).

```mermaid
sequenceDiagram
  participant UI as dApp
  participant W as wallet
  participant C as coin (NXC)
  participant V as convert
  participant T as token (KZR)
  UI->>W: sign ft_transfer_call(coin → convert, amount, msg)
  W->>C: ft_transfer_call   [player pays gas]
  C->>V: ft_on_transfer(sender, amount, msg)
  V->>V: quote + reserve (daily/lifetime caps, checked_add)
  V->>T: mint(sender, kzr_out)   [convert is a KZR minter]
  alt mint ok
    T-->>V: ()
    V-->>C: return 0 (all NXC consumed) + "conversion" event
  else mint fails
    V->>V: roll back reservation
    V-->>C: return amount (NXC refunded) + "conversion_rollback" event
  end
```

## Trust & security posture

- **Voucher authority.** `game-assets` mints only on an ed25519 signature from the configured `signer_public_key`, bound to `contract_id` + `chain_id` (domain separation), with per-voucher `nonce` (replay guard), `expires_at_ns` (TTL), `mission_hash` (per-mission / per-craft dedup), per-token `max_supply`, and a contract-wide daily mint cap. The signer key lives only in `game-api` env.
- **Relayer scope.** The relayer holds one funded key (`relayer.squadlegacy.testnet`) and only ever calls `mint_with_voucher` on `assets`. It cannot mint arbitrary tokens — every mint still needs a valid signer voucher. Per-account rate limiting guards spend.
- **Player-authorised burns.** `burn_for_craft` and conversion `ft_transfer_call` burn/move the player's own balance and are always wallet-signed; the relayer never moves player funds.
- **Conversion safety.** Reserve-then-rollback keeps caps monotonic even when the async KZR mint fails; caps use `checked_add`. See `docs/audit/AUDIT_READINESS.md`.

## Known gaps (tracked)

- Server-validated mission-state (ordering, min-time gate) + NXC mission reward — **ticket 22**.
- Full NEP-366 meta-transaction gasless burn (function-call access keys) — relayer hardening; the slice ships wallet-signed burns instead.
- KMS custody for signer/relayer keys (currently Render env vars) — **ticket 22**.
- Mainnet cut (Sputnik DAO admin, key-lock immutability) — out of scope for the slice; see `RUNBOOK.md`.
