import { createPrivateKey, createPublicKey, sign as edSign, verify as edVerify } from "node:crypto";

export interface MintVoucher {
  contract_id: string;
  chain_id: string;
  receiver_id: string;
  token_ids: string[];
  amounts: string[];
  nonce: bigint;
  expires_at_ns: bigint;
  mission_hash: Uint8Array;
}

class BorshWriter {
  private buf: number[] = [];
  u32(n: number): void {
    let x = n >>> 0;
    for (let i = 0; i < 4; i++) { this.buf.push(x & 0xff); x >>>= 8; }
  }
  u64(n: bigint): void { let x = n; for (let i = 0; i < 8; i++) { this.buf.push(Number(x & 0xffn)); x >>= 8n; } }
  u128(n: bigint): void { let x = n; for (let i = 0; i < 16; i++) { this.buf.push(Number(x & 0xffn)); x >>= 8n; } }
  raw(b: Uint8Array): void { for (const x of b) this.buf.push(x); }
  str(s: string): void { const b = Buffer.from(s, "utf8"); this.u32(b.length); this.raw(b); }
  out(): Buffer { return Buffer.from(this.buf); }
}

export function serializeVoucher(v: MintVoucher): Buffer {
  const w = new BorshWriter();
  w.str(v.contract_id);
  w.str(v.chain_id);
  w.str(v.receiver_id);
  w.u32(v.token_ids.length);
  for (const t of v.token_ids) w.str(t);
  w.u32(v.amounts.length);
  for (const a of v.amounts) w.u128(BigInt(a));
  w.u64(v.nonce);
  w.u64(v.expires_at_ns);
  if (v.mission_hash.length !== 32) throw new Error("mission_hash must be 32 bytes");
  w.raw(v.mission_hash);
  return w.out();
}

const toB64Url = (b64: string): string => Buffer.from(b64, "base64").toString("base64url");

export function signVoucher(v: MintVoucher, skB64: string, pkB64: string): string {
  const priv = createPrivateKey({
    key: { kty: "OKP", crv: "Ed25519", d: toB64Url(skB64), x: toB64Url(pkB64) },
    format: "jwk",
  });
  return edSign(null, serializeVoucher(v), priv).toString("base64");
}

export function verifyVoucher(v: MintVoucher, sigB64: string, pkB64: string): boolean {
  const pub = createPublicKey({ key: { kty: "OKP", crv: "Ed25519", x: toB64Url(pkB64) }, format: "jwk" });
  return edVerify(null, serializeVoucher(v), pub, Buffer.from(sigB64, "base64"));
}

export function voucherToJson(v: MintVoucher): string {
  const q = JSON.stringify;
  return "{" +
    `"contract_id":${q(v.contract_id)},` +
    `"chain_id":${q(v.chain_id)},` +
    `"receiver_id":${q(v.receiver_id)},` +
    `"token_ids":${q(v.token_ids)},` +
    `"amounts":${q(v.amounts)},` +
    `"nonce":${v.nonce.toString()},` +
    `"expires_at_ns":${v.expires_at_ns.toString()},` +
    `"mission_hash":[${Array.from(v.mission_hash).join(",")}]` +
    "}";
}
