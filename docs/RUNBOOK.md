# Runbook — deploy & operate

How to reproduce the testnet deployment from scratch, operate it, and what a mainnet cut would require. Testnet is deployed **mainnet-ready** (all network/contract IDs parameterised).

## Live deployment (current)

| Surface | URL / ID |
|---|---|
| dApp | https://app-ten-murex-87.vercel.app |
| game-api | https://kzr-game-api.onrender.com (`srv-d9ren9v40ujc73bac2r0`) |
| relayer | https://kzr-relayer.onrender.com (`srv-d9reng2jobas73d9dtvg`) |
| indexer | https://kzr-indexer.onrender.com (`srv-d9rfg8710e5c73fv1hhg`) |
| Repo | github.com/chakri-chax/kzr-near-poc |
| Vercel project | `prj_elIMEVcYNxx1JzaRJuAA1JvTTQmf` (team `team_C0YqZGC90lbDyDPFgvNWS8J1`) |

## Accounts (root `squadlegacy.testnet`)

| Account | Purpose | Init funding | Role wiring |
|---|---|---|---|
| `squadlegacy.testnet` | root · owner of all 4 contracts · KZR treasury | faucet | owner |
| `token.squadlegacy.testnet` | `kzr-token` (KZR) | 3 Ⓝ | minters: `convert` |
| `coin.squadlegacy.testnet` | `ingame-coin` (NXC) | 3 Ⓝ | minters: `gameapi`; sinks: `convert`; `convert` storage-registered |
| `assets.squadlegacy.testnet` | `game-assets` (NEP-245) | 4 Ⓝ | signer pubkey set; 7 token-ids registered; 1.5 Ⓝ storage budget |
| `convert.squadlegacy.testnet` | `ingame-conversion` | 3 Ⓝ | KZR minter + NXC sink; rate 1/100; caps 100/1000 KZR |
| `gameapi.squadlegacy.testnet` | signing identity / NXC minter | 1 Ⓝ | NXC minter |
| `relayer.squadlegacy.testnet` | pays gas for gasless mints | 1 Ⓝ | — |

## Keys

| Key | Where it lives | Notes |
|---|---|---|
| Signer ed25519 (`sk_b64`/`pk_b64`) | `near-poc/.signer.json` (gitignored) → game-api env `SIGNER_SK_B64`/`SIGNER_PK_B64` | pubkey `0fYqSct…`; set on `assets` via `set_signer_public_key`. **Raw 32-byte ed25519, not a NEAR access key.** |
| Relayer full-access key | `~/.near-credentials/testnet/relayer.squadlegacy.testnet.json` → relayer env `RELAYER_KEY` | pays gas |
| Root + sub-account keys | `~/.near-credentials/testnet/*.json` (legacy keychain) | created by `deploy-testnet.sh` |
| DB URL / password | `.env` `SUPABASE_PASSWORD` → indexer env `DATABASE_URL` | pooler host `aws-0-ap-southeast-2.pooler.supabase.com:5432` |

> **Never commit** `.env`, `.signer.json`, or `~/.near-credentials`. `.gitignore` covers the first two; the third is outside the repo.

## Env matrix

| Var | game-api | relayer | indexer | frontend (`NEXT_PUBLIC_`) |
|---|:--:|:--:|:--:|:--:|
| `SIGNER_SK_B64` / `SIGNER_PK_B64` | ● | | | |
| `ASSETS_CONTRACT` / `CHAIN_ID` | ● | | | |
| `VOUCHER_TTL_MS` (def 600000) | ● | | | |
| `RELAYER_ACCOUNT` / `RELAYER_KEY` | | ● | | |
| `ROOT` (def `squadlegacy.testnet`) | | ● | ● | ● |
| `RPC_URL` (def FastNEAR) | | ● | ● | ● |
| `GAME_API_URL` | | ● | | |
| `RATE_PER_MIN` (def 5) | | ● | | |
| `DATABASE_URL` (or `SUPABASE_*`) | | | ● | |
| `NEARBLOCKS_URL` / `NEARBLOCKS_API_KEY` | | | ● | |
| `POLL_MS` (def 10000; deploy uses 8000) | | | ● | |
| `NEXT_PUBLIC_RELAYER_URL` | | | | ● |
| `NEXT_PUBLIC_INDEXER_URL` | | | | ● |
| `NEXT_PUBLIC_DEMO_ACCOUNT` (def `kzr-dev.testnet`) | | | | ● |
| `PORT` (Render injects) | ● | ● | ● | |

## Reproduce testnet from scratch

**Toolchain:** Rust 1.9x + `wasm32-unknown-unknown`, `cargo-near` 0.22, `near-cli-rs` 0.29, Node ≥ 20.

```bash
cd near-poc

# 1. Build all 4 contracts to Wasm (reproducible)
cargo near build --no-docker    # or: cargo build --target wasm32-unknown-unknown --release, per crate

# 2. Generate the signer keypair → .signer.json (gitignored)
node scripts/gen-signer-key.mjs

# 3. Pin art + NEP-245 metadata to IPFS (needs PINATA_* in .env) → sets base_uri
node scripts/pin-assets.mjs

# 4. Create sub-accounts, deploy + init all 4 contracts, wire roles/sinks,
#    fund assets storage, register the 7 token-ids (idempotent)
ROOT=squadlegacy.testnet bash scripts/deploy-testnet.sh

# 5. Smoke-test the conversion loop (optional)
#    ft_transfer_call coin→convert and confirm KZR minted
```

`deploy-testnet.sh` init parameters (canonical):
- `kzr-token.new`: `owner_id=ROOT`, `treasury_id=ROOT`, `initial_supply=1000 KZR` (1e21 yocto)
- `ingame-coin.new`: `owner_id=ROOT`
- `game-assets.new`: `owner_id=ROOT`, `signer_public_key=<pk_b64>`, `chain_id=near:testnet`, `base_uri=<ipfs>`, `daily_mint_cap=1000000`
- `ingame-conversion.new`: `kzr_token`, `coin_token`, `rate_num=1`, `rate_den=100`, `daily_cap=100e18`, `lifetime_cap=1000e18`
- wiring: `token.add_minter(convert)`, `coin.add_minter(gameapi)`, `coin.register_sink(convert)`, `coin.storage_deposit(convert)`, `assets.storage_top_up(1.5Ⓝ)`, `assets.register_token(id,max)×7`

## Deploy the services (Render)

Each service: rootDir `services/<name>`, build `npm install`, start `npm start` (`tsx src/server.ts`), plan free, region oregon. Create via the Render API (`POST /v1/services`, `ownerId=tea-d0f4q8q4d50c739tlohg`) with the env vars from the matrix. The relayer's `GAME_API_URL` points at the game-api URL; the indexer's `DATABASE_URL` is the Supabase pooler string.

**Redeploy gotcha:** Render autoDeploy does **not** reliably fire on monorepo path-only changes. Force a deploy:
```bash
curl -X POST https://api.render.com/v1/services/<srv-id>/deploys \
  -H "Authorization: Bearer $RENDER_API_KEY" -H 'content-type: application/json' \
  -d '{"clearCache":"do_not_clear"}'
```

## Deploy the frontend (Vercel)

```bash
cd near-poc/app
npx vercel --prod --yes --token "$VERCEL_TOKEN"   # project already linked (app/.vercel/project.json)
```
`NEXT_PUBLIC_*` are baked at build time — set them on the project first (`POST /v10/projects/<id>/env?upsert=true`), then redeploy. Deployment Protection (SSO) is disabled on the project so the URL is public.

## Operational notes

- **Render free tier cold-starts** (~50s) and spins down when idle. The relayer's internal call to game-api can 502 if game-api is still waking — warm game-api first, or retry. The indexer's per-account cursor makes spin-down self-healing (it backfills the gap on wake).
- **NearBlocks free tier** throttles above ~10 requests/short-window; the indexer polls round-robin at `POLL_MS=8000` and backs off on 429. Set `NEARBLOCKS_API_KEY` for higher limits.
- **u64 in JSON:** never re-stringify `nonce`/`expires_at_ns`; the relayer forwards raw voucher bytes.

## Cut mainnet (checklist — out of scope for the slice, but ready)

1. Point `network-config mainnet`; create root + sub-accounts under a mainnet name.
2. Rebuild contracts; deploy with the same init params (adjust `chain_id=near:mainnet`, caps/supply as policy dictates).
3. Move signer + relayer keys into **KMS** (ticket 22); rotate the demo keys.
4. Set service env to mainnet RPC + `chain_id`; set frontend `NEXT_PUBLIC_ROOT` + RPC.
5. Transfer contract ownership to a **Sputnik DAO** (`set_owner`) and **remove full-access keys** from the contract accounts for immutability (design doc §3.5).
6. Real art/metadata re-pin; register final token-ids + `max_supply` policy.
