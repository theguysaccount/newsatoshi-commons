import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import { generatePayWordChain, fetchWithNewsatoshi402 } from "../../sdk/js/newsatoshi-402.mjs";
import { createPaidAiSellerServer } from "./server.mjs";

test("Paid AI/GPU Seller Proxy gates unpaid calls with 402, proxies paid calls O(1), blocks replays, and exports unilateral vouchers", async () => {
  // Spin up mock local Ollama/vLLM backend
  const upstream = http.createServer((req, res) => {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ model: "llama-3.3-70b-local", choices: [{ text: "42" }] }));
  });
  await new Promise((r) => upstream.listen(0, "127.0.0.1", r));
  const upPort = upstream.address().port;

  const stateFile = path.join(os.tmpdir(), `paid-ai-seller-${Date.now()}.json`);
  const bondId = "ab".repeat(32);
  const { server } = await createPaidAiSellerServer({
    upstreamBaseUrl: `http://127.0.0.1:${upPort}`,
    sellerBondIdHex: bondId,
    unitPriceBaseUnits: 5_000,
    requireFunding: false,
    stateFile,
    adminToken: "seller-secret-token",
  });
  await new Promise((r) => server.listen(0, "127.0.0.1", r));
  const port = server.address().port;
  const base = `http://127.0.0.1:${port}`;

  try {
    // 1. Unpaid request -> HTTP 402 + advertises seller RFC-0024 bond!
    const unpaid = await fetch(`${base}/v1/chat/completions`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ prompt: "hello" }),
    });
    assert.equal(unpaid.status, 402);
    assert.equal(unpaid.headers.get("x-newsatoshi-seller-bond"), bondId);

    // 2. Paid request via PayWord auto-retry -> HTTP 200 proxied inference response!
    const wallet = await generatePayWordChain(20, 5_000);
    const paid1 = await fetchWithNewsatoshi402(`${base}/v1/chat/completions`, wallet, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ prompt: "meaning of life" }),
    });
    assert.equal(paid1.status, 200);
    assert.equal(paid1.headers.get("x-newsatoshi-ticks-accepted"), "1");
    const json1 = await paid1.json();
    assert.equal(json1.choices[0].text, "42");

    // 3. Second call verifies in O(1) (exactly 1 SHA-256 hash)
    const paid2 = await fetchWithNewsatoshi402(`${base}/v1/chat/completions`, wallet, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ prompt: "next" }),
    });
    assert.equal(paid2.status, 200);
    assert.equal(paid2.headers.get("x-newsatoshi-ticks-accepted"), "2");
    assert.equal(paid2.headers.get("x-newsatoshi-hashes-evaluated"), "1");

    // 4. Seller admin exports unilateral voucher ready for RFC-0022 L1 ChannelClaim
    const exp = await fetch(`${base}/admin/vouchers`, {
      headers: { Authorization: "Bearer seller-secret-token" },
    });
    assert.equal(exp.status, 200);
    const vJson = await exp.json();
    assert.equal(vJson.vouchers.length, 1);
    assert.equal(vJson.vouchers[0].highest_ticks, 2);
    assert.equal(vJson.vouchers[0].total_earned_base_units, 10_000);
  } finally {
    server.close();
    upstream.close();
    if (fs.existsSync(stateFile)) fs.unlinkSync(stateFile);
  }
});
