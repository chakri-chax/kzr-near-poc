import { readFileSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ENV_FILE = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..", ".env");
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

function databaseUrl(): string {
  if (process.env.DATABASE_URL) return process.env.DATABASE_URL;
  const ref = (process.env.NEXT_PUBLIC_SUPABASE_URL ?? "").replace(/^https?:\/\//, "").replace(/\.supabase\.co.*/, "");
  const password = process.env.SUPABASE_PASSWORD;
  const host = process.env.SUPABASE_POOLER_HOST ?? "aws-0-ap-southeast-2.pooler.supabase.com";
  if (!ref || !password) throw new Error("no DATABASE_URL and cannot derive one from SUPABASE_* env");
  return `postgresql://postgres.${ref}:${encodeURIComponent(password)}@${host}:5432/postgres`;
}

export const CONTRACTS = {
  token: `token.${ROOT}`,
  coin: `coin.${ROOT}`,
  assets: `assets.${ROOT}`,
  convert: `convert.${ROOT}`,
};

export const CONTRACT_SET = new Set(Object.values(CONTRACTS));

export const CONFIG = {
  rpcUrl: process.env.RPC_URL ?? "https://test.rpc.fastnear.com",
  nearBlocksUrl: process.env.NEARBLOCKS_URL ?? "https://api-testnet.nearblocks.io/v1",
  nearBlocksKey: process.env.NEARBLOCKS_API_KEY ?? "",
  databaseUrl: databaseUrl(),
  accounts: Object.values(CONTRACTS),
  contracts: CONTRACTS,
  pollMs: Number(process.env.POLL_MS ?? 10000),
  txPageSize: Number(process.env.TX_PAGE_SIZE ?? 25),
  port: Number(process.env.PORT ?? 8082),
};
