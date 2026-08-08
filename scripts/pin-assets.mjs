import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = join(HERE, "..");

function loadEnv(path) {
  const out = {};
  for (const line of readFileSync(path, "utf8").split("\n")) {
    const m = line.match(/^\s*(?:export\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)\s*$/);
    if (!m) continue;
    let v = m[2].trim();
    if ((v.startsWith('"') && v.endsWith('"')) || (v.startsWith("'") && v.endsWith("'"))) v = v.slice(1, -1);
    out[m[1]] = v;
  }
  return out;
}

const env = loadEnv(join(ROOT, ".env"));
const JWT = env.PINATA_JWT;
if (!JWT) throw new Error("PINATA_JWT missing in .env");

const packId = (kind, game, cat, item) =>
  ((BigInt(kind) << 60n) | (BigInt(game) << 48n) | (BigInt(cat) << 32n) | BigInt(item)).toString();

const items = [
  { key: "rifle-cell", name: "Rifle Cell", kind: 0, game: 1, cat: 1, item: 1, color: "#f5a623", type_label: "consumable", cat_label: "ammo" },
  { key: "nano-medkit", name: "Nano Medkit", kind: 0, game: 1, cat: 4, item: 1, color: "#7ed321", type_label: "consumable", cat_label: "med" },
  { key: "weapon-mod-fragment", name: "Weapon-Mod Fragment", kind: 2, game: 1, cat: 3, item: 1, color: "#4a90e2", type_label: "module", cat_label: "weapon-mod" },
  { key: "mk1-stability-module", name: "MK-1 Stability Module", kind: 2, game: 1, cat: 3, item: 17, color: "#50e3c2", type_label: "module", cat_label: "weapon-mod" },
  { key: "adaptive-armor-skin", name: "Adaptive Armor Skin", kind: 1, game: 1, cat: 2, item: 1, color: "#bd10e0", type_label: "cosmetic", cat_label: "skin", cosmetic: true },
  { key: "first-restoration-badge", name: "First Restoration Badge", kind: 3, game: 1, cat: 0, item: 1, color: "#f8e71c", type_label: "achievement", cat_label: "none" },
  { key: "hackclaw", name: "Hackclaw", kind: 2, game: 1, cat: 5, item: 1, color: "#d0021b", type_label: "module", cat_label: "weapon" },
];
for (const it of items) it.token_id = packId(it.kind, it.game, it.cat, it.item);

const svg = (it) => `<svg xmlns="http://www.w3.org/2000/svg" width="512" height="512" viewBox="0 0 512 512">
<rect width="512" height="512" fill="#0b1020"/>
<circle cx="256" cy="196" r="120" fill="none" stroke="${it.color}" stroke-width="10" opacity="0.9"/>
<polygon points="256,110 322,220 190,220" fill="${it.color}" opacity="0.85"/>
<text x="256" y="400" fill="#e6ecff" font-family="monospace" font-size="34" text-anchor="middle">${it.name}</text>
<text x="256" y="440" fill="${it.color}" font-family="monospace" font-size="20" text-anchor="middle">Squad Legacy · ${it.type_label}</text>
</svg>`;

async function pinDir(files, label) {
  const form = new FormData();
  for (const f of files) form.append("file", new Blob([f.content]), `${label}/${f.name}`);
  form.append("pinataOptions", JSON.stringify({ cidVersion: 1 }));
  form.append("pinataMetadata", JSON.stringify({ name: `kzr-${label}` }));
  const res = await fetch("https://api.pinata.cloud/pinning/pinFileToIPFS", {
    method: "POST",
    headers: { Authorization: `Bearer ${JWT}` },
    body: form,
  });
  const body = await res.text();
  if (!res.ok) throw new Error(`Pinata ${label} ${res.status}: ${body}`);
  return JSON.parse(body).IpfsHash;
}

async function detectPrefix(cid, name) {
  for (const p of ["", "d/", "art/", "metadata/"]) {
    try {
      const r = await fetch(`https://gateway.pinata.cloud/ipfs/${cid}/${p}${name}`);
      if (r.ok) return p;
    } catch {}
  }
  return "";
}

const artCid = await pinDir(items.map((it) => ({ name: `${it.key}.svg`, content: svg(it) })), "art");
const artPrefix = await detectPrefix(artCid, `${items[0].key}.svg`);
console.log("art dir CID:", artCid, "prefix:", JSON.stringify(artPrefix));

const metaFiles = items.map((it) => {
  const meta = {
    title: it.name,
    description:
      `${it.type_label} · Squad Legacy` +
      (it.cosmetic ? " · No financial value. No resale guarantee." : ""),
    media: `ipfs://${artCid}/${artPrefix}${it.key}.svg`,
    copies: null,
    extra: JSON.stringify({ game: "Squad Legacy", category: it.cat_label, token_id: it.token_id }),
  };
  return { name: `${it.token_id}.json`, content: JSON.stringify(meta, null, 2) };
});
const metaCid = await pinDir(metaFiles, "metadata");
const metaPrefix = await detectPrefix(metaCid, `${items[0].token_id}.json`);
console.log("metadata dir CID:", metaCid, "prefix:", JSON.stringify(metaPrefix));

const baseUri = `ipfs://${metaCid}/${metaPrefix}`;
const manifest = {
  base_uri: baseUri,
  art_cid: artCid,
  metadata_cid: metaCid,
  gateway: env.PINATA_GATEWAY || null,
  items: items.map((it) => ({
    key: it.key,
    name: it.name,
    token_id: it.token_id,
    build_token_id: [it.kind, it.game, it.cat, it.item],
    metadata: `${baseUri}${it.token_id}.json`,
    media: `ipfs://${artCid}/${artPrefix}${it.key}.svg`,
  })),
};
mkdirSync(join(ROOT, "scripts"), { recursive: true });
writeFileSync(join(ROOT, "scripts", "asset-manifest.json"), JSON.stringify(manifest, null, 2));

console.log("\nBASE_URI=" + baseUri);
console.log("wrote scripts/asset-manifest.json");
