import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import {
  MAX_PAYWORD_TICKS,
  generatePayWordChain,
  verifyPayWordToken,
  createPayWordGate,
  createFilePayWordStore,
  verifyL1LockFunding,
  verifyL1ChannelFunding,
  verifyChannelMerkleProof,
  encodeChannelClaimTx,
  encodeChannelExpireTx,
  submitL1ChannelClaim,
  fetchWithNewsatoshi402,
} from "./newsatoshi-402.mjs";
import { forwardJsonRpcLine } from "./bin/newsatoshi-mcp.mjs";

test("PayWord O(1) incremental verification, forgery rejection, replay guard, and honest unfunded_demo tagging", async () => {
  assert.equal(MAX_PAYWORD_TICKS, 1_000_000);
  const wallet = await generatePayWordChain(20, 2_500);
  assert.equal(wallet.tipHex.length, 64);
  assert.equal(wallet.remainingTicks, 20);

  const p1 = wallet.nextPayment(1);
  const ok1 = await verifyPayWordToken(p1.tipHex, p1.tokenHex, p1.ticks, 2_500);
  assert.equal(ok1.valid, true);
  assert.equal(ok1.hashesEvaluated, 1);
  assert.equal(ok1.totalPaidBaseUnits, 2_500);

  // Forged token rejected
  const forged = await verifyPayWordToken(p1.tipHex, "00".repeat(32), 1, 2_500);
  assert.equal(forged.valid, false);

  // Wrong tick count rejected
  const wrongTicks = await verifyPayWordToken(p1.tipHex, p1.tokenHex, 2, 2_500);
  assert.equal(wrongTicks.valid, false);

  // Explicit unfunded sandbox mode labels receipts honestly (`collectibleOnChain: false`)
  const gate = createPayWordGate({
    unitPriceBaseUnits: 2_500,
    amountLove: "0.000025",
    allowUnfundedDemo: true,
  });
  const server = http.createServer(async (req, res) => {
    const hdr = req.headers["x-newsatoshi-payword"];
    const check = await gate.verifyRequest(typeof hdr === "string" ? hdr : "", req.url || "/infer");
    if (!check.paid) {
      res.writeHead(402, {
        "Content-Type": "application/json",
        "WWW-Authenticate": check.challenge.wwwAuthenticate,
      });
      res.end(JSON.stringify(check.challenge.body));
      return;
    }
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify(check));
  });

  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { port } = server.address();
  const url = `http://127.0.0.1:${port}/infer`;

  try {
    const r1 = await fetchWithNewsatoshi402(url, wallet);
    assert.equal(r1.status, 200);
    const body1 = await r1.json();
    assert.equal(body1.paid, true);
    assert.equal(body1.collectibleOnChain, false);
    assert.equal(body1.fundingStatus, "unfunded_demo");
    assert.equal(body1.ticks, 2);

    // Replay MUST fail with 402
    const replay = await fetch(url, {
      headers: { "X-Newsatoshi-PayWord": p1.headerValue },
    });
    assert.equal(replay.status, 402);

    // Next sequential call verifies incrementally in exactly 1 hash (O(1))
    const r2 = await fetchWithNewsatoshi402(url, wallet);
    assert.equal(r2.status, 200);
    const body2 = await r2.json();
    assert.equal(body2.ticks, 3);
    assert.equal(body2.verificationMode, "incremental");
    assert.equal(body2.hashesEvaluated, 1);
    assert.equal(body2.deltaPaidBaseUnits, 2_500);
  } finally {
    server.close();
  }
});

test("O(1) incremental verification scales cleanly to 1,000,000 ticks with 1 hash per step", async () => {
  const wallet = await generatePayWordChain(1_000_000, 1);
  // Jump to voucher #10,000 in one batch, then advance incrementally by 1
  const v10k = wallet.nextPayment(10_000);
  const res10k = await verifyPayWordToken(v10k.tipHex, v10k.tokenHex, v10k.ticks, 1);
  assert.equal(res10k.valid, true);
  assert.equal(res10k.hashesEvaluated, 10_000);

  const v10001 = wallet.nextPayment(1);
  const res10001 = await verifyPayWordToken(v10001.tipHex, v10001.tokenHex, v10001.ticks, 1, {
    tokenHex: v10k.tokenHex,
    ticks: v10k.ticks,
  });
  assert.equal(res10001.valid, true);
  assert.equal(res10001.mode, "incremental");
  assert.equal(res10001.hashesEvaluated, 1);

  // Advance across the entire 1,000,000-tick chain in steps <= 100,000 and prove final voucher #1,000,000 verifies in 1 hash
  let prev = { tokenHex: v10001.tokenHex, ticks: v10001.ticks };
  while (wallet.remainingTicks > 1) {
    const step = Math.min(99_999, wallet.remainingTicks - 1);
    const nxt = wallet.nextPayment(step);
    const chk = await verifyPayWordToken(nxt.tipHex, nxt.tokenHex, nxt.ticks, 1, prev);
    assert.equal(chk.valid, true);
    prev = { tokenHex: nxt.tokenHex, ticks: nxt.ticks };
  }
  const v1M = wallet.nextPayment(1);
  assert.equal(v1M.ticks, 1_000_000);
  const chk1M = await verifyPayWordToken(v1M.tipHex, v1M.tokenHex, v1M.ticks, 1, prev);
  assert.equal(chk1M.valid, true);
  assert.equal(chk1M.mode, "incremental");
  assert.equal(chk1M.hashesEvaluated, 1);
});

test("Production gate with requireFunding rejects unfunded channels, verifies live L1 locks, and persists watermarks across restarts", async () => {
  const wallet = await generatePayWordChain(10, 1_000);
  const merchant = "ns1merchant00000000000000000000000000000000000000000000000000000000";

  // 1. Mock L1 seed with NO open lock -> MUST reject
  const mockEmptyFetch = async (u) => ({
    ok: true,
    status: 200,
    json: async () => (u.includes("/status") ? { height: 5000 } : { open: [] }),
  });
  const unfunded = await verifyL1LockFunding({
    tipHex: wallet.tipHex,
    merchantAddress: merchant,
    requiredBaseUnits: 5_000,
    fetchImpl: mockEmptyFetch,
  });
  assert.equal(unfunded.funded, false);

  // 2. Mock L1 seed WITH funded open lock -> MUST accept
  const mockFundedFetch = async (u) => ({
    ok: true,
    status: 200,
    json: async () =>
      u.includes("/status")
        ? { height: 5000 }
        : { open: [{ id: "lock1", amount: 50_000, timeout: 5100, claim_addr: merchant }] },
  });
  const funded = await verifyL1LockFunding({
    tipHex: wallet.tipHex,
    merchantAddress: merchant,
    requiredBaseUnits: 5_000,
    fetchImpl: mockFundedFetch,
  });
  assert.equal(funded.funded, true);
  assert.equal(funded.amountBaseUnits, 50_000);

  // 3. Durable file store survives server restart & prevents replay + stays O(1)
  const tmpFile = path.join(os.tmpdir(), `ns402-store-${Date.now()}.json`);
  try {
    const store1 = await createFilePayWordStore(tmpFile);
    const gate1 = createPayWordGate({
      recipient: merchant,
      unitPriceBaseUnits: 1_000,
      requireFunding: true,
      fundingVerifier: async (req) =>
        verifyL1LockFunding({ ...req, fetchImpl: mockFundedFetch }),
      store: store1,
    });
    const p1 = wallet.nextPayment(1);
    const r1 = await gate1.verifyRequest(p1.headerValue);
    assert.equal(r1.paid, true);
    assert.equal(r1.collectibleOnChain, true);
    assert.equal(r1.fundingStatus, "verified_onchain_lock");

    // Simulate process restart with fresh gate reading same file
    const store2 = await createFilePayWordStore(tmpFile);
    const gate2 = createPayWordGate({
      recipient: merchant,
      unitPriceBaseUnits: 1_000,
      requireFunding: true,
      fundingVerifier: async (req) =>
        verifyL1LockFunding({ ...req, fetchImpl: mockFundedFetch }),
      store: store2,
    });
    const replayAfterRestart = await gate2.verifyRequest(p1.headerValue);
    assert.equal(replayAfterRestart.paid, false);

    const p2 = wallet.nextPayment(1);
    const r2 = await gate2.verifyRequest(p2.headerValue);
    assert.equal(r2.paid, true);
    assert.equal(r2.verificationMode, "incremental");
    assert.equal(r2.hashesEvaluated, 1);
    assert.equal(r2.collectibleOnChain, true);
  } finally {
    if (fs.existsSync(tmpFile)) fs.unlinkSync(tmpFile);
  }
});

test("stdio MCP bridge forwards JSON-RPC requests, drops notifications, and catches malformed JSON", async () => {
  const seen = [];
  const fakeFetch = async (url, init) => {
    seen.push({ url, body: JSON.parse(init.body) });
    return {
      ok: true,
      status: 200,
      text: async () =>
        JSON.stringify({ jsonrpc: "2.0", id: JSON.parse(init.body).id ?? null, result: { ok: true } }),
    };
  };

  const reply = await forwardJsonRpcLine(
    JSON.stringify({ jsonrpc: "2.0", id: 7, method: "tools/list", params: {} }),
    "https://commons.newsatoshi.org/mcp",
    fakeFetch
  );
  assert.equal(JSON.parse(reply).id, 7);
  assert.equal(JSON.parse(reply).result.ok, true);
  assert.equal(seen.length, 1);

  const notif = await forwardJsonRpcLine(
    JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" }),
    "https://commons.newsatoshi.org/mcp",
    fakeFetch
  );
  assert.equal(notif, null);

  const bad = await forwardJsonRpcLine("{not-json", "https://commons.newsatoshi.org/mcp", fakeFetch);
  assert.equal(JSON.parse(bad).error.code, -32700);
});

test("RFC-0022 L1 channel funding check, Merkle proof verification, and 70-byte unilateral ChannelClaim tx encoding", async () => {
  const crypto = await import("node:crypto");
  const wallet = await generatePayWordChain(1_000, 10_000);
  const seller = "ns1seller00000000000000000000000000000000000000000000000000000000";
  const channelId = "ab".repeat(32);
  const valueHash = "cd".repeat(32);
  const root = crypto
    .createHash("sha256")
    .update(Buffer.concat([Buffer.from("nsMKL"), Buffer.from(channelId, "hex"), Buffer.from(valueHash, "hex")]))
    .digest("hex");

  assert.equal(
    await verifyChannelMerkleProof({ root, key: channelId, value_hash: valueHash, siblings: [] }),
    true
  );
  assert.equal(
    await verifyChannelMerkleProof({ root, key: channelId, value_hash: "00".repeat(32), siblings: [] }),
    false
  );

  let postedBody = null;
  const mockNodeFetch = async (url, init = {}) => {
    if (url.includes("/api/channel?tip=")) {
      return {
        ok: true,
        status: 200,
        json: async () => ({
          height: 5000,
          channels: [
            {
              found: true,
              id: channelId,
              state: "open",
              confirmed: true,
              tip: wallet.tipHex,
              max_ticks: 1_000,
              unit_price: 10_000,
              claim_fee: 5_000,
              budget: 10_000_000,
              total_deposit: 10_005_000,
              timeout: 5100,
              seller,
              refund: "ns1buyer000000000000000000000000000000000000000000000000000000000",
              confirmations: 3,
              height: 5000,
              merkle_proof: { root, key: channelId, value_hash: valueHash, siblings: [] },
            },
          ],
        }),
      };
    }
    if (url.includes("/api/tx") && init.method === "POST") {
      postedBody = new Uint8Array(init.body);
      return {
        ok: true,
        status: 200,
        json: async () => ({ ok: true, txid: "ef".repeat(32) }),
      };
    }
    return { ok: false, status: 404, json: async () => ({ ok: false }) };
  };

  const chCheck = await verifyL1ChannelFunding({
    tipHex: wallet.tipHex,
    sellerAddress: seller,
    requiredTicks: 237,
    unitPriceBaseUnits: 10_000,
    fetchImpl: mockNodeFetch,
  });
  assert.equal(chCheck.funded, true);
  assert.equal(chCheck.rail, "rfc0022_payword_channel");
  assert.equal(chCheck.channelId, channelId);
  assert.equal(chCheck.merkleProofVerified, true);

  const gate = createPayWordGate({
    recipient: seller,
    unitPriceBaseUnits: 10_000,
    requireFunding: true,
    fundingRail: "channel",
    fetchImpl: mockNodeFetch,
  });
  const p237 = wallet.nextPayment(237);
  const gRes = await gate.verifyRequest(p237.headerValue);
  assert.equal(gRes.paid, true);
  assert.equal(gRes.collectibleOnChain, true);
  assert.equal(gRes.unilateralClaimReady, true);
  assert.equal(gRes.fundingStatus, "verified_onchain_channel");
  assert.equal(gRes.channelId, channelId);

  // Encode & submit exact seller-signed unilateral ChannelClaim (74 + 7856 = 7,930 bytes)
  const fakeSigHex = "11".repeat(7856);
  const rawClaim = encodeChannelClaimTx(channelId, 237, p237.tokenHex, fakeSigHex);
  assert.equal(rawClaim.length, 7930);
  assert.equal(rawClaim[0], 2);
  assert.equal(rawClaim[1], 11);
  assert.equal(new DataView(rawClaim.buffer).getUint32(34, true), 237);
  assert.equal(new DataView(rawClaim.buffer).getUint32(70, true), 7856);
  assert.equal(encodeChannelExpireTx(channelId).length, 34);

  const sub = await submitL1ChannelClaim({
    channelIdHex: channelId,
    ticks: 237,
    tokenHex: p237.tokenHex,
    sellerSigHex: fakeSigHex,
    fetchImpl: mockNodeFetch,
  });
  assert.equal(sub.ok, true);
  assert.equal(sub.txBytes, 7930);
  assert.equal(postedBody.length, 7930);
});
