import pg from "pg";
import { CONFIG, CONTRACTS } from "./config.ts";
import type { Row } from "./scan.ts";

export const pool = new pg.Pool({
  connectionString: CONFIG.databaseUrl,
  ssl: { rejectUnauthorized: false },
  max: 4,
});

export async function init(): Promise<void> {
  await pool.query(`
    create table if not exists idx_source (account text primary key, last_height bigint not null);
    create table if not exists idx_event (
      receipt_id text not null,
      log_index int not null,
      account text not null,
      token_id text not null default '',
      contract text not null,
      standard text not null,
      event text not null,
      kind text not null,
      sign int not null,
      amount numeric not null default 0,
      counterparty text not null default '',
      detail jsonb,
      block_height bigint not null,
      block_ts numeric not null,
      primary key (receipt_id, log_index, account, token_id)
    );
    create index if not exists idx_event_account on idx_event (account, block_height desc, log_index desc);
    create index if not exists idx_event_bal on idx_event (account, contract, token_id);
  `);
}

export async function getSource(account: string): Promise<number | null> {
  const r = await pool.query<{ last_height: string }>("select last_height from idx_source where account=$1", [account]);
  return r.rows.length ? Number(r.rows[0].last_height) : null;
}

export async function setSource(account: string, height: number): Promise<void> {
  await pool.query(
    "insert into idx_source (account, last_height) values ($1,$2) on conflict (account) do update set last_height=greatest(idx_source.last_height,$2)",
    [account, height],
  );
}

export async function insertRows(rows: Row[]): Promise<number> {
  if (rows.length === 0) return 0;
  const cols = ["receipt_id", "log_index", "account", "token_id", "contract", "standard", "event", "kind", "sign", "amount", "counterparty", "detail", "block_height", "block_ts"];
  const values: unknown[] = [];
  const tuples = rows.map((r, i) => {
    const b = i * cols.length;
    values.push(r.receipt_id, r.log_index, r.account, r.token_id, r.contract, r.standard, r.event, r.kind, r.sign, r.amount, r.counterparty, r.detail === null ? null : JSON.stringify(r.detail), r.block_height, r.block_ts);
    const placeholders = cols.map((_, k) => "$" + (b + k + 1)).join(",");
    return "(" + placeholders + ")";
  });
  const res = await pool.query(
    `insert into idx_event (${cols.join(",")}) values ${tuples.join(",")} on conflict do nothing`,
    values,
  );
  return res.rowCount ?? 0;
}

export async function activity(account: string, limit: number) {
  const r = await pool.query(
    `select event, kind, contract, token_id, sign, amount::text as amount, counterparty, detail, block_height::text as block_height, block_ts::text as block_ts, receipt_id
     from idx_event where account=$1 order by block_height desc, log_index desc limit $2`,
    [account, limit],
  );
  return r.rows;
}

export async function inventory(account: string) {
  const r = await pool.query(
    `select token_id, sum(sign*amount)::text as balance from idx_event
     where account=$1 and contract=$2 and token_id<>'' group by token_id having sum(sign*amount)>0`,
    [account, CONTRACTS.assets],
  );
  return r.rows;
}

export async function balances(account: string) {
  const r = await pool.query(
    `select contract, sum(sign*amount)::text as balance from idx_event
     where account=$1 and token_id='' and contract=any($2) group by contract`,
    [account, [CONTRACTS.token, CONTRACTS.coin]],
  );
  const out: Record<string, string> = { kzr: "0", nxc: "0" };
  for (const row of r.rows) {
    if (row.contract === CONTRACTS.token) out.kzr = row.balance;
    else if (row.contract === CONTRACTS.coin) out.nxc = row.balance;
  }
  return out;
}

export async function stats() {
  const r = await pool.query(
    "select count(*)::int as events, coalesce(max(block_height),0)::text as head, count(distinct account)::int as accounts from idx_event",
  );
  return r.rows[0];
}
