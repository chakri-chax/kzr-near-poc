import { CONFIG } from "./config.ts";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export interface Discovered {
  hash: string;
  sender: string;
  receiver: string;
  block_height: number;
  block_ts: string;
}

async function getPage(account: string, order: "asc" | "desc", cursor: string | null): Promise<{ txns: any[]; cursor: string | null }> {
  const u = new URL(`${CONFIG.nearBlocksUrl}/account/${account}/txns`);
  u.searchParams.set("per_page", String(CONFIG.txPageSize));
  u.searchParams.set("order", order);
  if (cursor) u.searchParams.set("cursor", cursor);
  const headers: Record<string, string> = {};
  if (CONFIG.nearBlocksKey) headers.Authorization = `Bearer ${CONFIG.nearBlocksKey}`;

  for (let attempt = 0; attempt < 4; attempt++) {
    const res = await fetch(u, { headers });
    if (res.status === 429) {
      await sleep(1500 * (attempt + 1));
      continue;
    }
    if (!res.ok) throw new Error(`nearblocks ${account}: http ${res.status}`);
    const body = await res.json();
    return { txns: body.txns ?? [], cursor: body.cursor ?? null };
  }
  throw new Error(`nearblocks ${account}: rate limited`);
}

export async function recentTxns(account: string, full: boolean): Promise<Discovered[]> {
  const seen = new Set<string>();
  const out: Discovered[] = [];
  let cursor: string | null = null;
  do {
    const page = await getPage(account, full ? "asc" : "desc", cursor);
    for (const t of page.txns) {
      const hash = t.transaction_hash;
      if (!hash || seen.has(hash)) continue;
      seen.add(hash);
      out.push({
        hash,
        sender: t.predecessor_account_id ?? account,
        receiver: t.receiver_account_id ?? account,
        block_height: Number(t.block?.block_height ?? 0),
        block_ts: String(t.block_timestamp ?? "0"),
      });
    }
    cursor = full ? page.cursor : null;
  } while (cursor);
  return out;
}
