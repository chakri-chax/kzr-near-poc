# integration-tests

near-workspaces sandbox tests exercising all four contracts together against real
deployed Wasm (no mocks). Standalone crate (excluded from the contract workspace so its
heavy dependency tree doesn't touch contract builds).

## What `tests/full_loop.rs` proves

Deploys `kzr-token`, `ingame-coin`, `game-assets`, `ingame-conversion` to a sandbox, wires
them (convert = KZR minter; gameapi/root = NXC minter; convert registered as an NXC sink;
Squad Legacy token-id registered; game-assets storage funded), then runs end-to-end:

1. **Voucher mint** — ed25519-signed Borsh voucher → 30 Rifle Cells.
2. **Replay rejection** — reused nonce fails on-chain.
3. **Burn-for-craft** — burns 10 cells → 20 remain.
4. **Conversion success** — 500 NXC `ft_transfer_call` → 5 KZR minted; NXC held by convert; lifetime counter = 5 KZR.
5. **Conversion rollback** — KZR paused so the mint fails → NXC refunded, reservation rolled back, no KZR minted. (The async reserve-then-rollback path — unit tests can't cover promise results.)

## Running

1. Build the four contract Wasms first (the test loads them from `../target/near/*/`):

   ```bash
   for c in kzr-token ingame-coin game-assets ingame-conversion; do
     (cd ../$c && cargo near build non-reproducible-wasm)
   done
   ```

2. Provide a NEAR sandbox binary. near-workspaces can auto-download it, but if that is
   flaky, fetch it explicitly:

   ```bash
   eval "$(./fetch-sandbox.sh)"   # sets NEAR_SANDBOX_BIN_PATH
   ```

3. Run:

   ```bash
   cargo test full_loop_and_rollback -- --nocapture
   ```

Sandbox version: `near-sandbox 2.13.3` (matches near-workspaces 0.22).
