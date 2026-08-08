import { CONFIG } from "./config.ts";
import { MISSIONS, CRAFT, missionHash, craftHash } from "./loot.ts";
import { signVoucher, voucherToJson, type MintVoucher } from "./voucher.ts";

function signedArgs(
  receiver: string,
  entries: { token_id: string; amount: string }[],
  mission_hash: Uint8Array,
): string {
  const now = Date.now();
  const voucher: MintVoucher = {
    contract_id: CONFIG.assetsContract,
    chain_id: CONFIG.chainId,
    receiver_id: receiver,
    token_ids: entries.map((e) => e.token_id),
    amounts: entries.map((e) => e.amount),
    nonce: BigInt(now) * 1000n + BigInt(Math.floor(Math.random() * 1000)),
    expires_at_ns: BigInt(now + CONFIG.voucherTtlMs) * 1_000_000n,
    mission_hash,
  };
  const signature = signVoucher(voucher, CONFIG.skB64, CONFIG.pkB64);
  return `{"voucher":${voucherToJson(voucher)},"signature":${JSON.stringify(signature)}}`;
}

export function buildMintArgs(receiver: string, missionId = "awaken-the-nexus"): string {
  const loot = MISSIONS[missionId];
  if (!loot) throw new Error(`unknown mission: ${missionId}`);
  return signedArgs(receiver, loot, missionHash(missionId, receiver));
}

export function buildCraftArgs(receiver: string): string {
  return signedArgs(receiver, CRAFT.output, craftHash(receiver));
}
