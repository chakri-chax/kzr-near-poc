import { CONFIG } from "./config.ts";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export async function rpc<T = any>(method: string, params: any): Promise<T> {
  let lastErr: unknown;
  for (let attempt = 0; attempt < 4; attempt++) {
    try {
      const res = await fetch(CONFIG.rpcUrl, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ jsonrpc: "2.0", id: "idx", method, params }),
      });
      const body = await res.json();
      if (body.error) throw new Error(`${method}: ${JSON.stringify(body.error).slice(0, 160)}`);
      return body.result as T;
    } catch (e) {
      lastErr = e;
      await sleep(300 * 3 ** attempt);
    }
  }
  throw lastErr;
}

export const txStatus = (hash: string, sender: string) =>
  rpc<{ receipts_outcome: { id: string; outcome: { executor_id: string; logs: string[] } }[] }>(
    "EXPERIMENTAL_tx_status",
    { tx_hash: hash, sender_account_id: sender, wait_until: "FINAL" },
  );
