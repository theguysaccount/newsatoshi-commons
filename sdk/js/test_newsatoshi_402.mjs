import test from "node:test";
import assert from "node:assert/strict";
import http from "node:http";
import {
  generatePayWordChain,
  verifyPayWordToken,
  createPayWordGate,
  fetchWithNewsatoshi402,
} from "./newsatoshi-402.mjs";

test("PayWord chain generation, verification, forgery rejection, and replay guard", async () => {
  const wallet = await generatePayWordChain(20, 2_500);
  assert.equal(wallet.tipHex.length, 64);
  assert.equal(wallet.remainingTicks, 20);

  const p1 = wallet.nextPayment(1);
  const ok1 = await verifyPayWordToken(p1.tipHex, p1.tokenHex, p1.ticks, 2_500);
  assert.equal(ok1.valid, true);
  assert.equal(ok1.totalPaidBaseUnits, 2_500);

  // Forged token rejected
  const forged = await verifyPayWordToken(p1.tipHex, "00".repeat(32), 1, 2_500);
  assert.equal(forged.valid, false);

  // Wrong tick count rejected
  const wrongTicks = await verifyPayWordToken(p1.tipHex, p1.tokenHex, 2, 2_500);
  assert.equal(wrongTicks.valid, false);

  // End-to-end live local HTTP 402 server + autonomous agent client
  const gate = createPayWordGate({ unitPriceBaseUnits: 2_500, amountLove: "0.000025" });
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
    res.end(JSON.stringify({ ok: true, ticks: check.ticks, deltaPaid: check.deltaPaidBaseUnits }));
  });

  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { port } = server.address();
  const url = `http://127.0.0.1:${port}/infer`;

  try {
    // Autonomous client receives 402, reveals tick #2, succeeds with HTTP 200
    const r1 = await fetchWithNewsatoshi402(url, wallet);
    assert.equal(r1.status, 200);
    const body1 = await r1.json();
    assert.equal(body1.ok, true);
    assert.equal(body1.ticks, 2);

    // Replaying tick #1 or tick #2 directly MUST fail with 402
    const replay = await fetch(url, {
      headers: { "X-Newsatoshi-PayWord": p1.headerValue },
    });
    assert.equal(replay.status, 402);

    // Next sequential call reveals tick #3 and succeeds
    const r2 = await fetchWithNewsatoshi402(url, wallet);
    assert.equal(r2.status, 200);
    const body2 = await r2.json();
    assert.equal(body2.ticks, 3);
    assert.equal(body2.deltaPaid, 2_500);
  } finally {
    server.close();
  }
});
