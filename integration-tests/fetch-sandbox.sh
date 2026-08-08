#!/usr/bin/env bash
set -euo pipefail

VER="2.13.3"
DEST="${1:-$HOME/.near-sandbox/$VER}"
URL="https://s3-us-west-1.amazonaws.com/build.nearprotocol.com/nearcore/Linux-x86_64/${VER}/near-sandbox.tar.gz"

mkdir -p "$DEST"
curl -fsSL "$URL" | tar -xz -C "$DEST"
BIN="$DEST/Linux-x86_64/near-sandbox"
chmod +x "$BIN"

echo "sandbox ready: $("$BIN" --version 2>&1 | head -1)"
echo "export NEAR_SANDBOX_BIN_PATH=$BIN"
