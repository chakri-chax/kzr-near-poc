import { createServer } from "node:http";
import { CONFIG } from "./config.ts";
import { buildMintArgs, buildCraftArgs } from "./mission.ts";

createServer((req, res) => {
  res.setHeader("content-type", "application/json");
  res.setHeader("access-control-allow-origin", "*");
  res.setHeader("access-control-allow-headers", "content-type");
  if (req.method === "OPTIONS") { res.statusCode = 204; res.end(); return; }
  if (req.method === "GET" && req.url === "/health") { res.end(JSON.stringify({ ok: true })); return; }
  const url = req.url ?? "";
  if (req.method === "POST" && (url.startsWith("/mission/complete") || url.startsWith("/craft/complete"))) {
    let body = "";
    req.on("data", (c) => (body += c));
    req.on("end", () => {
      try {
        const { account_id, mission_id } = JSON.parse(body || "{}");
        if (!account_id) { res.statusCode = 400; res.end(JSON.stringify({ error: "account_id required" })); return; }
        const args = url.startsWith("/craft/complete")
          ? buildCraftArgs(account_id)
          : buildMintArgs(account_id, mission_id ?? "awaken-the-nexus");
        res.end(args);
      } catch (e) {
        res.statusCode = 500;
        res.end(JSON.stringify({ error: String(e) }));
      }
    });
    return;
  }
  res.statusCode = 404;
  res.end(JSON.stringify({ error: "not found" }));
}).listen(CONFIG.port, () => process.stdout.write(`game-api signer on :${CONFIG.port}\n`));
