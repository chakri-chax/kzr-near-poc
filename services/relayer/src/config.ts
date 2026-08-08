import { readFileSync, existsSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const ROOT = process.env.ROOT ?? "squadlegacy.testnet";
const relayerId = process.env.RELAYER_ACCOUNT ?? `relayer.${ROOT}`;

function relayerKey(): string {
  if (process.env.RELAYER_KEY) return process.env.RELAYER_KEY;
  const path =
    process.env.RELAYER_KEY_FILE ?? join(homedir(), ".near-credentials", "testnet", `${relayerId}.json`);
  if (existsSync(path)) return JSON.parse(readFileSync(path, "utf8")).private_key;
  throw new Error("relayer key missing: set RELAYER_KEY, or provide the credentials file");
}

export const CONFIG = {
  networkId: "testnet",
  rpcUrl: process.env.RPC_URL ?? "https://test.rpc.fastnear.com",
  relayerId,
  relayerKey: relayerKey(),
  assets: `assets.${ROOT}`,
  gameApiUrl: process.env.GAME_API_URL ?? "http://localhost:8080",
  port: Number(process.env.PORT ?? 8081),
  perAccountPerMin: Number(process.env.RATE_PER_MIN ?? 5),
};
