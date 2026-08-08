import pg from "pg";
import { CONFIG } from "./config.ts";

export const pool = new pg.Pool({
  connectionString: CONFIG.databaseUrl,
  ssl: { rejectUnauthorized: false },
  max: 4,
});

export interface MissionRow {
  account_id: string;
  mission_id: string;
  step: number;
  status: string;
  proven: boolean;
  started_at: string;
  updated_at: string;
}

export async function init(): Promise<void> {
  await pool.query(`
    create table if not exists mission_state (
      account_id text not null,
      mission_id text not null,
      step int not null default 0,
      status text not null default 'in_progress',
      proven boolean not null default false,
      started_at bigint not null,
      updated_at bigint not null,
      primary key (account_id, mission_id)
    );
  `);
}

export async function getMission(accountId: string, missionId: string): Promise<MissionRow | null> {
  const r = await pool.query<MissionRow>(
    "select account_id, mission_id, step, status, proven, started_at::text, updated_at::text from mission_state where account_id=$1 and mission_id=$2",
    [accountId, missionId],
  );
  return r.rows[0] ?? null;
}

export async function startMission(accountId: string, missionId: string, nowMs: number): Promise<void> {
  await pool.query(
    `insert into mission_state (account_id, mission_id, step, status, proven, started_at, updated_at)
     values ($1,$2,0,'in_progress',true,$3,$3)
     on conflict (account_id, mission_id) do update set step=0, status='in_progress', proven=true, started_at=$3, updated_at=$3`,
    [accountId, missionId, nowMs],
  );
}

export async function advanceObjective(accountId: string, missionId: string, step: number, objectives: number, nowMs: number): Promise<MissionRow> {
  const row = await getMission(accountId, missionId);
  if (!row) throw new Error("mission not started");
  if (!row.proven) throw new Error("ownership not proven");
  if (row.status === "claimed") throw new Error("mission already claimed");
  if (step !== row.step + 1) throw new Error(`out-of-order objective: expected ${row.step + 1}, got ${step}`);
  if (step < 1 || step > objectives) throw new Error("invalid objective");
  const status = step >= objectives ? "complete" : "in_progress";
  const r = await pool.query<MissionRow>(
    "update mission_state set step=$3, status=$4, updated_at=$5 where account_id=$1 and mission_id=$2 returning account_id, mission_id, step, status, proven, started_at::text, updated_at::text",
    [accountId, missionId, step, status, nowMs],
  );
  return r.rows[0];
}

export async function markClaimed(accountId: string, missionId: string, nowMs: number): Promise<void> {
  await pool.query(
    "update mission_state set status='claimed', updated_at=$3 where account_id=$1 and mission_id=$2",
    [accountId, missionId, nowMs],
  );
}
