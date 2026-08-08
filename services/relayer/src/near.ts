import { connect, keyStores, KeyPair, type Account } from "near-api-js";
import { CONFIG } from "./config.ts";

let cached: Account | null = null;

async function relayer(): Promise<Account> {
  if (cached) return cached;
  const keyStore = new keyStores.InMemoryKeyStore();
  await keyStore.setKey(CONFIG.networkId, CONFIG.relayerId, KeyPair.fromString(CONFIG.relayerKey as any));
  const near = await connect({ networkId: CONFIG.networkId, nodeUrl: CONFIG.rpcUrl, keyStore });
  cached = await near.account(CONFIG.relayerId);
  return cached;
}

export async function submitRaw(contractId: string, methodName: string, argsJson: string, gasTgas = 100n) {
  const account = await relayer();
  return account.functionCall({
    contractId,
    methodName,
    args: new TextEncoder().encode(argsJson),
    gas: gasTgas * 10n ** 12n,
    attachedDeposit: 0n,
  });
}
