import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import { CONFIG } from "./config.ts";
import { buildMintArgs, buildCraftArgs } from "./mission.ts";
import { init, getMission, startMission, advanceObjective, markClaimed } from "./db.ts";
import { mintNxc } from "./near.ts";
import { verifyOwnership, type Nep413Proof } from "./nep413.ts";

const now = () => Date.now();

function send(res: ServerResponse, code: number, body: unknown) {
  res.statusCode = code;
  res.end(typeof body === "string" ? body : JSON.stringify(body));
}

function readJson(req: IncomingMessage): Promise<any> {
  return new Promise((resolve, reject) => {
    let body = "";
    req.on("data", (c) => (body += c));
    req.on("end", () => {
      try {
        resolve(JSON.parse(body || "{}"));
      } catch (e) {
        reject(e);
      }
    });
  });
}

createServer(async (req, res) => {
  res.setHeader("content-type", "application/json");
  res.setHeader("access-control-allow-origin", "*");
  res.setHeader("access-control-allow-headers", "content-type");
  if (req.method === "OPTIONS") return send(res, 204, "");

  const url = req.url ?? "";
  if (req.method === "GET" && url === "/health") return send(res, 200, { ok: true });

  try {
    if (req.method === "POST" && url.startsWith("/mission/start")) {
      const { account_id, mission_id = "awaken-the-nexus", proof } = await readJson(req);
      if (!account_id) return send(res, 400, { error: "account_id required" });
      if (!proof) return send(res, 400, { error: "ownership proof required" });
      if ((proof as Nep413Proof).accountId !== account_id) return send(res, 400, { error: "proof/account mismatch" });
      const v = await verifyOwnership(proof as Nep413Proof);
      if (!v.ok) return send(res, 401, { error: `ownership proof failed: ${v.reason}` });
      await startMission(account_id, mission_id, now());
      return send(res, 200, { ok: true, step: 0, objectives: CONFIG.objectives });
    }

    if (req.method === "POST" && url.startsWith("/mission/objective")) {
      const { account_id, mission_id = "awaken-the-nexus", step } = await readJson(req);
      if (!account_id || typeof step !== "number") return send(res, 400, { error: "account_id + step required" });
      try {
        const row = await advanceObjective(account_id, mission_id, step, CONFIG.objectives, now());
        return send(res, 200, { ok: true, step: row.step, status: row.status });
      } catch (e) {
        return send(res, 409, { error: String(e) });
      }
    }

    if (req.method === "POST" && url.startsWith("/mission/complete")) {
      const { account_id, mission_id = "awaken-the-nexus" } = await readJson(req);
      if (!account_id) return send(res, 400, { error: "account_id required" });
      const row = await getMission(account_id, mission_id);
      const serverConfirmed = row && row.proven && (row.status === "complete" || row.status === "claimed");
      if (serverConfirmed) {
        if (now() - Number(row!.started_at) < CONFIG.missionMinMs) return send(res, 425, { error: "mission completed too fast" });
        if (row!.status === "complete") {
          await mintNxc(account_id, CONFIG.nxcReward);
          await markClaimed(account_id, mission_id, now());
        }
        return send(res, 200, buildMintArgs(account_id, mission_id));
      }
      if (CONFIG.gating === "strict") {
        if (!row) return send(res, 409, { error: "mission not started" });
        if (!row.proven) return send(res, 401, { error: "ownership not proven" });
        return send(res, 409, { error: "mission objectives incomplete" });
      }
      return send(res, 200, buildMintArgs(account_id, mission_id));
    }

    if (req.method === "POST" && url.startsWith("/craft/complete")) {
      const { account_id } = await readJson(req);
      if (!account_id) return send(res, 400, { error: "account_id required" });
      return send(res, 200, buildCraftArgs(account_id));
    }

    return send(res, 404, { error: "not found" });
  } catch (e) {
    return send(res, 500, { error: String(e) });
  }
}).listen(CONFIG.port, () => process.stdout.write(`game-api signer on :${CONFIG.port}\n`));

init()
  .then(() => process.stdout.write("mission-state db ready\n"))
  .catch((e) => process.stderr.write(`db init failed (mission endpoints degraded): ${String(e)}\n`));
