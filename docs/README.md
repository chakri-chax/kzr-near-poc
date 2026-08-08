# KZR / Ultraverse NEAR — documentation

Living docs for the production, audit-ready vertical slice of the KZR/Ultraverse gaming stack on NEAR. Kept in sync with the build.

## The system in one paragraph

Four audit-ready contracts on `squadlegacy.testnet` (KZR token, NXC in-game coin, NEP-245 game assets with ed25519 voucher mint, one-way NXC→KZR conversion), fronted by three real backend services (a voucher **signer**, a gasless **relayer**, and a NEP-297 **indexer** → Supabase read model), with a Next.js dApp on Vercel dramatising the **Squad Legacy** mission loop: play a mission → claim loot gasless → craft an upgrade → convert currency, all on live testnet state.

## Documents

| Doc | What's in it |
|---|---|
| [ARCHITECTURE.md](ARCHITECTURE.md) | Components, topology diagram, and the claim / craft / conversion flow sequence diagrams; trust model; known gaps |
| [EVM_TO_NEAR_MAPPING.md](EVM_TO_NEAR_MAPPING.md) | `Kruzer.sol` → `kzr-token` function map; what's a direct port vs net-new; concept mappings (EIP-712 → ed25519 vouchers, ERC-1155 → NEP-245); token-id scheme |
| [API.md](API.md) | HTTP contracts for game-api / relayer / indexer, and the on-chain read/write surface the dApp uses |
| [RUNBOOK.md](RUNBOOK.md) | Accounts, keys, env matrix, reproduce-testnet steps, redeploy procedures, mainnet-cut checklist |
| [SCENARIO.md](SCENARIO.md) | The "Awaken the Nexus" live demo walkthrough |
| [audit/AUDIT_READINESS.md](audit/AUDIT_READINESS.md) | Contract audit hand-off package: entry points, invariants, NEP conformance, remediations |

## Live deployment

| Surface | URL |
|---|---|
| dApp | https://app-ten-murex-87.vercel.app |
| game-api (signer) | https://kzr-game-api.onrender.com |
| relayer (gasless) | https://kzr-relayer.onrender.com |
| indexer (read model) | https://kzr-indexer.onrender.com |
| Contracts | `token.` · `coin.` · `assets.` · `convert.squadlegacy.testnet` |

## Standing constraints

Production-manner, no code comments; no stubs/mocks (real signer/RPC/IPFS/testnet/DB); testnet now, mainnet-ready (all IDs parameterised); built to the migration baseline §10 compliance (no staking/APY/reflections, no on-chain randomness, no paid loot boxes, one-way conversion, deterministic loot).
