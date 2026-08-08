import { buildMintArgs } from "./mission.ts";

const receiver = process.argv[2];
if (!receiver) {
  process.stderr.write("usage: sign-cli <receiver_account_id> [mission_id]\n");
  process.exit(1);
}
process.stdout.write(buildMintArgs(receiver, process.argv[3] ?? "awaken-the-nexus"));
