import { createPublicKey, verify as edVerify, createHash } from "node:crypto";
import { CONFIG } from "./config.ts";

const NEP413_TAG = 2147484061;
const B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

function base58Decode(s: string): Buffer {
  let num = 0n;
  for (const ch of s) {
    const i = B58.indexOf(ch);
    if (i < 0) throw new Error("bad base58");
    num = num * 58n + BigInt(i);
  }
  const bytes: number[] = [];
  while (num > 0n) {
    bytes.unshift(Number(num & 0xffn));
    num >>= 8n;
  }
  for (const ch of s) {
    if (ch === "1") bytes.unshift(0);
    else break;
  }
  return Buffer.from(bytes);
}

class Writer {
  private buf: number[] = [];
  u8(n: number): void { this.buf.push(n & 0xff); }
  u32(n: number): void { let x = n >>> 0; for (let i = 0; i < 4; i++) { this.buf.push(x & 0xff); x >>>= 8; } }
  raw(b: Uint8Array): void { for (const x of b) this.buf.push(x); }
  str(s: string): void { const b = Buffer.from(s, "utf8"); this.u32(b.length); this.raw(b); }
  out(): Buffer { return Buffer.from(this.buf); }
}

export interface Nep413Proof {
  accountId: string;
  publicKey: string;
  signature: string;
  message: string;
  nonce: string;
  recipient: string;
  callbackUrl?: string;
}

function ed25519Data(publicKey: string): Buffer {
  const raw = publicKey.startsWith("ed25519:") ? publicKey.slice("ed25519:".length) : publicKey;
  const data = base58Decode(raw);
  if (data.length !== 32) throw new Error("bad ed25519 key length");
  return data;
}

export function verifySignature(p: Nep413Proof): boolean {
  const nonce = Buffer.from(p.nonce, "base64");
  if (nonce.length !== 32) return false;
  const w = new Writer();
  w.u32(NEP413_TAG);
  w.str(p.message);
  w.raw(nonce);
  w.str(p.recipient);
  if (p.callbackUrl == null) w.u8(0);
  else { w.u8(1); w.str(p.callbackUrl); }
  const hash = createHash("sha256").update(w.out()).digest();
  const pub = createPublicKey({
    key: { kty: "OKP", crv: "Ed25519", x: ed25519Data(p.publicKey).toString("base64url") },
    format: "jwk",
  });
  return edVerify(null, hash, pub, Buffer.from(p.signature, "base64"));
}

async function keyBelongsToAccount(accountId: string, publicKey: string): Promise<boolean> {
  const res = await fetch(CONFIG.rpcUrl, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: "k",
      method: "query",
      params: { request_type: "view_access_key", finality: "final", account_id: accountId, public_key: publicKey },
    }),
  });
  const body = await res.json();
  return !body.error && body.result && body.result.permission !== undefined;
}

export async function verifyOwnership(p: Nep413Proof): Promise<{ ok: boolean; reason?: string }> {
  if (!p.accountId || !p.publicKey || !p.signature) return { ok: false, reason: "missing proof fields" };
  if (p.recipient !== CONFIG.nep413Recipient) return { ok: false, reason: "bad recipient" };
  try {
    if (!verifySignature(p)) return { ok: false, reason: "bad signature" };
  } catch (e) {
    return { ok: false, reason: String(e) };
  }
  if (!(await keyBelongsToAccount(p.accountId, p.publicKey))) return { ok: false, reason: "key not on account" };
  return { ok: true };
}
