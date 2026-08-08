import { CONFIG } from "./config.ts";
import { MISSIONS, missionHash } from "./loot.ts";
import { signVoucher, voucherToJson, type MintVoucher } from "./voucher.ts";

export function buildMintArgs(receiver: string, missionId = "awaken-the-nexus"): string {
  const loot = MISSIONS[missionId];
  if (!loot) throw new Error(`unknown mission: ${missionId}`);
  const now = Date.now();
  const voucher: MintVoucher = {
    contract_id: CONFIG.assetsContract,
    chain_id: CONFIG.chainId,
    receiver_id: receiver,
    token_ids: loot.map((l) => l.token_id),
    amounts: loot.map((l) => l.amount),
    nonce: BigInt(now) * 1000n + BigInt(Math.floor(Math.random() * 1000)),
    expires_at_ns: BigInt(now + CONFIG.voucherTtlMs) * 1_000_000n,
    mission_hash: missionHash(missionId, receiver),
  };
  const signature = signVoucher(voucher, CONFIG.skB64, CONFIG.pkB64);
  return `{"voucher":${voucherToJson(voucher)},"signature":${JSON.stringify(signature)}}`;
}
