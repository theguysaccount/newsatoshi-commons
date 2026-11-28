import assert from "node:assert/strict";
import fs from "node:fs";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { createAutoPayFetch, generatePayWordChain } from "./newsatoshi-402.mjs";
import { createPaywallServer } from "./bin/newsatoshi-paywall.mjs";

function listen(srv) {
  return new Promise((resolve) => srv.listen(0, "127.0.0.1", () => resolve(srv.address().port)));
}

test("newsatoshi-paywall gates any upstream API via HTTP 402 PayWords, blocks replays, persists state, and emits claim + FairCurve commands", async () => {
  const upstream = http.createServer((req, res) => {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ ok: true, upstreamPath: req.url, method: req.method }));
  });
  const upPort = await listen(upstream);

  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "ns-paywall-test-"));
  const stateFile = path.join(tmpDir, "state.json");
  const payTo = "0x9f4b3214536f326b150e236c0f731b0e18a95e7b";
  const fairCurve = "0xC3098ba25E39Effa63dE94EbACE6381781ad4cEd";

  const { server: pw1 } = createPaywallServer({
    upstream: `http://127.0.0.1:${upPort}`,
    payTo,
    chain: "base",
    priceUsdc: 0.001,
    sandbox: true,
    stateFile,
    fairCurve,
  });
  const pwPort = await listen(pw1);
  const baseUrl = `http://127.0.0.1:${pwPort}`;

  try {
    // 1. Free /health bypasses paywall
    const rHealth = await fetch(`${baseUrl}/health`);
    assert.equal(rHealth.status, 200);

    // 2. Unpaid call returns standard x402 v2 batch-settlement HTTP 402
    const r402 = await fetch(`${baseUrl}/v1/inference`, { method: "POST" });
    assert.equal(r402.status, 402);
    const j402 = await r402.json();
    assert.equal(j402.x402Version, 2);
    assert.equal(j402.scheme, "batch-settlement");
    assert.equal(j402.binding, "paywords-sha256");
    assert.equal(j402.network, "eip155:8453");
    assert.equal(j402.unitAmount, "1000");

    // 3. Autonomous AI agent pays transparently with 1-line createAutoPayFetch (3 calls)
    const wallet = await generatePayWordChain(10, 1000);
    const channelId = "0x" + "ab".repeat(32);
    const capturedSigs = {};
    const capturingFetch = (url, init = {}) => {
      const h = new Headers(init.headers || {});
      const sig = h.get("PAYMENT-SIGNATURE");
      if (sig) {
        const parsed = JSON.parse(Buffer.from(sig, "base64").toString("utf8"));
        capturedSigs[parsed.index] = sig;
      }
      return fetch(url, init);
    };
    const payFetch = createAutoPayFetch({ wallet, chain: "base", channelId, fetchImpl: capturingFetch });

    for (let step = 1; step <= 3; step++) {
      const rOk = await payFetch(`${baseUrl}/v1/inference?n=${step}`, { method: "POST" });
      assert.equal(rOk.status, 200);
      const receiptHdr = rOk.headers.get("payment-response");
      assert.ok(receiptHdr, "PAYMENT-RESPONSE header missing");
      const receipt = JSON.parse(Buffer.from(receiptHdr, "base64").toString("utf8"));
      assert.equal(receipt.verifiedStep, step);
      const body = await rOk.json();
      assert.equal(body.ok, true);
      assert.equal(body.upstreamPath, `/v1/inference?n=${step}`);
    }

    // 4. Replay of step #2 MUST be rejected HTTP 402
    assert.ok(capturedSigs[2], "captured step #2 signature");
    const rReplay = await fetch(`${baseUrl}/v1/inference`, {
      method: "POST",
      headers: { "PAYMENT-SIGNATURE": capturedSigs[2] },
    });
    assert.equal(rReplay.status, 402);
    assert.equal((await rReplay.json()).error, "replay_rejected_monotonic_cursor");

    // 5. Forged preimage at step #4 MUST be rejected HTTP 402
    const forgedSig = Buffer.from(
      JSON.stringify({
        channelId,
        index: 4,
        tokenHex: "0x" + "00".repeat(32),
        rootHex: "0x" + wallet.tipHex,
        maxSteps: 10,
      })
    ).toString("base64");
    const rForge = await fetch(`${baseUrl}/v1/inference`, {
      method: "POST",
      headers: { "PAYMENT-SIGNATURE": forgedSig },
    });
    assert.equal(rForge.status, 402);
    assert.equal((await rForge.json()).error, "invalid_sha256_payword_proof");

    // 6. Operator status & claim commands include exact claim(3) + injectRevenue(3000)
    const status = await (await fetch(`${baseUrl}/_paywall/status`)).json();
    assert.equal(status.totalVerifiedCalls, 3);
    assert.equal(status.totalEarnedUsdcRaw, 3000);
    const claims = await (await fetch(`${baseUrl}/_paywall/claim-commands`)).json();
    assert.equal(claims.claims.length, 1);
    assert.match(claims.claims[0].claimCommand, /claim\(bytes32,uint32,bytes32\)" 0xabab/);
    assert.match(claims.claims[0].injectRevenueCommands[1], /injectRevenue\(uint256\)" 3000/);

    // 7. Restart proxy loading persisted stateFile -> replay of step #3 STILL rejected, step #4 succeeds!
    pw1.close();
    const { server: pw2 } = createPaywallServer({
      upstream: `http://127.0.0.1:${upPort}`,
      payTo,
      chain: "base",
      priceUsdc: 0.001,
      sandbox: true,
      stateFile,
    });
    const pw2Port = await listen(pw2);
    try {
      assert.ok(capturedSigs[3], "captured step #3 signature");
      const r3 = await fetch(`http://127.0.0.1:${pw2Port}/v1/inference`, {
        method: "POST",
        headers: { "PAYMENT-SIGNATURE": capturedSigs[3] },
      });
      assert.equal(r3.status, 402);

      const r4 = await payFetch(`http://127.0.0.1:${pw2Port}/v1/inference`, { method: "POST" });
      assert.equal(r4.status, 200);
    } finally {
      pw2.close();
    }
  } finally {
    pw1.close();
    upstream.close();
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});
