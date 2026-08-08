import { providers } from "near-api-js";
import { RPC_URL, CONTRACTS } from "./config";

const provider = new providers.JsonRpcProvider({ url: RPC_URL });

export async function view<T>(
  contractId: string,
  method: string,
  args: Record<string, unknown> = {},
): Promise<T> {
  const res = await provider.query<any>({
    request_type: "call_function",
    account_id: contractId,
    method_name: method,
    args_base64: btoa(JSON.stringify(args)),
    finality: "final",
  });
  return JSON.parse(new TextDecoder().decode(Uint8Array.from(res.result)));
}

export const getInventory = (account: string, tokenIds: string[]) =>
  view<string[]>(CONTRACTS.assets, "mt_batch_balance_of", { account_id: account, token_ids: tokenIds });

export const getFtBalance = (contract: string, account: string) =>
  view<string>(contract, "ft_balance_of", { account_id: account });

export const getRate = () => view<[string, string]>(CONTRACTS.convert, "get_rate", {});

export function fmtToken(yocto: string, decimals = 18, dp = 2): string {
  const n = BigInt(yocto);
  const base = 10n ** BigInt(decimals);
  const whole = n / base;
  const frac = ((n % base) * 10n ** BigInt(dp)) / base;
  return `${whole.toString()}.${frac.toString().padStart(dp, "0")}`;
}
