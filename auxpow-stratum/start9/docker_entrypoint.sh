#!/bin/sh
set -eu

NS_RPC_AUTH="${NS_RPC_AUTH:-auxpow:$(openssl rand -hex 16)}"
NS_PAYOUT_ADDRESS="${NS_PAYOUT_ADDRESS:-}"
BTC_RPC_URL="${BTC_RPC_URL:-}"
BTC_RPC_AUTH="${BTC_RPC_AUTH:-}"
BTC_PAYOUT_SCRIPT_HEX="${BTC_PAYOUT_SCRIPT_HEX:-51}"

newsatoshi --net mainnet-v2 --datadir /data node \
  --peer newsatoshi-seed-v2.fly.dev:8445@b024aaeafa0e4ca384b99483c28c3bedde45a5461a560063b283766414dfbbcc \
  --peer pool.newsatoshi.org:8445 \
  --rpc 19556 --rpcbind 127.0.0.1 --rpcauth "$NS_RPC_AUTH" \
  --status 8080 --statusbind 127.0.0.1 &

sleep 3

exec python3 /app/stratum_auxpow.py \
  --bind 0.0.0.0 --port 3333 --status-port 3334 \
  --ns-rpc http://127.0.0.1:19556/ --ns-rpc-auth "$NS_RPC_AUTH" \
  --payout-address "$NS_PAYOUT_ADDRESS" \
  --btc-rpc "$BTC_RPC_URL" --btc-rpc-auth "$BTC_RPC_AUTH" \
  --btc-payout-script-hex "$BTC_PAYOUT_SCRIPT_HEX" \
  --coinbase-tag "/start9-btc+love-auxpow/"
