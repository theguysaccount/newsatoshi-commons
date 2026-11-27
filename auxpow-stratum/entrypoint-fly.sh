#!/usr/bin/env sh
set -eu

mkdir -p /data/v2

# Ephemeral local-only loopback RPC credential (never exposed outside container)
RPC_PASS=$(openssl rand -hex 24)
RPC_AUTH="auxpow:${RPC_PASS}"

P2P_VK=$(/usr/local/bin/newsatoshi --net "${NS_NETWORK:-mainnet-v2}" --datadir /data/v2 p2p-key)
echo "[newsatoshi-pool] Mainnet v2 (nsM2) NSPQ2 SLH-DSA p2p-key: ${P2P_VK}"
echo "[newsatoshi-pool] starting Newsatoshi Mainnet v2 node (datadir=/data/v2, p2p=:8445, rpc=127.0.0.1:19556)..."
/usr/local/bin/newsatoshi \
  --net "${NS_NETWORK:-mainnet-v2}" \
  --datadir /data/v2 \
  node \
  --port 8445 \
  --external "${PUBLIC_STRATUM_HOST:-pool.newsatoshi.org}:8445" \
  --peer newsatoshi-seed-v2.internal:8445 \
  --rpc 19556 \
  --rpcbind 127.0.0.1 \
  --rpcauth "${RPC_AUTH}" \
  --status 8081 \
  --statusbind 127.0.0.1 &

NODE_PID=$!

cleanup() {
  echo "[newsatoshi-pool] shutting down..."
  kill "${NODE_PID}" 2>/dev/null || true
}
trap cleanup TERM INT

sleep 2

echo "[newsatoshi-pool] starting zero-fee direct-payout AuxPoW Stratum v1 on :3333 & status HTTP on :8080..."
exec python3 -u /app/stratum_auxpow.py \
  --bind 0.0.0.0 \
  --stratum-port 3333 \
  --status-port 8080 \
  --ns-rpc-url "http://127.0.0.1:19556/" \
  --ns-rpc-auth "${RPC_AUTH}" \
  --public-host "${PUBLIC_STRATUM_HOST:-pool.newsatoshi.org}" \
  --tag "${COINBASE_TAG:-/pool.newsatoshi.org/}" \
  --poll-interval "${POLL_INTERVAL:-2.0}"
