import { createServer } from "node:http";
import { CONFIG } from "./config.ts";
import { activity, inventory, balances, stats } from "./db.ts";
import { startIndexer } from "./indexer.ts";

function send(res: any, code: number, body: unknown) {
  res.statusCode = code;
  res.end(JSON.stringify(body));
}

createServer(async (req, res) => {
  res.setHeader("content-type", "application/json");
  res.setHeader("access-control-allow-origin", "*");
  res.setHeader("access-control-allow-headers", "content-type");
  if (req.method === "OPTIONS") { res.statusCode = 204; res.end(); return; }

  const url = new URL(req.url ?? "/", "http://x");
  const account = url.searchParams.get("account") ?? "";
  try {
    if (url.pathname === "/health") return send(res, 200, { ok: true, ...(await stats()) });
    if (url.pathname === "/activity") {
      if (!account) return send(res, 400, { error: "account required" });
      const limit = Math.min(Number(url.searchParams.get("limit") ?? 25), 100);
      return send(res, 200, { account, activity: await activity(account, limit) });
    }
    if (url.pathname === "/inventory") {
      if (!account) return send(res, 400, { error: "account required" });
      return send(res, 200, { account, inventory: await inventory(account) });
    }
    if (url.pathname === "/balances") {
      if (!account) return send(res, 400, { error: "account required" });
      return send(res, 200, { account, ...(await balances(account)) });
    }
    return send(res, 404, { error: "not found" });
  } catch (e) {
    return send(res, 500, { error: String(e).slice(0, 200) });
  }
}).listen(CONFIG.port, () => process.stdout.write(`indexer api on :${CONFIG.port}\n`));

startIndexer().catch((e) => {
  process.stderr.write(`indexer failed to start: ${String(e)}\n`);
  process.exit(1);
});
