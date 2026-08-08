#!/usr/bin/env bash
set -euo pipefail

ROOT="${ROOT:-squadlegacy.testnet}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
POC="$(cd "$HERE/.." && pwd)"

NET=(network-config testnet)
SEND=(sign-with-legacy-keychain send)
GAS=(prepaid-gas "100.0 Tgas")
CGAS=(prepaid-gas "30.0 Tgas")
YOCTO=(attached-deposit "1 yoctoNEAR")
ZERO=(attached-deposit "0 NEAR")

TOKEN="token.$ROOT"; COIN="coin.$ROOT"; ASSETS="assets.$ROOT"
CONVERT="convert.$ROOT"; GAMEAPI="gameapi.$ROOT"; RELAYER="relayer.$ROOT"

BASE_URI="$(python3 -c "import json;print(json.load(open('$POC/scripts/asset-manifest.json'))['base_uri'])")"
SIGNER_PK="$(python3 -c "import json;print(json.load(open('$POC/.signer.json'))['pk_b64'])")"
echo "ROOT=$ROOT  BASE_URI=$BASE_URI  SIGNER_PK=$SIGNER_PK"

exists() { near account view-account-summary "$1" "${NET[@]}" now >/dev/null 2>&1; }

echo "### 1. sub-accounts (idempotent)"
for pair in "$TOKEN 3" "$COIN 3" "$ASSETS 4" "$CONVERT 3" "$GAMEAPI 1" "$RELAYER 1"; do
  set -- $pair
  if exists "$1"; then echo "  $1 exists"; else
    near account create-account fund-myself "$1" "$2 NEAR" autogenerate-new-keypair \
      save-to-legacy-keychain sign-as "$ROOT" "${NET[@]}" "${SEND[@]}" >/dev/null
    echo "  created $1 ($2 NEAR)"
  fi
done

has_code() { near account view-account-summary "$1" "${NET[@]}" now 2>/dev/null | grep -q "SHA256"; }

echo "### 2. deploy + init"
near contract deploy "$TOKEN" use-file "$POC/target/near/kzr_token/kzr_token.wasm" \
  with-init-call new json-args "{\"owner_id\":\"$ROOT\",\"treasury_id\":\"$ROOT\",\"initial_supply\":\"1000000000000000000000\"}" \
  "${GAS[@]}" "${ZERO[@]}" "${NET[@]}" "${SEND[@]}" >/dev/null && echo "  kzr-token deployed"
near contract deploy "$COIN" use-file "$POC/target/near/ingame_coin/ingame_coin.wasm" \
  with-init-call new json-args "{\"owner_id\":\"$ROOT\"}" \
  "${GAS[@]}" "${ZERO[@]}" "${NET[@]}" "${SEND[@]}" >/dev/null && echo "  ingame-coin deployed"
near contract deploy "$ASSETS" use-file "$POC/target/near/game_assets/game_assets.wasm" \
  with-init-call new json-args "{\"owner_id\":\"$ROOT\",\"signer_public_key\":\"$SIGNER_PK\",\"chain_id\":\"near:testnet\",\"base_uri\":\"$BASE_URI\",\"daily_mint_cap\":\"1000000\"}" \
  "${GAS[@]}" "${ZERO[@]}" "${NET[@]}" "${SEND[@]}" >/dev/null && echo "  game-assets deployed"
near contract deploy "$CONVERT" use-file "$POC/target/near/ingame_conversion/ingame_conversion.wasm" \
  with-init-call new json-args "{\"owner_id\":\"$ROOT\",\"kzr_token\":\"$TOKEN\",\"coin_token\":\"$COIN\",\"rate_num\":\"1\",\"rate_den\":\"100\",\"daily_cap\":\"100000000000000000000\",\"lifetime_cap\":\"1000000000000000000000\"}" \
  "${GAS[@]}" "${ZERO[@]}" "${NET[@]}" "${SEND[@]}" >/dev/null && echo "  ingame-conversion deployed"

call() { # $1 contract  $2 method  $3 json  $4... extra deposit args
  local c="$1" m="$2" j="$3"; shift 3
  near contract call-function as-transaction "$c" "$m" json-args "$j" \
    "${CGAS[@]}" "$@" sign-as "$ROOT" "${NET[@]}" "${SEND[@]}" >/dev/null
}

echo "### 3. wire roles + sink"
call "$TOKEN" add_minter "{\"account_id\":\"$CONVERT\"}" "${YOCTO[@]}" && echo "  convert = KZR minter"
call "$COIN" add_minter "{\"account_id\":\"$GAMEAPI\"}" "${YOCTO[@]}" && echo "  gameapi = NXC minter"
call "$COIN" register_sink "{\"account_id\":\"$CONVERT\"}" "${YOCTO[@]}" && echo "  convert = NXC sink"
call "$COIN" storage_deposit "{\"account_id\":\"$CONVERT\"}" attached-deposit "0.01 NEAR" && echo "  convert registered on coin (NEP-145)"

echo "### 4. fund game-assets storage budget"
call "$ASSETS" storage_top_up "{}" attached-deposit "1.5 NEAR" && echo "  assets storage topped up"

echo "### 5. register token-ids"
python3 -c "
import json
mx={'rifle-cell':10000000,'nano-medkit':10000000,'weapon-mod-fragment':10000000,'mk1-stability-module':100000,'adaptive-armor-skin':100000,'first-restoration-badge':1000000,'hackclaw':100000}
for it in json.load(open('$POC/scripts/asset-manifest.json'))['items']:
    print(it['token_id'], mx[it['key']], it['key'])
" | while read TID MAX KEY; do
  call "$ASSETS" register_token "{\"token_id\":\"$TID\",\"max_supply\":\"$MAX\"}" "${YOCTO[@]}" && echo "  registered $KEY ($TID)"
done

echo "### DONE"
echo "token=$TOKEN coin=$COIN assets=$ASSETS convert=$CONVERT gameapi=$GAMEAPI relayer=$RELAYER"
