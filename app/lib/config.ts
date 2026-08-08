export const RPC_URL = process.env.NEXT_PUBLIC_RPC_URL ?? "https://test.rpc.fastnear.com";
const ROOT = process.env.NEXT_PUBLIC_ROOT ?? "squadlegacy.testnet";

export const CONTRACTS = {
  token: `token.${ROOT}`,
  coin: `coin.${ROOT}`,
  assets: `assets.${ROOT}`,
  convert: `convert.${ROOT}`,
};

export const DEMO_ACCOUNT = process.env.NEXT_PUBLIC_DEMO_ACCOUNT ?? "kzr-dev.testnet";
export const EXPLORER = "https://testnet.nearblocks.io/address";
export const RELAYER_URL = process.env.NEXT_PUBLIC_RELAYER_URL ?? "http://localhost:8081";
export const INDEXER_URL = process.env.NEXT_PUBLIC_INDEXER_URL ?? "http://localhost:8082";

export interface Item {
  token_id: string;
  name: string;
  category: string;
  color: string;
  icon: string;
}

export const ROSTER: Item[] = [
  { token_id: "281479271677953", name: "Rifle Cell", category: "Ammo", color: "var(--ammo)", icon: "i-ammo" },
  { token_id: "281492156579841", name: "Nano Medkit", category: "Med", color: "var(--med)", icon: "i-med" },
  { token_id: "2306124497075306497", name: "Weapon-Mod Fragment", category: "Weapon-mod", color: "var(--mod)", icon: "i-mod" },
  { token_id: "2306124497075306513", name: "MK-1 Stability Module", category: "Module", color: "var(--mod)", icon: "i-mod" },
  { token_id: "2306124505665241089", name: "Hackclaw", category: "Weapon", color: "var(--weapon)", icon: "i-wpn" },
  { token_id: "1153202988173492225", name: "Adaptive Armor Skin", category: "Cosmetic", color: "var(--skin)", icon: "i-skin" },
  { token_id: "3459045988797251585", name: "First Restoration Badge", category: "Achievement", color: "var(--ach)", icon: "i-ach" },
];
