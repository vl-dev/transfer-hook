#!/usr/bin/env bash
set -euo pipefail

# Configurable parameters (override via env or args)
URL=${URL:-localhost}
TOKEN_PROGRAM_ID=${TOKEN_PROGRAM_ID:-TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb}
TRANSFER_HOOK_PROGRAM_ID=${TRANSFER_HOOK_PROGRAM_ID:-TokenHookExampLe8smaVNrxTBezWTRbEwxwb1Zykrb}
MINT_AMOUNT=${MINT_AMOUNT:-1000000}
TRANSFER_AMOUNT=${TRANSFER_AMOUNT:-100}
RECIPIENT=${1:-${RECIPIENT:-7A679qBPzsHRBoj1P7nypqtkRgQdpAr2oVxbSLi9iBpy}}
MINT=${MINT:-4GFAx7e1QWXWyCPX7HJ2C8u4TpXbKZsDdbP6hnM1hcjy}

command -v spl-token >/dev/null 2>&1 || { echo "spl-token CLI not found in PATH" >&2; exit 1; }

echo "Airdropping ..."
solana airdrop 10  --url $URL

echo "Creating token (program: $TOKEN_PROGRAM_ID, transfer-hook: $TRANSFER_HOOK_PROGRAM_ID) on $URL..."
CREATE_OUT=$(spl-token \
  --program-id "$TOKEN_PROGRAM_ID" \
  create-token \
  --transfer-hook "$TRANSFER_HOOK_PROGRAM_ID" \
  --url "$URL" 2>&1 \
  -- "./mint.json" | tee /dev/stderr)

# Parse mint from the first instruction result
# Prefer the explicit "Address:" line; fallback to the "Creating token <MINT>" line
MINT=$(printf "%s\n" "$CREATE_OUT" | awk '
  BEGIN{mint=""}
  /^Address:/ && mint=="" {mint=$2}
  /^Creating token / && mint=="" {mint=$3}
  END{if(mint!=""){print mint}else{exit 1}}
') || {
  echo "Failed to parse mint from create-token output" >&2
  exit 1
}

echo "Mint: $MINT"

echo "Creating associated token account for payer..."
spl-token create-account "$MINT" --url "$URL"

echo "Minting $MINT_AMOUNT tokens to payer's ATA..."
spl-token mint "$MINT" "$MINT_AMOUNT" --url "$URL"

echo "Building transfer-hook Account Meta"
./target/release/spl-transfer-hook create-extra-metas "$TRANSFER_HOOK_PROGRAM_ID" "$MINT" --url "$URL" ./accounts-config.json 

# echo "Initializing transfer account..."
PROGRAM_ID="$TRANSFER_HOOK_PROGRAM_ID" RPC_URL="http://$URL:8899" npx --yes ts-node scripts/initialize-transfer-account.ts

echo "Transferring $TRANSFER_AMOUNT tokens to $RECIPIENT..."
BLOCKHASH=$(curl -s "http://localhost:8899" -X POST -H "Content-Type: application/json" -d '{"jsonrpc":"2.0","id":1,"method":"getLatestBlockhash"}' | jq -r '.result.value.blockhash')

spl-token transfer "$MINT" "$TRANSFER_AMOUNT" "$RECIPIENT" --url "$URL" --allow-unfunded-recipient --fund-recipient --blockhash "$BLOCKHASH" --mint-decimals 9

echo "Done. Mint: $MINT"

echo "Getting user transfer account..."
PROGRAM_ID="$TRANSFER_HOOK_PROGRAM_ID" RPC_URL="http://$URL:8899" npx --yes ts-node scripts/get-user-transfer-account.ts