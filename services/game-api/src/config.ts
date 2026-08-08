import { readFileSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const POC = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..");

function signerKeys(): { sk_b64: string; pk_b64: string } {
  if (process.env.SIGNER_SK_B64 && process.env.SIGNER_PK_B64) {
    return { sk_b64: process.env.SIGNER_SK_B64, pk_b64: process.env.SIGNER_PK_B64 };
  }
  const path = join(POC, ".signer.json");
  if (existsSync(path)) return JSON.parse(readFileSync(path, "utf8"));
  throw new Error("signer key missing: set SIGNER_SK_B64 + SIGNER_PK_B64, or provide .signer.json");
}

const s = signerKeys();

export const CONFIG = {
  skB64: s.sk_b64,
  pkB64: s.pk_b64,
  assetsContract: process.env.ASSETS_CONTRACT ?? "assets.squadlegacy.testnet",
  chainId: process.env.CHAIN_ID ?? "near:testnet",
  voucherTtlMs: Number(process.env.VOUCHER_TTL_MS ?? 10 * 60 * 1000),
  port: Number(process.env.PORT ?? 8080),
};
