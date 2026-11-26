#!/usr/bin/env bash
# Independent on-chain AuxPoW pool diversity & real-difficulty gate verifier.
# Reads primary block telemetry directly from an L1 node `/status` JSON endpoint
# and checks whether >= 3 distinct pools are mining at difficulty >= 10,000.
set -euo pipefail

URL="${1:-https://newsatoshi.org/mainnet/status}"
MIN_DIFF="${MIN_DIFF:-10000}"

RAW="$(curl -fsS "$URL")"
node -e '
const data = JSON.parse(process.argv[1]);
const minDiff = Number(process.argv[2]);
const diff = Number(data.difficulty ?? 0);
const height = Number(data.height ?? 0);
const recent = Array.isArray(data.recent) ? data.recent : [];
const auxBlocks = recent.filter((b) => b.merge_mined);
const tags = new Set();
for (const b of auxBlocks) {
  if (b.coinbase_tag && typeof b.coinbase_tag === "string" && b.coinbase_tag.trim()) {
    tags.add(b.coinbase_tag.trim());
  }
}
const diffOk = diff >= minDiff;
const poolsOk = diffOk && tags.size >= 3;
console.log(JSON.stringify({
  ok: true,
  endpoint: process.argv[3],
  height,
  difficulty: diff,
  min_difficulty_threshold: minDiff,
  difficulty_gate_passed: diffOk,
  recent_auxpow_blocks: auxBlocks.length,
  distinct_coinbase_tags_observed: Array.from(tags),
  gate3_ready: poolsOk,
  verdict: poolsOk
    ? "GATE_3_READY: >=3 independent pools at Bitcoin-grade difficulty"
    : `GATE_3_BLOCKED: difficulty=${diff.toFixed(2)} (need >=${minDiff}), distinct_tags=${tags.size} (need >=3)`
}, null, 2));
process.exit(poolsOk ? 0 : 1);
' "$RAW" "$MIN_DIFF" "$URL"
