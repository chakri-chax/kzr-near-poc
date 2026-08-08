import { readFileSync, existsSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const POC = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..");

const ENV_FILE = join(POC, ".env");
if (existsSync(ENV_FILE)) {
  for (const line of readFileSync(ENV_FILE, "utf8").split("\n")) {
    const t = line.trim();
    if (!t || t.startsWith("#") || !t.includes("=")) continue;
    const key = t.slice(0, t.indexOf("=")).trim();
    let val = t.slice(t.indexOf("=") + 1).trim();
    if ((val.startsWith('"') && val.endsWith('"')) || (val.startsWith("'") && val.endsWith("'"))) val = val.slice(1, -1);
    if (process.env[key] === undefined) process.env[key] = val;
  }
}

const ROOT = process.env.ROOT ?? "squadlegacy.testnet";

function signerKeys(): { sk_b64: string; pk_b64: string } {
  if (process.env.SIGNER_SK_B64 && process.env.SIGNER_PK_B64) {
    return { sk_b64: process.env.SIGNER_SK_B64, pk_b64: process.env.SIGNER_PK_B64 };
  }
  const path = join(POC, ".signer.json");
  if (existsSync(path)) return JSON.parse(readFileSync(path, "utf8"));
  throw new Error("signer key missing: set SIGNER_SK_B64 + SIGNER_PK_B64, or provide .signer.json");
}

function gameapiKey(account: string): string {
  if (process.env.GAMEAPI_KEY) return process.env.GAMEAPI_KEY;
  const path = process.env.GAMEAPI_KEY_FILE ?? join(homedir(), ".near-credentials", "testnet", `${account}.json`);
  if (existsSync(path)) return JSON.parse(readFileSync(path, "utf8")).private_key;
  throw new Error("gameapi key missing: set GAMEAPI_KEY, or provide the credentials file");
}

function databaseUrl(): string {
  if (process.env.DATABASE_URL) return process.env.DATABASE_URL;
  const ref = (process.env.NEXT_PUBLIC_SUPABASE_URL ?? "").replace(/^https?:\/\//, "").replace(/\.supabase\.co.*/, "");
  const password = process.env.SUPABASE_PASSWORD;
  const host = process.env.SUPABASE_POOLER_HOST ?? "aws-0-ap-southeast-2.pooler.supabase.com";
  if (!ref || !password) throw new Error("no DATABASE_URL and cannot derive one from SUPABASE_* env");
  return `postgresql://postgres.${ref}:${encodeURIComponent(password)}@${host}:5432/postgres`;
}

const s = signerKeys();
const gameapiAccount = process.env.GAMEAPI_ACCOUNT ?? `gameapi.${ROOT}`;

export const CONFIG = {
  skB64: s.sk_b64,
  pkB64: s.pk_b64,
  assetsContract: process.env.ASSETS_CONTRACT ?? `assets.${ROOT}`,
  coinContract: process.env.COIN_CONTRACT ?? `coin.${ROOT}`,
  chainId: process.env.CHAIN_ID ?? "near:testnet",
  networkId: process.env.NETWORK_ID ?? "testnet",
  rpcUrl: process.env.RPC_URL ?? "https://test.rpc.fastnear.com",
  gameapiAccount,
  gameapiKey: gameapiKey(gameapiAccount),
  databaseUrl: databaseUrl(),
  voucherTtlMs: Number(process.env.VOUCHER_TTL_MS ?? 10 * 60 * 1000),
  missionMinMs: Number(process.env.MISSION_MIN_MS ?? 3000),
  gating: process.env.MISSION_GATING ?? "lenient",
  nxcReward: process.env.NXC_REWARD ?? "500",
  nep413Recipient: process.env.NEP413_RECIPIENT ?? "squadlegacy.testnet",
  objectives: Number(process.env.MISSION_OBJECTIVES ?? 4),
  port: Number(process.env.PORT ?? 8080),
};
