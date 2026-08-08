import { INDEXER_URL, CONTRACTS, ROSTER } from "./config";
import { fmtToken } from "./near";

export interface Activity {
  event: string;
  kind: string;
  contract: string;
  token_id: string;
  sign: number;
  amount: string;
  counterparty: string;
  detail: { nxc_in?: string; kzr_out?: string; nxc_refunded?: string } | null;
  block_height: string;
  block_ts: string;
  receipt_id: string;
}

export interface FeedEntry {
  id: string;
  icon: string;
  color: string;
  verb: string;
  rest: string;
  when: string;
}

const OURS = new Set(Object.values(CONTRACTS));
const NAMES = Object.fromEntries(ROSTER.map((r) => [r.token_id, r.name]));
const name = (id: string) => NAMES[id] ?? `#${id.slice(0, 8)}`;
const items = (rows: Activity[]) => rows.map((r) => `${r.amount}× ${name(r.token_id)}`).join(", ");

const TONE: Record<string, string> = {
  primary: "var(--primary)",
  info: "var(--info)",
  ammo: "var(--ammo)",
  mod: "var(--mod)",
  warn: "var(--warn)",
  text: "var(--text)",
};

function ago(tsNs: string): string {
  const ms = Number(tsNs.slice(0, 13));
  const s = Math.max(0, (Date.now() - ms) / 1000);
  if (s < 60) return `${Math.floor(s)}s ago`;
  if (s < 3600) return `${Math.floor(s / 60)}m ago`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
  return `${Math.floor(s / 86400)}d ago`;
}

export async function fetchActivity(account: string, limit = 30): Promise<Activity[]> {
  const r = await fetch(`${INDEXER_URL}/activity?account=${encodeURIComponent(account)}&limit=${limit}`);
  if (!r.ok) throw new Error(`indexer ${r.status}`);
  const j = await r.json();
  return (j.activity ?? []) as Activity[];
}

export function buildFeed(rows: Activity[]): FeedEntry[] {
  const kept = rows.filter(
    (r) =>
      r.contract !== CONTRACTS.token &&
      !((r.kind === "transfer_in" || r.kind === "transfer_out") && OURS.has(r.counterparty)),
  );

  const groups = new Map<string, Activity[]>();
  for (const r of kept) {
    const g = groups.get(r.receipt_id);
    if (g) g.push(r);
    else groups.set(r.receipt_id, [r]);
  }

  const out: FeedEntry[] = [];
  for (const [id, g] of groups) {
    const r0 = g[0];
    const when = ago(r0.block_ts);
    let verb = "";
    let rest = "";
    let icon = "•";
    let tone = "text";

    if (r0.event === "conversion") {
      verb = "Converted";
      rest = `${fmtToken(r0.detail?.nxc_in ?? "0", 18, 0)} NXC → ${fmtToken(r0.detail?.kzr_out ?? "0", 18, 2)} KZR`;
      icon = "⇄";
      tone = "info";
    } else if (r0.event === "conversion_rollback") {
      verb = "Conversion reversed";
      rest = `${fmtToken(r0.detail?.nxc_refunded ?? "0", 18, 0)} NXC refunded`;
      icon = "↺";
      tone = "warn";
    } else if (r0.event === "mt_mint") {
      verb = "Claimed";
      rest = `loot — ${items(g)}`;
      icon = "✦";
      tone = "primary";
    } else if (r0.event === "mt_burn") {
      verb = "Crafted";
      rest = `— consumed ${items(g)}`;
      icon = "⚒";
      tone = "mod";
    } else if (r0.event === "mt_transfer") {
      const incoming = r0.kind === "transfer_in";
      verb = incoming ? "Received" : "Sent";
      rest = `${items(g)} ${incoming ? "from" : "to"} ${r0.counterparty}`;
      icon = "⇅";
      tone = "text";
    } else if (r0.contract === CONTRACTS.coin) {
      const amt = `${fmtToken(r0.amount, 18, 0)} NXC`;
      icon = "◆";
      tone = "ammo";
      if (r0.event === "ft_mint") { verb = "Earned"; rest = amt; }
      else if (r0.kind === "transfer_in") { verb = "Received"; rest = `${amt} from ${r0.counterparty}`; icon = "⇅"; }
      else if (r0.kind === "transfer_out") { verb = "Sent"; rest = `${amt} to ${r0.counterparty}`; icon = "⇅"; }
      else if (r0.event === "ft_burn") { verb = "Burned"; rest = amt; }
    }

    if (verb) out.push({ id, icon, color: TONE[tone], verb, rest, when });
  }
  return out;
}
