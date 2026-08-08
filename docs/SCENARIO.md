# Live demo scenario — "Awaken the Nexus"

A walkthrough of the Squad Legacy vertical slice for a live audience. Everything below is a **real testnet transaction** against the deployed contracts — no mocks.

## Fiction (from the Squad Legacy canon)

The Ultraverse fractured. **Pioneers** venture into corrupted **Nexus Zones** to reclaim dormant power nodes for **the Restoration**. **Sephrenia**, the mission AI, guides the Pioneer. This demo runs **Nexus Zone 07 — "Awaken the Nexus."**

## Cast (on-chain)

- **You** — the Pioneer (a NEAR testnet wallet).
- **KZR** — Kruzer Coin, the hard currency (`token.`).
- **NXC** — Nexus Credits, the in-game soft currency (`coin.`).
- **Assets** — weapons, ammo, mods, cosmetics, achievements (`assets.`, NEP-245).

## Walkthrough

### 0. Land (unconnected)
Open **app-ten-murex-87.vercel.app**. The page is already live — it shows the demo Pioneer `kzr-dev.testnet`'s real balances, inventory, and activity feed. Sephrenia briefs the mission.
> *Talking point:* every number on screen is read live from testnet via RPC; the activity feed is materialised from on-chain NEP-297 events by the indexer.

### 1. Connect
Click **Connect Wallet** → MyNearWallet or Meteor. The header now shows your account + live KZR/NXC chips.

### 2. Play the mission (interactive)
Work the objective console — each step advances with Sephrenia narration:
1. **Deploy to Nexus Zone 07**
2. **Advance to the power node**
3. **Hold the node** (a brief hold sequence)
4. **Stabilize & awaken**

> *Talking point:* the objective flow is the "live experience". Mission-state ordering/anti-cheat validation is server-side hardening (ticket 22); the reward's real anti-abuse is on-chain (a mission can only be claimed once — `mission_hash` dedup).

### 3. Claim loot — **gasless**
The console unlocks **Claim Loot**. Click it. No wallet popup, no gas: the relayer submits a signer-authorised voucher mint on your behalf.
- You receive **30 Rifle Cell · 3 Nano Medkit · 2 Weapon-Mod Fragment · 1 Hackclaw** in one atomic batch.
- Inventory tiles fill in; the activity feed gains **"Claimed loot — …"** within ~30s.

> *Talking point:* the mint is authorised by an ed25519 voucher (domain-bound, nonce, expiry, dedup), so the relayer can pay gas without ever being able to mint arbitrary items.

### 4. Craft an upgrade — burn → **gasless mint**
The Craft card is now enabled (you have ≥20 Cell + ≥2 Fragment). Click **Craft Upgrade**:
1. Your wallet asks you to sign **`burn_for_craft`** (20 Rifle Cell + 2 Weapon-Mod Fragment) — you spend your own tokens, so you authorise it.
2. The relayer then mints the output **gasless**: **MK-1 Stability Module + First Restoration Badge** (the badge is a once-per-Pioneer award).
- Inventory shows Cell 30→10, Fragment 2→0, MK-1 0→1, Badge 0→1.

> *Talking point:* MyNearWallet redirects to sign the burn; the app resumes the gasless mint on return, so a burn can never strand the output.

### 5. Convert soft currency → hard currency
In the Convert card, enter an NXC amount (rate **100 NXC → 1 KZR**). Click **Convert to KZR** and sign. The `ingame-conversion` contract burns/holds your NXC and async-mints KZR; your KZR chip updates, and the feed shows **"Converted N NXC → M KZR."**

> *Talking point:* conversion is one-way and async — reserve-then-rollback keeps the daily/lifetime caps correct even if the cross-contract KZR mint fails.

## What a fresh Pioneer sees vs the demo account

`kzr-dev.testnet` has already claimed, crafted, and converted — so an unconnected visitor sees a *completed* Pioneer (great for showing the end state). A freshly connected wallet starts at mission step 0 with an empty inventory and plays the whole loop.

## Reset for a repeat demo

- A new testnet wallet is the cleanest reset (each Pioneer's claim/craft dedup is per-account).
- The same account **cannot** re-claim `awaken-the-nexus` (on-chain dedup) or re-craft (per-account craft `mission_hash` + input depletion) — this is intended.
- NXC for the convert step: seed via `coin.mint` from the `gameapi` minter (mission reward NXC minting is ticket 22).

## The loop, one line

**Play mission → claim loot (gasless) → inventory → craft upgrade (burn + gasless mint) → convert NXC→KZR → activity feed** — all real, all on testnet.
