import { connect, keyStores, KeyPair, type Account } from "near-api-js";
import { CONFIG } from "./config.ts";

let cached: Account | null = null;

async function gameapi(): Promise<Account> {
  if (cached) return cached;
  const keyStore = new keyStores.InMemoryKeyStore();
  await keyStore.setKey(CONFIG.networkId, CONFIG.gameapiAccount, KeyPair.fromString(CONFIG.gameapiKey as any));
  const near = await connect({ networkId: CONFIG.networkId, nodeUrl: CONFIG.rpcUrl, keyStore });
  cached = await near.account(CONFIG.gameapiAccount);
  return cached;
}

export async function mintNxc(accountId: string, wholeNxc: string): Promise<string> {
  const amount = (BigInt(wholeNxc) * 10n ** 18n).toString();
  const account = await gameapi();
  const outcome = await account.functionCall({
    contractId: CONFIG.coinContract,
    methodName: "mint",
    args: { account_id: accountId, amount },
    gas: 30n * 10n ** 12n,
    attachedDeposit: 0n,
  });
  return outcome?.transaction?.hash ?? "";
}
