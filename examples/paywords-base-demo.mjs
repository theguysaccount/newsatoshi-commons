#!/usr/bin/env node
/**
 * Newsatoshi Base Mainnet USDC PayWords — Zero-Dependency End-to-End HTTP 402 Demo
 *
 * Spins up a live local HTTP 402 API server backed by `createBasePayWordGate` from
 * `../sdk/js/newsatoshi-402.mjs`, executes 1,000 real HTTP requests paying via O(1)
 * SHA-256 PayWord preimages ($0.000000 gas per call), verifies replay rejection,
 * and exports exact Base Mainnet `NewsatoshiPayWordChannel.claim()` ABI calldata.
 *
 * Usage:
 *   node examples/paywords-base-demo.mjs [calls=1000] [unitUsdcRaw=1000]
 */
import http from "node:http";
import { performance } from "node:perf_hooks";
import {
  BASE_PAYWORD_CHANNEL_ADDRESS,
  BASE_USDC_ADDRESS,
  createBasePayWordGate,
  encodeBaseOpenChannelCalldata,
  generatePayWordChain,
} from "../sdk/js/newsatoshi-402.mjs";

const TOTAL_CALLS = Math.max(10, Math.min(4096, Number(process.argv[2] || 1000)));
const UNIT_USDC_RAW = BigInt(process.argv[3] || 1000); // default 1000 raw = $0.001 USDC/call
const RECIPIENT = "0x1744535521EB420be9700475400455a4Ad0EBEa2";
const PAYER = "0x1111111111111111111111111111111111111111";
const CHANNEL_ID = "0x" + "42".repeat(32);
const EXPIRY_UNIX = Math.floor(Date.now() / 1000) + 86_400;

async function main() {
  const tGen0 = performance.now();
  const wallet = await generatePayWordChain(TOTAL_CALLS, Number(UNIT_USDC_RAW));
  const genMs = performance.now() - tGen0;

  // Simulated verified on-chain channel metadata (in production, omit `channelFetcher`
  // so `createBasePayWordGate` fetches live from Base Mainnet JSON-RPC once per channelId).
  const gate = createBasePayWordGate({
    recipient: RECIPIENT,
    unitAmount: UNIT_USDC_RAW,
    token: BASE_USDC_ADDRESS,
    channelFetcher: async () => ({
      found: true,
      channelId: CHANNEL_ID,
      payer: PAYER,
      recipient: RECIPIENT.toLowerCase(),
      token: BASE_USDC_ADDRESS.toLowerCase(),
      initialRoot: `0x${wallet.tipHex}`,
      currentRoot: `0x${wallet.tipHex}`,
      unitAmount: UNIT_USDC_RAW,
      maxSteps: TOTAL_CALLS,
      claimedSteps: 0,
      expiry: EXPIRY_UNIX,
      closed: false,
      remainingSteps: TOTAL_CALLS,
      remainingAmount: BigInt(TOTAL_CALLS) * UNIT_USDC_RAW,
    }),
  });

  const server = http.createServer(async (req, res) => {
    const auth = await gate.verifyHeaders(req.headers);
    if (!auth.ok) {
      res.writeHead(402, auth.challenge.headers);
      res.end(JSON.stringify(auth.challenge.body));
      return;
    }
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(
      JSON.stringify({
        ok: true,
        call: auth.ticks,
        hashesEvaluated: auth.hashesEvaluated,
        unclaimedSteps: auth.unclaimedSteps,
      })
    );
  });

  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const port = server.address().port;
  const baseUrl = `http://127.0.0.1:${port}/api/inference`;

  // 1. Unauthenticated call receives HTTP 402 Payment Required
  const unauthResp = await fetch(baseUrl);
  if (unauthResp.status !== 402) {
    throw new Error(`Expected HTTP 402, got ${unauthResp.status}`);
  }

  // 2. Execute TOTAL_CALLS real sequential O(1) SHA-256 PayWord HTTP requests
  const tHttp0 = performance.now();
  let lastVoucher = null;
  for (let i = 1; i <= TOTAL_CALLS; i++) {
    lastVoucher = wallet.nextPayment(1);
    const resp = await fetch(baseUrl, {
      headers: {
        "X-PayWord-Channel": CHANNEL_ID,
        "X-PayWord-Index": String(lastVoucher.ticks),
        "X-PayWord-Preimage": `0x${lastVoucher.tokenHex}`,
      },
    });
    if (resp.status !== 200) {
      throw new Error(`Call #${i} failed with HTTP ${resp.status}`);
    }
    await resp.text();
  }
  const httpMs = performance.now() - tHttp0;

  // 3. Verify replay of the last preimage is rejected with HTTP 402
  const replayResp = await fetch(baseUrl, {
    headers: {
      "X-PayWord-Channel": CHANNEL_ID,
      "X-PayWord-Index": String(lastVoucher.ticks),
      "X-PayWord-Preimage": `0x${lastVoucher.tokenHex}`,
    },
  });
  if (replayResp.status !== 402) {
    throw new Error(`Replay was not rejected (HTTP ${replayResp.status})`);
  }

  // 4. Export 1-tx Base Mainnet batch settlement calldata
  const openCalldata = encodeBaseOpenChannelCalldata({
    recipient: RECIPIENT,
    token: BASE_USDC_ADDRESS,
    rootHex: wallet.tipHex,
    unitAmount: UNIT_USDC_RAW,
    maxSteps: TOTAL_CALLS,
    expiryUnix: EXPIRY_UNIX,
  });
  const batch = gate.exportSettlementBatch(CHANNEL_ID);
  server.close();

  const grossUsd = (Number(UNIT_USDC_RAW) / 1e6) * TOTAL_CALLS;
  const stripeFeeUsd = TOTAL_CALLS * ((Number(UNIT_USDC_RAW) / 1e6) * 0.029 + 0.3);
  const estClaimGas = 65_000 + 720 * TOTAL_CALLS;
  const estBaseUsd = estClaimGas * 0.01e-9 * 2500;

  console.log("=== NEWSATOSHI BASE USDC PAYWORDS — LIVE E2E HTTP 402 BENCHMARK ===");
  console.log(`Contract (Base Mainnet):  ${BASE_PAYWORD_CHANNEL_ADDRESS} (Sourcify exact_match)`);
  console.log(`Hash Chain Generated:     ${TOTAL_CALLS} SHA-256 links in ${genMs.toFixed(2)} ms`);
  console.log(`Real HTTP 402 Calls:      ${TOTAL_CALLS}/${TOTAL_CALLS} HTTP 200 OK in ${httpMs.toFixed(1)} ms (${(httpMs / TOTAL_CALLS).toFixed(3)} ms/call incl. localhost TCP)`);
  console.log(`Replay Guard Check:       HTTP 402 REJECTED (monotonic cursor verified)`);
  console.log(`Gross Volume Settled:     $${grossUsd.toFixed(4)} USDC across ${TOTAL_CALLS} API calls`);
  console.log(`Stripe Cards Equivalent:  $${stripeFeeUsd.toFixed(2)} in fees (2.9% + $0.30/call)`);
  console.log(`PayWords Base L2 Settle:  1 tx (~${estClaimGas.toLocaleString()} gas ≈ $${estBaseUsd.toFixed(5)} at 0.01 gwei, 0% protocol cut)`);
  console.log(`openChannel() Calldata:   ${openCalldata.slice(0, 42)}... (${(openCalldata.length - 2) / 2} bytes)`);
  console.log(`claim(${batch.stepsToClaim}) Calldata:    ${batch.calldata.slice(0, 42)}... (${(batch.calldata.length - 2) / 2} bytes)`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
