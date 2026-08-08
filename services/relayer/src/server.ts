import { createServer } from "node:http";
import { CONFIG } from "./config.ts";
import { submitRaw } from "./near.ts";

const hits = new Map<string, number[]>();
function rateOk(id: string): boolean {
  const now = Date.now();
  const w = (hits.get(id) ?? []).filter((t) => now - t < 60_000);
  if (w.length >= CONFIG.perAccountPerMin) return false;
  w.push(now);
  hits.set(id, w);
  return true;
}

createServer((req, res) => {
  res.setHeader("content-type", "application/json");
  res.setHeader("access-control-allow-origin", "*");
  res.setHeader("access-control-allow-headers", "content-type");
  if (req.method === "OPTIONS") { res.statusCode = 204; res.end(); return; }
  if (req.method === "GET" && req.url === "/health") {
    res.end(JSON.stringify({ ok: true, relayer: CONFIG.relayerId }));
    return;
  }
  if (req.method === "POST" && (req.url ?? "").startsWith("/relay/claim")) {
    let body = "";
    req.on("data", (c) => (body += c));
    req.on("end", async () => {
      try {
        const { account_id, mission_id = "awaken-the-nexus" } = JSON.parse(body || "{}");
        if (!account_id) { res.statusCode = 400; res.end(JSON.stringify({ error: "account_id required" })); return; }
        if (!rateOk(account_id)) { res.statusCode = 429; res.end(JSON.stringify({ error: "rate limited" })); return; }
        const r = await fetch(`${CONFIG.gameApiUrl}/mission/complete`, {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ account_id, mission_id }),
        });
        const argsJson = await r.text();
        if (!r.ok) { res.statusCode = 502; res.end(JSON.stringify({ error: "game-api", detail: argsJson })); return; }
        const outcome: any = await submitRaw(CONFIG.assets, "mint_with_voucher", argsJson, 100n);
        res.end(JSON.stringify({ ok: true, tx: outcome?.transaction?.hash ?? null, receiver: account_id }));
      } catch (e) {
        res.statusCode = 500;
        res.end(JSON.stringify({ error: String(e) }));
      }
    });
    return;
  }
  res.statusCode = 404;
  res.end(JSON.stringify({ error: "not found" }));
}).listen(CONFIG.port, () => process.stdout.write(`relayer on :${CONFIG.port} as ${CONFIG.relayerId}\n`));
