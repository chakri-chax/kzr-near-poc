import { CONTRACT_SET } from "./config.ts";
import { txStatus } from "./rpc.ts";
import type { Discovered } from "./nearblocks.ts";

export interface Row {
  receipt_id: string;
  log_index: number;
  account: string;
  token_id: string;
  contract: string;
  standard: string;
  event: string;
  kind: string;
  sign: number;
  amount: string;
  counterparty: string;
  detail: unknown;
  block_height: number;
  block_ts: string;
}

function normalize(
  logs: string[],
  receiptId: string,
  executor: string,
  height: number,
  ts: string,
): Row[] {
  const rows: Row[] = [];
  logs.forEach((line, logIndex) => {
    if (!line.startsWith("EVENT_JSON:")) return;
    let payload: any;
    try {
      payload = JSON.parse(line.slice("EVENT_JSON:".length));
    } catch {
      return;
    }
    const standard: string = payload.standard;
    const event: string = payload.event;
    const data: any[] = Array.isArray(payload.data) ? payload.data : [];
    const base = { receipt_id: receiptId, log_index: logIndex, contract: executor, standard, event, block_height: height, block_ts: ts };

    for (const d of data) {
      if (standard === "nep141") {
        if (event === "ft_mint") rows.push({ ...base, account: d.owner_id, token_id: "", kind: "mint", sign: 1, amount: String(d.amount), counterparty: "", detail: null });
        else if (event === "ft_burn") rows.push({ ...base, account: d.owner_id, token_id: "", kind: "burn", sign: -1, amount: String(d.amount), counterparty: "", detail: null });
        else if (event === "ft_transfer") {
          rows.push(
            { ...base, account: d.old_owner_id, token_id: "", kind: "transfer_out", sign: -1, amount: String(d.amount), counterparty: d.new_owner_id, detail: null },
            { ...base, account: d.new_owner_id, token_id: "", kind: "transfer_in", sign: 1, amount: String(d.amount), counterparty: d.old_owner_id, detail: null },
          );
        }
      } else if (standard === "nep245") {
        const ids: string[] = d.token_ids ?? [];
        const amts: string[] = d.amounts ?? [];
        ids.forEach((tid, k) => {
          const amount = String(amts[k]);
          if (event === "mt_mint") rows.push({ ...base, account: d.owner_id, token_id: tid, kind: "mint", sign: 1, amount, counterparty: "", detail: null });
          else if (event === "mt_burn") rows.push({ ...base, account: d.owner_id, token_id: tid, kind: "burn", sign: -1, amount, counterparty: "", detail: null });
          else if (event === "mt_transfer") {
            rows.push(
              { ...base, account: d.old_owner_id, token_id: tid, kind: "transfer_out", sign: -1, amount, counterparty: d.new_owner_id, detail: null },
              { ...base, account: d.new_owner_id, token_id: tid, kind: "transfer_in", sign: 1, amount, counterparty: d.old_owner_id, detail: null },
            );
          }
        });
      } else if (standard === "kzr_conversion") {
        rows.push({ ...base, account: d.account_id, token_id: "", kind: event, sign: 0, amount: "0", counterparty: "", detail: d });
      }
    }
  });
  return rows;
}

export async function enrich(tx: Discovered): Promise<Row[]> {
  let status;
  try {
    status = await txStatus(tx.hash, tx.sender);
  } catch {
    try {
      status = await txStatus(tx.hash, tx.receiver);
    } catch {
      return [];
    }
  }
  const rows: Row[] = [];
  for (const ro of status.receipts_outcome ?? []) {
    if (!CONTRACT_SET.has(ro.outcome.executor_id)) continue;
    rows.push(...normalize(ro.outcome.logs ?? [], ro.id, ro.outcome.executor_id, tx.block_height, tx.block_ts));
  }
  return rows;
}
