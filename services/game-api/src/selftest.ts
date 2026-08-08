import { CONFIG } from "./config.ts";
import { MISSIONS, missionHash } from "./loot.ts";
import { serializeVoucher, signVoucher, verifyVoucher, type MintVoucher } from "./voucher.ts";

const loot = MISSIONS["awaken-the-nexus"];
const v: MintVoucher = {
  contract_id: CONFIG.assetsContract,
  chain_id: CONFIG.chainId,
  receiver_id: "player.testnet",
  token_ids: loot.map((l) => l.token_id),
  amounts: loot.map((l) => l.amount),
  nonce: 42n,
  expires_at_ns: 4102444800000000000n,
  mission_hash: missionHash("awaken-the-nexus", "player.testnet"),
};

const sig = signVoucher(v, CONFIG.skB64, CONFIG.pkB64);
const ok = verifyVoucher(v, sig, CONFIG.pkB64);
const tampered: MintVoucher = { ...v, amounts: ["999", ...v.amounts.slice(1)] };
const tamperedRejected = !verifyVoucher(tampered, sig, CONFIG.pkB64);

console.log("borsh bytes:", serializeVoucher(v).length);
console.log("token_ids:", v.token_ids.join(","));
console.log("self-verify:", ok);
console.log("tamper-rejected:", tamperedRejected);
if (!ok || !tamperedRejected) process.exit(1);
console.log("SELFTEST OK");
