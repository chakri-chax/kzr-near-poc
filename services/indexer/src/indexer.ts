import { CONFIG } from "./config.ts";
import { recentTxns } from "./nearblocks.ts";
import { enrich } from "./scan.ts";
import { init, getSource, setSource, insertRows } from "./db.ts";

const log = (m: string) => process.stdout.write(`[indexer] ${m}\n`);
const enriched = new Set<string>();

async function pollAccount(account: string): Promise<void> {
  const cursor = await getSource(account);
  const full = cursor === null;
  const txns = await recentTxns(account, full);
  if (txns.length === 0) {
    if (full) await setSource(account, 0);
    return;
  }
  const maxHeight = txns.reduce((m, t) => Math.max(m, t.block_height), cursor ?? 0);
  const fresh = txns.filter((t) => (full || t.block_height > cursor) && !enriched.has(t.hash));

  const rows = [];
  for (const t of fresh) {
    enriched.add(t.hash);
    rows.push(...(await enrich(t)));
  }
  const inserted = await insertRows(rows);
  await setSource(account, maxHeight);
  if (inserted > 0 || full) log(`${account}: +${inserted} rows from ${fresh.length} new txns (head ${maxHeight})`);
}

let rr = 0;
async function loop(): Promise<void> {
  const account = CONFIG.accounts[rr % CONFIG.accounts.length];
  rr += 1;
  try {
    await pollAccount(account);
  } catch (e) {
    log(`poll ${account} error: ${String(e).slice(0, 160)}`);
  }
  setTimeout(loop, CONFIG.pollMs);
}

export async function startIndexer(): Promise<void> {
  await init();
  log(`tracking ${CONFIG.accounts.join(", ")} via ${CONFIG.nearBlocksUrl} (poll ${CONFIG.pollMs}ms round-robin)`);
  loop();
}
