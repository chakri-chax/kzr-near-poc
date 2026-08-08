import { createHash } from "node:crypto";

export function packTokenId(kind: number, game: number, cat: number, item: number): string {
  return ((BigInt(kind) << 60n) | (BigInt(game) << 48n) | (BigInt(cat) << 32n) | BigInt(item)).toString();
}

export interface LootEntry { token_id: string; amount: string; }

export const MISSIONS: Record<string, LootEntry[]> = {
  "awaken-the-nexus": [
    { token_id: packTokenId(0, 1, 1, 1), amount: "30" },
    { token_id: packTokenId(0, 1, 4, 1), amount: "3" },
    { token_id: packTokenId(2, 1, 3, 1), amount: "2" },
    { token_id: packTokenId(2, 1, 5, 1), amount: "1" },
  ],
};

export function missionHash(missionId: string, accountId: string): Uint8Array {
  return createHash("sha256").update(`${missionId}:${accountId}`).digest();
}
