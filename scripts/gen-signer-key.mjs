import { generateKeyPairSync } from "node:crypto";
import { writeFileSync, existsSync, readFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const path = join(ROOT, ".signer.json");

if (existsSync(path)) {
  console.log("SIGNER_PK=" + JSON.parse(readFileSync(path, "utf8")).pk_b64 + " (existing)");
} else {
  const { publicKey, privateKey } = generateKeyPairSync("ed25519");
  const pub = Buffer.from(publicKey.export({ format: "jwk" }).x, "base64url");
  const seed = Buffer.from(privateKey.export({ format: "jwk" }).d, "base64url");
  const out = { pk_b64: pub.toString("base64"), sk_b64: seed.toString("base64") };
  writeFileSync(path, JSON.stringify(out, null, 2));
  console.log("SIGNER_PK=" + out.pk_b64);
}
